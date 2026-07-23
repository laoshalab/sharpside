//! Admin · 运营后台。
//!
//! 对应 `docs/ARCHITECTURE.md` §14「Venue × 业务面」二维菜单与 `docs/FRONTEND_DESIGN.md` §7。
//! F0：axum 服务 `static/` 静态树（ES Modules，零构建）+ API 挂在 `/api/*`（同源），
//! admin token 鉴权。Leptos SSR 前端待有网络环境后落地。
//!
//! 静态服务用 `ServeDir`（tower-http fs feature）；SPA fallback 把未知非 `/api` 路径回退到
//! `index.html`，让 hash 路由（`#/mappings` 等）直接访问/刷新仍能命中前端入口。

use crate::config::Config;
use crate::state::AppState;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::Router;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;
mod auth;
mod config;
mod error;
mod routes;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 安全修复 4.2：生产或 LOG_FORMAT=json → JSON 结构化日志。
    {
        let filter = EnvFilter::from_default_env();
        let use_json = sharpside_shared::secrets::is_production()
            || std::env::var("LOG_FORMAT").ok().as_deref() == Some("json");
        if use_json {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_current_span(false)
                .with_span_list(false)
                .init();
        } else {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }

    let config = Config::from_env();
    tracing::info!(listen = %config.listen_addr, "admin 启动");

    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    sharpside_db::migrate(&db).await?;
    tracing::info!("db 迁移完成");

    let state = AppState::new(config.clone(), db);
    // API 挂在 /api/*（admin 前端同源调用），静态资源在 /。
    let app = Router::new()
        .nest("/api", routes::router())
        .nest_service(
            "/",
            ServeDir::new("apps/admin/static")
                .fallback(ServeFile::new("apps/admin/static/index.html")),
        )
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "admin HTTP 监听");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// 优雅关停信号：监听 Ctrl-C / SIGTERM，触发后 axum 停止接收新连接并排空在途请求。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("install ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("收到终止信号，开始优雅关停（排空在途请求）");
}

/// 安全修复 3.2：CSP + 安全响应头（与 web 同口径）。
/// admin 无内联脚本，`script-src 'self'`；connect 同源防 token 外泄。
async fn security_headers(req: axum::extract::Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: https:; font-src 'self' data:; connect-src 'self'; \
             object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    h.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    h.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    res
}
