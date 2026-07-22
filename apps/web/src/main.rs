//! Sharpside web · 用户前端服务。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.5 BFF 与 `docs/FRONTEND_DESIGN.md`。
//! F0：axum 服务 `static/` 静态树（ES Modules，零构建）+ `/api/*` 反代 gateway，
//! 让前端同源调用 BFF。Leptos SSR+WASM 待有网络环境后落地（见 `docs/TECH_STACK_RUST.md`）。
//!
//! 静态服务用 `ServeDir`（tower-http fs feature）；SPA fallback 把未知非 `/api` 路径回退到
//! `index.html`，让 hash 路由（`#/traders/...`）在直接访问或刷新时仍能命中前端入口。

use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use std::env;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// 设置 `Cache-Control: no-store`，避免浏览器缓存 ES 模块导致前端改动不生效（冒烟/开发期）。
/// 生产可改为对带 hash 的资源用长缓存、index.html 用 no-cache。
async fn no_store(req: axum::extract::Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    res
}

#[derive(Clone)]
struct State {
    gateway_url: String,
    http: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let listen = env::var("WEB_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let gateway_url = env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8085".into());

    let state = State {
        gateway_url: gateway_url.trim_end_matches('/').to_string(),
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
    };

    let app = Router::new()
        // API 反代到 gateway（透传完整路径：/api/venue-hub/... /api/follow/... /api/account/... /api/copier/... /api/me/dashboard）
        .route("/api/*path", any(proxy))
        // 静态资源（styles/、main.js、api/、store/、components/、pages/）
        // SPA fallback：未知非 `/api` 路径回退 index.html，让 hash 路由直接访问/刷新可命中。
        .nest_service(
            "/",
            ServeDir::new("apps/web/static").fallback(ServeFile::new("apps/web/static/index.html")),
        )
        .layer(axum::middleware::from_fn(no_store))
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!(listen = %listen, gateway = %gateway_url, "web 启动");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn proxy(
    axum::extract::State(state): axum::extract::State<Arc<State>>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    req: axum::extract::Request,
) -> Response {
    let path = uri.path(); // 透传完整路径（gateway 期望 /api/venue-hub/... 等）
    let url = format!("{}{}", state.gateway_url, path);
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let url = format!("{url}{query}");

    let (parts, body) = req.into_parts();
    let mut fwd = reqwest::Client::new().request(convert_method(parts.method), &url);
    // 透传 header（剔除 hop-by-hop）
    for (k, v) in parts.headers.iter() {
        let name = k.as_str().to_lowercase();
        if matches!(name.as_str(), "host" | "content-length" | "connection") {
            continue;
        }
        fwd = fwd.header(k.clone(), v.clone());
    }
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "body read failed").into_response(),
    };
    if !bytes.is_empty() {
        fwd = fwd.body(bytes);
    }

    match fwd.build() {
        Ok(built) => match state.http.execute(built).await {
            Ok(resp) => {
                let status = axum::http::StatusCode::from_u16(resp.status().as_u16())
                    .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                let headers = resp.headers().clone();
                let body_bytes = resp.bytes().await.unwrap_or_default();
                let mut response = Response::new(body_bytes.into());
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                response
            }
            Err(e) => (
                axum::http::StatusCode::BAD_GATEWAY,
                format!("gateway upstream error: {e}"),
            )
                .into_response(),
        },
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            format!("build request failed: {e}"),
        )
            .into_response(),
    }
}

fn convert_method(m: axum::http::Method) -> reqwest::Method {
    match m {
        axum::http::Method::GET => reqwest::Method::GET,
        axum::http::Method::POST => reqwest::Method::POST,
        axum::http::Method::PUT => reqwest::Method::PUT,
        axum::http::Method::PATCH => reqwest::Method::PATCH,
        axum::http::Method::DELETE => reqwest::Method::DELETE,
        _ => reqwest::Method::GET,
    }
}
