//! 跨 Venue 市场映射的类型定义。
//!
//! 对应 `docs/VENUE_DESIGN.md` §6。DB 持久化由 `crates/db` 的 `queries/mappings` 负责，
//! 本 crate 只含纯逻辑：候选生成、单位换算、执行参数校验。

use serde::{Deserialize, Serialize};
use sharpside_shared::Platform;
use sharpside_venues_core::Market;
use thiserror::Error;

/// 启发式匹配产出的候选映射。对应 `docs/VENUE_DESIGN.md` §6.2。
///
/// `confidence` ≥ 阈值（默认 0.7）才入表 `market_mappings`（`manual_verified=false`），
/// 进 admin 审核队列。
///
/// 持有 `&Market` 引用（借用自输入切片），是临时计算产物，不序列化。
#[derive(Debug, Clone)]
pub struct CandidateMapping<'a> {
    pub from: &'a Market,
    pub to: &'a Market,
    /// 0.0–1.0，由 [`crate::similarity::similarity`] 计算
    pub confidence: f64,
}

/// 已解析的映射，跟单翻译用。对应 `docs/VENUE_DESIGN.md` §6.3 `resolve_mapping` 的返回。
///
/// 由 `crates/db::queries::mappings::resolve_mapping` 从 `market_mappings` 表读出
/// （`manual_verified=true AND resolution_verified=true AND status='active'`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub from_platform: Platform,
    pub from_market_id: String,
    pub to_platform: Platform,
    pub to_market_id: String,
    pub confidence: f64,
    /// YES↔NO 翻转：true 表示 to 侧 outcome 与 from 侧相反，跟单时须翻转 side
    pub direction_flip: bool,
    /// 该映射建议的最小成交额，低于此跳过
    pub min_notional: Option<f64>,
}

/// 执行参数。对应 `docs/VENUE_DESIGN.md` §6.4。
///
/// 按 Venue 差异化套用：费率、最小 notional、滑点保护、最小股数。
/// `min_notional` 来自 `market_mappings.min_notional` 或 Venue 默认。
/// `min_size` 来自 Polymarket CLOB `/markets/{condition_id}` 的 `minimum_order_size`（每市场不同，
/// 服务端强制；下单前校验避免撞 400），可被 `market_mappings.min_size` 覆盖。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecParams {
    /// taker 费率（bps）。Kalshi 峰值 175bps；Polymarket 75–180bps 按类目
    pub taker_fee_bps: f64,
    /// 最小成交额（USDC 等价）
    pub min_notional: f64,
    /// 最大滑点（bps），下单前 book() 比对中间价，超限拒单
    pub max_slippage_bps: f64,
    /// 最小下单股数（0 = 未知/不限；>0 时 size < 此值拒单）。来自 Venue 服务端元数据。
    pub min_size: f64,
}

impl ExecParams {
    /// Polymarket 默认执行参数（按类目 75–180bps，取中位 120bps）。
    pub fn polymarket_default() -> Self {
        Self {
            taker_fee_bps: 120.0,
            min_notional: 1.0,
            max_slippage_bps: 200.0, // 2%
            min_size: 0.0,           // 由 CLOB /markets 元数据填充
        }
    }

    /// Kalshi 默认执行参数（峰值 175bps）。
    pub fn kalshi_default() -> Self {
        Self {
            taker_fee_bps: 175.0,
            min_notional: 1.0,
            max_slippage_bps: 200.0,
            min_size: 0.0,
        }
    }
}

/// 执行参数校验错误。对应 `docs/VENUE_DESIGN.md` §6.4 + §9 错误处理。
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq)]
pub enum ExecError {
    #[error("slippage {actual_bps:.0}bps exceeds max {max_bps:.0}bps")]
    SlippageExceeded { actual_bps: f64, max_bps: f64 },
    #[error("notional {notional:.2} below min {min_notional:.2}")]
    BelowMinNotional { notional: f64, min_notional: f64 },
    #[error("order book empty, cannot compute mid price")]
    EmptyBook,
}
