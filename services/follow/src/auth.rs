//! 鉴权：JWT 校验 + `AuthUser` extractor。
//!
//! 与 account 服务共用同一 JWT secret（HS256）。Claims 结构本地复制（重构候选：抽 `crates/auth`）。

use crate::error::ApiError;
use crate::state::AppState;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, serde::Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

fn verify_jwt(token: &str, secret: &str) -> Result<Uuid, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::Unauthorized("invalid or expired token".into()))?;
    data.claims
        .sub
        .parse::<Uuid>()
        .map_err(|_| ApiError::Unauthorized("invalid subject".into()))
}

fn extract_bearer(parts: &Parts) -> Result<String, ApiError> {
    let header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::Unauthorized("missing authorization header".into()))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::Unauthorized("expected bearer token".into()))?;
    Ok(token.trim().to_string())
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_bearer(parts).map_err(|e| e.into_response())?;
        let user_id =
            verify_jwt(&token, &state.config.jwt_secret).map_err(|e| e.into_response())?;
        Ok(AuthUser { user_id })
    }
}
