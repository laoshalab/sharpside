//! 鉴权：JWT + daemon_api_key 双模式。对应 `docs/ARCHITECTURE.md` §6.5 与 `docs/FLOWS.md` §7。
//!
//! - JWT 模式：`Authorization: Bearer <jwt>`，由 account 服务签发，gateway 校验
//! - daemon_api_key 模式：`X-Daemon-Api-Key: <key>`，daemon 长轮询用，单独限流组

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims。对应 `docs/ARCHITECTURE.md` §6.5 鉴权。
///
/// `jti` 必填：与 account/copier 对齐，无 jti 的 token 在上游被拒（安全修复 1.2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// subject：用户 id（uuid 字符串）
    pub sub: String,
    /// expiration：Unix 时间戳（秒）
    pub exp: usize,
    /// JWT 唯一 ID：用于吊销（denylist）。gateway 查 `account.jwt_denylist`（只读 PG）。
    pub jti: String,
}

/// 已认证用户（JWT 模式）。作为 axum extractor 使用：`async fn handler(user: AuthUser)`。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

/// 签发 JWT。由 account 服务调用（gateway 也提供以便测试）。
pub fn issue_jwt(user_id: &str, secret: &str, ttl_seconds: i64) -> ApiResult<String> {
    let exp = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.into(),
        exp,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("jwt encode: {e}")))
}

/// 校验 JWT。
pub fn verify_jwt(token: &str, secret: &str) -> ApiResult<Claims> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|e| ApiError::Unauthorized(format!("invalid jwt: {e}")))
}

/// 安全修复 3.1：优先 HttpOnly cookie `sharpside_token`（浏览器路径），
/// 回退 `Authorization: Bearer`（程序化客户端 / 过渡兼容）。
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
    let header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::Unauthorized("missing authorization (cookie or Bearer)".into()))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::Unauthorized("expected Bearer scheme".into()))?;
    Ok(token.trim().to_string())
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts)?;
        let claims = verify_jwt(&token, &state.config.jwt_secret)?;
        // 与 account/follow/copier 同表：logout 后 jti 入 denylist，gateway 本地鉴权立即失效。
        if sharpside_db::queries::account::is_jwt_revoked(&state.db, &claims.jti)
            .await
            .map_err(|e| ApiError::Internal(format!("denylist check: {e}")))?
        {
            return Err(ApiError::Unauthorized("token revoked".into()));
        }
        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    #[test]
    fn jwt_roundtrip() {
        let token = issue_jwt("user-1", SECRET, 60).unwrap();
        let claims = verify_jwt(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn jwt_rejects_wrong_secret() {
        let token = issue_jwt("user-1", SECRET, 60).unwrap();
        assert!(verify_jwt(&token, "other").is_err());
    }
}
