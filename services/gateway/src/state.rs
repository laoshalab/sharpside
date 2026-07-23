//! 应用共享状态。对应 `docs/ARCHITECTURE.md` §6.5。

use crate::config::Config;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sqlx::PgPool;
use std::convert::Infallible;
use std::sync::Arc;

/// Gateway 共享状态，通过 `Router::with_state` 注入。
///
/// 用 `Arc` 包裹以便在多个 handler / middleware 间廉价克隆。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    /// JWT denylist（与 account 同表）；本服务只读、不跑 migrate。
    pub db: PgPool,
}

impl AppState {
    pub fn new(config: Config, db: PgPool) -> Self {
        Self {
            config: Arc::new(config),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
            db,
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
