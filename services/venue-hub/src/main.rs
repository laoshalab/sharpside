//! VenueHub · 多平台采集 + 市场映射 + 身份 + 绩效 + 热钥浮仓 + 影子校验。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.1 与 `docs/VENUE_DESIGN.md` §12。
//! 内部 worker：ingest / mapping / identity / perf / hot，单进程多 worker。
//! 启动时按环境变量配置注入 `VenueRegistry`，Venue 启停 = 配置开关。
//!
//! Phase 1a Step 10 落地。

mod config;
mod error;
mod registry;
mod routes;
mod state;
mod workers;

use crate::config::Config;
use crate::registry::build_registry;
use crate::state::AppState;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    tracing::info!(listen = %config.listen_addr, "venue-hub 启动");

    // 配置一致性告警：follow_url 非空但 follow_signal_secret 为空时，follow 侧会 401 拒收所有信号，
    // 导致 hot worker 检出的仓位变化被静默丢弃。生产由 assert_secret 兜底 panic；此处覆盖 dev 误配。
    if !config.follow_url.trim().is_empty() && config.follow_signal_secret.trim().is_empty() {
        tracing::warn!(
            follow_url = %config.follow_url,
            "FOLLOW_SIGNAL_SECRET 为空但 FOLLOW_URL 已配置：所有信号将被 follow 拒收。请设 FOLLOW_SIGNAL_SECRET 并与 follow 的 INTERNAL_SIGNAL_SECRET 一致"
        );
    }

    // DB 连接 + 迁移
    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    sharpside_db::migrate(&db).await?;
    tracing::info!("db 迁移完成");

    // Venue 注册表
    let registry = build_registry(&config);
    tracing::info!(venues = registry.platforms().len(), "venue 注册完成");

    let state = AppState::new(config.clone(), db.clone(), registry);

    // worker（后台 tokio task，任一 panic 会被观测）
    let mut workers = workers::spawn_all(state.clone());
    tracing::info!("worker 已启动：ingest / backfill / mapping / identity / perf / official_pnl / hot / shadow");

    // HTTP API
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "venue-hub HTTP 监听");

    let serve = axum::serve(listener, app);

    // 任一退出（serve 完成 / 所有 worker 结束 / 信号）即收尾
    tokio::select! {
        res = serve => {
            if let Err(e) = res {
                tracing::error!(error = %e, "HTTP serve 退出");
            }
        }
        _ = async move {
            while workers.join_next().await.is_some() {}
        } => {
            tracing::error!("所有 worker 已退出");
        }
    }

    Ok(())
}
