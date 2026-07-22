//! 健康检查端点。对应 `docs/ARCHITECTURE.md` §6.5。

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    service: &'static str,
}

/// `GET /health` — 存活探针（k8s liveness）。
pub async fn live() -> Json<Health> {
    Json(Health {
        status: "ok",
        service: "gateway",
    })
}

/// `GET /ready` — 就绪探针（k8s readiness）。MVP 始终返回 ok。
pub async fn ready() -> Json<Health> {
    Json(Health {
        status: "ok",
        service: "gateway",
    })
}
