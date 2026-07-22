//! 应用共享状态 + admin token 鉴权 extractor。

use crate::config::Config;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use sqlx::PgPool;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
}

impl AppState {
    pub fn new(config: Config, db: PgPool) -> Self {
        Self {
            config: Arc::new(config),
            db,
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

/// admin 鉴权：`Authorization: Bearer <admin_token>`。
#[derive(Debug, Clone)]
pub struct AdminAuth {
    #[allow(dead_code)]
    pub token: String,
}

#[async_trait]
impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                crate::error::ApiError::Unauthorized("missing Authorization".into()).into_response()
            })?;
        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            crate::error::ApiError::Unauthorized("expected Bearer".into()).into_response()
        })?;
        if token.trim() != state.config.admin_token {
            return Err(
                crate::error::ApiError::Unauthorized("invalid admin token".into()).into_response(),
            );
        }
        Ok(AdminAuth {
            token: token.trim().to_string(),
        })
    }
}
