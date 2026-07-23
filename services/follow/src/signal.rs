//! 信号派生：`trader.position.changed` → 匹配 follow_relation → 派生 `copy_order`。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.2 与 `docs/FLOWS.md` §5。
//! 纯函数 [`derive_copy_orders`] 无 IO，便于单测；HTTP 端点 `/internal/signals` 调用后落库。
//!
//! 规则：
//! - 跟随 trader：`(follow_platform, follow_address)` 命中信号源
//! - 跟随 identity：`follow_identity_id` 命中信号的 `identity_id`，且须 `manual_verified=true`，否则派生为 skipped + 告警
//! - `same_venue_only=true` 且 source_venue != execute_venue → skipped
//! - sizing：Fixed = amount/price；Proportional = signal.size*ratio
//! - `max_notional_per_order > 0` 且 size*price 超限 → skipped

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sharpside_db::models::FollowRelation;
use sharpside_shared::{Channel, FollowConfig, Platform, Side, SizingMode};
use std::collections::HashSet;
use uuid::Uuid;

/// 仓位变化信号（venue-hub 检出后 POST 到 `/internal/signals`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub platform: Platform,
    /// 交易者地址（链上小写 / KYC id）
    pub trader_id: String,
    pub token_id: String,
    pub market_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub ts: DateTime<Utc>,
    /// 该 trader 已链接的 identity_id（若有）；identity 跟随靠此匹配
    #[serde(default)]
    pub identity_id: Option<Uuid>,
    /// 逐笔信号 = 成交 ID（raw_trades.trade_id/tx_hash）；diff 信号 = None。
    /// 与 venue-hub 侧共同决定 signal_id，避免同秒同 token 多笔撞键。
    #[serde(default)]
    pub source_id: Option<String>,
}

/// 派生出的待入队指令（含 skip_reason；Some 表示入队即 skipped）。
#[derive(Debug, Clone, Serialize)]
pub struct DerivedOrder {
    pub follow_relation_id: Uuid,
    pub user_id: Uuid,
    pub source_venue: Platform,
    pub execute_venue: Platform,
    pub source_market_id: String,
    pub source_token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub channel: Channel,
    pub signal_at: DateTime<Utc>,
    pub skip_reason: Option<String>,
}

/// 已 `manual_verified=true` 的 identity 集合（由 handler 从 db 查后传入）。
pub fn derive_copy_orders(
    event: &SignalEvent,
    relations: &[FollowRelation],
    verified_identity_ids: &HashSet<Uuid>,
) -> Vec<DerivedOrder> {
    let mut out = Vec::new();
    for rel in relations {
        let Some((execute_venue, channel, config)) = parse_relation(rel) else {
            // P1-11：config/venue/channel 解析失败不再静默丢弃，记 error 便于排查。
            // 旧逻辑静默 continue：matched_relations 仍计数但 enqueued=0，metrics 误导且无日志。
            tracing::error!(
                relation_id = %rel.id,
                user_id = %rel.user_id,
                execute_venue = %rel.execute_venue,
                channel = %rel.channel,
                "follow_relation 解析失败（config/venue/channel 非法），静默跳过；请核对 relation 配置",
            );
            continue;
        };
        let matched = match_relation(rel, event, verified_identity_ids);
        let (matched, identity_skip) = match matched {
            MatchResult::NoMatch => continue,
            MatchResult::Matched => (true, None),
            MatchResult::IdentityUnverified => {
                (true, Some("identity 未 manual_verified".to_string()))
            }
        };
        let _ = matched;

        let mut skip = identity_skip;

        // same_venue_only 校验
        if skip.is_none() && rel.same_venue_only && event.platform != execute_venue {
            skip = Some("same_venue_only 违反（source != execute）".to_string());
        }

        // sizing
        let mut size = 0.0f64;
        if skip.is_none() {
            match config.sizing {
                SizingMode::Fixed { amount } => {
                    // 安全修复 4.4：Fixed amount 须 > 0。
                    if amount <= 0.0 {
                        skip = Some(format!("Fixed amount 须 > 0，当前 {amount}"));
                    } else if event.price > 0.0 {
                        size = amount / event.price;
                    } else {
                        skip = Some("Fixed sizing 但 price<=0".into());
                    }
                }
                SizingMode::Proportional { ratio } => {
                    // 安全修复 4.4：Proportional ratio ∈ (0, 1]。
                    if !(ratio > 0.0 && ratio <= 1.0) {
                        skip = Some(format!("Proportional ratio 须 ∈ (0,1]，当前 {ratio}"));
                    } else {
                        size = event.size * ratio;
                    }
                }
            }
        }

        // max_notional_per_order
        if skip.is_none()
            && config.max_notional_per_order > 0.0
            && size * event.price > config.max_notional_per_order
        {
            skip = Some(format!(
                "超出单笔 max_notional {}",
                config.max_notional_per_order
            ));
        }

        out.push(DerivedOrder {
            follow_relation_id: rel.id,
            user_id: rel.user_id,
            source_venue: event.platform,
            execute_venue,
            source_market_id: event.market_id.clone(),
            source_token_id: event.token_id.clone(),
            side: event.side,
            price: event.price,
            size,
            channel,
            signal_at: event.ts,
            skip_reason: skip,
        });
    }
    out
}

