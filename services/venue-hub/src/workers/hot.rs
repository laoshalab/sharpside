//! hot worker — 第 3 层（方案 A）降级为**对账补漏**：仍写浮仓快照，但信号只在
//! trade_watch（逐笔主源）漏掉仓位变化时才补发，跨源去重靠 `raw_trades` 信号账。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.1 / `docs/VENUE_DESIGN.md` §8 / `docs/FLOWS.md` §5。
//!
//! 每个 tick：对每个已注册 signal_source Venue 的到期监控目标（热钥 ∪ 活跃直接跟随 ∪
//! 活跃 identity 跟随下的各 trader），
//!   1. 拉 positions → 写 `trader_positions_snapshot`（append-only，推进快照供下轮对账）
//!   2. **对账**：用"落后一轮闭合窗口"——prev（上一轮 T_prev）减 prev_prev（再上一轮 T_prev_prev），
//!      Δ 落在已闭合区间 [T_prev_prev, T_prev] 内。此时 trade_watch（3s 轮询）早已轮询过该区间，
//!      故对 raw_trades 求带符号 size 之和即"trades 已覆盖量"，**无竞态、无双计**。
//!   3. 残差 = Δ − covered。|残差| > ε 才补发 diff 信号（trades 漏的 / 非交易仓位变化）。
//!
//! 为何落后一轮：若用 current vs prev，窗口 [T_prev, T_now] 含尚未被 trade_watch 轮询到的成交，
//! 残差会把"trads 还没抓到"误判为漏 → 双计。落后一轮保证窗口完全闭合，trade_watch 已覆盖。
//! 代价：diff 补漏滞后 2 个扫描周期（安全网，可接受）。
//!
//! trade_watch 宕机时 covered=0 → 残差=Δ → diff 接管为信号源（期望的降级行为）。
//! 赎回/split/转账导致的非交易仓位变化：残差非零会补发，但 copier 的 market_tradable 校验
//! 会 skip 已结算市场（赎回主场景），避免对已结算市场跟卖。

use crate::registry::enabled_signal_sources;
use crate::state::AppState;
use crate::workers::signal_emit::{emit_signals, SignalPayload};
use rust_decimal::prelude::ToPrimitive;
use sharpside_db::queries::{monitor, raw};
use sharpside_shared::{Platform, Side};
use sharpside_venues_core::VenueCapabilities;
use std::collections::HashMap;
use std::time::Duration;

/// 仓位变化阈值：|delta| > 此值才 emit（过滤浮点噪声）。
const DELTA_EPSILON: f64 = 1e-6;

pub async fn run(state: AppState) {
    // hot_secs 为调度节拍（自适应扫描 Phase B）：每这么久检查一次"谁到期"。
    // 真实扫描周期 = 热钥 per-row scan_interval_secs / 跟随类 follow_scan_secs。
    let tick = state.config.workers.hot_secs.max(1);
    let follow_interval = state.config.workers.follow_scan_secs.max(1) as i32;
    let due_cap = state.config.workers.hot_due_cap.max(1) as i64;
    let mut ticker = tokio::time::interval(Duration::from_secs(tick));
    loop {
        ticker.tick().await;
        for platform in enabled_signal_sources(&state.config.venues) {
            if let Some(venue) = state.registry.get(platform) {
                if !venue
                    .info()
                    .capabilities
                    .contains(VenueCapabilities::SIGNAL_SOURCE)
                {
                    continue;
                }
                let targets = match monitor::list_due_signal_targets(
                    &state.db,
                    platform.as_str(),
                    follow_interval,
                    due_cap,
                )
                .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(platform = platform.as_str(), error = %e, "hot 读到期监控目标清单失败");
                        continue;
                    }
                };
                for w in &targets {
                    match venue.positions(&w.address).await {
                        Ok(positions) => {
                            // 1. 写新快照（推进 last_scanned_at，供下轮 prev 用）
                            let now = chrono::Utc::now();
                            for p in &positions {
                                let _ = monitor::insert_position_snapshot(
                                    &state.db,
                                    platform.as_str(),
                                    &w.address,
                                    &p.token_id,
                                    Some(p.market_id.as_str()),
                                    p.size,
                                    p.avg_price,
                                    p.current_price,
                                    p.pnl,
                                    now,
                                )
                                .await;
                            }
                            // 2. 对账补漏（落后一轮闭合窗口 + 覆盖检查）
                            let signals = reconcile_positions(
                                platform,
                                &w.address,
                                w.identity_id,
                                &state,
                            )
                            .await;
                            // 3. emit 残差信号
                            emit_signals(&state, signals).await;
                        }
                        Err(sharpside_venues_core::VenueError::Unsupported(_)) => {}
                        Err(e) => {
                            tracing::warn!(platform = platform.as_str(), address = %w.address, error = %e, "hot 拉 positions 失败")
                        }
                    }
                }
            }
        }
        state.worker_ticks.touch_hot();
    }
}

