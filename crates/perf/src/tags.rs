//! 运营标签计算（DW / type-3）。对应 `docs/PERFORMANCE_PIPELINE.md` §4.4。
//!
//! 标签规则（`docs/PERFORMANCE_PIPELINE.md` §4.4）：
//! - `DW:diamond`：median(holding_seconds) > 24h（持有型）
//! - `DW:win`：win_rate > 60% 且 roi > 0（高胜率型）
//! - `type-3:limit_sniper`：限价单占比 > 70% 且 fill 时长 < 2 block
//! - `type-3:market_follow`：市价单占比 > 70%
//! - `type-3:rebalance`：同 token_id 单日反向交易次数 > 阈值
//!
//! **阈值不硬编码**：全部从 [`TagThresholds`] 参数取，venue-hub 服务从 `tag_rules` 表读取后传入。
//! 运营后台改阈值，下次重算生效。

use crate::types::{FillStats, TagThresholds};
use sharpside_shared::{Performance, Tag, TagKind};

/// 计算单个交易者的所有标签。
///
/// `perf`：该交易者某周期绩效（由 [`crate::metrics::compute_performance`] 产出）。
/// `median_holding_seconds`：所有仓位持有秒数的中位数（由 [`crate::timeline`] 产出）。
/// `fill_stats`：成交手法统计（由 venue-hub 从 raw_trades 聚合）。
/// `thresholds`：标签阈值（默认值见 [`TagThresholds::default`]，生产从 `tag_rules` 表读）。
pub fn compute_tags(
    perf: &Performance,
    median_holding_seconds: Option<i64>,
    fill_stats: &FillStats,
    thresholds: &TagThresholds,
) -> Vec<Tag> {
    let mut tags = Vec::new();

    // DW:diamond — 持有型
    if let Some(holding) = median_holding_seconds {
        if holding > thresholds.dw_diamond_min_holding_seconds {
            tags.push(Tag {
                kind: TagKind::DwDiamond,
                attrs: Some(serde_json::json!({ "median_holding_seconds": holding })),
            });
        }
    }

    // DW:win — 高胜率型
    if perf.win_rate > thresholds.dw_win_min_win_rate && perf.roi > thresholds.dw_win_min_roi {
        tags.push(Tag {
            kind: TagKind::DwWin,
            attrs: Some(serde_json::json!({
                "win_rate": perf.win_rate,
                "roi": perf.roi,
            })),
        });
    }

    // type-3:limit_sniper — 限价狙击
    if fill_stats.limit_ratio() > thresholds.limit_sniper_min_limit_ratio
        && fill_stats.avg_limit_fill_seconds < thresholds.limit_sniper_max_fill_seconds
    {
        tags.push(Tag {
            kind: TagKind::Type3LimitSniper,
            attrs: Some(serde_json::json!({
                "limit_ratio": fill_stats.limit_ratio(),
                "avg_limit_fill_seconds": fill_stats.avg_limit_fill_seconds,
            })),
        });
    }

    // type-3:market_follow — 市价跟随
    if fill_stats.market_ratio() > thresholds.market_follow_min_market_ratio {
        tags.push(Tag {
            kind: TagKind::Type3MarketFollow,
            attrs: Some(serde_json::json!({
                "market_ratio": fill_stats.market_ratio(),
            })),
        });
    }

    // type-3:rebalance — 再平衡
    if fill_stats.max_daily_reversals > thresholds.rebalance_min_daily_reversals {
        tags.push(Tag {
            kind: TagKind::Type3Rebalance,
            attrs: Some(serde_json::json!({
                "max_daily_reversals": fill_stats.max_daily_reversals,
            })),
        });
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_shared::Performance;

    fn perf(win_rate: f64, roi: f64) -> Performance {
        Performance {
            win_rate,
            roi,
            ..Performance::zero()
        }
    }

    #[test]
    fn dw_diamond_long_holder() {
        let p = perf(0.5, 0.0);
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, Some(100_000), &fills, &thresholds); // > 86400
        assert!(tags.iter().any(|t| t.kind == TagKind::DwDiamond));
    }

    #[test]
    fn dw_diamond_short_holder_no_tag() {
        let p = perf(0.5, 0.0);
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, Some(3600), &fills, &thresholds); // < 86400
        assert!(!tags.iter().any(|t| t.kind == TagKind::DwDiamond));
    }

    #[test]
    fn dw_win_high_winrate_positive_roi() {
        let p = perf(0.65, 0.30);
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.iter().any(|t| t.kind == TagKind::DwWin));
    }

    #[test]
    fn dw_win_low_winrate_no_tag() {
        let p = perf(0.55, 0.30); // win_rate < 0.60
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(!tags.iter().any(|t| t.kind == TagKind::DwWin));
    }

    #[test]
    fn dw_win_negative_roi_no_tag() {
        let p = perf(0.65, -0.10); // roi < 0
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(!tags.iter().any(|t| t.kind == TagKind::DwWin));
    }

    #[test]
    fn limit_sniper_high_limit_ratio_fast_fill() {
        let p = perf(0.5, 0.0);
        let fills = FillStats {
            limit_orders: 80,
            market_orders: 20,
            avg_limit_fill_seconds: 10.0, // < 24
            max_daily_reversals: 0,
        };
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.iter().any(|t| t.kind == TagKind::Type3LimitSniper));
        assert!(!tags.iter().any(|t| t.kind == TagKind::Type3MarketFollow));
    }

    #[test]
    fn limit_sniper_slow_fill_no_tag() {
        let p = perf(0.5, 0.0);
        let fills = FillStats {
            limit_orders: 80,
            market_orders: 20,
            avg_limit_fill_seconds: 60.0, // > 24
            max_daily_reversals: 0,
        };
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(!tags.iter().any(|t| t.kind == TagKind::Type3LimitSniper));
    }

    #[test]
    fn market_follow_high_market_ratio() {
        let p = perf(0.5, 0.0);
        let fills = FillStats {
            limit_orders: 20,
            market_orders: 80,
            avg_limit_fill_seconds: 0.0,
            max_daily_reversals: 0,
        };
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.iter().any(|t| t.kind == TagKind::Type3MarketFollow));
        assert!(!tags.iter().any(|t| t.kind == TagKind::Type3LimitSniper));
    }

    #[test]
    fn rebalance_high_daily_reversals() {
        let p = perf(0.5, 0.0);
        let fills = FillStats {
            limit_orders: 0,
            market_orders: 0,
            avg_limit_fill_seconds: 0.0,
            max_daily_reversals: 5, // > 3
        };
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.iter().any(|t| t.kind == TagKind::Type3Rebalance));
    }

    #[test]
    fn custom_thresholds_override_defaults() {
        let p = perf(0.55, 0.10); // 默认 win_rate 阈值 0.60，不达标；放宽阈值 + roi>0
        let fills = FillStats::default();
        let thresholds = TagThresholds {
            dw_win_min_win_rate: 0.50, // 放宽
            ..TagThresholds::default()
        };
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.iter().any(|t| t.kind == TagKind::DwWin));
    }

    #[test]
    fn no_tags_for_mediocre_trader() {
        let p = perf(0.50, 0.0);
        let fills = FillStats::default();
        let thresholds = TagThresholds::default();
        let tags = compute_tags(&p, None, &fills, &thresholds);
        assert!(tags.is_empty());
    }
}
