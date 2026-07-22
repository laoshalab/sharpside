//! daemon_api_key 鉴权（通道 B）。对应 `docs/FLOWS.md` §7。
//!
//! daemon 携带 `X-User-Id: <uuid>` + `X-Daemon-Api-Key: <明文 key>`；
//! copier 按 user_id 取 `account.users.daemon_api_key_hash` 校验。
//! account 现用 argon2 颁发新 key（PHC 字符串 `$argon2...`）；存量 PBKDF2 哈希
//! （`iterations$salt_hex$hash_hex`）仍须兼容校验。按前缀分发。

use crate::error::ApiError;
use crate::state::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use sha2::Sha256;
use sharpside_db::queries::account as acct;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// JWT claims（与 account/gateway 共用 HS256）。
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[allow(dead_code)]
    pub exp: usize,
}

/// 已认证用户（JWT 模式）。用户态端点 `/me/*` 用。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
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
                ApiError::Unauthorized("missing Authorization header".into()).into_response()
            })?;
        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError::Unauthorized("expected Bearer scheme".into()).into_response()
        })?;
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;
        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &validation,
        )
        .map_err(|e| ApiError::Unauthorized(format!("invalid jwt: {e}")).into_response())?;
        let user_id = Uuid::parse_str(&claims.claims.sub).map_err(|e| {
            ApiError::Unauthorized(format!("invalid user id in jwt: {e}")).into_response()
        })?;
        Ok(AuthUser { user_id })
    }
}

#[derive(Debug, Clone)]
pub struct DaemonAuth {
    pub user_id: Uuid,
}

#[async_trait]
impl FromRequestParts<AppState> for DaemonAuth {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user_id = parts
            .headers
            .get("X-User-Id")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                ApiError::Unauthorized("missing/invalid X-User-Id".into()).into_response()
            })?;

        let key = parts
            .headers
            .get("X-Daemon-Api-Key")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                ApiError::Unauthorized("missing X-Daemon-Api-Key".into()).into_response()
            })?;

        let user = acct::get_user(&state.db, user_id)
            .await
            .map_err(|e| ApiError::Db(e).into_response())?;
        match user.daemon_api_key_hash.as_deref() {
            Some(stored) if verify_password(key, stored) => Ok(DaemonAuth { user_id }),
            _ => Err(ApiError::Unauthorized("invalid daemon api key".into()).into_response()),
        }
    }
}

// ── PBKDF2-HMAC-SHA256（与 account/auth.rs 一致）──

fn pbkdf2_block(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut u: Vec<u8> = Vec::with_capacity(salt.len() + 4);
    u.extend_from_slice(salt);
    u.extend_from_slice(&1u32.to_be_bytes());

    let mut mac = HmacSha256::new_from_slice(password).expect("hmac key");
    mac.update(&u);
    let mut t = mac.finalize().into_bytes().to_vec();
    let mut out = t.clone();

    for _ in 1..iterations {
        let mut mac = HmacSha256::new_from_slice(password).expect("hmac key");
        mac.update(&t);
        t = mac.finalize().into_bytes().to_vec();
        for (o, ti) in out.iter_mut().zip(t.iter()) {
            *o ^= ti;
        }
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn verify_password(password: &str, stored: &str) -> bool {
    if stored.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(stored) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    } else {
        verify_password_pbkdf2(password, stored)
    }
}

/// 旧格式校验（`iterations$salt_hex$hash_hex`），常时比较。
fn verify_password_pbkdf2(password: &str, stored: &str) -> bool {
    let Some((iter_s, rest)) = stored.split_once('$') else {
        return false;
    };
    let Some((salt_hex, hash_hex)) = rest.split_once('$') else {
        return false;
    };
    let Ok(iterations) = iter_s.parse::<u32>() else {
        return false;
    };
    let Ok(salt) = unhex(salt_hex) else {
        return false;
    };
    let Ok(expected) = unhex(hash_hex) else {
        return false;
    };
    let dk = pbkdf2_block(password.as_bytes(), &salt, iterations);
    constant_time_eq(&dk, &expected)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unhex(s: &str) -> Result<Vec<u8>, ()> {
    if !s.len().is_multiple_of(2) {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::SaltString;
    use argon2::PasswordHasher;
    use rand::rngs::OsRng;

    fn hash_pbkdf2(password: &str, iterations: u32) -> String {
        let salt = uuid::Uuid::new_v4();
        let dk = pbkdf2_block(password.as_bytes(), salt.as_bytes(), iterations);
        format!("{iterations}${}${}", hex(salt.as_bytes()), hex(&dk))
    }

    fn hash_argon2(password: &str) -> String {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string()
    }

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    #[test]
    fn verify_legacy_pbkdf2() {
        let stored = hash_pbkdf2("secret-key", 1000);
        assert!(!stored.starts_with("$argon2"));
        assert!(verify_password("secret-key", &stored));
        assert!(!verify_password("other", &stored));
    }

    #[test]
    fn verify_argon2() {
        let stored = hash_argon2("secret-key");
        assert!(stored.starts_with("$argon2"));
        assert!(verify_password("secret-key", &stored));
        assert!(!verify_password("other", &stored));
    }

    #[test]
    fn malformed_stored_rejected() {
        assert!(!verify_password("x", "garbage"));
        assert!(!verify_password("x", "10$onlyone"));
    }
}
