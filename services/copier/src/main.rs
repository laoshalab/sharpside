//! Copier · 通道 × Venue 执行 + 风控。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §6-7,§10。
//! - 通道 A（TG，平台代签）：worker 轮询 `copy_order(channel=tg, pending)` → 管辖域过滤 →
//!   跨 Venue 映射 → 单位换算 → 风控 → `Venue::place_order` → 写 `copy_execution`
//! - 通道 B（daemon，零钥）：daemon 拉取 `/me/copy-orders` 本地签名下单后回传 `/result`
//! - 统一风控：min_notional / 日成交上限 / 持仓上限 / rapid-flip 守卫（三级覆盖见 risk.rs）
//!
//! Phase 1a Step 12 落地。dry_run 默认 true（离线/无凭证闭环演示）。

mod auth;
mod config;
mod error;
mod exec;
mod redeem_worker;
mod risk;
mod routes;
mod state;

use crate::config::Config;
use crate::state::AppState;
use sharpside_kms::Kms;
use sharpside_venues_core::VenueRegistry;
use sharpside_venues_polymarket::PolymarketVenue;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

fn build_registry(kms: Option<Arc<dyn Kms>>) -> VenueRegistry {
    let mut registry = VenueRegistry::new();
    // MVP：仅 Polymarket 作为 execution venue。新增平台在此注册。
    let mut venue = PolymarketVenue::new();
    if let Some(k) = kms {
        venue = venue.with_kms(k);
    }
    // 提现走 WALLET batch transfer，需 relayer。RelayerClient::new() 读 env
    // POLYMARKET_RELAYER_URL / POLYMARKET_BUILDER_*；缺凭证时 submit 会 401，
    // 但余额查询/下单不受影响，故无条件注入。
    venue = venue.with_relayer(sharpside_venues_polymarket::RelayerClient::new());
    registry.register(Arc::new(venue));
    registry
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    tracing::info!(listen = %config.listen_addr, dry_run = config.dry_run, "copier 启动");

    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    sharpside_db::migrate(&db).await?;
    tracing::info!("db 迁移完成");

    // KMS：生产优先 LocalKms（落盘 master key + AES-256-GCM），dev 回退 DevKms（明文透传）。
    // copier 用 KMS 解密 DepositWalletDelegated.encrypted_owner_key / encrypted_l2_secret。
    // 须与 account 服务用同一 master key 文件（SHARPSIDE_KMS_MASTER_KEY_PATH）才能解密 provision 写入的密文。
    let kms: Option<Arc<dyn Kms>> = if std::env::var("SHARPSIDE_KMS_MASTER_KEY_PATH").is_ok() {
        match sharpside_kms::LocalKms::from_env() {
            Ok(k) => {
                tracing::info!(kms = k.name(), "KMS 已启用（生产路径）");
                Some(Arc::new(k))
            }
            Err(e) => return Err(anyhow::anyhow!("LocalKms 构造失败: {e}")),
        }
    } else if std::env::var("SHARPSIDE_KMS_DEV_PLAINTEXT").ok().as_deref() == Some("1") {
        tracing::warn!(
            "DevKms 已启用（明文透传）—— 仅 dev/测试，生产须设 SHARPSIDE_KMS_MASTER_KEY_PATH"
        );
        Some(Arc::new(sharpside_kms::DevKms::from_env()))
    } else {
        tracing::info!(
            "KMS 未注入（无 master key / dev 明文）—— place_order 走 env / dev_signer 路径"
        );
        None
    };

    let registry = build_registry(kms);
    tracing::info!(venues = registry.platforms().len(), "venue 注册完成");

    let state = AppState::new(config.clone(), db.clone(), registry);

    // 通道 A 执行 worker
    let exec_state = state.clone();
    let mut workers = tokio::task::JoinSet::new();
    workers.spawn(async move {
        exec::run(exec_state).await;
    });
    tracing::info!("worker 已启动：exec(通道A tg)");

    // 自动赎回 worker（扫新结算市场 → 对有仓位用户自动 redeem）
    let redeem_state = state.clone();
    workers.spawn(async move {
        redeem_worker::run(redeem_state).await;
    });
    tracing::info!(
        enabled = config.redeem_worker_enabled,
        interval_secs = config.worker_redeem_secs,
        "worker 已启动：redeem(自动赎回)"
    );

    // HTTP API（daemon 通道 B 端点）
    let app = routes::router().with_state(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "copier HTTP 监听");
    let serve = axum::serve(listener, app);

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
