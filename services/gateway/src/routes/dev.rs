//! 开发辅助端点。仅用于本地开发/测试，**生产部署应通过网关层禁用**。
//!
//! `GET /dev/token?user_id=<id>` — 签发一个 JWT，便于本地测试 `/me/dashboard` 等 BFF 端点。

use crate::auth::issue_jwt;
use crate::error::ApiResult;
use crate::state::AppState;
use axum::response::IntoResponse;
use serde::Serialize;

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub user_id: String,
}

pub async fn issue_dev_token(
    state: AppState,
    axum::extract::Query(q): axum::extract::Query<UserIdQuery>,
) -> ApiResult<axum::response::Response> {
    let token = issue_jwt(
        &q.user_id,
        &state.config.jwt_secret,
        state.config.jwt_ttl_seconds,
    )?;
    // 安全修复 3.1：dev token 也写 HttpOnly cookie，便于浏览器本地开发（命中 /dev/token 后即可访问 /me/*）。
    let cookie = sharpside_shared::session::build_set_cookie(
        &token,
        state.config.jwt_ttl_seconds,
        state.config.cookie_secure,
    );
    let mut resp = axum::Json(TokenResponse {
        token,
        user_id: q.user_id,
    })
    .into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&cookie) {
        resp.headers_mut()
            .insert(axum::http::header::SET_COOKIE, v);
    }
    Ok(resp)
}

#[derive(Debug, serde::Deserialize)]
pub struct UserIdQuery {
    pub user_id: String,
}
