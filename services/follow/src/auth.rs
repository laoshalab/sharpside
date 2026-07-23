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
use sharpside_db::queries::account as acct;
use uuid::Uuid;

/// `jti` 必填：无 jti 的旧 token 解码失败 → 强制重新登录，保证可吊销（安全修复 1.2）。
#[derive(Debug, serde::Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    jti: String,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

/// 校验签名 + exp，返回 Claims（含 jti）。不查 denylist（由 extractor 查）。
fn verify_jwt(token: &str, secret: &str) -> Result<Claims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::Unauthorized("invalid or expired token".into()))?;
    Ok(data.claims)
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

/// 安全修复 3.1：优先 HttpOnly cookie，回退 Bearer。
fn extract_token(parts: &Parts) -> Result<String, ApiError> {
    if let Some(cookie) = parts
        .headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
    {
        if let Some(t) = sharpside_shared::session::extract_token_from_cookie_header(cookie) {
            return Ok(t);
        }
    }
    extract_bearer(parts)
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts).map_err(|e| e.into_response())?;
        let claims = verify_jwt(&token, &state.config.jwt_secret).map_err(|e| e.into_response())?;
        let user_id = claims
            .sub
            .parse::<Uuid>()
            .map_err(|_| ApiError::Unauthorized("invalid subject".into()).into_response())?;
        // 吊销检查（denylist）：与 account/copier 同表同机制。
        if acct::is_jwt_revoked(&state.db, &claims.jti)
            .await
            .map_err(|e| ApiError::Db(e).into_response())?
        {
            return Err(ApiError::Unauthorized("token revoked".into()).into_response());
        }
        Ok(AuthUser { user_id })
    }
}
