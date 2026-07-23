//! 健康检查端点。对应 `docs/ARCHITECTURE.md` §6.5。

use crate::state::AppState;
use axum::Json;
use axum::http::StatusCode;
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

/// `GET /ready` — 就绪探针（k8s readiness）。并发探测四个上游的 /healthz，
/// 任一不可达则返 503，避免编排器把流量打到上游未就绪的 gateway。
pub async fn ready(state: AppState) -> Result<Json<Health>, StatusCode> {
    let cfg = &state.config.upstreams;
    let client = &state.http;
    let vh_url = format!("{}/healthz", cfg.venue_hub);
    let fl_url = format!("{}/healthz", cfg.follow);
    let cp_url = format!("{}/healthz", cfg.copier);
    let ac_url = format!("{}/healthz", cfg.account);
    let (vh, fl, cp, ac) = tokio::join!(
        probe_upstream(client, &vh_url),
        probe_upstream(client, &fl_url),
        probe_upstream(client, &cp_url),
        probe_upstream(client, &ac_url),
    );
    let results = [vh, fl, cp, ac];
    if results.iter().all(|ok| *ok) {
        Ok(Json(Health {
            status: "ok",
            service: "gateway",
        }))
    } else {
        let failed: Vec<&str> = ["venue-hub", "follow", "copier", "account"]
            .iter()
            .zip(results.iter())
            .filter(|(_, ok)| !**ok)
            .map(|(n, _)| *n)
            .collect();
        tracing::warn!(upstreams = ?failed, "就绪探针：部分上游不可达");
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// 探测单个上游 /healthz，2s 超时，成功（2xx）返回 true。
async fn probe_upstream(client: &reqwest::Client, url: &str) -> bool {
    matches!(
        client
            .get(url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    )
}