/// 对账补漏：prev（上一轮）vs prev_prev（再上一轮）的仓位 Δ，减去 raw_trades 已覆盖量，残差才 emit。
///
/// 落后一轮闭合窗口：窗口 [T_prev_prev, T_prev] 完全闭合，trade_watch 已轮询过，覆盖查询无竞态。
/// 需要至少两轮快照（prev + prev_prev）才能对账；不足则静默（bootstrap）。
async fn reconcile_positions(
    platform: Platform,
    address: &str,
    identity_id: Option<uuid::Uuid>,
    state: &AppState,
) -> Vec<SignalPayload> {
    // prev = 上一轮快照（T_prev）。注意：本轮 current 已写入，故 latest_snapshots 返回的是上一轮。
    let prev = match monitor::latest_snapshots(&state.db, platform.as_str(), address).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(platform = platform.as_str(), address, error = %e, "对账读 prev 快照失败，本轮不 emit");
            return Vec::new();
        }
    };
    if prev.is_empty() {
        // 仅有本轮刚写的快照、或从未扫描：不足两轮，bootstrap 静默。
        return Vec::new();
    }
    // T_prev = prev 快照的捕获时刻（取 max，同一轮扫描各 token captured_at 一致）。
    let t_prev = match prev.iter().map(|s| s.captured_at).max() {
        Some(t) => t,
        None => return Vec::new(),
    };
    // prev_prev = T_prev 之前最近一轮快照（T_prev_prev）。
    let prev_prev = match monitor::latest_snapshots_before(&state.db, platform.as_str(), address, t_prev).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(platform = platform.as_str(), address, error = %e, "对账读 prev_prev 快照失败，本轮不 emit");
            return Vec::new();
        }
    };
    if prev_prev.is_empty() {
        // 仅一轮历史：bootstrap 静默（需两轮才能构成闭合窗口）。
        return Vec::new();
    }
    let t_prev_prev = prev_prev
        .iter()
        .map(|s| s.captured_at)
        .max()
        .unwrap_or(t_prev);

    let prev_prev_map: HashMap<String, &sharpside_db::models::PositionSnapshot> =
        prev_prev.iter().map(|s| (s.token_id.clone(), s)).collect();
    let prev_tokens: HashMap<&str, ()> = prev.iter().map(|s| (s.token_id.as_str(), ())).collect();

    let now = chrono::Utc::now();
    let trader_id = platform.normalize_trader_id(address);
    let mut signals = Vec::new();

    // prev 中各 token 的变化（相对 prev_prev）
    for s in &prev {
        let prev_prev_size = prev_prev_map
            .get(&s.token_id)
            .map(|p| p.size.to_f64().unwrap_or(0.0))
            .unwrap_or(0.0);
        let prev_size = s.size.to_f64().unwrap_or(0.0);
        let delta = prev_size - prev_prev_size;
        if delta.abs() <= DELTA_EPSILON {
            continue;
        }
        // 覆盖检查：raw_trades 在闭合窗口 [T_prev_prev, T_prev] 的带符号 size 之和。
        let covered = match raw::sum_signed_trade_size(
            &state.db,
            platform.as_str(),
            address,
            &s.token_id,
            t_prev_prev,
            t_prev,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    platform = platform.as_str(), address, token = %s.token_id, error = %e,
                    "覆盖查询失败，该 token 本轮跳过（下轮重试）",
                );
                continue;
            }
        };
        let residual = delta - covered;
        if residual.abs() <= DELTA_EPSILON {
            // trades 已完全覆盖该 Δ → 静默（不双计）
            continue;
        }
        let side = if residual > 0.0 { Side::Buy } else { Side::Sell };
        let price = s.avg_price.to_f64().unwrap_or(0.0);
        signals.push(SignalPayload {
            platform,
            trader_id: trader_id.clone(),
            token_id: s.token_id.clone(),
            market_id: s.condition_id.clone().unwrap_or_default(),
            side,
            price: if price > 0.0 { price } else { s.current_price.to_f64().unwrap_or(0.0) },
            size: residual.abs(),
            ts: now,
            identity_id,
            source_id: None,
        });
    }

    // prev_prev 有但 prev 无：窗口内完全平仓（trades 应已覆盖 → 残差通常为 0，但若 trades 漏则补发 Sell）。
    for s in &prev_prev {
        if prev_tokens.contains_key(s.token_id.as_str()) {
            continue;
        }
        let prev_prev_size = s.size.to_f64().unwrap_or(0.0);
        if prev_prev_size <= DELTA_EPSILON {
            continue;
        }
        // 平仓 Δ = -prev_prev_size（仓位从 prev_prev_size → 0）
        let delta = -prev_prev_size;
        let covered = match raw::sum_signed_trade_size(
            &state.db,
            platform.as_str(),
            address,
            &s.token_id,
            t_prev_prev,
            t_prev,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    platform = platform.as_str(), address, token = %s.token_id, error = %e,
                    "覆盖查询（平仓）失败，该 token 本轮跳过",
                );
                continue;
            }
        };
        let residual = delta - covered;
        if residual.abs() <= DELTA_EPSILON {
            continue;
        }
        // 平仓残差为负 → Sell |residual|
        let price = s.avg_price.to_f64().unwrap_or(0.0);
        signals.push(SignalPayload {
            platform,
            trader_id: trader_id.clone(),
            token_id: s.token_id.clone(),
            market_id: s.condition_id.clone().unwrap_or_default(),
            side: Side::Sell,
            price: if price > 0.0 { price } else { s.current_price.to_f64().unwrap_or(0.0) },
            size: residual.abs(),
            ts: now,
            identity_id,
            source_id: None,
        });
    }

    signals
}
