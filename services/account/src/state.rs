//! 应用共享状态。

use crate::config::Config;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sqlx::PgPool;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    /// KMS：加密 owner EOA 私钥 / L2 secret 落库（provision 用），copier 侧注入同一 KMS 解密。
    pub kms: Arc<dyn sharpside_kms::Kms>,
    /// /auth/* 限流器（按 IP 分桶）。对应安全审计 H3。
    pub auth_limiter: Arc<crate::rate_limit::AuthLimiter>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, kms: Arc<dyn sharpside_kms::Kms>) -> Self {
        let auth_limiter = crate::rate_limit::make_auth_limiter(config.auth_rate_limit_per_min);
        Self {
            config: Arc::new(config),
            db,
            kms,
            auth_limiter,
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
