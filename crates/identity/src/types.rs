//! 跨 Venue 身份链接的类型定义。
//!
//! 对应 `docs/VENUE_DESIGN.md` §7。DB 持久化（`identities` 表 + `traders.identity_id`）
//! 由 `crates/db` 负责，本 crate 只含纯启发式逻辑。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 默认身份链接阈值。对应 `docs/VENUE_DESIGN.md` §7.2 `if score >= 0.6`。
pub const DEFAULT_IDENTITY_THRESHOLD: f64 = 0.6;

/// 启发式链接产出的候选对。对应 `docs/VENUE_DESIGN.md` §7.2 `CandidateLink`。
///
/// 持有 `&Trader` 引用（借用自输入切片），是临时计算产物，不序列化。
/// `confidence` ≥ 阈值（默认 0.6）才进 admin 审核队列。
#[derive(Debug, Clone)]
pub struct CandidateLink<'a> {
    pub a: &'a sharpside_venues_core::Trader,
    pub b: &'a sharpside_venues_core::Trader,
    /// 0.0–1.0，由 [`crate::similarity::identity_similarity`] 计算
    pub confidence: f64,
}

/// 身份链接错误。
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq)]
pub enum IdentityError {
    #[error("no verified identity link for {platform_a}:{id_a} ↔ {platform_b}:{id_b}")]
    NoVerifiedLink {
        platform_a: sharpside_shared::Platform,
        id_a: String,
        platform_b: sharpside_shared::Platform,
        id_b: String,
    },
}
