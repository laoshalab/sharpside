//! Follow · 跟随关系 + 信号派生。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.2 / `docs/FLOWS.md` §4-5。
//! - 跟随关系 CRUD：可跟随单 Venue Trader 或跨 Venue Identity（须 manual_verified）
//! - 信号派生：venue-hub 检出 `trader.position.changed` 后 POST `/internal/signals`，
//!   匹配活跃 follow_relation → 派生 `copy_order` → 入 `account.copy_queue`（pending/skipped）
//!
//! Phase 1a Step 11 落地。

mod auth;
mod config;
mod error;
mod routes;
mod signal;
mod state;
mod watchlist;

use crate::config::Config;
use crate::state::AppState;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    tracing::info!(listen = %config.listen_addr, "follow 启动");

    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    sharpside_db::migrate(&db).await?;
    tracing::info!("db 迁移完成");

    let state = AppState::new(config.clone(), db);
    let app = routes::router().with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "follow HTTP 监听");
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
