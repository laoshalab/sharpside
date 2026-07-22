//! 应用共享状态。

use crate::config::Config;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sharpside_venues_core::VenueRegistry;
use sqlx::PgPool;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub registry: Arc<VenueRegistry>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, registry: VenueRegistry) -> Self {
        Self {
            config: Arc::new(config),
            db,
            registry: Arc::new(registry),
        }
    }
}

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
