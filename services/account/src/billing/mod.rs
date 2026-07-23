//! Pro+ USDC（Polygon）订阅计费。
//!
//! - HTTP：`/me/billing/*` + `POST /internal/billing/confirm`
//! - Worker：过期 + submitted 支付的链上 RPC 确认（`confirm.rs`）

pub mod confirm;
pub mod routes;
pub mod worker;
