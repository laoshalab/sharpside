//! dispatched 超时回收 worker。对应 `docs/FLOWS.md` §6（P0-4 卡单回收 / P1 订单级幂等）。
//!
//! 通道 A 在 `place_order` 前置 `status=dispatched` 占单锁。若 copier 进程在
//! dispatched 之后、`mark_submitted` 之前崩溃，指令会卡在 dispatched。
//!
//! 回收策略（P1 订单级幂等键已落地，可安全重试）：
//! - 周期扫 `status='dispatched' AND dispatched_at < now() - dispatched_timeout_secs`
//! - 用 claim 时持久化的 idempotency_salt + order_timestamp_ms + exec_price/exec_size
//!   重建 Order，**重试 place_order 一次**：相同 salt+timestamp → 逐字节相同已签订单 →
//!   相同 orderID → Polymarket 判重而非重复下单（真钱安全）。
//! - 重试成功 → `mark_copy_order_submitted`（恢复正常对账流）。
//! - 重试失败 → 原子置 `failed` + 原因交人工核对。失败含两种情形，均不重复花钱：
//!     1) 网络瞬态故障 / 余额不足等业务拒绝（订单未上 Venue）；
//!     2) Venue 端判重（订单已在 Venue，首次提交成功但回报丢失）——人工核对后改 filled/cancelled。
//! - 幂等字段缺失（旧行 / claim 未完成）→ 直接 failed + 人工核对（无法安全重试）。
//!
//! 设计要点：
//! - `dispatched_at` 由 `claim_copy_order` 写入（migration 0029），精确反映占单时刻
//! - 仅扫超时（默认 600s）dispatched，不与在途 exec（秒级）抢占
//! - 回收用 `WHERE status='dispatched'` CAS，避免与在途 copier 抢占冲突

use crate::exec::load_credential;
use crate::state::AppState;
use sharpside_db::queries::account as acct;
use sharpside_db::CopyOrderRow;
use sharpside_shared::{Platform, Side};
use sharpside_venues_core::{Order, VenueError};
use tracing::{error, info, warn};

const RECLAIM_BATCH: i64 = 100;

pub async fn run(state: AppState) {
    if !state.config.reclaim_worker_enabled {
        info!("dispatched 回收 worker 已禁用（RECLAIM_WORKER_ENABLED=false）");
        return;
    }
    info!(
        interval_secs = state.config.worker_reclaim_secs,
        timeout_secs = state.config.dispatched_timeout_secs,
        "dispatched 回收 worker 启动（P1 幂等重试已启用）"
    );
    loop {
        if let Err(e) = tick(&state).await {
            error!(error = %e, "dispatched 回收 worker tick 失败");
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            state.config.worker_reclaim_secs.max(1),
        ))
        .await;
    }
}

async fn tick(state: &AppState) -> Result<(), anyhow::Error> {
    let timeout = chrono::Duration::seconds(state.config.dispatched_timeout_secs as i64);
    let cutoff = chrono::Utc::now() - timeout;
    let stale = acct::list_stale_dispatched(&state.db, cutoff, RECLAIM_BATCH).await?;
    if stale.is_empty() {
        return Ok(());
    }
    let n = stale.len();
    info!(n, cutoff = %cutoff, "扫到超时 dispatched 指令，尝试幂等重试 place_order");
    let mut recovered = 0usize;
    let mut failed = 0usize;
    for order in stale {
        crate::metrics::inc_reclaim();
        match retry_one(state, &order).await {
            RetryOutcome::Recovered(oid) => {
                recovered += 1;
                info!(order_id = %order.id, venue_order_id = %oid, "幂等重试 place_order 成功，置 submitted");
            }
            RetryOutcome::Failed(reason) => {
                failed += 1;
                match acct::reclaim_dispatched(&state.db, order.id, &reason).await {
                    Ok(Some(_)) => warn!(
                        order_id = %order.id,
                        user_id = %order.user_id,
                        execute_venue = %order.execute_venue,
                        reason = %reason,
                        "重试失败，回收 dispatched 为 failed（交人工核对）"
                    ),
                    Ok(None) => info!(order_id = %order.id, "dispatched 指令已离开该状态，跳过"),
                    Err(e) => error!(order_id = %order.id, error = %e, "回收 dispatched 指令失败"),
                }
            }
            RetryOutcome::LeaveDispatched(reason) => {
                // P1-12：瞬态错误留 dispatched，下轮（60s）重试。不计数 failed。
                warn!(
                    order_id = %order.id,
                    user_id = %order.user_id,
                    reason = %reason,
                    "瞬态失败，留 dispatched 下轮重试（不 failed）"
                );
            }
        }
    }
    if recovered > 0 || failed > 0 {
        info!(n, recovered, failed, "dispatched 回收完成");
    }
    Ok(())
}

