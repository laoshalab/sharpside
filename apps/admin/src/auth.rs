//! 安全修复 3.3：admin SSO/OIDC + 短时 session。
//!
//! 弃共享 `ADMIN_TOKEN`：浏览器登录走 OIDC（Authorization Code Flow），
//! 回调校验 id_token（JWKS 签名 + iss/aud/exp）→ 校验邮箱白名单 → 签发**短时** session JWT，
//! 写 HttpOnly cookie。`AdminAuth` extractor 从 cookie 验 session（dev 回退 Bearer admin_token）。
//! 操作者身份（email）由 session 决定，不再由客户端 body 传入（审计可信）。

use crate::config::OidcConfig;
use crate::error::ApiError;
use crate::state::AppState;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// admin session cookie 名。
pub const SESSION_COOKIE: &str = "sharpside_admin_session";
/// OIDC state 防 CSRF cookie 名。
const OAUTH_STATE_COOKIE: &str = "sharpside_admin_oauth_state";

/// admin session JWT claims。`sub` = admin email。
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
}

/// 鉴权后的 admin 身份（handler extractor）。
#[derive(Debug, Clone)]
pub struct AdminAuth {
    pub email: String,
}

/// 签发 admin session JWT（HS256，session_secret）。
pub fn issue_session(secret: &str, email: &str, ttl_seconds: i64) -> Result<String, ApiError> {
    let now = Utc::now().timestamp() as usize;
    let claims = SessionClaims {
        sub: email.to_string(),
        exp: (Utc::now() + Duration::seconds(ttl_seconds)).timestamp() as usize,
        iat: now,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("session jwt encode: {e}")))
}

/// 校验 session JWT 签名 + exp。
pub fn verify_session(secret: &str, token: &str) -> Result<SessionClaims, ApiError> {
    let mut v = Validation::new(Algorithm::HS256);
    v.validate_exp = true;
    let data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &v,
    )
    .map_err(|_| ApiError::Unauthorized("invalid or expired admin session".into()))?;
    Ok(data.claims)
}

fn build_session_cookie(token: &str, ttl: i64, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={ttl}{s}",
        s = if secure { "; Secure" } else { "" }
    )
}

fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{s}",
        s = if secure { "; Secure" } else { "" }
    )
}

fn read_cookie(parts: &Parts, name: &str) -> Option<String> {
    let header = parts.headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in header.split(';') {
        let p = pair.trim();
        if let Some(rest) = p.strip_prefix(name) {
            if let Some(val) = rest.strip_prefix('=') {
                let v = val.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[async_trait]
impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1) 优先 session cookie（生产路径）。
        if let Some(tok) = read_cookie(parts, SESSION_COOKIE) {
            if let Some(oidc) = state.config.oidc.as_ref() {
                let claims = verify_session(&oidc.session_secret, &tok)
                    .map_err(|e| e.into_response())?;
                return Ok(AdminAuth { email: claims.sub });
            }
        }
        // 2) dev 回退：非生产且未配 OIDC 时，接受 `Authorization: Bearer <admin_token>`。
        if !sharpside_shared::secrets::is_production() && state.config.oidc.is_none() {
            if let Some(h) = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
            {
                if let Some(tok) = h.strip_prefix("Bearer ") {
                    if sharpside_shared::secrets::constant_time_eq(
                        tok.trim().as_bytes(),
                        state.config.admin_token.as_bytes(),
                    ) {
                        return Ok(AdminAuth {
                            email: "dev-admin".to_string(),
                        });
                    }
                }
            }
        }
        Err(ApiError::Unauthorized("admin session required".into()).into_response())
    }
}

// ── OIDC ──

#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    iss: String,
    aud: String,
    exp: usize,
    email: Option<String>,
    email_verified: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<jsonwebtoken::jwk::Jwk>,
}

