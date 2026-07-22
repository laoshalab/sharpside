//! 鉴权：JWT 签发/校验 + 密码哈希（argon2 Argon2id）+ `AuthUser` extractor。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.4 / §6.5。
//!
//! 密码哈希：argon2（Argon2id，PHC 字符串）。校验时按前缀分发：`$argon2` 走 argon2，
//! 否则按旧 `iterations$salt_hex$hash_hex` 走 PBKDF2 兼容验证（存量 daemon_api_key 不失效）。

use crate::error::ApiError;
use crate::state::AppState;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// JWT claims。`sub` = user_id。
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

/// 鉴权后的用户身份（handler extractor）。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

/// 签发 JWT。
pub fn issue_jwt(user_id: Uuid, secret: &str, ttl_seconds: i64) -> Result<String, ApiError> {
    let exp = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        exp,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("jwt encode: {e}")))?;
    Ok(token)
}

/// 校验 JWT，返回 user_id。
pub fn verify_jwt(token: &str, secret: &str) -> Result<Uuid, ApiError> {
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

/// 从 `Authorization: Bearer <token>` 提取并校验。
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

// ── 密码哈希：argon2（Argon2id），兼容旧 PBKDF2 哈希 ──
//
// 新哈希用 argon2 产出 PHC 字符串（`$argon2...`）。校验时按前缀分发：
// `$argon2` → argon2 验证；否则按旧 `iterations$salt_hex$hash_hex` 走 PBKDF2 兼容验证，
// 保证存量用户/daemon_api_key 不失效。

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::rngs::OsRng;

/// 哈希密码（argon2 Argon2id，随机盐，默认参数）。返回 PHC 字符串。
///
/// 失败时返回 `Err(ApiError::Internal)`，**绝不静默返回空串**——
/// 空串入库会导致用户永不可登录且无人察觉（对应安全审计 M1）。
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(format!("argon2 hash 失败: {e}")))
}

/// 校验密码。`$argon2` 前缀走 argon2；否则走旧 PBKDF2 兼容路径。
pub fn verify_password(password: &str, stored: &str) -> bool {
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

// ── 旧 PBKDF2-HMAC-SHA256（仅用于校验存量哈希）──

/// PBKDF2 单块派生（dkLen=32 → 一个 32 字节块，i=1）。
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

#[cfg(test)]
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
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

/// 旧 PBKDF2 哈希（仅测试/迁移用）。
#[cfg(test)]
fn hash_password_pbkdf2(password: &str, iterations: u32) -> String {
    let salt = *Uuid::new_v4().as_bytes();
    let dk = pbkdf2_block(password.as_bytes(), &salt, iterations);
    format!("{iterations}${}${}", hex(&salt), hex(&dk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_and_verify_round_trip() {
        let stored = hash_password("correct horse battery staple").unwrap();
        assert!(stored.starts_with("$argon2"));
        assert!(verify_password("correct horse battery staple", &stored));
        assert!(!verify_password("wrong password", &stored));
    }

    #[test]
    fn verify_legacy_pbkdf2_hash_still_works() {
        // 存量 PBKDF2 哈希须仍可校验（兼容旧 daemon_api_key / 用户密码）。
        let legacy = hash_password_pbkdf2("legacy password", 1000);
        assert!(!legacy.starts_with("$argon2"));
        assert!(verify_password("legacy password", &legacy));
        assert!(!verify_password("not legacy", &legacy));
    }

    #[test]
    fn jwt_issue_and_verify_round_trip() {
        let uid = Uuid::new_v4();
        let token = issue_jwt(uid, "secret", 60).unwrap();
        let back = verify_jwt(&token, "secret").unwrap();
        assert_eq!(back, uid);
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let uid = Uuid::new_v4();
        let token = issue_jwt(uid, "secret-a", 60).unwrap();
        assert!(verify_jwt(&token, "secret-b").is_err());
    }

    #[test]
    fn jwt_expired_rejected() {
        let uid = Uuid::new_v4();
        // 签发一个已过期 120 秒的 token（jsonwebtoken 默认 60s leeway）
        let exp = (Utc::now() - Duration::seconds(120)).timestamp() as usize;
        let claims = Claims {
            sub: uid.to_string(),
            exp,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(verify_jwt(&token, "secret").is_err());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(&[1, 2, 3], &[1, 2, 3]));
        assert!(!constant_time_eq(&[1, 2, 3], &[1, 2, 4]));
        assert!(!constant_time_eq(&[1, 2], &[1, 2, 3]));
    }
}