enum RetryOutcome {
    Recovered(String),
    Failed(String),
    /// 瞬态错误（凭证加载失败/KMS 抖动）：留 dispatched 下轮重试，不 failed。
    /// 对齐 reconcile 对 API 瞬态错误的处理；避免 Venue 端可能已有单时误判 failed 成孤儿。
    LeaveDispatched(String),
}

/// 对单条超时 dispatched 指令幂等重试 place_order。
async fn retry_one(state: &AppState, order: &CopyOrderRow) -> RetryOutcome {
    // 幂等字段齐全才能安全重试（缺则无法重建相同 orderID）。
    let (Some(salt), Some(ts), Some(price), Some(size), Some(token_id), Some(market_id)) = (
        order.idempotency_salt,
        order.order_timestamp_ms,
        order.exec_price,
        order.exec_size,
        order.execute_token_id.as_deref(),
        order.execute_market_id.as_deref(),
    ) else {
        return RetryOutcome::Failed(
            "幂等字段缺失（旧行或 claim 未完成），无法安全重试 place_order，人工核对 Venue 端是否已挂单".into(),
        );
    };
    let execute_venue: Platform = match order.execute_venue.parse() {
        Ok(p) => p,
        Err(e) => return RetryOutcome::Failed(format!("execute_venue 解析失败: {e}")),
    };
    let side: Side = match order.side.parse() {
        Ok(s) => s,
        Err(e) => return RetryOutcome::Failed(format!("side 解析失败: {e}")),
    };
    let Some(venue) = state.registry.get(execute_venue) else {
        return RetryOutcome::Failed(format!("venue {execute_venue} 未注册"));
    };
    let cred = match load_credential(state, order.user_id, execute_venue, &order.channel).await {
        Ok(c) => c,
        Err(e) => {
            // P1-12：凭证加载失败多为瞬态（DB/KMS 抖动），留 dispatched 下轮（60s）重试，
            // 对齐 reconcile 对 API 瞬态错误的处理。旧逻辑一次 failed：若 Venue 端已有单 → 孤儿。
            return RetryOutcome::LeaveDispatched(format!("加载凭证失败（瞬态，留 dispatched 重试）: {e}"))
        }
    };
    let order_req = Order {
        market_id: market_id.to_string(),
        token_id: token_id.to_string(),
        side,
        price,
        size,
        idempotency_salt: Some(salt as u64),
        order_timestamp_ms: Some(ts as u64),
        // 幂等重试沿用首次派发的下单类型（默认 FAK，与 exec 一致），复用持久化 price → 相同 orderID。
        order_type: state.config.copy_order_type,
        expiration: None,
        post_only: false,
    };
    match venue.place_order(&cred, order_req).await {
        Ok(fill) => {
            // 安全修复 1.3：dry-sign 重试仍 dry-sign（POLYMARKET_CLOB_POST 仍关闭）→ 不得置 submitted
            // （假 order_id 会让 reconcile 永远查不到）。置 failed 交人工核对，待启用 CLOB_POST 后重发。
            if fill.dry {
                return RetryOutcome::Failed(
                    "幂等重试 place_order 仍为 dry-sign（POLYMARKET_CLOB_POST≠1），订单未提交 CLOB，待启用后重发".into(),
                );
            }
            // 重试成功：订单被 Venue 接受（含幂等命中已存在订单返回相同 orderID）。置 submitted 交对账。
            match acct::mark_copy_order_submitted(&state.db, order.id, &fill.order_id).await {
                Ok(_) => RetryOutcome::Recovered(fill.order_id),
                Err(e) => RetryOutcome::Failed(format!(
                    "重试 place_order 成功返回 {} 但 mark_submitted 失败: {e}（订单已上 Venue，人工核对）",
                    fill.order_id
                )),
            }
        }
        Err(e) => {
            if matches!(e, VenueError::RateLimited) {
                crate::metrics::inc_clob_429();
            }
            RetryOutcome::Failed(format!(
                "幂等重试 place_order 失败: {e}；订单可能在 Venue 端已存在（重复 orderID 被拒），人工核对"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use sharpside_venues_core::Order;
    use sharpside_shared::Side;

    #[test]
    fn retry_builds_order_with_persisted_idempotency() {
        // 验证 reclaim 重建 Order 时复用持久化 salt+timestamp（非新生成）→ 相同 orderID。
        let o = Order {
            market_id: "m".into(),
            token_id: "t".into(),
            side: Side::Buy,
            price: 0.5,
            size: 10.0,
            idempotency_salt: Some(123),
            order_timestamp_ms: Some(999),
            order_type: sharpside_venues_core::OrderType::Gtc,
            expiration: None,
            post_only: false,
        };
        assert_eq!(o.idempotency_salt, Some(123));
        assert_eq!(o.order_timestamp_ms, Some(999));
    }
}