fn random_hex(n: usize) -> String {
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// `GET /api/auth/oidc/login` → 302 到 OIDC authorize；写 state cookie 防 CSRF。
pub async fn oidc_login(state: AppState) -> Result<axum::response::Response, ApiError> {
    let oidc = state
        .config
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::Internal("OIDC 未配置".into()))?;
    let disc = discover(&oidc.issuer).await?;
    let state_val = random_hex(16);
    // state cookie：短时 JWT（复用 session_secret 签名），携带 state 值。
    let now = Utc::now().timestamp() as usize;
    let state_jwt = encode(
        &Header::new(Algorithm::HS256),
        &serde_json::json!({ "state": state_val, "exp": now + 600, "iat": now }),
        &EncodingKey::from_secret(oidc.session_secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("state jwt encode: {e}")))?;
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        disc.authorization_endpoint,
        urlencode(&oidc.client_id),
        urlencode(&oidc.redirect_uri),
        urlencode("openid email profile"),
        urlencode(&state_val)
    );
    let mut resp = axum::response::Redirect::to(&auth_url).into_response();
    resp.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&format!(
            "{OAUTH_STATE_COOKIE}={state_jwt}; HttpOnly; SameSite=Lax; Path=/; Max-Age=600{}",
            if state.config.cookie_secure { "; Secure" } else { "" }
        ))
        .map_err(|e| ApiError::Internal(format!("set-cookie: {e}")))?,
    );
    Ok(resp)
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// `GET /api/auth/oidc/callback?code=&state=` → 换 token → 校验 id_token → 签 session → 302 回 `/`。
pub async fn oidc_callback(
    state: AppState,
    headers: axum::http::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<CallbackQuery>,
) -> Result<axum::response::Response, ApiError> {
    let oidc = state
        .config
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::Internal("OIDC 未配置".into()))?;

    // 1) state 校验（CSRF）：cookie 内 JWT 解出 state，须与 query 一致。
    let state_cookie = read_cookie_from_headers(&headers, OAUTH_STATE_COOKIE)
        .ok_or_else(|| ApiError::Unauthorized("missing oauth state cookie".into()))?;
    let mut sv = Validation::new(Algorithm::HS256);
    sv.validate_exp = true;
    let state_data: serde_json::Value = decode(
        &state_cookie,
        &DecodingKey::from_secret(oidc.session_secret.as_bytes()),
        &sv,
    )
    .map_err(|_| ApiError::Unauthorized("invalid oauth state".into()))?
    .claims;
    let expected_state = state_data
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Unauthorized("malformed oauth state".into()))?;
    if !sharpside_shared::secrets::constant_time_eq(
        expected_state.as_bytes(),
        q.state.as_bytes(),
    ) {
        return Err(ApiError::Unauthorized("oauth state mismatch".into()));
    }

    // 2) code → token（含 id_token）。
    let disc = discover(&oidc.issuer).await?;
    let id_token = exchange_code(&disc.token_endpoint, &oidc, &q.code).await?;

    // 3) 校验 id_token：JWKS 签名 + iss + aud + exp；取 email。
    let email = verify_id_token(&disc, &oidc, &id_token).await?;

    // 4) 邮箱白名单。
    let email_lc = email.to_lowercase();
    let allowed = oidc
        .allowed_emails
        .iter()
        .any(|e| e == &email_lc);
    if !allowed {
        tracing::warn!(email = %email, "OIDC 登录被拒：邮箱不在 admin 白名单");
        return Err(ApiError::Unauthorized("email not in admin allowlist".into()));
    }

    // 5) 签 session + 写 cookie + 302 回前端根。
    let session = issue_session(&oidc.session_secret, &email_lc, oidc.session_ttl_seconds)?;
    tracing::info!(email = %email_lc, "admin OIDC 登录成功，签发短时 session");
    let mut resp = axum::response::Redirect::to("/").into_response();
    resp.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&build_session_cookie(
            &session,
            oidc.session_ttl_seconds,
            state.config.cookie_secure,
        ))
        .map_err(|e| ApiError::Internal(format!("set-cookie: {e}")))?,
    );
    // 清 state cookie。
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&format!(
            "{OAUTH_STATE_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
            if state.config.cookie_secure { "; Secure" } else { "" }
        ))
        .map_err(|e| ApiError::Internal(format!("set-cookie: {e}")))?,
    );
    Ok(resp)
}

/// `POST /api/auth/oidc/logout` → 清 session cookie。
pub async fn oidc_logout(state: AppState) -> Result<axum::response::Response, ApiError> {
    let mut resp = axum::Json(serde_json::json!({ "ok": true })).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&clear_session_cookie(state.config.cookie_secure)) {
        resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
    }
    Ok(resp)
}

