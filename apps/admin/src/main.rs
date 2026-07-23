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
use axum::routing::Router;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

mod config;
mod error;
mod routes;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

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
