//! 开发辅助端点。仅用于本地开发/测试，**生产部署应通过网关层禁用**。
//!
//! `GET /dev/token?user_id=<id>` — 签发一个 JWT，便于本地测试 `/me/dashboard` 等 BFF 端点。

use crate::auth::issue_jwt;
use crate::error::ApiResult;
use crate::state::AppState;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub user_id: String,
}

pub async fn issue_dev_token(
    state: AppState,
    axum::extract::Query(q): axum::extract::Query<UserIdQuery>,
) -> ApiResult<Json<TokenResponse>> {
    let token = issue_jwt(
        &q.user_id,
        &state.config.jwt_secret,
        state.config.jwt_ttl_seconds,
    )?;
    Ok(Json(TokenResponse {
        token,
        user_id: q.user_id,
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct UserIdQuery {
    pub user_id: String,
}
