//! BFF 聚合端点。对应 `docs/ARCHITECTURE.md` §6.5（一次调用拼装：跨平台排行榜 + 身份绩效 + 跟随状态 + 可用执行 Venue）。
//!
//! 上游不可达时该字段返回空结构 / 默认值，不阻塞整体 BFF（降级）。

use crate::auth::{AuthUser, DaemonAuth};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use sharpside_shared::allowed_execute_venues;

/// BFF 聚合响应：用户仪表盘。对应 `docs/FRONTEND_DESIGN.md` §6.6。
#[derive(Debug, Serialize, Deserialize)]
pub struct Dashboard {
    pub user_id: String,
    pub leaderboard: serde_json::Value,
    pub follows: serde_json::Value,
    /// 按用户 jurisdiction 推导的可执行 Venue（对齐 copier `allowed_execute_venues`）。
    pub available_venues: Vec<String>,
    /// 活跃跟随数（follows 数组中 active=true 的条目数）。
    pub active_follows: i64,
    /// 观察名单收藏数（watchlist；上游不可达则 0）。
    pub watchlist_count: i64,
    /// 累计跟单指令数（copier copy_order；上游不可达则 0）。
    pub total_copy_orders: i64,
    /// 累计成交数（copier copy_execution 总数）。
    pub total_executions: i64,
    /// 累计已实现 PnL（优先 portfolio_kpi.total_pnl；否则从成交近似）。
    pub total_pnl: f64,
    /// 组合 KPI 子集（来自 `GET /copier/me/portfolio?period=1m` 的 `kpi`；上游不可达为 null）。
    pub portfolio_kpi: serde_json::Value,
    /// 用户管辖域（来自 account `/me`；不可达则 "other"）。
    pub jurisdiction: String,
}

/// `GET /me/dashboard` — BFF 一次拼装。需 JWT 鉴权。
///
/// 并发拉取：venue-hub 排行榜 + follow 列表 + account `/me`（jurisdiction）+
/// copier portfolio（kpi）+ copy-executions + copy-orders/recent。
pub async fn dashboard(
    state: AppState,
    user: AuthUser,
    headers: HeaderMap,
) -> ApiResult<Json<Dashboard>> {
    let cfg = &state.config;
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let auth = auth_header.as_deref();

    let (leaderboard, follows, me, portfolio, execs, orders, watchlist) = tokio::join!(
        fetch_or_empty(&state, &cfg.upstreams.venue_hub, "/traders?limit=20", None),
        fetch_or_empty(&state, &cfg.upstreams.follow, "/follows", auth),
        fetch_or_empty(&state, &cfg.upstreams.account, "/me", auth),
        fetch_or_empty(
            &state,
            &cfg.upstreams.copier,
            "/me/portfolio?period=1m",
            auth
        ),
        fetch_or_empty(
            &state,
            &cfg.upstreams.copier,
            "/me/copy-executions?limit=10000",
            auth
        ),
        fetch_or_empty(
            &state,
            &cfg.upstreams.copier,
            "/me/copy-orders/recent?limit=10000",
            auth
        ),
        fetch_or_empty(&state, &cfg.upstreams.follow, "/me/watchlists", auth),
    );

    let jurisdiction = me
        .get("jurisdiction")
        .and_then(|v| v.as_str())
        .unwrap_or("other")
        .to_string();
    let available_venues = allowed_execute_venues(&jurisdiction)
        .into_iter()
        .map(|p| p.as_str().to_string())
        .collect();

    let active_follows = follows
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|f| f.get("active").and_then(|v| v.as_bool()).unwrap_or(false))
                .count() as i64
        })
        .unwrap_or(0);

    let watchlist_count = watchlist.as_array().map(|a| a.len() as i64).unwrap_or(0);

    let total_executions = execs.as_array().map(|a| a.len() as i64).unwrap_or(0);
    let total_copy_orders = orders
        .as_array()
        .map(|a| a.len() as i64)
        .unwrap_or(total_executions);

    let portfolio_kpi = portfolio
        .get("kpi")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let total_pnl = portfolio_kpi
        .get("total_pnl")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| approx_pnl_from_execs(&execs));

    Ok(Json(Dashboard {
        user_id: user.user_id,
        leaderboard,
        follows,
        available_venues,
        active_follows,
        watchlist_count,
        total_copy_orders,
        total_executions,
        total_pnl,
        portfolio_kpi,
        jurisdiction,
    }))
}

// 管辖域 → 允许的 execution_venue。已下沉到 `sharpside_shared::allowed_execute_venues`，
// 供 follow（创建时前置校验）/ copier（执行时兜底）/ gateway（BFF 展示）共用。
// 此处仅 re-use，不再保留本地拷贝。
fn approx_pnl_from_execs(execs: &serde_json::Value) -> f64 {
    execs
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|e| e.get("side").and_then(|v| v.as_str()) == Some("SELL"))
                .map(|e| {
                    let size = e.get("filled_size").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let price = e
                        .get("filled_price")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let fee = e.get("fee").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    size * price - fee
                })
                .sum::<f64>()
        })
        .unwrap_or(0.0)
}

/// daemon 长轮询：`GET /me/copy-orders?since=`。对应 `docs/FLOWS.md` §7。
pub async fn copy_orders(
    state: AppState,
    _daemon: DaemonAuth,
    axum::extract::Query(q): axum::extract::Query<SinceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let url = format!(
        "{}/copy-orders?since={}",
        state.config.upstreams.copier, q.since
    );
    let resp = state.http.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!("copier {}", resp.status())));
    }
    let body: serde_json::Value = resp.json().await?;
    Ok(Json(body))
}

#[derive(Debug, Deserialize)]
pub struct SinceQuery {
    pub since: String,
}

/// 拉取上游，失败返回空 Value（降级，不阻塞整体 BFF）。
async fn fetch_or_empty(
    state: &AppState,
    base: &str,
    path: &str,
    auth: Option<&str>,
) -> serde_json::Value {
    let url = format!("{base}{path}");
    let mut req = state.http.get(&url);
    if let Some(a) = auth {
        req = req.header(axum::http::header::AUTHORIZATION, a);
    }
    match req.send().await {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    }
}
