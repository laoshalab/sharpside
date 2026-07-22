//! 跨 Venue 市场映射。对应 `docs/VENUE_DESIGN.md` §6 与 `docs/ARCHITECTURE.md` §8。
//!
//! 纯逻辑 crate，无 IO：
//! - [`similarity`] — 启发式匹配（标题/标签/结算日期相似度），产候选映射
//! - [`unit`] — 价格/数量单位换算（USDC CTF ↔ USD cents ↔ ...）
//! - [`exec`] — 执行参数校验（滑点保护、最小 notional）
//!
//! DB 持久化与查询（`resolve_mapping` 等）由 `crates/db::queries::mappings` 负责，
//! 本 crate 不直接访问数据库。
//!
//! 设计要点：
//! - **纯函数**：所有计算无副作用、无 IO，便于单测与重算
//! - **阈值参数化**：[`similarity::candidate_mappings`] 接收 `threshold`，由 venue-hub 从配置读取
//! - **不依赖具体 Venue adapter**：只依赖 `sharpside-venues-core` 的通用类型（`Market`/`Unit`/`Order`/`OrderBook`）

#![forbid(unsafe_code)]

pub mod exec;
pub mod similarity;
pub mod types;
pub mod unit;

pub use exec::apply_exec_params;
pub use similarity::{
    candidate_mappings, end_date_closeness, similarity, tag_overlap, token_jaccard,
    DEFAULT_AUTO_MATCH_THRESHOLD,
};
pub use types::{CandidateMapping, ExecError, ExecParams, Mapping};
pub use unit::{convert_price, convert_size};
