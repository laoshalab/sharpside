//! 绩效指标计算。对应 `docs/PERFORMANCE_PIPELINE.md` §4。
//!
//! 公式：
//! - `unrealized_pnl = final_open_size * (current_price - avg_cost)`
//! - `total_pnl = sum(realized_pnl) + unrealized_pnl`
//! - `cost_basis = sum(buy_size * buy_price)`
//! - `roi = total_pnl / cost_basis`
//! - `win_rate = wins / (wins + losses)`
//! - `gross_profit = sum(realized_pnl where > 0)`，`gross_loss = sum(realized_pnl where < 0)`（负数）
//! - `profit_factor = gross_profit / abs(gross_loss)`
//! - `sharpe = mean(daily_pnl) / std(daily_pnl) * sqrt(365)`（年化）
//! - `sortino = mean(daily_pnl) / std(daily_pnl where < 0) * sqrt(365)`
//! - `max_drawdown = max over t of (peak_before[t] - equity[t]) / peak_before[t]`

use crate::types::PositionTimeline;
use sharpside_shared::Performance;

/// 年化因子平方根（sqrt(365)）。
const SQRT_365: f64 = 19.1049731745428;

/// 计算单周期绩效。
///
/// `current_prices`：`token_id -> current_price`，用于算 unrealized_pnl。
/// `daily_pnls`：每日 PnL 序列（由 [`crate::equity::compute_equity_curve`] 产出），
///              用于算 Sharpe / Sortino / max_drawdown。
/// `period`：`1d` / `1w` / `1m` / `1y` / `ytd` / `all`，仅用于标注，不影响计算。
pub fn compute_performance(
    timelines: &[PositionTimeline],
    current_prices: &std::collections::HashMap<String, f64>,
    daily_pnls: &[f64],
    _period: sharpside_shared::PerformancePeriod,
) -> Performance {
    let mut realized_pnl = 0.0;
    let mut unrealized_pnl = 0.0;
    let mut cost_basis = 0.0;
    let mut total_volume = 0.0;
    let mut gross_profit = 0.0;
    let mut gross_loss = 0.0;
    let mut wins = 0i64;
    let mut losses = 0i64;
    let mut position_count = 0i64;
    let mut open_positions = 0i64;

    for p in timelines {
        position_count += 1;
        realized_pnl += p.realized_pnl;
        cost_basis += p.total_bought_size * p.avg_cost;
        total_volume += p.total_bought_size * p.avg_cost + p.total_sold_size * p.avg_cost;

        if p.realized_pnl > 0.0 {
            wins += 1;
            gross_profit += p.realized_pnl;
        } else if p.realized_pnl < 0.0 {
            losses += 1;
            gross_loss += p.realized_pnl; // 负数
        }

        if !p.is_closed {
            open_positions += 1;
            let price = current_prices
                .get(&p.token_id)
                .copied()
                .unwrap_or(p.avg_cost);
            unrealized_pnl += p.unrealized_pnl(price);
        }
    }

    let total_pnl = realized_pnl + unrealized_pnl;
    let roi = if cost_basis > 0.0 {
        total_pnl / cost_basis
    } else {
        0.0
    };
    let win_rate = if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    };
    let profit_factor = if gross_loss.abs() > 0.0 {
        gross_profit / gross_loss.abs()
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let (sharpe, sortino, max_drawdown) = compute_risk_metrics(daily_pnls);

    Performance {
        roi,
        sharpe,
        sortino,
        win_rate,
        max_drawdown,
        realized_pnl,
        unrealized_pnl,
        gross_profit,
        gross_loss,
        profit_factor,
        wins,
        losses,
        position_count,
        open_positions,
        total_volume,
        cost_basis,
    }
}

