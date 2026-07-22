//! 应用共享状态。对应 `docs/ARCHITECTURE.md` §6.1。
//!
//! `AppState` 持有 db 连接池、Venue 注册表与 HTTP 客户端，通过 `Router::with_state` 注入。

use crate::config::Config;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sharpside_venues_core::VenueRegistry;
use sqlx::PgPool;
use std::convert::Infallible;
use std::sync::Arc;

/// VenueHub 共享状态。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub registry: Arc<VenueRegistry>,
    /// 预留：跨服务回源 / 外部抓取用（当前 Venue adapter 自带 client）。
    #[allow(dead_code)]
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, registry: VenueRegistry) -> Self {
        Self {
            config: Arc::new(config),
            db,
            registry: Arc::new(registry),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
        }
    }
}

/// 让 handler 可直接以 `state: AppState` 作为 extractor（免去每次写 `State<AppState>`）。
#[async_trait]
impl FromRequestParts<AppState> for AppState {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.clone())
    }
}
