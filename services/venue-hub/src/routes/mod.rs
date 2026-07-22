//! 路由聚合。对应 `docs/ARCHITECTURE.md` §6.1 对外 API。
//!
//! 对外端点：
//! - `GET /healthz` / `GET /readyz`
//! - `GET /venues`
//! - `GET /traders?platform=&limit=&offset=`
//! - `GET /traders/{platform}/{address}`
//! - `POST /traders/import` 导入地址触发回填
//! - `POST /traders/import/batch` 批量导入地址（逐条回填）
//! - `GET /identities/{id}`
//! - `GET /markets?platform=&q=`
//! - `GET /market-mappings?from_platform=&from_market_id=&to_platform=`

use crate::state::AppState;
use axum::Router;

mod health;
pub mod identities;
pub mod markets;
pub mod traders;
pub mod venues;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(health::healthz))
        .route("/readyz", axum::routing::get(health::readyz))
        .route("/venues", axum::routing::get(venues::list_venues))
        .route("/traders", axum::routing::get(traders::list_traders))
        .route(
            "/traders/sparklines",
            axum::routing::get(traders::list_sparklines),
        )
        .route(
            "/traders/:platform/:address",
            axum::routing::get(traders::get_trader),
        )
        .route(
            "/traders/:platform/:address/performance",
            axum::routing::get(traders::get_performance),
        )
        .route(
            "/traders/:platform/:address/equity-curve",
            axum::routing::get(traders::get_equity_curve),
        )
        .route(
            "/traders/:platform/:address/positions",
            axum::routing::get(traders::get_positions),
        )
        .route(
            "/traders/:platform/:address/trades",
            axum::routing::get(traders::get_trades),
        )
        .route(
            "/traders/import",
            axum::routing::post(traders::import_trader),
        )
        .route(
            "/traders/import/batch",
            axum::routing::post(traders::import_traders_batch),
        )
        .route(
            "/identities",
            axum::routing::get(identities::list_identities),
        )
        .route(
            "/identities/:id",
            axum::routing::get(identities::get_identity),
        )
        .route("/markets", axum::routing::get(markets::list_markets))
        .route(
            "/market-mappings",
            axum::routing::get(markets::list_market_mappings),
        )
        .with_state(state)
}
