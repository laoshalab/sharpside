//! Gateway 服务入口。对应 `docs/ARCHITECTURE.md` §6.5。
//!
//! 启动流程：加载配置 → 初始化 tracing → 构建 AppState + 限流器 → 绑定监听 → serve。

mod auth;
mod config;
mod error;
mod rate_limit;
mod routes;
mod state;

use config::Config;
use rate_limit::RateLimiters;
use state::AppState;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();

    tracing::info!(listen = %config.listen_addr, "sharpside-gateway starting");

    let state = AppState::new(config.clone());
    let limiters = RateLimiters::new(config.rate_limit.default_rps);
    let app = routes::router(state, limiters);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);
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
