//! 绩效计算：仓位重建 + 指标计算，纯函数易测。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` §3-§4。
//! 输入 `raw_trades` → 重建 `position_timeline` → 计算 `trader_performance` + `trader_equity_curve` + `trader_tag`。
//! 标签阈值从 `tag_rules` 表读，不硬编码。
//!
//! 设计要点：
//! - **纯函数**：所有计算无副作用、无 IO，便于单测与重算
//! - **不依赖 venues/core**：避免依赖方向倒置，输入类型本地定义
//! - **阈值参数化**：[`tags::compute_tags`] 接收 [`types::TagThresholds`]，由 venue-hub 从 `tag_rules` 表读取后传入
//!
//! 模块：
//! - [`timeline`] — 仓位重建（[`timeline::reconstruct_position_timeline`]）
//! - [`metrics`] — 绩效指标（[`metrics::compute_performance`]）
//! - [`equity`] — 权益曲线（[`equity::compute_equity_curve`]）
//! - [`tags`] — 运营标签（[`tags::compute_tags`]）
//! - [`types`] — 内部类型

#![forbid(unsafe_code)]

pub mod equity;
pub mod metrics;
pub mod tags;
pub mod timeline;
pub mod types;

pub use equity::{compute_equity_curve, daily_pnls};
pub use metrics::compute_performance;
pub use tags::compute_tags;
pub use timeline::reconstruct_position_timeline;
pub use types::{
    DailyMark, EquityPoint, FillStats, PositionTimeline, Settlement, TagThresholds, TradeInput,
};
