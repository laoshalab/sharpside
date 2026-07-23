//! dispatched 超时回收 worker。对应 `docs/FLOWS.md` §6（P0-4 卡单回收）。
//!
//! 通道 A 在 `place_order` 前置 `status=dispatched` 占单锁。若 copier 进程在
//! dispatched 之后、`record_fill` 之前崩溃，指令会永久卡在 dispatched。
//!
//! 回收策略（保守，绝不重下）：
//! - 周期扫 `status='dispatched' AND dispatched_at < now() - dispatched_timeout_secs`
//! - 原子置 `failed` + 原因，交人工核对 Venue 端是否已挂单后决定真实状态
//! - **不重试 `place_order`**：跟单无客户端幂等键，重试可能在 Venue 端重复下单（真钱损失）
//!
//! 设计要点：
//! - `dispatched_at` 由 `claim_copy_order` 写入（migration 0029），精确反映占单时刻
//! - 用 `enqueued_at` 判超时会误伤 pending 等待较久才被 claim 的正常单
//! - 回收用 `WHERE status='dispatched'` CAS，避免与在途 copier 抢占冲突
//! - 纯清理操作（不涉及真钱），默认启用

use crate::state::AppState;
use sharpside_db::queries::account as acct;
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
        "dispatched 回收 worker 启动"
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
    info!(n, cutoff = %cutoff, "扫到超时 dispatched 指令，开始回收");
    let reason = format!(
        "dispatched 超时（>{}s），疑似进程崩溃，需人工核对 Venue 端是否已挂单",
        state.config.dispatched_timeout_secs
    );
    let mut reclaimed = 0usize;
    for order in stale {
        match acct::reclaim_dispatched(&state.db, order.id, &reason).await {
            Ok(Some(_)) => {
                reclaimed += 1;
                warn!(
                    order_id = %order.id,
                    user_id = %order.user_id,
                    execute_venue = %order.execute_venue,
                    "回收 dispatched 超时指令为 failed（交人工核对）"
                );
            }
            Ok(None) => {
                // 已被在途 copier 推进到 filled/failed，或已被并发回收，跳过
                info!(order_id = %order.id, "dispatched 指令已离开该状态，跳过回收");
            }
            Err(e) => {
                error!(order_id = %order.id, error = %e, "回收 dispatched 指令失败");
            }
        }
    }
    if reclaimed > 0 {
        info!(n, reclaimed, "dispatched 回收完成");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// 回收逻辑为薄层（查询 + CAS 更新），核心原子性由 DB 层 `reclaim_dispatched`
    /// 的 `WHERE status='dispatched'` 保证，无需单测；集成验证依赖真实 DB。
    #[test]
    fn reclaim_module_compiles() {}
}