/// `GET /api/auth/me` → 返回当前 admin email（session 有效时）。
pub async fn oidc_me(_state: AppState, auth: AdminAuth) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "email": auth.email })))
}

fn read_cookie_from_headers(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let h = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in h.split(';') {
        let p = pair.trim();
        if let Some(rest) = p.strip_prefix(name) {
            if let Some(val) = rest.strip_prefix('=') {
                let v = val.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "+".into(),
            c if c.is_alphanumeric() || "-_.~".contains(c) => c.to_string(),
            c => format!("%{:02X}", c as u8),
        })
        .collect()
}

async fn discover(issuer: &str) -> Result<Discovery, ApiError> {
    let url = format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::Internal(format!("reqwest build: {e}")))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("oidc discovery: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::Internal(format!("oidc discovery status: {e}")))?;
    resp.json::<Discovery>()
        .await
        .map_err(|e| ApiError::Internal(format!("oidc discovery parse: {e}")))
}

async fn exchange_code(token_endpoint: &str, oidc: &OidcConfig, code: &str) -> Result<String, ApiError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::Internal(format!("reqwest build: {e}")))?;
    let resp = client
        .post(token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &oidc.redirect_uri),
            ("client_id", &oidc.client_id),
            ("client_secret", &oidc.client_secret),
        ])
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("oidc token exchange: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::Internal(format!("oidc token status: {e}")))?;
    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("oidc token parse: {e}")))?;
    tr.id_token
        .ok_or_else(|| ApiError::Internal("oidc token 响应缺 id_token".into()))
}

async fn verify_id_token(disc: &Discovery, oidc: &OidcConfig, id_token: &str) -> Result<String, ApiError> {
    // 取 kid（header）以匹配 JWKS。
    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|e| ApiError::Internal(format!("id_token header: {e}")))?;
    let kid = header.kid.ok_or_else(|| ApiError::Internal("id_token 无 kid".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::Internal(format!("reqwest build: {e}")))?;
    let jwks: Jwks = client
        .get(&disc.jwks_uri)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("jwks fetch: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::Internal(format!("jwks status: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("jwks parse: {e}")))?;
    let key = jwks
        .keys
        .iter()
        .find(|k| k.common.key_id.as_deref() == Some(kid.as_str()))
        .ok_or_else(|| ApiError::Internal("JWKS 无匹配 kid".into()))?;
    let decoding = DecodingKey::from_jwk(key)
        .map_err(|e| ApiError::Internal(format!("jwk decode: {e}")))?;

    let mut v = Validation::new(Algorithm::RS256);
    v.validate_exp = true;
    v.set_issuer(&[&oidc.issuer]);
    v.set_audience(&[&oidc.client_id]);
    let data = decode::<IdTokenClaims>(id_token, &decoding, &v)
        .map_err(|e| ApiError::Unauthorized(format!("id_token 校验失败: {e}")))?;
    if data.claims.email_verified == Some(false) {
        return Err(ApiError::Unauthorized("email 未验证".into()));
    }
    data.claims
        .email
        .ok_or_else(|| ApiError::Unauthorized("id_token 无 email claim".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_issue_verify_round_trip() {
        let s = issue_session("secret", "admin@example.com", 60).unwrap();
        let claims = verify_session("secret", &s).unwrap();
        assert_eq!(claims.sub, "admin@example.com");
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn session_wrong_secret_rejected() {
        let s = issue_session("a", "admin@example.com", 60).unwrap();
        assert!(verify_session("b", &s).is_err());
    }

    #[test]
    fn session_expired_rejected() {
        let exp = (Utc::now() - Duration::seconds(120)).timestamp() as usize;
        let claims = SessionClaims {
            sub: "x".into(),
            exp,
            iat: exp,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let s = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(verify_session("secret", &s).is_err());
    }

    #[test]
    fn cookie_parse_and_build() {
        assert!(build_session_cookie("t", 3600, true).contains("; Secure"));
        assert!(!build_session_cookie("t", 3600, false).contains("; Secure"));
        assert!(clear_session_cookie(true).contains("Max-Age=0"));
    }

    #[test]
    fn urlenc_handles_special() {
        assert_eq!(urlencode("openid email"), "openid+email");
        assert_eq!(urlencode("a/b"), "a%2Fb");
    }
}

