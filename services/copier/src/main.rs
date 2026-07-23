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
mod metrics;
mod reclaim_worker;
mod reconcile_worker;
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
    // ⚠ 同步更新 `sharpside_shared::jurisdiction::implemented_execute_venues()`（follow 创建门控用）。
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
    tracing::info!(listen = %config.listen_addr, dry_run = config.dry_run, "copier 启动");

    // 生产硬禁 DEV 覆盖路径：误设会让全员 Channel A 用同一私钥签名。
    if sharpside_shared::secrets::is_production() {
        if std::env::var("POLYMARKET_DEV_PRIVATE_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some()
        {
            return Err(anyhow::anyhow!(
                "生产环境禁止 POLYMARKET_DEV_PRIVATE_KEY：会覆盖 per-user KMS 密钥"
            ));
        }
        if std::env::var("POLYMARKET_DEV_PLAINTEXT_HANDLE").ok().as_deref() == Some("1") {
            return Err(anyhow::anyhow!(
                "生产环境禁止 POLYMARKET_DEV_PLAINTEXT_HANDLE=1：密文当明文"
            ));
        }
    }

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
        if sharpside_shared::secrets::is_production() {
            return Err(anyhow::anyhow!(
            "生产环境禁止 DevKms（SHARPSIDE_KMS_DEV_PLAINTEXT=1）：库内密钥可逆，须设 SHARPSIDE_KMS_MASTER_KEY_PATH（LocalKms）"
        ));
        }
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

    // 真钱门禁：COPIER_DRY_RUN=0 必须 production + LocalKms（禁止 DevKms / 无 KMS）。
    // 唯一例外：dev 下显式设 SHARPSIDE_ALLOW_DEVKMS_E2E=1 时放行 DevKms 实盘
    // （仅供 e2e_real_sign / e2e_real_trade 脚本验证签名/提交链路；生产忽略此 env，
    //   仍强制 production + LocalKms，杜绝误用）。
    if !config.dry_run {
        let allow_devkms_e2e =
            std::env::var("SHARPSIDE_ALLOW_DEVKMS_E2E").ok().as_deref() == Some("1");
        if !sharpside_shared::secrets::is_production() {
            if !allow_devkms_e2e {
                return Err(anyhow::anyhow!(
                    "COPIER_DRY_RUN=0 须 APP_ENV=production，或 dev 下显式设 SHARPSIDE_ALLOW_DEVKMS_E2E=1（仅 e2e）"
                ));
            }
            if kms.is_none() {
                return Err(anyhow::anyhow!(
                    "COPIER_DRY_RUN=0 须 KMS 已启用（dev e2e 设 SHARPSIDE_KMS_DEV_PLAINTEXT=1）"
                ));
            }
            tracing::warn!(
                kms = kms.as_ref().map(|k| k.name()),
                "COPIER_DRY_RUN=0：dev e2e 实盘已放行（SHARPSIDE_ALLOW_DEVKMS_E2E=1）——\
                 仅限本地 e2e，生产须 APP_ENV=production + LocalKms"
            );
        } else {
            if std::env::var("SHARPSIDE_KMS_MASTER_KEY_PATH").is_err() {
                return Err(anyhow::anyhow!(
                    "COPIER_DRY_RUN=0 须 SHARPSIDE_KMS_MASTER_KEY_PATH（LocalKms 站内签）"
                ));
            }
            if kms.is_none() {
                return Err(anyhow::anyhow!(
                    "COPIER_DRY_RUN=0 须 LocalKms 已启用（构造失败或未注入）"
                ));
            }
            tracing::warn!("COPIER_DRY_RUN=0：实盘执行已启用（production + LocalKms）");
        }
    }

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

    // dispatched 超时回收 worker（扫卡死 dispatched → 原子置 failed，不重下）
    let reclaim_state = state.clone();
    workers.spawn(async move {
        reclaim_worker::run(reclaim_state).await;
    });
    tracing::info!(
        enabled = config.reclaim_worker_enabled,
        interval_secs = config.worker_reclaim_secs,
        timeout_secs = config.dispatched_timeout_secs,
        "worker 已启动：reclaim(dispatched 超时回收)"
    );

    // 成交对账 worker（扫 submitted → 查 Venue 真实成交回写，替代"提交即记全成"）
    let reconcile_state = state.clone();
    workers.spawn(async move {
        reconcile_worker::run(reconcile_state).await;
    });
    tracing::info!(
        enabled = config.reconcile_worker_enabled,
        interval_secs = config.worker_reconcile_secs,
        timeout_secs = config.reconcile_timeout_secs,
        "worker 已启动：reconcile(成交对账)"
    );

    // HTTP API（daemon 通道 B 端点）
    let app = routes::router().with_state(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "copier HTTP 监听");
    let serve = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());

    tokio::select! {
        res = serve => {
            if let Err(e) = res {
                tracing::error!(error = %e, "HTTP serve 退出");
            }
        }
        _ = async {
            while workers.join_next().await.is_some() {}
        } => {
            tracing::error!("所有 worker 已退出");
        }
    }

    // 收尾：中止残留 worker 并等待退出（信号触发 graceful shutdown 后，HTTP 已排空）
    workers.abort_all();
    while workers.join_next().await.is_some() {}
    tracing::info!("copier 已关停");
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
    tracing::info!("收到终止信号，开始优雅关停（排空在途请求 + 中止 worker）");
}
