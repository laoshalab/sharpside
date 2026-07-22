//! 身份端点。对应 `docs/ARCHITECTURE.md` §6.1 / `docs/VENUE_DESIGN.md` §7.1。
//!
//! - `GET /identities` — 已人工校对身份列表（用户端跨 Venue 跟随下拉用）
//! - `GET /identities/{id}` — 跨平台身份详情

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::Path;
use axum::Json;
use serde::Serialize;
use sharpside_db::queries::identities as identity_q;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct IdentityDetail {
    pub identity: sharpside_db::Identity,
    pub traders: Vec<sharpside_db::Trader>,
}

/// 已人工校对身份列表（`manual_verified=true`）。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.10 跨 Venue 身份下拉。
pub async fn list_identities(
    state: AppState,
) -> Result<Json<Vec<sharpside_db::Identity>>, ApiError> {
    let rows = identity_q::list_verified_identities(&state.db).await?;
    Ok(Json(rows))
}

/// 跨平台身份详情：identity 元信息 + 已链接的所有 trader。
pub async fn get_identity(
    state: AppState,
    Path(id): Path<Uuid>,
) -> Result<Json<IdentityDetail>, ApiError> {
    let identity = identity_q::get_identity(&state.db, id).await?;
    let traders = identity_q::list_identity_traders(&state.db, id).await?;
    Ok(Json(IdentityDetail { identity, traders }))
}
