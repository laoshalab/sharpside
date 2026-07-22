//! 绩效物化类型：`Performance` / `PerformancePeriod`。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` §4 与 `docs/VENUEHUB_STORAGE.md` §6 `trader_performance` 表。
//! per `(platform, address, period)` 物化，覆盖写六行：`1d`/`1w`/`1m`/`1y`/`ytd`/`all`。

use serde::{Deserialize, Serialize};

/// 绩效周期。与 `trader_performance.period` 列取值一致。
///
/// 六档切片（对应前端周期 tab `1天/1周/1个月/1年/年初至今/全部`）：
/// - `1d` 近 1 天 · `1w` 近 7 天 · `1m` 近 30 天 · `1y` 近 365 天
/// - `ytd` 年初至今（当年 1 月 1 日起）· `all` 全历史
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PerformancePeriod {
    #[serde(rename = "1d")]
    OneDay,
    #[serde(rename = "1w")]
    OneWeek,
    #[serde(rename = "1m")]
    OneMonth,
    #[serde(rename = "1y")]
    OneYear,
    #[serde(rename = "ytd")]
    Ytd,
    #[serde(rename = "all")]
    All,
}

impl PerformancePeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneDay => "1d",
            Self::OneWeek => "1w",
            Self::OneMonth => "1m",
            Self::OneYear => "1y",
            Self::Ytd => "ytd",
            Self::All => "all",
        }
    }
}

impl std::str::FromStr for PerformancePeriod {
    type Err = UnknownPeriod;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1d" => Ok(Self::OneDay),
            "1w" => Ok(Self::OneWeek),
            "1m" => Ok(Self::OneMonth),
            "1y" => Ok(Self::OneYear),
            "ytd" => Ok(Self::Ytd),
            "all" => Ok(Self::All),
            other => Err(UnknownPeriod(other.into())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown period: {0}")]
pub struct UnknownPeriod(pub String);

/// 单周期绩效。字段口径与 `trader_performance` 表一一对应。
///
/// 计算公式见 `docs/PERFORMANCE_PIPELINE.md` §4：
/// - `roi = total_pnl / cost_basis`
/// - `win_rate = wins / (wins + losses)`
/// - `sharpe = mean(daily_pnl) / std(daily_pnl) * sqrt(365)`
/// - `max_drawdown = max over t of (peak - equity) / peak`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Performance {
    pub roi: f64,
    pub sharpe: f64,
    pub sortino: f64,
    pub win_rate: f64,
    pub max_drawdown: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub profit_factor: f64,
    pub wins: i64,
    pub losses: i64,
    pub position_count: i64,
    pub open_positions: i64,
    pub total_volume: f64,
    pub cost_basis: f64,
}

impl Performance {
    /// 全零占位，用于新导入交易者首次回填前的展示降级。
    pub fn zero() -> Self {
        Self {
            roi: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            win_rate: 0.0,
            max_drawdown: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            gross_profit: 0.0,
            gross_loss: 0.0,
            profit_factor: 0.0,
            wins: 0,
            losses: 0,
            position_count: 0,
            open_positions: 0,
            total_volume: 0.0,
            cost_basis: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_serde_round_trip() {
        for p in [
            PerformancePeriod::OneDay,
            PerformancePeriod::OneWeek,
            PerformancePeriod::OneMonth,
            PerformancePeriod::OneYear,
            PerformancePeriod::Ytd,
            PerformancePeriod::All,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let back: PerformancePeriod = serde_json::from_str(&json).unwrap();
            assert_eq!(p, back);
        }
    }

    #[test]
    fn performance_serde_round_trip() {
        let p = Performance {
            roi: 0.35,
            sharpe: 1.8,
            sortino: 2.1,
            win_rate: 0.65,
            max_drawdown: 0.12,
            realized_pnl: 1200.0,
            unrealized_pnl: 300.0,
            gross_profit: 2000.0,
            gross_loss: -800.0,
            profit_factor: 2.5,
            wins: 13,
            losses: 7,
            position_count: 20,
            open_positions: 3,
            total_volume: 50000.0,
            cost_basis: 4000.0,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Performance = serde_json::from_str(&json).unwrap();
        assert_eq!(p.roi, back.roi);
        assert_eq!(p.wins, back.wins);
    }
}
