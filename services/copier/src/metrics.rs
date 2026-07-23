//! Prometheus `/metrics`（安全修复 4.1 · copier）。

use crate::error::ApiError;
use crate::state::AppState;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use sharpside_db::queries::account as acct;
use std::sync::atomic::{AtomicU64, Ordering};

static RECLAIM_TOTAL: AtomicU64 = AtomicU64::new(0);
static CLOB_429_TOTAL: AtomicU64 = AtomicU64::new(0);

pub fn inc_reclaim() {
    RECLAIM_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_clob_429() {
    CLOB_429_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub async fn metrics(state: AppState) -> Result<impl IntoResponse, ApiError> {
    let pending = acct::count_copy_orders_by_status(&state.db, "pending")
        .await
        .unwrap_or(-1);
    let dispatched = acct::count_copy_orders_by_status(&state.db, "dispatched")
        .await
        .unwrap_or(-1);
    let submitted = acct::count_copy_orders_by_status(&state.db, "submitted")
        .await
        .unwrap_or(-1);
    let pending_age = acct::max_copy_order_age_secs(&state.db, "pending")
        .await
        .unwrap_or(0.0);
    let dispatched_age = acct::max_copy_order_age_secs(&state.db, "dispatched")
        .await
        .unwrap_or(0.0);

    let reclaim = RECLAIM_TOTAL.load(Ordering::Relaxed);
    let clob_429 = CLOB_429_TOTAL.load(Ordering::Relaxed);

    let body = format!(
        r#"# HELP sharpside_copy_orders Copy orders by status
# TYPE sharpside_copy_orders gauge
sharpside_copy_orders{{status="pending"}} {pending}
sharpside_copy_orders{{status="dispatched"}} {dispatched}
sharpside_copy_orders{{status="submitted"}} {submitted}
# HELP sharpside_copy_order_age_seconds Max age of orders in status (seconds)
# TYPE sharpside_copy_order_age_seconds gauge
sharpside_copy_order_age_seconds{{status="pending"}} {pending_age}
sharpside_copy_order_age_seconds{{status="dispatched"}} {dispatched_age}
# HELP sharpside_reclaim_total Reclaim worker reclaim attempts
# TYPE sharpside_reclaim_total counter
sharpside_reclaim_total {reclaim}
# HELP sharpside_clob_429_total CLOB HTTP 429 / RateLimited observed by copier
# TYPE sharpside_clob_429_total counter
sharpside_clob_429_total {clob_429}
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
