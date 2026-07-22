//! 绩效计算内部类型。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` §3（position_timeline）与 §4（指标公式）。
//! 这些类型是 `crates/perf` 的计算中间产物，落库时由 venue-hub 服务映射到
//! `trader_hub.position_timeline` / `trader_hub.trader_equity_curve` 表。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sharpside_shared::{Platform, Side};

/// 绩效计算的成交输入。对应 `trader_hub.raw_trades` 一行的计算视图。
///
/// perf crate 不依赖 `crates/venues/core`（避免依赖方向倒置），
/// 故在此定义本地输入类型；venue-hub 服务从 `raw_trades` 读取后映射到此。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeInput {
    pub platform: Platform,
    pub address: String,
    pub token_id: String,
    pub condition_id: Option<String>,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub ts: DateTime<Utc>,
}

/// 重建后的单仓位时间线。对应 `trader_hub.position_timeline` 一行。
///
/// 由 [`crate::timeline::reconstruct_position_timeline`] 从 `raw_trades` 回放得到。
/// 一个 `(platform, address, token_id)` 对应一条。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PositionTimeline {
    pub platform: Platform,
    pub address: String,
    pub token_id: String,
    pub condition_id: Option<String>,
    /// 首笔买入时间
    pub opened_at: Option<DateTime<Utc>>,
    /// 平仓/结算时间
    pub closed_at: Option<DateTime<Utc>>,
    pub total_bought_size: f64,
    pub total_sold_size: f64,
    /// 加权平均成本
    pub avg_cost: f64,
    /// 已实现 PnL（SELL 时按 (sell_price - avg_cost) * sell_size 累加；结算时按 outcome 算）
    pub realized_pnl: f64,
    /// 当前剩余持仓
    pub final_open_size: f64,
    pub is_closed: bool,
    /// 持有时长（秒），median 用于 DW:diamond
    pub holding_seconds: Option<i64>,
}

impl PositionTimeline {
    /// 浮动 PnL = final_open_size * (current_price - avg_cost)。对应 §4.1。
    pub fn unrealized_pnl(&self, current_price: f64) -> f64 {
        self.final_open_size * (current_price - self.avg_cost)
    }

    /// 是否为盈利仓位（realized_pnl > 0）。
    pub fn is_win(&self) -> bool {
        self.realized_pnl > 0.0
    }

    /// 是否为亏损仓位（realized_pnl < 0）。
    pub fn is_loss(&self) -> bool {
        self.realized_pnl < 0.0
    }
}

/// 权益曲线单点。对应 `trader_hub.trader_equity_curve` 一行。
///
/// `ts` 为时间戳（小时级粒度），`daily_pnl` 为相对前一点的权益增量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EquityPoint {
    pub platform: Platform,
    pub address: String,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub equity: f64,
    pub daily_pnl: f64,
    pub drawdown_pct: f64,
}

/// 成交手法统计，用于 type-3 标签。对应 `docs/PERFORMANCE_PIPELINE.md` §4.4。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FillStats {
    /// 限价单数
    pub limit_orders: u32,
    /// 市价单数
    pub market_orders: u32,
    /// 限价单平均 fill 时长（秒，< 2 block ≈ < 24s 视为快速成交）
    pub avg_limit_fill_seconds: f64,
    /// 同 token_id 单日反向交易次数的最大值
    pub max_daily_reversals: u32,
}

impl FillStats {
    /// 限价单占比。
    pub fn limit_ratio(&self) -> f64 {
        let total = self.limit_orders + self.market_orders;
        if total == 0 {
            0.0
        } else {
            self.limit_orders as f64 / total as f64
        }
    }

    /// 市价单占比。
    pub fn market_ratio(&self) -> f64 {
        let total = self.limit_orders + self.market_orders;
        if total == 0 {
            0.0
        } else {
            self.market_orders as f64 / total as f64
        }
    }
}

/// 标签阈值。对应 `trader_hub.tag_rules` 表，由 venue-hub 服务读取后传入。
///
/// **不硬编码**：所有阈值都是参数，运营后台改 `tag_rules` 后下次重算生效。
/// 默认值对应 `docs/PERFORMANCE_PIPELINE.md` §4.4 的示例阈值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagThresholds {
    /// DW:diamond：median(holding_seconds) > 此值（默认 86400 = 24h）
    pub dw_diamond_min_holding_seconds: i64,
    /// DW:win：win_rate > 此值（默认 0.60）
    pub dw_win_min_win_rate: f64,
    /// DW:win：roi > 此值（默认 0.0）
    pub dw_win_min_roi: f64,
    /// type-3:limit_sniper：限价单占比 > 此值（默认 0.70）
    pub limit_sniper_min_limit_ratio: f64,
    /// type-3:limit_sniper：avg fill 时长 < 此值秒（默认 24 = 2 block）
    pub limit_sniper_max_fill_seconds: f64,
    /// type-3:market_follow：市价单占比 > 此值（默认 0.70）
    pub market_follow_min_market_ratio: f64,
    /// type-3:rebalance：同 token 单日反向交易次数 > 此值（默认 3）
    pub rebalance_min_daily_reversals: u32,
}

impl Default for TagThresholds {
    fn default() -> Self {
        Self {
            dw_diamond_min_holding_seconds: 86_400, // 24h
            dw_win_min_win_rate: 0.60,
            dw_win_min_roi: 0.0,
            limit_sniper_min_limit_ratio: 0.70,
            limit_sniper_max_fill_seconds: 24.0, // ~2 block
            market_follow_min_market_ratio: 0.70,
            rebalance_min_daily_reversals: 3,
        }
    }
}

/// 每日 mark 价格输入，用于权益曲线计算。`(token_id, date) -> mark_price`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMark {
    pub token_id: String,
    pub date: chrono::NaiveDate,
    pub price: f64,
}

/// 结算结果。市场到期时按 outcome(0/1) 计算 realized_pnl。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub token_id: String,
    /// YES token 结算价：1.0 = YES 胜，0.0 = YES 败
    pub outcome: f64,
    pub settled_at: DateTime<Utc>,
}
