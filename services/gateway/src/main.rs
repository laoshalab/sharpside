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
    let limiters = RateLimiters::new(config.rate_limit.default_rps, config.rate_limit.daemon_rps);
    let app = routes::router(state, limiters);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
