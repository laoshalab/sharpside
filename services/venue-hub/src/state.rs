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

/// 运维/admin 鉴权：`Authorization: Bearer <admin_token>`。保护写端点（如 `/traders/import*`）。
///
/// 常时比较，避免按字节短路带来的时序侧信道。
#[derive(Debug, Clone)]
pub struct AdminAuth {
    #[allow(dead_code)]
    pub token: String,
}

#[async_trait]
impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = crate::error::ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                crate::error::ApiError::Unauthorized("missing Authorization header".into())
            })?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| crate::error::ApiError::Unauthorized("expected Bearer scheme".into()))?;
        if !sharpside_shared::secrets::constant_time_eq(
            token.trim().as_bytes(),
            state.config.admin_token.as_bytes(),
        ) {
            return Err(crate::error::ApiError::Forbidden("invalid admin token".into()));
        }
        Ok(AdminAuth {
            token: token.trim().to_string(),
        })
    }
}