/// 从每日 PnL 序列算 Sharpe / Sortino / max_drawdown。
fn compute_risk_metrics(daily_pnls: &[f64]) -> (f64, f64, f64) {
    if daily_pnls.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let mean_pnl = mean(daily_pnls);
    let std_dev = std(daily_pnls, mean_pnl);
    let sharpe = if std_dev > 0.0 {
        mean_pnl / std_dev * SQRT_365
    } else {
        0.0
    };

    let downside: Vec<f64> = daily_pnls.iter().copied().filter(|&x| x < 0.0).collect();
    let sortino = if !downside.is_empty() {
        let dstd = std(&downside, mean(&downside));
        if dstd > 0.0 {
            mean_pnl / dstd * SQRT_365
        } else {
            0.0
        }
    } else {
        0.0
    };

    // max_drawdown：从累计权益曲线算
    let mut equity = 0.0;
    let mut peak = 0.0;
    let mut max_dd = 0.0;
    for &pnl in daily_pnls {
        equity += pnl;
        if equity > peak {
            peak = equity;
        }
        if peak > 0.0 {
            let dd = (peak - equity) / peak;
            if dd > max_dd {
                max_dd = dd;
            }
        }
    }

    (sharpe, sortino, max_dd)
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// 总体标准差（非样本标准差）。
fn std(xs: &[f64], mean: f64) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / xs.len() as f64;
    var.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TradeInput;
    use chrono::TimeZone;
    use sharpside_shared::{PerformancePeriod, Platform, Side};

    fn make_timeline(
        realized: f64,
        bought: f64,
        avg: f64,
        open: f64,
        closed: bool,
        token: &str,
    ) -> PositionTimeline {
        PositionTimeline {
            platform: Platform::Polymarket,
            address: "0xa".into(),
            token_id: token.into(),
            condition_id: None,
            opened_at: None,
            closed_at: None,
            total_bought_size: bought,
            total_sold_size: if closed { bought } else { 0.0 },
            avg_cost: avg,
            realized_pnl: realized,
            final_open_size: open,
            is_closed: closed,
            holding_seconds: None,
        }
    }

    #[test]
    fn roi_basic() {
        // cost_basis = 100*0.50 = 50, realized = 10, unrealized = 0 → roi = 10/50 = 0.2
        let tl = vec![make_timeline(10.0, 100.0, 0.50, 0.0, true, "t1")];
        let prices = std::collections::HashMap::new();
        let perf = compute_performance(&tl, &prices, &[], PerformancePeriod::All);
        assert!((perf.roi - 0.2).abs() < 1e-9);
        assert!((perf.cost_basis - 50.0).abs() < 1e-9);
        assert_eq!(perf.wins, 1);
        assert_eq!(perf.losses, 0);
        assert!((perf.win_rate - 1.0).abs() < 1e-9);
        assert!((perf.gross_profit - 10.0).abs() < 1e-9);
        assert!((perf.gross_loss - 0.0).abs() < 1e-9);
    }

    #[test]
    fn win_rate_mixed() {
        let tl = vec![
            make_timeline(10.0, 100.0, 0.50, 0.0, true, "t1"),
            make_timeline(-5.0, 100.0, 0.50, 0.0, true, "t2"),
            make_timeline(3.0, 100.0, 0.50, 0.0, true, "t3"),
        ];
        let prices = std::collections::HashMap::new();
        let perf = compute_performance(&tl, &prices, &[], PerformancePeriod::All);
        assert_eq!(perf.wins, 2);
        assert_eq!(perf.losses, 1);
        assert!((perf.win_rate - (2.0 / 3.0)).abs() < 1e-9);
        assert!((perf.gross_profit - 13.0).abs() < 1e-9);
        assert!((perf.gross_loss - (-5.0)).abs() < 1e-9);
        assert!((perf.profit_factor - (13.0 / 5.0)).abs() < 1e-9);
    }

    #[test]
    fn unrealized_pnl_for_open_position() {
        // open position: bought 100 @ 0.50, current 0.60 → unrealized = 100*(0.60-0.50)=10
        let tl = vec![make_timeline(0.0, 100.0, 0.50, 100.0, false, "t1")];
        let mut prices = std::collections::HashMap::new();
        prices.insert("t1".to_string(), 0.60);
        let perf = compute_performance(&tl, &prices, &[], PerformancePeriod::All);
        assert!((perf.unrealized_pnl - 10.0).abs() < 1e-9);
        assert_eq!(perf.open_positions, 1);
        assert_eq!(perf.wins, 0);
        assert_eq!(perf.losses, 0); // open position 不计入 win/loss
    }

    #[test]
    fn sharpe_from_daily_pnls() {
        // 稳定每日 +1 → std=0 → sharpe=0
        let tl = vec![make_timeline(10.0, 100.0, 0.50, 0.0, true, "t1")];
        let prices = std::collections::HashMap::new();
        let daily = vec![1.0, 1.0, 1.0, 1.0];
        let perf = compute_performance(&tl, &prices, &daily, PerformancePeriod::All);
        assert!((perf.sharpe - 0.0).abs() < 1e-9);
    }

    #[test]
    fn max_drawdown_from_daily_pnls() {
        let tl = vec![];
        let prices = std::collections::HashMap::new();
        // +10, -15, +5 → peak 10, trough -5, dd = (10-(-5))/10 = 1.5
        let daily = vec![10.0, -15.0, 5.0];
        let perf = compute_performance(&tl, &prices, &daily, PerformancePeriod::All);
        assert!((perf.max_drawdown - 1.5).abs() < 1e-9);
    }

    #[test]
    fn empty_timelines_yield_zero() {
        let tl: Vec<PositionTimeline> = vec![];
        let prices = std::collections::HashMap::new();
        let perf = compute_performance(&tl, &prices, &[], PerformancePeriod::All);
        let zero = Performance::zero();
        assert!((perf.roi - zero.roi).abs() < 1e-9);
        assert_eq!(perf.position_count, 0);
    }

    #[test]
    fn full_reconstruction_to_performance() {
        use crate::timeline::reconstruct_position_timeline;
        let trades = vec![
            TradeInput {
                platform: Platform::Polymarket,
                address: "0xa".into(),
                token_id: "t1".into(),
                condition_id: None,
                side: Side::Buy,
                price: 0.50,
                size: 100.0,
                ts: chrono::Utc.timestamp_opt(0, 0).unwrap(),
            },
            TradeInput {
                platform: Platform::Polymarket,
                address: "0xa".into(),
                token_id: "t1".into(),
                condition_id: None,
                side: Side::Sell,
                price: 0.60,
                size: 100.0,
                ts: chrono::Utc.timestamp_opt(86400, 0).unwrap(),
            },
        ];
        let timeline = reconstruct_position_timeline(&trades, &[]);
        let prices = std::collections::HashMap::new();
        let perf = compute_performance(&timeline, &prices, &[], PerformancePeriod::All);
        assert!((perf.realized_pnl - 10.0).abs() < 1e-9);
        assert!((perf.roi - 0.2).abs() < 1e-9);
        assert_eq!(perf.wins, 1);
    }
}
