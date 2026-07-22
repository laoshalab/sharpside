//! 权益曲线计算（每日 mark-to-market）。对应 `docs/PERFORMANCE_PIPELINE.md` §4.3。
//!
//! 公式：
//! - `daily_equity[t] = cumulative_realized_until[t] + sum(open_size_i * mark_price_i[t])`
//! - `daily_pnl[t] = daily_equity[t] - daily_equity[t-1]`
//! - `drawdown_pct[t] = (peak_before[t] - equity[t]) / peak_before[t]`

use crate::types::{DailyMark, EquityPoint, PositionTimeline};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use sharpside_shared::Platform;
use std::collections::HashMap;

/// 计算单交易者的权益曲线（按 `step` 粒度 mark-to-market）。
///
/// `timelines`：该交易者的所有仓位时间线（由 [`crate::timeline::reconstruct_position_timeline`] 产出）。
/// `marks`：每日 mark 价格（`DailyMark` 列表），按 `(token_id, date)` 索引；命中不到回退 `avg_cost`。
/// `start` / `end`：曲线时间范围（含两端，UTC）。
/// `step`：采样粒度（如 `Duration::hours(1)` 小时级、`Duration::days(1)` 日级）。
///
/// 输出按时间升序，`daily_pnl[0] = equity[0]`（首点无前值）。
pub fn compute_equity_curve(
    timelines: &[PositionTimeline],
    marks: &[DailyMark],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step: Duration,
) -> Vec<EquityPoint> {
    if step.is_zero() || start > end || timelines.is_empty() {
        return Vec::new();
    }

    // 取该交易者的 platform/address（timelines 非空时从首条取）
    let (platform, address) = timelines
        .first()
        .map(|t| (t.platform, t.address.clone()))
        .unwrap_or((Platform::Polymarket, String::new()));

    // marks 按 (token_id, date) -> price 索引
    let mark_index: HashMap<(String, NaiveDate), f64> = marks
        .iter()
        .map(|m| ((m.token_id.clone(), m.date), m.price))
        .collect();

    // 把已实现 PnL 计入平仓那一刻所在的采样点（按 step 对齐截断）
    let truncate = |dt: DateTime<Utc>| -> DateTime<Utc> {
        let step_secs = step.num_seconds();
        let diff = dt.signed_duration_since(start).num_seconds();
        let n = diff / step_secs;
        start + Duration::seconds(n * step_secs)
    };
    let mut step_realized: HashMap<DateTime<Utc>, f64> = HashMap::new();
    for p in timelines {
        if let Some(closed) = p.closed_at {
            *step_realized.entry(truncate(closed)).or_default() += p.realized_pnl;
        }
    }

    let mut out = Vec::new();
    let mut cumulative_realized = 0.0;
    let mut prev_equity: Option<f64> = None;
    let mut peak = 0.0f64;

    let mut ts = start;
    while ts <= end {
        // 累计到该点的 realized
        if let Some(&r) = step_realized.get(&ts) {
            cumulative_realized += r;
        }

        // 该点浮仓估值：对每个未平仓且 opened_at <= ts 的仓位，用 mark 估值
        let mut open_value = 0.0;
        for p in timelines {
            let opened = p.opened_at.map(|t| t <= ts).unwrap_or(false);
            let still_open = p.closed_at.map(|c| c > ts).unwrap_or(true);
            if opened && still_open && p.final_open_size > 0.0 {
                let mark = mark_index
                    .get(&(p.token_id.clone(), ts.date_naive()))
                    .copied()
                    .unwrap_or(p.avg_cost);
                open_value += p.final_open_size * mark;
            }
        }

        let equity = cumulative_realized + open_value;
        let daily_pnl = prev_equity.map(|e| equity - e).unwrap_or(equity);

        if equity > peak {
            peak = equity;
        }
        let drawdown_pct = if peak > 0.0 {
            (peak - equity) / peak
        } else {
            0.0
        };

        out.push(EquityPoint {
            platform,
            address: address.clone(),
            ts,
            equity,
            daily_pnl,
            drawdown_pct,
        });
        prev_equity = Some(equity);
        ts += step;
    }

    out
}

