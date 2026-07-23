//! Sharpside 端到端共享类型。
//!
//! 对应 `docs/TECH_STACK_RUST.md` §8：serde 端到端 schema 共享 `crates/shared`。
//! 所有跨服务通信的类型在此 crate 定义，services / apps / adapters 共用。
//!
//! 依赖方向：`shared` 是最底层 crate，不依赖任何内部 crate。
//! `crates/venues/core` 依赖 `shared` 并 re-export 基础枚举（`Platform`/`Side`）。
//!
//! 模块：
//! - [`platform`] — `Platform` 枚举（Venue 一等公民标识）
//! - [`order`] — `CopyOrder` / `Side` / `Channel`（跟单指令）
//! - [`event`] — `TradeEvent`（`trader.position.changed` 信号）
//! - [`perf`] — `Performance` / `PerformancePeriod`（绩效物化）
//! - [`tag`] — `Tag` / `TagKind`（DW / type-3 标签）
//! - [`follow`] — `FollowConfig` / `SizingMode`（跟随配置）
//! - [`watchlist`] — `WatchlistCreate` / `WatchlistUpgrade` / 配额（观察名单）
//! - [`jurisdiction`] — `allowed_execute_venues`（管辖域→可执行 Venue 映射）

#![forbid(unsafe_code)]

pub mod client_ip;
pub mod event;
pub mod follow;
pub mod jurisdiction;
pub mod order;
pub mod perf;
pub mod platform;
pub mod secrets;
pub mod session;
pub mod signal;
pub mod tag;
pub mod watchlist;

pub use event::TradeEvent;
pub use follow::{FollowConfig, SizingMode};
pub use jurisdiction::{allowed_execute_venues, is_allowed_venue, is_implemented_venue};
pub use order::{Channel, CopyOrder, CopyOrderStatus, Side};
pub use perf::{Performance, PerformancePeriod};
pub use platform::{normalize_trader_id, Platform, UnknownPlatform};
pub use signal::signal_id;
pub use tag::{Tag, TagKind};
pub use watchlist::{
    watchlist_limit, WatchlistCreate, WatchlistUpgrade, WATCHLIST_LIMIT_FREE,
    WATCHLIST_LIMIT_PRO_PLUS,
};

/// 协议语义化版本，daemon 启动时检查兼容性（对应 `TECH_STACK_RUST.md` §14）。
pub const PROTOCOL_VERSION: &str = "0.1.0";
