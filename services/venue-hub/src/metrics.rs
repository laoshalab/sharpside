//! Prometheus `/metrics`（安全修复 4.1）。
//!
//! 手写 Prometheus text exposition（无额外 crate，避免依赖拉取阻塞）；
//! Grafana/Prometheus 可直接 scrape。

use crate::error::ApiError;
use crate::state::AppState;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;

/// `GET /metrics` — Prometheus 文本格式。
pub async fn metrics(state: AppState) -> Result<impl IntoResponse, ApiError> {
    let pending = sharpside_db::queries::outbox::count_pending_outbox(&state.db)
        .await
        .unwrap_or(-1);
    let deadletter = sharpside_db::queries::outbox::count_deadletter_outbox(&state.db)
        .await
        .unwrap_or(-1);
    let ticks = state.worker_ticks.snapshot();
    let now = chrono::Utc::now().timestamp();

    let ingest_age = if ticks.ingest_last_tick_at > 0 {
        now - ticks.ingest_last_tick_at
    } else {
        -1
    };
    let hot_age = if ticks.hot_last_tick_at > 0 {
        now - ticks.hot_last_tick_at
    } else {
        -1
    };

    let hot_emit_fail = HOT_EMIT_FAIL.load(std::sync::atomic::Ordering::Relaxed);
    let deadletter_alerts = DEADLETTER_ALERTS.load(std::sync::atomic::Ordering::Relaxed);

    let body = format!(
        r#"# HELP sharpside_outbox_pending Undelivered signal outbox rows
# TYPE sharpside_outbox_pending gauge
sharpside_outbox_pending {pending}
# HELP sharpside_outbox_deadletter Deadlettered signal outbox rows
# TYPE sharpside_outbox_deadletter gauge
sharpside_outbox_deadletter {deadletter}
# HELP sharpside_worker_tick_age_seconds Seconds since last successful worker tick (-1 if never)
# TYPE sharpside_worker_tick_age_seconds gauge
sharpside_worker_tick_age_seconds{{worker="ingest"}} {ingest_age}
sharpside_worker_tick_age_seconds{{worker="hot"}} {hot_age}
# HELP sharpside_hot_emit_fail_total Hot worker emit failures (outbox enqueue path)
# TYPE sharpside_hot_emit_fail_total counter
sharpside_hot_emit_fail_total {hot_emit_fail}
# HELP sharpside_signal_deadletter_total Signals that reached deadletter (replay exhausted)
# TYPE sharpside_signal_deadletter_total counter
sharpside_signal_deadletter_total {deadletter_alerts}
"#
    );

    Ok((
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    ))
}

static HOT_EMIT_FAIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static DEADLETTER_ALERTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn inc_hot_emit_fail() {
    HOT_EMIT_FAIL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn inc_deadletter() {
    DEADLETTER_ALERTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// 健康检查 JSON（非 Prometheus）——保留给调试。
#[allow(dead_code)]
pub async fn metrics_json(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    let pending = sharpside_db::queries::outbox::count_pending_outbox(&state.db).await?;
    Ok(Json(serde_json::json!({ "outbox_pending": pending })))
}
