//! 鉴权：导入端点接受 Admin token **或** 用户 JWT（cookie / Bearer）。
//!
//! 用户侧 ImportBox 走 HttpOnly cookie；运维/curl 仍可用 `VENUE_HUB_ADMIN_TOKEN`。
//! Claims 与 account/follow 共用同一 `JWT_SECRET`（HS256）。

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

#[derive(Debug, serde::Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    jti: String,
}

/// 谁发起了导入（审计用）。
#[derive(Debug, Clone)]
pub enum ImportCaller {
    Admin,
    User { user_id: Uuid },
}

impl ImportCaller {
    /// 写审计日志用的调用方标签。
    pub fn audit_label(&self) -> String {
        match self {
            ImportCaller::Admin => "admin".into(),
            ImportCaller::User { user_id } => format!("user:{user_id}"),
        }
    }
}

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

fn extract_bearer(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn extract_cookie_token(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(sharpside_shared::session::extract_token_from_cookie_header)
}

fn try_admin(parts: &Parts, state: &AppState) -> bool {
    let Some(token) = extract_bearer(parts) else {
        return false;
    };
    sharpside_shared::secrets::constant_time_eq(
        token.as_bytes(),
        state.config.admin_token.as_bytes(),
    )
}

async fn try_user(parts: &Parts, state: &AppState) -> Result<Uuid, ApiError> {
    let token = extract_cookie_token(parts)
        .or_else(|| extract_bearer(parts))
        .ok_or_else(|| ApiError::Unauthorized("missing credentials".into()))?;
    let claims = verify_jwt(&token, &state.config.jwt_secret)?;
    let user_id = claims
        .sub
        .parse::<Uuid>()
        .map_err(|_| ApiError::Unauthorized("invalid subject".into()))?;
    if acct::is_jwt_revoked(&state.db, &claims.jti).await? {
        return Err(ApiError::Unauthorized("token revoked".into()));
    }
    Ok(user_id)
}

#[async_trait]
impl FromRequestParts<AppState> for ImportCaller {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Admin Bearer 优先（运维 curl / 脚本）；否则用户 JWT（cookie 或 Bearer）。
        if try_admin(parts, state) {
            return Ok(ImportCaller::Admin);
        }
        match try_user(parts, state).await {
            Ok(user_id) => Ok(ImportCaller::User { user_id }),
            Err(e) => Err(e.into_response()),
        }
    }
}