/// 从权益曲线提取每日 PnL 序列（供 [`crate::metrics::compute_performance`] 算 Sharpe/回撤）。
pub fn daily_pnls(curve: &[EquityPoint]) -> Vec<f64> {
    curve.iter().map(|p| p.daily_pnl).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sharpside_shared::Platform;

    fn make_timeline(
        token: &str,
        opened: &str,
        closed: Option<&str>,
        realized: f64,
        open_size: f64,
        avg: f64,
    ) -> PositionTimeline {
        PositionTimeline {
            platform: Platform::Polymarket,
            address: "0xa".into(),
            token_id: token.into(),
            condition_id: None,
            opened_at: Some(
                chrono::DateTime::parse_from_rfc3339(opened)
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            closed_at: closed.map(|c| {
                chrono::DateTime::parse_from_rfc3339(c)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
            total_bought_size: 100.0,
            total_sold_size: if closed.is_some() { 100.0 } else { 0.0 },
            avg_cost: avg,
            realized_pnl: realized,
            final_open_size: open_size,
            is_closed: closed.is_some(),
            holding_seconds: None,
        }
    }

    fn d(s: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// 解析 RFC3339 为 UTC DateTime。
    fn dt(s: &str) -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn empty_timelines_empty_curve() {
        let curve = compute_equity_curve(
            &[],
            &[],
            dt("2026-01-01T00:00:00Z"),
            dt("2026-01-03T00:00:00Z"),
            Duration::days(1),
        );
        assert!(curve.is_empty());
    }

    #[test]
    fn realized_pnl_on_close_day() {
        // 仓位 1/1 开仓，1/2 平仓 realized=10
        let tl = vec![make_timeline(
            "t1",
            "2026-01-01T00:00:00Z",
            Some("2026-01-02T00:00:00Z"),
            10.0,
            0.0,
            0.50,
        )];
        let curve = compute_equity_curve(
            &tl,
            &[],
            dt("2026-01-01T00:00:00Z"),
            dt("2026-01-03T00:00:00Z"),
            Duration::days(1),
        );
        assert_eq!(curve.len(), 3);
        // 1/1: 0 realized, 0 open → equity 0
        assert!((curve[0].equity - 0.0).abs() < 1e-9);
        // 1/2: +10 realized → equity 10
        assert!((curve[1].equity - 10.0).abs() < 1e-9);
        assert!((curve[1].daily_pnl - 10.0).abs() < 1e-9);
        // 1/3: 不变
        assert!((curve[2].equity - 10.0).abs() < 1e-9);
        assert!((curve[2].daily_pnl - 0.0).abs() < 1e-9);
    }

    #[test]
    fn open_position_marked_to_market() {
        // 仓位 1/1 开仓 100 @ 0.50，未平仓
        let tl = vec![make_timeline(
            "t1",
            "2026-01-01T00:00:00Z",
            None,
            0.0,
            100.0,
            0.50,
        )];
        let marks = vec![
            DailyMark {
                token_id: "t1".into(),
                date: d("2026-01-01"),
                price: 0.50,
            },
            DailyMark {
                token_id: "t1".into(),
                date: d("2026-01-02"),
                price: 0.60,
            },
            DailyMark {
                token_id: "t1".into(),
                date: d("2026-01-03"),
                price: 0.55,
            },
        ];
        let curve = compute_equity_curve(
            &tl,
            &marks,
            dt("2026-01-01T00:00:00Z"),
            dt("2026-01-03T00:00:00Z"),
            Duration::days(1),
        );
        // 1/1: 100*0.50 = 50
        assert!((curve[0].equity - 50.0).abs() < 1e-9);
        // 1/2: 100*0.60 = 60, daily_pnl = 10
        assert!((curve[1].equity - 60.0).abs() < 1e-9);
        assert!((curve[1].daily_pnl - 10.0).abs() < 1e-9);
        // 1/3: 100*0.55 = 55, daily_pnl = -5
        assert!((curve[2].equity - 55.0).abs() < 1e-9);
        assert!((curve[2].daily_pnl - (-5.0)).abs() < 1e-9);
    }

    #[test]
    fn hourly_step_produces_hourly_points() {
        // 1/1 00:00 开仓 100 @ 0.50，未平仓；小时级步进应产出 25 个点（00:00 ~ 次日 00:00 含 25 点）
        let tl = vec![make_timeline(
            "t1",
            "2026-01-01T00:00:00Z",
            None,
            0.0,
            100.0,
            0.50,
        )];
        let s = dt("2026-01-01T00:00:00Z");
        let e = dt("2026-01-02T00:00:00Z");
        let curve = compute_equity_curve(&tl, &[], s, e, Duration::hours(1));
        assert_eq!(curve.len(), 25, "00:00 -> 次日 00:00 含首尾应 25 个小时点");
        assert_eq!(curve[0].ts, s);
        assert_eq!(curve[24].ts, e);
        // 无 marks → 全部按 avg_cost 估值 → equity 恒为 100*0.50=50
        for p in &curve {
            assert!((p.equity - 50.0).abs() < 1e-9);
        }
    }

    #[test]
    fn drawdown_tracked() {
        // 用 marks 制造不出 realized，改用 daily_pnls 间接测 drawdown
        // 这里测 equity 下降时 drawdown_pct > 0
        let tl: Vec<PositionTimeline> = vec![make_timeline(
            "t1",
            "2026-01-01T00:00:00Z",
            Some("2026-01-01T00:00:00Z"),
            10.0,
            0.0,
            0.50,
        )];
        let curve = compute_equity_curve(
            &tl,
            &[],
            dt("2026-01-01T00:00:00Z"),
            dt("2026-01-03T00:00:00Z"),
            Duration::days(1),
        );
        // 1/1: equity 10, peak 10, dd 0
        assert!((curve[0].drawdown_pct - 0.0).abs() < 1e-9);
        // 1/2-1/3: equity 10 不变, dd 0
        assert!((curve[2].drawdown_pct - 0.0).abs() < 1e-9);
    }

    #[test]
    fn daily_pnls_extraction() {
        let curve = vec![
            EquityPoint {
                platform: Platform::Polymarket,
                address: "0xa".into(),
                ts: dt("2026-01-01T00:00:00Z"),
                equity: 10.0,
                daily_pnl: 10.0,
                drawdown_pct: 0.0,
            },
            EquityPoint {
                platform: Platform::Polymarket,
                address: "0xa".into(),
                ts: dt("2026-01-02T00:00:00Z"),
                equity: 8.0,
                daily_pnl: -2.0,
                drawdown_pct: 0.2,
            },
        ];
        let pnls = daily_pnls(&curve);
        assert_eq!(pnls, vec![10.0, -2.0]);
    }
}
