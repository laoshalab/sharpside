//! 健康检查端点。对应 `docs/ARCHITECTURE.md` §6.1。

use crate::error::ApiError;
use crate::state::AppState;
use axum::Json;

/// `GET /healthz` — 存活探针。
pub async fn healthz() -> &'static str {
    "ok"
}

/// `GET /readyz` — 就绪探针（能 ping 通 DB）。
pub async fn readyz(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    sharpside_db::ping(&state.db).await?;
    Ok(Json(serde_json::json!({ "db": "ok" })))
}
