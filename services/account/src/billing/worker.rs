//! Billing worker：过期发票/权益 + submitted receipt 确认 + getLogs 认领。

use crate::state::AppState;
use sharpside_db::queries::billing as bill;
use tracing::{info, warn};

pub async fn run(state: AppState) {
    if !state.config.billing_worker_enabled {
        info!("billing worker 已禁用（BILLING_WORKER_ENABLED=false）");
        return;
    }
    info!(
        confirm_secs = state.config.worker_billing_secs,
        expiry_secs = state.config.worker_billing_expiry_secs,
        confirmations = state.config.billing_confirmations,
        lookback = state.config.billing_logs_lookback_blocks,
        billing_configured = state.config.billing_enabled(),
        "billing worker 启动（receipt + getLogs 认领 + 过期）"
    );

    let mut confirm_tick = tokio::time::interval(std::time::Duration::from_secs(
        state.config.worker_billing_secs.max(5),
    ));
    let mut expiry_tick = tokio::time::interval(std::time::Duration::from_secs(
        state.config.worker_billing_expiry_secs.max(30),
    ));
    confirm_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    expiry_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = confirm_tick.tick() => {
                if let Err(e) = crate::billing::confirm::process_confirm_batch(&state).await {
                    warn!(error = %e, "billing confirm 扫描失败");
                }
            }
            _ = expiry_tick.tick() => {
                if let Err(e) = run_expiry(&state).await {
                    warn!(error = %e, "billing expiry 失败");
                }
            }
        }
    }
}

async fn run_expiry(state: &AppState) -> Result<(), sharpside_db::DbError> {
    let expired_inv = bill::expire_stale_invoices(&state.db).await?;
    let expired_sub =
        bill::expire_subscriptions(&state.db, state.config.billing_grace_secs).await?;
    if expired_inv > 0 || expired_sub > 0 {
        info!(
            expired_invoices = expired_inv,
            expired_subscriptions = expired_sub,
            "billing expiry 完成"
        );
    }
    Ok(())
}
