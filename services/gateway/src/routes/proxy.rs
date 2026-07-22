//! 通用反向代理。对应 `docs/ARCHITECTURE.md` §6.5（统一入口转发到各上游服务）。
//!
//! 把 `/api/venue-hub/*` → venue-hub，`/api/follow/*` → follow，等等。
//! MVP 简单透传 method + path + body；生产加请求/响应头改写与错误归一。

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;

/// 代理到指定上游。`upstream_key` ∈ {venue-hub, follow, copier, account}。
pub async fn proxy_to(
    state: AppState,
    upstream_key: &'static str,
    subpath: &str,
    req: Request,
) -> ApiResult<Response> {
    let base = upstream_base(&state, upstream_key)?;
    let url = if subpath.is_empty() {
        base
    } else {
        format!("{base}/{subpath}")
    };
    // 透传 query string（前端分页/筛选/period tabs 全靠它；缺失会导致上游拿到默认参数）。
    let url = match req.uri().query() {
        Some(q) if !q.is_empty() => format!("{url}?{q}"),
        _ => url,
    };

    let method = req.method().clone();
    // 透传原始请求头（Content-Type / Authorization 等）。缺失会破坏 POST JSON 解析与上游 JWT 鉴权。
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| ApiError::BadRequest(format!("body read: {e}")))?
        .to_vec();

    let mut req_builder = state.http.request(method, &url).body(body_bytes);
    for (name, value) in headers.iter() {
        // 跳过 hop-by-hop 头（host / content-length 由 reqwest 自动设置；connection 不应转发）。
        if matches!(name.as_str(), "host" | "content-length" | "connection") {
            continue;
        }
        req_builder = req_builder.header(name, value);
    }

    let resp = req_builder.send().await?;

    let status = resp.status();
    let resp_body = resp
        .bytes()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    let mut out = Response::new(Body::from(resp_body));
    *out.status_mut() = status;
    Ok(out)
}

fn upstream_base(state: &AppState, key: &str) -> ApiResult<String> {
    let base = match key {
        "venue-hub" => &state.config.upstreams.venue_hub,
        "follow" => &state.config.upstreams.follow,
        "copier" => &state.config.upstreams.copier,
        "account" => &state.config.upstreams.account,
        _ => return Err(ApiError::Internal(format!("unknown upstream {key}"))),
    };
    Ok(base.trim_end_matches('/').to_string())
}

/// 代理到 venue-hub。
pub async fn to_venue_hub(
    state: AppState,
    path: axum::extract::Path<String>,
    req: Request,
) -> ApiResult<Response> {
    proxy_to(state, "venue-hub", &path.0, req).await
}

/// 代理到 follow。
pub async fn to_follow(
    state: AppState,
    path: axum::extract::Path<String>,
    req: Request,
) -> ApiResult<Response> {
    proxy_to(state, "follow", &path.0, req).await
}

/// 代理到 copier。
pub async fn to_copier(
    state: AppState,
    path: axum::extract::Path<String>,
    req: Request,
) -> ApiResult<Response> {
    proxy_to(state, "copier", &path.0, req).await
}

/// 代理到 account。
pub async fn to_account(
    state: AppState,
    path: axum::extract::Path<String>,
    req: Request,
) -> ApiResult<Response> {
    proxy_to(state, "account", &path.0, req).await
}
