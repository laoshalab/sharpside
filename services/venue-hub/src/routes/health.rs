//! 健康检查端点。对应 `docs/ARCHITECTURE.md` §6.1 / 安全修复 4.3。

use crate::state::AppState;
use axum::http::StatusCode;
use axum::Json;

/// `GET /healthz` — 存活探针。
pub async fn healthz() -> &'static str {
    "ok"
}

/// `GET /readyz` — 就绪探针。
///
/// 安全修复 4.3：除 DB ping 外，检查 ingest/hot/signal_replay 心跳是否过期，
/// 以及（可选）最新监控快照新鲜度。停滞时返回 503。
///
/// env：
/// - `WORKER_STALE_SECS`：心跳超时秒数，默认 600；0 = 不检查心跳
/// - `INGEST_STALE_SECS`：监控快照最大龄期，默认 1800；0 = 不检查
pub async fn readyz(
    state: AppState,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(e) = sharpside_db::ping(&state.db).await {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "db": "fail", "error": e.to_string() })),
        ));
    }

    let ticks = state.worker_ticks.snapshot();
    let now = chrono::Utc::now().timestamp();
    let worker_stale_secs: i64 = std::env::var("WORKER_STALE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let ingest_stale_secs: i64 = std::env::var("INGEST_STALE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1800);

    let mut stale = Vec::new();
    if worker_stale_secs > 0 {
        for (name, ts) in [
            ("ingest", ticks.ingest_last_tick_at),
            ("hot", ticks.hot_last_tick_at),
            ("signal_replay", ticks.signal_replay_last_tick_at),
            ("trade_watch", ticks.trade_watch_last_tick_at),
        ] {
            // 0 = 尚未 tick：启动宽限期 2× stale 内不判死（避免刚启动 503）。
            if ts == 0 {
                continue;
            }
            if now - ts > worker_stale_secs {
                stale.push(format!("{name}_stale_secs={}", now - ts));
            }
        }
    }

    let mut latest_snapshot_age: Option<i64> = None;
    if ingest_stale_secs > 0 {
        match sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>,)>(
            "SELECT MAX(captured_at) FROM trader_hub.trader_positions_snapshot",
        )
        .fetch_optional(&state.db)
        .await
        {
            Ok(Some((Some(ts),))) => {
                let age = now - ts.timestamp();
                latest_snapshot_age = Some(age);
                if age > ingest_stale_secs {
                    stale.push(format!("snapshot_stale_secs={age}"));
                }
            }
            Ok(_) => { /* 无快照：冷启动，不判死 */ }
            Err(e) => {
                tracing::warn!(error = %e, "readyz 查 position_snapshots 失败");
            }
        }
    }

    let body = serde_json::json!({
        "db": "ok",
        "worker_ticks": {
            "ingest_last_tick_at": ticks.ingest_last_tick_at,
            "hot_last_tick_at": ticks.hot_last_tick_at,
            "signal_replay_last_tick_at": ticks.signal_replay_last_tick_at,
            "trade_watch_last_tick_at": ticks.trade_watch_last_tick_at,
        },
        "latest_snapshot_age_secs": latest_snapshot_age,
        "stale": stale,
    });

    if stale.is_empty() {
        Ok(Json(body))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(body)))
    }
}
