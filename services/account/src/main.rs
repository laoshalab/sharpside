//! Account · 用户 / Pro+ 订阅 / 管辖域 / per-Venue 凭证 / daemon_api_key / deposit wallet。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.4。
//! 身份：钱包登录（SIWE）或 TG 登录；邮箱认证已移除。
//! KMS：LocalKms（生产）/ DevKms（dev，SHARPSIDE_KMS_DEV_PLAINTEXT=1）。

mod auth;
mod billing;
mod config;
mod deposit;
mod error;
mod migrate;
mod rate_limit;
mod routes;
mod siwe;
mod state;

use crate::config::Config;
use crate::state::AppState;
use sharpside_kms::Kms;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

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
    tracing::info!(listen = %config.listen_addr, "account 启动");

    let db = sharpside_db::connect(&config.database_url, config.db_max_connections).await?;
    sharpside_db::migrate(&db).await?;
    tracing::info!("db 迁移完成");

    // KMS：生产优先 LocalKms；dev 回退 DevKms（须 SHARPSIDE_KMS_DEV_PLAINTEXT=1）。
    // 须与 copier 使用同一 master key / 明文策略，才能解密 provision 写入的密文。
    let kms: Arc<dyn Kms> = if std::env::var("SHARPSIDE_KMS_MASTER_KEY_PATH").is_ok() {
        match sharpside_kms::LocalKms::from_env() {
            Ok(k) => {
                tracing::info!(kms = k.name(), "KMS 已启用（生产路径）");
                Arc::new(k)
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
        Arc::new(sharpside_kms::DevKms::from_env())
    } else {
        return Err(anyhow::anyhow!(
            "KMS 未配置：生产设 SHARPSIDE_KMS_MASTER_KEY_PATH，或 dev 设 SHARPSIDE_KMS_DEV_PLAINTEXT=1"
        ));
    };

    let state = AppState::new(config.clone(), db, kms);

    let mut workers = tokio::task::JoinSet::new();
    {
        let billing_state = state.clone();
        workers.spawn(async move {
            crate::billing::worker::run(billing_state).await;
        });
        tracing::info!(
            enabled = config.billing_worker_enabled,
            billing_configured = config.billing_enabled(),
            "worker 已启动：billing（receipt + getLogs 认领 + 过期）"
        );
    }

    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "account HTTP 监听");
    tokio::select! {
        result = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal()) => {
            result?;
        }
        _ = workers.join_next() => {
            tracing::error!("billing worker 意外退出");
        }
    }
    workers.abort_all();
    while workers.join_next().await.is_some() {}
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
