//! Account · 用户 / Pro+ 订阅 / 管辖域 / per-Venue 凭证 / daemon_api_key / deposit wallet。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.4。
//! 身份：钱包登录（SIWE）或 TG 登录；邮箱认证已移除。
//! KMS：LocalKms（生产）/ DevKms（dev，SHARPSIDE_KMS_DEV_PLAINTEXT=1）。

mod auth;
mod config;
mod deposit;
mod error;
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
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

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
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(listen = %config.listen_addr, "account HTTP 监听");
    axum::serve(listener, app).await?;
    Ok(())
}
