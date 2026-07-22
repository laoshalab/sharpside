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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// subject：用户 id（uuid 字符串）
    pub sub: String,
    /// expiration：Unix 时间戳（秒）
    pub exp: usize,
}

/// 已认证用户（JWT 模式）。作为 axum extractor 使用：`async fn handler(user: AuthUser)`。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

/// daemon 身份（daemon_api_key 模式）。作为 axum extractor 使用。
#[derive(Debug, Clone)]
pub struct DaemonAuth {
    /// 预留：后续审计/日志按 key 归因
    #[allow(dead_code)]
    pub api_key: String,
}

/// 签发 JWT。由 account 服务调用（gateway 也提供以便测试）。
pub fn issue_jwt(user_id: &str, secret: &str, ttl_seconds: i64) -> ApiResult<String> {
    let exp = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.into(),
        exp,
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

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing Authorization header".into()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("expected Bearer scheme".into()))?;

        let claims = verify_jwt(token, &state.config.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}

#[async_trait]
impl FromRequestParts<AppState> for DaemonAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get("X-Daemon-Api-Key")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing X-Daemon-Api-Key header".into()))?;

        if key != state.config.daemon_api_key {
            return Err(ApiError::Forbidden("invalid daemon api key".into()));
        }
        Ok(DaemonAuth {
            api_key: key.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    #[test]
    fn jwt_roundtrip() {
        let token = issue_jwt("user-123", SECRET, 60).unwrap();
        let claims = verify_jwt(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "user-123");
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let token = issue_jwt("user-123", SECRET, 60).unwrap();
        let err = verify_jwt(&token, "other-secret").unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn jwt_expired_rejected() {
        // exp 设为 120 秒前（超过 jsonwebtoken 默认 60s leeway）
        let exp = (Utc::now() - Duration::seconds(120)).timestamp() as usize;
        let claims = Claims {
            sub: "u".into(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        let err = verify_jwt(&token, SECRET).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }
}
