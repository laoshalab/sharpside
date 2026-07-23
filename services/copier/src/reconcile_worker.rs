//! 成交对账 worker（P0）。对应 `docs/FLOWS.md` §6（Channel A 真实成交回写）。
//!
//! 此前 exec worker 把 `place_order` 返回的 orderID 当成"全部成交"立即记 filled，
//! 限价单实际可能挂单未成交 / 部分成交，导致账实不符。现 exec worker 改为 place_order 成功后
//! 置 `submitted`（持久化 venue_order_id），由本 worker 轮询 `Venue::order_state` 回写真实成交：
//!
//! - Filled → insert_copy_execution(真实 filled_size/filled_price) + status=filled
//! - Cancelled → 若 filled_size>0 记部分成交 + status=cancelled；否则 status=cancelled
//! - Rejected → status=failed
//! - Open/PartiallyFilled → 留 submitted 重试；超 `reconcile_timeout_secs` 仍 LIVE 则撤单 + 置 cancelled
//!
//! 单笔对账失败（Venue API 瞬态故障）不置终态，留 submitted 下轮重试，避免误判。
//! dry_run 路径不进 submitted（exec 直接记合成 filled），故本 worker 仅作用于 live Channel A。

use crate::exec::load_credential;
use crate::state::AppState;
use sharpside_db::queries::account as acct;
use sharpside_shared::Platform;
use sharpside_venues_core::OrderStatus;
use tracing::{error, info, warn};

const RECONCILE_BATCH: i64 = 100;

pub async fn run(state: AppState) {
    if !state.config.reconcile_worker_enabled {
        info!("成交对账 worker 已禁用（RECONCILE_WORKER_ENABLED=false）");
        return;
    }
    info!(
        interval_secs = state.config.worker_reconcile_secs,
        timeout_secs = state.config.reconcile_timeout_secs,
        "成交对账 worker 启动"
    );
    loop {
        if let Err(e) = tick(&state).await {
            error!(error = %e, "成交对账 tick 失败");
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            state.config.worker_reconcile_secs.max(1),
        ))
        .await;
    }
}

async fn tick(state: &AppState) -> Result<(), anyhow::Error> {
    let pending = acct::list_submitted_copy_orders(&state.db, RECONCILE_BATCH).await?;
    if pending.is_empty() {
        return Ok(());
    }
    let timeout = chrono::Duration::seconds(state.config.reconcile_timeout_secs as i64);
    let now = chrono::Utc::now();
    for order in pending {
        if let Err(e) = reconcile_one(state, &order, now, timeout).await {
            warn!(order_id = %order.id, error = %e, "对账单笔失败，留 submitted 下轮重试");
        }
    }
    Ok(())
}

async fn reconcile_one(
    state: &AppState,
    order: &sharpside_db::CopyOrderRow,
    now: chrono::DateTime<chrono::Utc>,
    timeout: chrono::Duration,
) -> Result<(), anyhow::Error> {
    let execute_venue: Platform = order
        .execute_venue
        .parse()
        .map_err(|e| anyhow::anyhow!("execute_venue 解析失败: {e}"))?;
    let venue_order_id = order
        .venue_order_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("submitted 但 venue_order_id 为空（数据异常）"))?;

    let Some(venue) = state.registry.get(execute_venue) else {
        return Err(anyhow::anyhow!("venue {execute_venue} 未注册"));
    };
    let cred = load_credential(state, order.user_id, execute_venue, &order.channel).await?;

    let st = venue.order_state(&cred, venue_order_id).await?;

    match st.status {
        OrderStatus::Filled => {
            record_fill(state, order, venue_order_id, st.filled_size, st.filled_price, st.fee)
                .await?;
            acct::update_copy_order_status(&state.db, order.id, "filled", None).await?;
            info!(order_id = %order.id, venue_order_id, filled_size = st.filled_size, "对账确认成交");
        }
        OrderStatus::Cancelled | OrderStatus::Rejected => {
            let terminal = if matches!(st.status, OrderStatus::Rejected) {
                "failed"
            } else {
                "cancelled"
            };
            if st.filled_size > 0.0 {
                record_fill(state, order, venue_order_id, st.filled_size, st.filled_price, st.fee)
                    .await?;
                acct::update_copy_order_status(
                    &state.db,
                    order.id,
                    terminal,
                    Some(&format!("部分成交 {} 后终态 {:?}", st.filled_size, st.status)),
                )
                .await?;
            } else {
                acct::update_copy_order_status(
                    &state.db,
                    order.id,
                    terminal,
                    Some(&format!("订单 {:?}，无成交", st.status)),
                )
                .await?;
            }
            info!(order_id = %order.id, venue_order_id, status = ?st.status, "对账终态");
        }
        OrderStatus::Open | OrderStatus::PartiallyFilled => {
            // 仍在挂单。超时则撤单 + 置 cancelled（撤单后下一轮会拿到 Cancelled 终态）。
            let submitted_at = order.submitted_at.unwrap_or(now);
            if now - submitted_at > timeout {
                warn!(order_id = %order.id, venue_order_id, "submitted 超时未成交，撤单");
                if let Err(e) = venue.cancel_order(&cred, venue_order_id).await {
                    // 撤单失败：可能订单已成交/已撤，或 API 瞬态故障。留 submitted 下轮重查，
                    // 不本地臆断终态（避免与 Venue 端状态不一致）。
                    warn!(order_id = %order.id, error = %e, "撤单失败，留 submitted 下轮重查");
                }
            }
            // 未超时或撤单已发：留 submitted，下轮重查。
        }
    }
    Ok(())
}

/// 写真实成交到 copy_execution（与 exec.rs record_fill_with 同口径）。
async fn record_fill(
    state: &AppState,
    order: &sharpside_db::CopyOrderRow,
    venue_order_id: &str,
    filled_size: f64,
    filled_price: f64,
    fee: f64,
) -> Result<(), anyhow::Error> {
    let exec_market_id = order
        .execute_market_id
        .clone()
        .unwrap_or_else(|| order.source_market_id.clone());
    let exec_token_id = order
        .execute_token_id
        .clone()
        .unwrap_or_else(|| order.source_token_id.clone());
    acct::insert_copy_execution(
        &state.db,
        order.id,
        order.user_id,
        order.execute_venue.as_str(),
        &exec_market_id,
        &exec_token_id,
        Some(venue_order_id),
        order.side.as_str(),
        filled_size,
        filled_price,
        fee,
        None,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// 对账逻辑为薄层（查询 + Venue 调用 + 状态回写），核心原子性由 DB 层
    /// `update_copy_order_status` 的状态机保证，Venue 交互由集成测试覆盖；无需单测。
    #[test]
    fn reconcile_module_compiles() {}
}
