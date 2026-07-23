//! Gateway 服务入口。对应 `docs/ARCHITECTURE.md` §6.5。
//!
//! 启动流程：加载配置 → 初始化 tracing → 连接 PG（denylist）→ AppState + 限流器 → serve。

mod auth;
mod config;
mod error;
mod rate_limit;
mod routes;
mod state;

use config::Config;
use rate_limit::RateLimiters;
use state::AppState;
use std::net::SocketAddr;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();

    // 安全修复 4.2：生产或 LOG_FORMAT=json → JSON 结构化日志。
    {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let use_json = sharpside_shared::secrets::is_production()
            || std::env::var("LOG_FORMAT").ok().as_deref() == Some("json");
        if use_json {
            fmt()
                .json()
                .with_env_filter(filter)
                .with_current_span(false)
                .with_span_list(false)
                .init();
        } else {
            fmt().with_env_filter(filter).init();
        }
    }

    tracing::info!(listen = %config.listen_addr, "sharpside-gateway starting");

    // 只读连库查 jwt_denylist；迁移由 venue-hub / account 负责。
    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    tracing::info!("db 已连接（JWT denylist）");

    let state = AppState::new(config.clone(), db);
    let limiters = RateLimiters::new(config.rate_limit.default_rps);
    let app = routes::router(state, limiters);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