enum MatchResult {
    NoMatch,
    Matched,
    IdentityUnverified,
}

fn match_relation(
    rel: &FollowRelation,
    event: &SignalEvent,
    verified_identity_ids: &HashSet<Uuid>,
) -> MatchResult {
    // trader 跟随
    if let (Some(fp), Some(fa)) = (
        rel.follow_platform.as_deref(),
        rel.follow_address.as_deref(),
    ) {
        // 安全修复 4.4：匹配前归一地址（checksum vs 小写都能命中）。
        let fa_norm = sharpside_shared::normalize_trader_id(fp, fa);
        let event_norm =
            sharpside_shared::normalize_trader_id(event.platform.as_str(), &event.trader_id);
        if fp == event.platform.as_str() && fa_norm == event_norm {
            return MatchResult::Matched;
        }
        return MatchResult::NoMatch;
    }
    // identity 跟随
    if let Some(identity_id) = rel.follow_identity_id {
        if event.identity_id == Some(identity_id) {
            if verified_identity_ids.contains(&identity_id) {
                return MatchResult::Matched;
            }
            return MatchResult::IdentityUnverified;
        }
    }
    MatchResult::NoMatch
}

fn parse_relation(rel: &FollowRelation) -> Option<(Platform, Channel, FollowConfig)> {
    let execute_venue = rel.execute_venue.parse::<Platform>().ok()?;
    let channel = rel.channel.parse::<Channel>().ok()?;
    let config: FollowConfig = serde_json::from_value(rel.config.clone()).ok()?;
    Some((execute_venue, channel, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn rel_trader(user: Uuid, fp: &str, fa: &str, exec: &str, cfg: FollowConfig) -> FollowRelation {
        FollowRelation {
            id: Uuid::new_v4(),
            user_id: user,
            follow_platform: Some(fp.into()),
            follow_address: Some(fa.into()),
            follow_identity_id: None,
            execute_venue: exec.into(),
            channel: "daemon".into(),
            config: serde_json::to_value(&cfg).unwrap(),
            same_venue_only: cfg.same_venue_only,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    fn rel_identity(
        user: Uuid,
        identity_id: Uuid,
        exec: &str,
        cfg: FollowConfig,
    ) -> FollowRelation {
        FollowRelation {
            id: Uuid::new_v4(),
            user_id: user,
            follow_platform: None,
            follow_address: None,
            follow_identity_id: Some(identity_id),
            execute_venue: exec.into(),
            channel: "daemon".into(),
            config: serde_json::to_value(&cfg).unwrap(),
            same_venue_only: cfg.same_venue_only,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    fn cfg_fixed(amount: f64, same_venue_only: bool) -> FollowConfig {
        FollowConfig {
            sizing: SizingMode::Fixed { amount },
            max_notional_per_order: 0.0,
            daily_max_notional: 0.0,
            max_open_positions: 0,
            execute_venue: Platform::Polymarket,
            channel: Channel::Daemon,
            same_venue_only,
        }
    }

    fn event(platform: Platform, trader: &str, identity_id: Option<Uuid>) -> SignalEvent {
        SignalEvent {
            platform,
            trader_id: trader.into(),
            token_id: "tok".into(),
            market_id: "mkt".into(),
            side: Side::Buy,
            price: 0.5,
            size: 100.0,
            ts: Utc::now(),
            identity_id,
            source_id: None,
        }
    }

    #[test]
    fn trader_match_fixed_sizing() {
        let user = Uuid::new_v4();
        let rel = rel_trader(
            user,
            "polymarket",
            "0xabc",
            "polymarket",
            cfg_fixed(50.0, false),
        );
        let ev = event(Platform::Polymarket, "0xabc", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert_eq!(out.len(), 1);
        assert!(out[0].skip_reason.is_none());
        // Fixed 50 / price 0.5 = 100 shares
        assert!((out[0].size - 100.0).abs() < 1e-6);
    }

    #[test]
    fn no_match_when_trader_differs() {
        let user = Uuid::new_v4();
        let rel = rel_trader(
            user,
            "polymarket",
            "0xabc",
            "polymarket",
            cfg_fixed(50.0, false),
        );
        let ev = event(Platform::Polymarket, "0xother", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert!(out.is_empty());
    }

    #[test]
    fn same_venue_only_violated_skipped() {
        let user = Uuid::new_v4();
        let rel = rel_trader(user, "polymarket", "0xabc", "kalshi", cfg_fixed(50.0, true));
        let ev = event(Platform::Polymarket, "0xabc", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .skip_reason
            .as_deref()
            .unwrap()
            .contains("same_venue_only"));
    }

    #[test]
    fn identity_unverified_skipped() {
        let user = Uuid::new_v4();
        let identity = Uuid::new_v4();
        let rel = rel_identity(user, identity, "polymarket", cfg_fixed(50.0, false));
        let ev = event(Platform::Polymarket, "0xabc", Some(identity));
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .skip_reason
            .as_deref()
            .unwrap()
            .contains("manual_verified"));
    }

    #[test]
    fn identity_verified_matched() {
        let user = Uuid::new_v4();
        let identity = Uuid::new_v4();
        let rel = rel_identity(user, identity, "polymarket", cfg_fixed(50.0, false));
        let ev = event(Platform::Polymarket, "0xabc", Some(identity));
        let verified = [identity].into_iter().collect();
        let out = derive_copy_orders(&ev, &[rel], &verified);
        assert_eq!(out.len(), 1);
        assert!(out[0].skip_reason.is_none());
    }

    #[test]
    fn proportional_sizing() {
        let user = Uuid::new_v4();
        let cfg = FollowConfig {
            sizing: SizingMode::Proportional { ratio: 0.5 },
            max_notional_per_order: 0.0,
            daily_max_notional: 0.0,
            max_open_positions: 0,
            execute_venue: Platform::Polymarket,
            channel: Channel::Daemon,
            same_venue_only: false,
        };
        let rel = rel_trader(user, "polymarket", "0xabc", "polymarket", cfg);
        let ev = event(Platform::Polymarket, "0xabc", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert!((out[0].size - 50.0).abs() < 1e-6);
    }

    #[test]
    fn max_notional_exceeded_skipped() {
        let user = Uuid::new_v4();
        let cfg = FollowConfig {
            sizing: SizingMode::Fixed { amount: 1000.0 },
            max_notional_per_order: 100.0,
            daily_max_notional: 0.0,
            max_open_positions: 0,
            execute_venue: Platform::Polymarket,
            channel: Channel::Daemon,
            same_venue_only: false,
        };
        let rel = rel_trader(user, "polymarket", "0xabc", "polymarket", cfg);
        // price 0.5 → size 2000 → notional 1000 > 100
        let ev = event(Platform::Polymarket, "0xabc", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert!(out[0]
            .skip_reason
            .as_deref()
            .unwrap()
            .contains("max_notional"));
    }

    /// 安全修复 4.4：checksum 地址与小写地址应匹配。
    #[test]
    fn trader_match_case_insensitive_for_chain() {
        let user = Uuid::new_v4();
        let rel = rel_trader(
            user,
            "polymarket",
            "0xAbC123",
            "polymarket",
            cfg_fixed(50.0, false),
        );
        let ev = event(Platform::Polymarket, "0xabc123", None);
        let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
        assert_eq!(out.len(), 1);
        assert!(out[0].skip_reason.is_none());
    }

    /// 安全修复 4.4：ratio ≤0 或 >1 → skipped。
    #[test]
    fn proportional_ratio_out_of_bounds_skipped() {
        let user = Uuid::new_v4();
        for bad in [0.0, -0.5, 1.5] {
            let cfg = FollowConfig {
                sizing: SizingMode::Proportional { ratio: bad },
                max_notional_per_order: 0.0,
                daily_max_notional: 0.0,
                max_open_positions: 0,
                execute_venue: Platform::Polymarket,
                channel: Channel::Daemon,
                same_venue_only: false,
            };
            let rel = rel_trader(user, "polymarket", "0xabc", "polymarket", cfg);
            let ev = event(Platform::Polymarket, "0xabc", None);
            let out = derive_copy_orders(&ev, &[rel], &HashSet::new());
            assert!(
                out[0]
                    .skip_reason
                    .as_deref()
                    .unwrap_or("")
                    .contains("ratio"),
                "ratio={bad} 应 skip"
            );
        }
    }

    #[test]
    fn _ensure_json_import_used() {
        // 占位：serde_json 在测试 helper 中通过 to_value 使用
        let _ = json!({"a":1});
    }
}
