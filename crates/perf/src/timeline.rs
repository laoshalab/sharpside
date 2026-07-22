//! 仓位重建：从 `raw_trades` 回放重建 `position_timeline`。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` §3。
//! 对每个 `(platform, address, token_id)` 按时间顺序回放 trades：
//! - BUY 累加 running_size，更新 avg_cost（加权平均）
//! - SELL 计算 realized_pnl = (sell_price - avg_cost) * sell_size，扣减 running_size
//! - 结算（市场到期）按 outcome(0/1) 计算 realized_pnl，清零 running_size
//! - 任意时刻 open_size = running_size

use crate::types::{Settlement, TradeInput};
use sharpside_shared::Side;
use std::collections::HashMap;

use crate::types::PositionTimeline;

/// 单个仓位的回放累加器。
#[derive(Debug, Clone)]
struct PositionAccumulator {
    platform: sharpside_shared::Platform,
    address: String,
    token_id: String,
    condition_id: Option<String>,
    opened_at: Option<chrono::DateTime<chrono::Utc>>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    total_bought_size: f64,
    total_sold_size: f64,
    avg_cost: f64,
    realized_pnl: f64,
    running_size: f64,
    is_closed: bool,
    /// 每段持仓的时长（秒），用于算 median
    holding_segments: Vec<i64>,
    /// 当前段的开仓时间
    segment_opened_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl PositionAccumulator {
    fn new(trade: &TradeInput) -> Self {
        Self {
            platform: trade.platform,
            address: trade.address.clone(),
            token_id: trade.token_id.clone(),
            condition_id: trade.condition_id.clone(),
            opened_at: None,
            closed_at: None,
            total_bought_size: 0.0,
            total_sold_size: 0.0,
            avg_cost: 0.0,
            realized_pnl: 0.0,
            running_size: 0.0,
            is_closed: false,
            holding_segments: Vec::new(),
            segment_opened_at: None,
        }
    }

    fn apply(&mut self, trade: &TradeInput) {
        if self.is_closed {
            // 已结算的仓位再收到 trade，重开新段
            self.is_closed = false;
            self.opened_at = Some(trade.ts);
            self.segment_opened_at = Some(trade.ts);
            self.running_size = 0.0;
            self.avg_cost = 0.0;
        }
        // 首次开仓时记录 opened_at / segment_opened_at
        if self.opened_at.is_none() && trade.side == Side::Buy {
            self.opened_at = Some(trade.ts);
            self.segment_opened_at = Some(trade.ts);
        }
        if self.segment_opened_at.is_none() && trade.side == Side::Buy {
            self.segment_opened_at = Some(trade.ts);
        }
        match trade.side {
            Side::Buy => {
                let new_size = self.running_size + trade.size;
                self.avg_cost = if new_size > 0.0 {
                    (self.avg_cost * self.running_size + trade.price * trade.size) / new_size
                } else {
                    trade.price
                };
                self.running_size = new_size;
                self.total_bought_size += trade.size;
            }
            Side::Sell => {
                let sell_size = trade.size.min(self.running_size);
                if sell_size > 0.0 {
                    self.realized_pnl += (trade.price - self.avg_cost) * sell_size;
                    self.running_size -= sell_size;
                    self.total_sold_size += sell_size;
                }
                if self.running_size <= 0.0 {
                    self.close_segment(trade.ts);
                    self.running_size = 0.0;
                }
            }
        }
    }

    fn settle(&mut self, outcome: f64, settled_at: chrono::DateTime<chrono::Utc>) {
        if self.is_closed || self.running_size <= 0.0 {
            return;
        }
        // 结算：YES token outcome=1 → 每单位值 1；outcome=0 → 值 0
        self.realized_pnl += self.running_size * (outcome - self.avg_cost);
        self.running_size = 0.0;
        self.is_closed = true;
        self.closed_at = Some(settled_at);
        self.close_segment(settled_at);
    }

    fn close_segment(&mut self, at: chrono::DateTime<chrono::Utc>) {
        if let Some(opened) = self.segment_opened_at {
            let secs = (at - opened).num_seconds();
            if secs > 0 {
                self.holding_segments.push(secs);
            }
        }
        self.segment_opened_at = None;
        if self.running_size <= 0.0 {
            self.is_closed = true;
            self.closed_at = Some(at);
        }
    }

    fn finalize(self) -> PositionTimeline {
        let holding_seconds = median(&self.holding_segments);
        PositionTimeline {
            platform: self.platform,
            address: self.address,
            token_id: self.token_id,
            condition_id: self.condition_id,
            opened_at: self.opened_at,
            closed_at: self.closed_at,
            total_bought_size: self.total_bought_size,
            total_sold_size: self.total_sold_size,
            avg_cost: self.avg_cost,
            realized_pnl: self.realized_pnl,
            final_open_size: self.running_size,
            is_closed: self.is_closed,
            holding_seconds,
        }
    }
}

/// 从 `raw_trades` 重建仓位时间线。
///
/// 输入按 `(platform, address, token_id)` 分组，组内按 `ts` 升序回放。
/// `settlements` 提供市场到期结算结果（outcome 0/1），按 `token_id` 匹配。
pub fn reconstruct_position_timeline(
    trades: &[TradeInput],
    settlements: &[Settlement],
) -> Vec<PositionTimeline> {
    // 按 (platform, address, token_id) 分组
    let mut groups: HashMap<(sharpside_shared::Platform, String, String), Vec<&TradeInput>> =
        HashMap::new();
    for t in trades {
        groups
            .entry((t.platform, t.address.clone(), t.token_id.clone()))
            .or_default()
            .push(t);
    }

    // 组内按 ts 升序
    for group in groups.values_mut() {
        group.sort_by_key(|t| t.ts);
    }

    let mut out = Vec::with_capacity(groups.len());
    for ((platform, address, token_id), group) in groups {
        let first = *group.first().expect("non-empty group");
        let mut acc = PositionAccumulator::new(first);
        for trade in group.iter() {
            acc.apply(trade);
        }

        // 应用结算（按 token_id 匹配，取最早的未应用结算）
        if let Some(settle) = settlements
            .iter()
            .filter(|s| s.token_id == token_id)
            .min_by_key(|s| s.settled_at)
        {
            acc.settle(settle.outcome, settle.settled_at);
        }

        let _ = (platform, address); // 已在 acc 内
        out.push(acc.finalize());
    }

    // 稳定排序：按 (platform, address, token_id) 输出，便于测试断言
    out.sort_by(|a, b| {
        (a.platform, &a.address, &a.token_id).cmp(&(b.platform, &b.address, &b.token_id))
    });
    out
}

/// 计算中位数（整数秒）。空切片返回 None。
fn median(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    let med = if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    };
    Some(med)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_shared::Platform;

