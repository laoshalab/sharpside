//! 跨 Venue 交易者身份链接。对应 `docs/VENUE_DESIGN.md` §7 与 `docs/ARCHITECTURE.md` §6.1。
//!
//! 纯逻辑 crate，无 IO：
//! - [`similarity::identity_similarity`] — 跨 Venue 身份相似度（x_username + alias）
//! - [`similarity::candidate_identities`] — 启发式产候选链接，进 admin 审核队列
//!
//! DB 持久化（`identities` 表 + `traders.identity_id`）由 `crates/db` 负责，
//! 本 crate 不直接访问数据库。
//!
//! 设计要点：
//! - **纯函数**：所有计算无副作用、无 IO，便于单测与重算
//! - **阈值参数化**：[`similarity::candidate_identities`] 接收 `threshold`，由 venue-hub 从配置读取
//! - **不依赖具体 Venue adapter**：只依赖 `sharpside-venues-core` 的通用 `Trader` 类型
//! - **持仓相似度预留扩展点**：由 VenueHub 离线计算后注入（见 `docs/VENUE_DESIGN.md` §7.2 注释）

#![forbid(unsafe_code)]

pub mod similarity;
pub mod types;

pub use similarity::{candidate_identities, identity_similarity};
pub use types::{CandidateLink, IdentityError, DEFAULT_IDENTITY_THRESHOLD};
