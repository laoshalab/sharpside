//! `account.signal_outbox` 查询。对应 H4 修复：信号投递 outbox + 重发。
//!
//! venue-hub hot worker emit 失败时调 [`enqueue_signal_outbox`] 落表；
//! signal_replay worker 用 [`list_due_outbox`] 取到期行，重发后调
//! [`mark_outbox_delivered`] / [`bump_outbox_attempt`]。

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::error::DbError;

/// outbox 行（replay worker用）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SignalOutboxRow {
    pub id: i64,
    pub signal_id: String,
    pub payload: Value,
    pub target_url: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

/// 落表一条待重发信号。signal_id 唯一：已存在则不重复入（DO NOTHING）。
/// 返回是否实际插入（false = 同 signal_id 已有未投递行，忽略即可）。
pub async fn enqueue_signal_outbox(
    pool: &PgPool,
    signal_id: &str,
    payload: &Value,
    target_url: &str,
    max_attempts: i32,
) -> Result<bool, DbError> {
    let res = sqlx::query(
        r#"
        INSERT INTO account.signal_outbox (signal_id, payload, target_url, max_attempts)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (signal_id) DO NOTHING
        "#,
    )
    .bind(signal_id)
    .bind(payload)
    .bind(target_url)
    .bind(max_attempts)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// 取到期、未投递、未死信的待重发行（按 next_attempt_at 升序，限 limit）。
pub async fn list_due_outbox(pool: &PgPool, limit: i64) -> Result<Vec<SignalOutboxRow>, DbError> {
    let rows = sqlx::query_as::<_, SignalOutboxRow>(
        r#"
        SELECT id, signal_id, payload, target_url, attempts, max_attempts,
               next_attempt_at, last_error
        FROM account.signal_outbox
        WHERE delivered_at IS NULL AND deadlettered_at IS NULL AND next_attempt_at <= now()
        ORDER BY next_attempt_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 标记成功投递。
pub async fn mark_outbox_delivered(pool: &PgPool, id: i64) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE account.signal_outbox SET delivered_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 重发失败：attempts += 1，记 last_error，按指数退避排 next_attempt_at。
/// 若 attempts 达到 max_attempts，置 deadlettered_at（停止后续重发）。
pub async fn bump_outbox_attempt(
    pool: &PgPool,
    id: i64,
    max_attempts: i32,
    error: &str,
    backoff_secs: i64,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE account.signal_outbox SET
            attempts        = attempts + 1,
            last_error      = $2,
            next_attempt_at = now() + make_interval(secs => $3),
            deadlettered_at = CASE WHEN attempts + 1 >= $4 THEN now() ELSE NULL END
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .bind(backoff_secs)
    .bind(max_attempts)
    .execute(pool)
    .await?;
    Ok(())
}

/// 未投递且未死信的 outbox 深度（Prometheus / readyz）。
pub async fn count_pending_outbox(pool: &PgPool) -> Result<i64, DbError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM account.signal_outbox
        WHERE delivered_at IS NULL AND deadlettered_at IS NULL
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// 死信条数。
pub async fn count_deadletter_outbox(pool: &PgPool) -> Result<i64, DbError> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM account.signal_outbox WHERE deadlettered_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