    fn t(
        platform: Platform,
        addr: &str,
        token: &str,
        side: Side,
        price: f64,
        size: f64,
        ts: &str,
    ) -> TradeInput {
        TradeInput {
            platform,
            address: addr.into(),
            token_id: token.into(),
            condition_id: None,
            side,
            price,
            size,
            ts: chrono::DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&chrono::Utc),
        }
    }

    fn ts(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn median_basic() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[5]), Some(5));
        assert_eq!(median(&[1, 3]), Some(2));
        assert_eq!(median(&[1, 3, 5]), Some(3));
        assert_eq!(median(&[1, 2, 3, 4]), Some(2)); // (2+3)/2 = 2 (integer)
    }

    #[test]
    fn buy_then_sell_realizes_pnl() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.50,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.60,
                100.0,
                "2026-01-01T01:00:00Z",
            ),
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        assert_eq!(timeline.len(), 1);
        let p = &timeline[0];
        assert!((p.avg_cost - 0.50).abs() < 1e-9);
        assert!((p.realized_pnl - 10.0).abs() < 1e-9); // (0.60 - 0.50) * 100
        assert!(p.is_closed);
        assert_eq!(p.final_open_size, 0.0);
        assert_eq!(p.total_bought_size, 100.0);
        assert_eq!(p.total_sold_size, 100.0);
    }

    #[test]
    fn weighted_avg_cost_on_partial_buy() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.40,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.60,
                100.0,
                "2026-01-01T01:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.55,
                200.0,
                "2026-01-01T02:00:00Z",
            ),
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let p = &timeline[0];
        // avg_cost = (0.40*100 + 0.60*100) / 200 = 0.50
        assert!((p.avg_cost - 0.50).abs() < 1e-9);
        // realized = (0.55 - 0.50) * 200 = 10.0
        assert!((p.realized_pnl - 10.0).abs() < 1e-9);
        assert!(p.is_closed);
    }

    #[test]
    fn settlement_yes_wins() {
        let trades = vec![t(
            Platform::Polymarket,
            "0xa",
            "tok1",
            Side::Buy,
            0.30,
            100.0,
            "2026-01-01T00:00:00Z",
        )];
        let settlements = vec![Settlement {
            token_id: "tok1".into(),
            outcome: 1.0, // YES wins
            settled_at: ts("2026-01-10T00:00:00Z"),
        }];
        let timeline = reconstruct_position_timeline(&trades, &settlements);
        let p = &timeline[0];
        // realized = 100 * (1.0 - 0.30) = 70.0
        assert!((p.realized_pnl - 70.0).abs() < 1e-9);
        assert!(p.is_closed);
        assert_eq!(p.final_open_size, 0.0);
    }

    #[test]
    fn settlement_yes_loses() {
        let trades = vec![t(
            Platform::Polymarket,
            "0xa",
            "tok1",
            Side::Buy,
            0.30,
            100.0,
            "2026-01-01T00:00:00Z",
        )];
        let settlements = vec![Settlement {
            token_id: "tok1".into(),
            outcome: 0.0, // YES loses
            settled_at: ts("2026-01-10T00:00:00Z"),
        }];
        let timeline = reconstruct_position_timeline(&trades, &settlements);
        let p = &timeline[0];
        // realized = 100 * (0.0 - 0.30) = -30.0
        assert!((p.realized_pnl - (-30.0)).abs() < 1e-9);
        assert!(p.is_closed);
    }

    #[test]
    fn open_position_not_closed() {
        let trades = vec![t(
            Platform::Polymarket,
            "0xa",
            "tok1",
            Side::Buy,
            0.50,
            100.0,
            "2026-01-01T00:00:00Z",
        )];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let p = &timeline[0];
        assert!(!p.is_closed);
        assert_eq!(p.final_open_size, 100.0);
        assert_eq!(p.realized_pnl, 0.0);
    }

    #[test]
    fn multiple_tokens_grouped_separately() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.50,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok2",
                Side::Buy,
                0.30,
                50.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.55,
                100.0,
                "2026-01-02T00:00:00Z",
            ),
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        assert_eq!(timeline.len(), 2);
        // 稳定排序按 token_id
        assert_eq!(timeline[0].token_id, "tok1");
        assert_eq!(timeline[1].token_id, "tok2");
        // tok1 已平仓盈利
        assert!((timeline[0].realized_pnl - 5.0).abs() < 1e-9);
        // tok2 仍持仓
        assert!(!timeline[1].is_closed);
    }

    #[test]
    fn holding_seconds_recorded() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.50,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.60,
                100.0,
                "2026-01-02T00:00:00Z",
            ), // +86400s
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let p = &timeline[0];
        assert_eq!(p.holding_seconds, Some(86_400)); // 24h
    }

    #[test]
    fn partial_sell_keeps_open() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.50,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.60,
                40.0,
                "2026-01-02T00:00:00Z",
            ),
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let p = &timeline[0];
        assert!(!p.is_closed);
        assert_eq!(p.final_open_size, 60.0);
        // realized = (0.60 - 0.50) * 40 = 4.0
        assert!((p.realized_pnl - 4.0).abs() < 1e-9);
    }

    #[test]
    fn sell_more_than_held_clamps() {
        let trades = vec![
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Buy,
                0.50,
                100.0,
                "2026-01-01T00:00:00Z",
            ),
            t(
                Platform::Polymarket,
                "0xa",
                "tok1",
                Side::Sell,
                0.60,
                150.0,
                "2026-01-02T00:00:00Z",
            ), // 卖 150 但只持 100
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let p = &timeline[0];
        assert!(p.is_closed);
        assert_eq!(p.final_open_size, 0.0);
        assert_eq!(p.total_sold_size, 100.0); // 只算实际能卖的 100
        assert!((p.realized_pnl - 10.0).abs() < 1e-9); // (0.60-0.50)*100
    }
}
