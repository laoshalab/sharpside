//! signal_replay worker — 重发 outbox 中未投递的信号（H4 修复）。
//!
//! 对应 `docs/FLOWS.md` §5 信号投递可靠性。hot worker emit 失败的信号落
//! `account.signal_outbox`，本 worker 周期性扫到期行重发到 follow `/internal/signals`：
//! - 成功（2xx）→ `delivered_at = now()`
//! - 失败 → `attempts += 1` + 指数退避 `next_attempt_at`；达 `max_attempts` 置死信
//!   （`deadlettered_at`，停止重发并 error 告警，交人工核对）
//!
//! 幂等保证：follow 侧 `copy_order (signal_id, follow_relation_id)` 唯一约束，
//! 重发同信号命中即跳过，绝不重复下单。

use crate::state::AppState;
use std::time::Duration;

/// 每 tick 最多重发的行数（避免单轮拉满 DB / follow）。
const REPLAY_BATCH: i64 = 50;
/// 退避上限（秒）。
const BACKOFF_CAP_SECS: i64 = 3600;

pub async fn run(state: AppState) {
    let interval = state.config.workers.signal_replay_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        if let Err(e) = replay_once(&state).await {
            tracing::warn!(error = %e, "signal_replay 本轮失败，等下一周期");
        }
    }
}

async fn replay_once(state: &AppState) -> Result<(), anyhow::Error> {
    let follow_url = state.config.follow_url.trim();
    if follow_url.is_empty() {
        return Ok(());
    }
    let secret = state.config.follow_signal_secret.trim();
    let rows = sharpside_db::queries::outbox::list_due_outbox(&state.db, REPLAY_BATCH).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let n = rows.len();
    for row in rows {
        let mut req = state.http.post(&row.target_url).json(&row.payload);
        if !secret.is_empty() {
            req = req.header("x-internal-secret", secret);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Err(e) =
                    sharpside_db::queries::outbox::mark_outbox_delivered(&state.db, row.id).await
                {
                    tracing::warn!(error = %e, outbox_id = row.id, "mark_outbox_delivered 失败");
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let err = format!("HTTP {status}: {}", body.chars().take(200).collect::<String>());
                bump(state, row, err).await;
            }
            Err(e) => {
                bump(state, row, format!("send error: {e}")).await;
            }
        }
    }
    tracing::info!(n, "signal_replay 本轮处理完成");
    Ok(())
}

/// 退避重排或置死信。指数退避 = min(2^attempts, CAP)，attempts 从 1 起。
async fn bump(state: &AppState, row: sharpside_db::queries::outbox::SignalOutboxRow, err: String) {
    let next_attempts = row.attempts + 1;
    let backoff = (1i64 << next_attempts.min(20)).min(BACKOFF_CAP_SECS);
    if let Err(e) = sharpside_db::queries::outbox::bump_outbox_attempt(
        &state.db,
        row.id,
        row.max_attempts,
        &err,
        backoff,
    )
    .await
    {
        tracing::error!(error = %e, outbox_id = row.id, "bump_outbox_attempt 失败");
        return;
    }
    if next_attempts >= row.max_attempts {
        tracing::error!(
            outbox_id = row.id,
            signal_id = %row.signal_id,
            attempts = next_attempts,
            error = %err,
            "信号重发达上限，置死信，需人工核对"
        );
    } else {
        tracing::warn!(
            outbox_id = row.id,
            signal_id = %row.signal_id,
            attempts = next_attempts,
            error = %err,
            backoff_secs = backoff,
            "信号重发失败，退避重排"
        );
    }
}
