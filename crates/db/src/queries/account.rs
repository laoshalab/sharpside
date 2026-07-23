//! `account` schema 查询：users / follow_relation / copy_order / copy_execution /
//! user_venue_credentials。对应 `docs/ARCHITECTURE.md` §6.2-6.4 与 `docs/FLOWS.md` §4-7。
//!
//! account 服务管用户/订阅/凭证；follow 服务管跟随关系与信号派生（写 copy_order）；
//! copier 服务消费 copy_order、写 copy_execution。本模块提供三服务共用的持久化原语。

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::DbError;
use crate::models::{
    CopyExecution, CopyOrderRow, FollowRelation, Redemption, User, UserVenueCredential, UserWallet,
    Watchlist, Withdrawal,
};

// ── users ──
// 身份方式：TG（upsert_tg_user）或 钱包（upsert_wallet_user，见文件末尾 user_wallets 段）。
// 邮箱认证已移除（0015_drop_email_auth.sql）。

/// 创建/关联 TG 用户（无密码）。已存在则返回既有行。
pub async fn upsert_tg_user(pool: &PgPool, tg_id: i64) -> Result<User, DbError> {
    let row = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO account.users (tg_id)
        VALUES ($1)
        ON CONFLICT (tg_id) DO UPDATE SET updated_at = now()
        RETURNING *
        "#,
    )
    .bind(tg_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_user(pool: &PgPool, id: Uuid) -> Result<User, DbError> {
    let row = sqlx::query_as::<_, User>("SELECT * FROM account.users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| DbError::not_found(format!("user {id}")))?;
    Ok(row)
}

/// 更新订阅档位与到期。
pub async fn update_subscription(
    pool: &PgPool,
    id: Uuid,
    tier: &str,
    until: Option<DateTime<Utc>>,
) -> Result<User, DbError> {
    let row = sqlx::query_as::<_, User>(
        r#"
        UPDATE account.users SET
            subscription_tier  = $2,
            subscription_until = $3,
            updated_at         = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(tier)
    .bind(until)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("user {id}")))?;
    Ok(row)
}

/// 设置 daemon_api_key 的 hash（绝不存明文）。
pub async fn set_daemon_api_key_hash(
    pool: &PgPool,
    id: Uuid,
    key_hash: &str,
) -> Result<User, DbError> {
    let row = sqlx::query_as::<_, User>(
        r#"
        UPDATE account.users SET
            daemon_api_key_hash       = $2,
            daemon_api_key_rotated_at = now(),
            updated_at                = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(key_hash)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("user {id}")))?;
    Ok(row)
}

// ── follow_relation ──

/// 跟随单 Venue trader 的关系。
#[allow(clippy::too_many_arguments)]
pub async fn create_follow_trader(
    pool: &PgPool,
    user_id: Uuid,
    follow_platform: &str,
    follow_address: &str,
    execute_venue: &str,
    channel: &str,
    config: &serde_json::Value,
    same_venue_only: bool,
) -> Result<FollowRelation, DbError> {
    let row = sqlx::query_as::<_, FollowRelation>(
        r#"
        INSERT INTO account.follow_relation
            (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only)
        VALUES ($1,$2,$3,$4,$5,$6,$7)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(follow_platform)
    .bind(follow_address)
    .bind(execute_venue)
    .bind(channel)
    .bind(config)
    .bind(same_venue_only)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 跟随跨 Venue identity 的关系。
pub async fn create_follow_identity(
    pool: &PgPool,
    user_id: Uuid,
    follow_identity_id: Uuid,
    execute_venue: &str,
    channel: &str,
    config: &serde_json::Value,
    same_venue_only: bool,
) -> Result<FollowRelation, DbError> {
    let row = sqlx::query_as::<_, FollowRelation>(
        r#"
        INSERT INTO account.follow_relation
            (user_id, follow_identity_id, execute_venue, channel, config, same_venue_only)
        VALUES ($1,$2,$3,$4,$5,$6)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(follow_identity_id)
    .bind(execute_venue)
    .bind(channel)
    .bind(config)
    .bind(same_venue_only)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_follow(pool: &PgPool, id: Uuid) -> Result<FollowRelation, DbError> {
    let row =
        sqlx::query_as::<_, FollowRelation>("SELECT * FROM account.follow_relation WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| DbError::not_found(format!("follow {id}")))?;
    Ok(row)
}

/// 查找用户对某 trader 的活跃跟随关系（唯一性预检用）。返回 Some 即已存在。
pub async fn find_active_follow_trader(
    pool: &PgPool,
    user_id: Uuid,
    follow_platform: &str,
    follow_address: &str,
) -> Result<Option<FollowRelation>, DbError> {
    let row = sqlx::query_as::<_, FollowRelation>(
        r#"
        SELECT * FROM account.follow_relation
        WHERE user_id = $1 AND follow_platform = $2 AND follow_address = $3 AND active = true
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(follow_platform)
    .bind(follow_address)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 查找用户对某 identity 的活跃跟随关系（唯一性预检用）。返回 Some 即已存在。
pub async fn find_active_follow_identity(
    pool: &PgPool,
    user_id: Uuid,
    follow_identity_id: Uuid,
) -> Result<Option<FollowRelation>, DbError> {
    let row = sqlx::query_as::<_, FollowRelation>(
        r#"
        SELECT * FROM account.follow_relation
        WHERE user_id = $1 AND follow_identity_id = $2 AND active = true
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(follow_identity_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 列出某用户的活跃跟随关系。
pub async fn list_follows_by_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<FollowRelation>, DbError> {
    let rows = sqlx::query_as::<_, FollowRelation>(
        "SELECT * FROM account.follow_relation WHERE user_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出指向某 trader 的活跃跟随关系（信号派生用）。
pub async fn list_follows_of_trader(
    pool: &PgPool,
    follow_platform: &str,
    follow_address: &str,
) -> Result<Vec<FollowRelation>, DbError> {
    let rows = sqlx::query_as::<_, FollowRelation>(
        r#"
        SELECT * FROM account.follow_relation
        WHERE follow_platform = $1 AND follow_address = $2 AND active = true AND deleted_at IS NULL
        "#,
    )
    .bind(follow_platform)
    .bind(follow_address)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出指向某 identity 的活跃跟随关系（信号派生用）。
pub async fn list_follows_of_identity(
    pool: &PgPool,
    follow_identity_id: Uuid,
) -> Result<Vec<FollowRelation>, DbError> {
    let rows = sqlx::query_as::<_, FollowRelation>(
        "SELECT * FROM account.follow_relation WHERE follow_identity_id = $1 AND active = true AND deleted_at IS NULL",
    )
    .bind(follow_identity_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 更新跟随配置（active / config / execute_venue / channel）。
pub async fn update_follow(
    pool: &PgPool,
    id: Uuid,
    active: Option<bool>,
    config: Option<&serde_json::Value>,
    execute_venue: Option<&str>,
    channel: Option<&str>,
) -> Result<FollowRelation, DbError> {
    let row = sqlx::query_as::<_, FollowRelation>(
        r#"
        UPDATE account.follow_relation SET
            active        = COALESCE($2, active),
            config        = COALESCE($3, config),
            execute_venue = COALESCE($4, execute_venue),
            channel       = COALESCE($5, channel),
            deleted_at    = CASE WHEN COALESCE($2, active) = true THEN NULL ELSE deleted_at END,
            updated_at    = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(active)
    .bind(config)
    .bind(execute_venue)
    .bind(channel)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("follow {id}")))?;
    Ok(row)
}

/// 删除（置 active=false + deleted_at=now，归档；与「暂停」区分）。
pub async fn delete_follow(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query(
        "UPDATE account.follow_relation SET active = false, deleted_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!("follow {id}")));
    }
    Ok(())
}

// ── copy_order ──

/// 入队一条跟单指令（status=pending）。`id` 由调用方生成（便于 HTTP 返回）。
///
/// `signal_id` 为信号去重键（migration 0031）：同一 (signal_id, follow_relation_id) 命中
/// 唯一约束时返回 `DbError::Conflict`，调用方据此跳过（outbox 重发同信号不重复下单）。
/// 历史行 / 非 signal 派生传 None，不参与唯一约束。
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_copy_order(
    pool: &PgPool,
    id: Uuid,
    follow_relation_id: Uuid,
    user_id: Uuid,
    source_venue: &str,
    execute_venue: &str,
    source_market_id: &str,
    source_token_id: &str,
    side: &str,
    price: f64,
    size: f64,
    channel: &str,
    signal_at: DateTime<Utc>,
    skip_reason: Option<&str>,
    status: &str,
    signal_id: Option<&str>,
) -> Result<CopyOrderRow, DbError> {
    let price =
        rust_decimal::Decimal::try_from(price).map_err(|e| DbError::Invalid(e.to_string()))?;
    let size =
        rust_decimal::Decimal::try_from(size).map_err(|e| DbError::Invalid(e.to_string()))?;
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        INSERT INTO account.copy_order
            (id, follow_relation_id, user_id, source_venue, execute_venue,
             source_market_id, source_token_id, side, price, size, channel,
             signal_at, status, skip_reason, signal_id)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(follow_relation_id)
    .bind(user_id)
    .bind(source_venue)
    .bind(execute_venue)
    .bind(source_market_id)
    .bind(source_token_id)
    .bind(side)
    .bind(price)
    .bind(size)
    .bind(channel)
    .bind(signal_at)
    .bind(status)
    .bind(skip_reason)
    .bind(signal_id)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref de) if de.is_unique_violation() => {
            DbError::Conflict("copy_order signal_id 重复（已派生，跳过）".to_string())
        }
        other => other.into(),
    })?;
    Ok(row)
}

/// daemon 长轮询：取某用户某通道自 since 起的待派发指令。
pub async fn list_copy_orders_since(
    pool: &PgPool,
    user_id: Uuid,
    channel: &str,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<CopyOrderRow>, DbError> {
    let rows = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        SELECT * FROM account.copy_order
        WHERE user_id = $1 AND channel = $2 AND status = 'pending' AND enqueued_at >= $3
        ORDER BY enqueued_at ASC
        LIMIT $4
        "#,
    )
    .bind(user_id)
    .bind(channel)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出用户近期跟单指令（所有状态：filled/failed/skipped/dispatched/pending）。
/// 用于前端「近期跟单指令」视图，展示失败/跳过原因（skip_reason）。
/// 按 enqueued_at 倒序，取最近 limit 条。
pub async fn list_recent_copy_orders(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<CopyOrderRow>, DbError> {
    let rows = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        SELECT * FROM account.copy_order
        WHERE user_id = $1
        ORDER BY enqueued_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 更新指令状态（copier/daemon 回传结果用）。
pub async fn update_copy_order_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    skip_reason: Option<&str>,
) -> Result<CopyOrderRow, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        UPDATE account.copy_order SET
            status      = $2,
            skip_reason = COALESCE($3, skip_reason)
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(skip_reason)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("copy_order {id}")))?;
    Ok(row)
}

/// 原子 CAS 转终态：仅当当前 status ∈ {pending, dispatched} 时置为 `new_status`，返回抢占到的行；
/// 否则返回 None（已被先前上报 / worker 置终态 → 调用方幂等返回，不重复入账）。
///
/// 对应安全修复 1.4：`/result` 用此 CAS 抢占，重复上报幂等返回 200，不重复 insert copy_execution。
pub async fn claim_copy_order_status(
    pool: &PgPool,
    id: Uuid,
    new_status: &str,
    skip_reason: Option<&str>,
) -> Result<Option<CopyOrderRow>, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        UPDATE account.copy_order SET
            status      = $2,
            skip_reason = COALESCE($3, skip_reason)
        WHERE id = $1 AND status IN ('pending', 'dispatched')
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(new_status)
    .bind(skip_reason)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 原子抢占一条 pending 指令：`pending → dispatched`，仅当当前仍为 pending 才成功。
///
/// 多 copier worker 并发时，SELECT pending 后各自跑风控，最后用此 CAS 抢占：
/// `UPDATE ... WHERE id=$1 AND status='pending' RETURNING *`。只有一个 worker 能拿到行，
/// 其余拿到 None 即放弃（风控工作白做但绝不重复下单）。避免长事务跨网络 await 持锁。
/// 同时回写 `execute_market_id` / `execute_token_id`（跨 Venue 映射后的真实执行目标，供
/// copy_execution 记录与赎回对账使用；入队时这两列为 NULL）。
pub async fn claim_copy_order(
    pool: &PgPool,
    id: Uuid,
    exec_market_id: Option<&str>,
    exec_token_id: Option<&str>,
    idempotency_salt: i64,
    order_timestamp_ms: i64,
    exec_price: f64,
    exec_size: f64,
) -> Result<Option<CopyOrderRow>, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        UPDATE account.copy_order SET
            status             = 'dispatched',
            dispatched_at      = now(),
            execute_market_id  = COALESCE($2, execute_market_id),
            execute_token_id   = COALESCE($3, execute_token_id),
            idempotency_salt    = $4,
            order_timestamp_ms = $5,
            exec_price         = $6,
            exec_size          = $7
        WHERE id = $1 AND status = 'pending'
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(exec_market_id)
    .bind(exec_token_id)
    .bind(idempotency_salt)
    .bind(order_timestamp_ms)
    .bind(exec_price)
    .bind(exec_size)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// place_order 成功后置 `submitted`：持久化 Venue 返回的订单 ID + 提交时刻。
/// 由 reconcile worker 据此轮询 `Venue::order_state` 回写真实成交。
/// 用 `WHERE id=$1 AND status='dispatched'` CAS，避免与 reclaim worker 抢占冲突。
pub async fn mark_copy_order_submitted(
    pool: &PgPool,
    id: Uuid,
    venue_order_id: &str,
) -> Result<Option<CopyOrderRow>, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        UPDATE account.copy_order SET
            status         = 'submitted',
            submitted_at   = now(),
            venue_order_id = $2
        WHERE id = $1 AND status = 'dispatched'
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(venue_order_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 列出 submitted 状态的指令（reconcile worker 用），按提交时间升序。
pub async fn list_submitted_copy_orders(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<CopyOrderRow>, DbError> {
    let rows = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        SELECT * FROM account.copy_order
        WHERE status = 'submitted' AND venue_order_id IS NOT NULL
        ORDER BY submitted_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 安全修复 4.1：按 status 计数 copy_order（metrics）。
pub async fn count_copy_orders_by_status(pool: &PgPool, status: &str) -> Result<i64, DbError> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM account.copy_order WHERE status = $1",
    )
    .bind(status)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// pending/dispatched 队列最大龄期（秒）；无行返回 0。
pub async fn max_copy_order_age_secs(pool: &PgPool, status: &str) -> Result<f64, DbError> {
    let row: (Option<f64>,) = sqlx::query_as(
        r#"
        SELECT EXTRACT(EPOCH FROM (now() - MIN(created_at)))::float8
        FROM account.copy_order
        WHERE status = $1
        "#,
    )
    .bind(status)
    .fetch_one(pool)
    .await?;
    Ok(row.0.unwrap_or(0.0))
}

/// 列出 dispatched_at 早于 cutoff 的 dispatched 指令（reclaim worker 用）。
/// 这些指令疑似 copier 进程崩溃后卡死，需回收。
pub async fn list_stale_dispatched(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<CopyOrderRow>, DbError> {
    let rows = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        SELECT * FROM account.copy_order
        WHERE status = 'dispatched' AND dispatched_at IS NOT NULL AND dispatched_at < $1
        ORDER BY dispatched_at ASC
        LIMIT $2
        "#,
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 原子回收一条 dispatched 指令为 failed：仅当当前仍为 dispatched 才成功。
/// 不重试 place_order（无客户端幂等键，重试可能真钱重复下单）；
/// 仅置 failed + 原因，交人工核对 Venue 端是否已挂单后决定 filled/failed。
pub async fn reclaim_dispatched(
    pool: &PgPool,
    id: Uuid,
    reason: &str,
) -> Result<Option<CopyOrderRow>, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        UPDATE account.copy_order SET
            status      = 'failed',
            skip_reason = $2
        WHERE id = $1 AND status = 'dispatched'
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(reason)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 通道 B 回写执行目标（daemon 上报前补齐 execute_market_id/execute_token_id）。
/// 仅当列仍为 NULL 时写入，避免覆盖已 claim 的值。
pub async fn set_copy_order_exec_targets(
    pool: &PgPool,
    id: Uuid,
    exec_market_id: Option<&str>,
    exec_token_id: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE account.copy_order SET
            execute_market_id = COALESCE($2, execute_market_id),
            execute_token_id = COALESCE($3, execute_token_id)
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(exec_market_id)
    .bind(exec_token_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── copy_execution ──

#[allow(clippy::too_many_arguments)]
pub async fn insert_copy_execution(
    pool: &PgPool,
    copy_order_id: Uuid,
    user_id: Uuid,
    venue: &str,
    market_id: &str,
    token_id: &str,
    venue_order_id: Option<&str>,
    side: &str,
    filled_size: f64,
    filled_price: f64,
    fee: f64,
    tx_hash: Option<&str>,
) -> Result<Option<CopyExecution>, DbError> {
    // 安全修复 1.4：ON CONFLICT (copy_order_id) DO NOTHING —— 同一 copy_order
    // 重复上报 / 跨通道竞争时只写一条成交行。返回 None 表示已存在（幂等）。
    let to_dec = |v: f64| -> Result<rust_decimal::Decimal, DbError> {
        rust_decimal::Decimal::try_from(v).map_err(|e| DbError::Invalid(e.to_string()))
    };
    let row = sqlx::query_as::<_, CopyExecution>(
        r#"
        INSERT INTO account.copy_execution
            (copy_order_id, user_id, venue, market_id, token_id, venue_order_id,
             side, filled_size, filled_price, fee, tx_hash)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (copy_order_id) DO NOTHING
        RETURNING *
        "#,
    )
    .bind(copy_order_id)
    .bind(user_id)
    .bind(venue)
    .bind(market_id)
    .bind(token_id)
    .bind(venue_order_id)
    .bind(side)
    .bind(to_dec(filled_size)?)
    .bind(to_dec(filled_price)?)
    .bind(to_dec(fee)?)
    .bind(tx_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 用户态：列出某用户全部 copy_execution（任意状态），按 executed_at 降序。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.3 近期成交 / §6.6 仪表盘。
pub async fn list_copy_executions_for_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<CopyExecution>, DbError> {
    let rows = sqlx::query_as::<_, CopyExecution>(
        r#"
        SELECT * FROM account.copy_execution
        WHERE user_id = $1
        ORDER BY executed_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// copy_execution JOIN copy_order：取 signal_at + follow_relation_id（延迟统计与分跟随聚合用）。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.3 延迟分布 / 分跟随 P&L。
///
/// `#[serde(flatten)] exec` 与 sqlx `FromRow` derive 不兼容，故手动实现 `FromRow`。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CopyExecutionWithSignal {
    #[serde(flatten)]
    pub exec: CopyExecution,
    pub signal_at: Option<DateTime<Utc>>,
    pub follow_relation_id: Option<Uuid>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for CopyExecutionWithSignal {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let exec = CopyExecution {
            id: row.try_get("id")?,
            copy_order_id: row.try_get("copy_order_id")?,
            user_id: row.try_get("user_id")?,
            venue: row.try_get("venue")?,
            market_id: row.try_get("market_id")?,
            token_id: row.try_get("token_id")?,
            venue_order_id: row.try_get("venue_order_id")?,
            side: row.try_get("side")?,
            filled_size: row.try_get("filled_size")?,
            filled_price: row.try_get("filled_price")?,
            fee: row.try_get("fee")?,
            tx_hash: row.try_get("tx_hash")?,
            executed_at: row.try_get("executed_at")?,
        };
        Ok(Self {
            exec,
            signal_at: row.try_get("signal_at")?,
            follow_relation_id: row.try_get("follow_relation_id")?,
        })
    }
}

/// 列出某用户全部成交 + 关联 copy_order 的 signal_at / follow_relation_id。
/// 按 executed_at 升序（FIFO 仓位重建与权益曲线按时间累积）。
pub async fn list_copy_executions_with_signal(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<CopyExecutionWithSignal>, DbError> {
    let rows = sqlx::query_as::<_, CopyExecutionWithSignal>(
        r#"
        SELECT e.*, o.signal_at, o.follow_relation_id
        FROM account.copy_execution e
        LEFT JOIN account.copy_order o ON o.id = e.copy_order_id
        WHERE e.user_id = $1
        ORDER BY e.executed_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `copy_execution` JOIN `copy_order` 的用户态成交行，带过滤参数。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.11 成交历史筛选。
/// 过滤：since（executed_at >= since）/ follow_id（order.follow_relation_id）/ venue（e.venue）/ status（order.status）。
///
/// `#[serde(flatten)] exec` 与 sqlx `FromRow` derive 不兼容，故手动实现 `FromRow`。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CopyExecutionOut {
    #[serde(flatten)]
    pub exec: CopyExecution,
    pub follow_relation_id: Option<Uuid>,
    pub status: Option<String>,
    pub skip_reason: Option<String>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for CopyExecutionOut {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let exec = CopyExecution {
            id: row.try_get("id")?,
            copy_order_id: row.try_get("copy_order_id")?,
            user_id: row.try_get("user_id")?,
            venue: row.try_get("venue")?,
            market_id: row.try_get("market_id")?,
            token_id: row.try_get("token_id")?,
            venue_order_id: row.try_get("venue_order_id")?,
            side: row.try_get("side")?,
            filled_size: row.try_get("filled_size")?,
            filled_price: row.try_get("filled_price")?,
            fee: row.try_get("fee")?,
            tx_hash: row.try_get("tx_hash")?,
            executed_at: row.try_get("executed_at")?,
        };
        Ok(Self {
            exec,
            follow_relation_id: row.try_get("follow_relation_id")?,
            status: row.try_get("status")?,
            skip_reason: row.try_get("skip_reason")?,
        })
    }
}

/// 列出某用户成交（带过滤），按 executed_at DESC 分页。
pub async fn list_copy_executions_filtered(
    pool: &PgPool,
    user_id: Uuid,
    since: Option<DateTime<Utc>>,
    follow_id: Option<Uuid>,
    venue: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<CopyExecutionOut>, DbError> {
    let rows = sqlx::query_as::<_, CopyExecutionOut>(
        r#"
        SELECT e.*, o.follow_relation_id, o.status, o.skip_reason
        FROM account.copy_execution e
        LEFT JOIN account.copy_order o ON o.id = e.copy_order_id
        WHERE e.user_id = $1
          AND ($2::timestamptz IS NULL OR e.executed_at >= $2)
          AND ($3::uuid IS NULL OR o.follow_relation_id = $3)
          AND ($4::text IS NULL OR e.venue = $4)
          AND ($5::text IS NULL OR o.status = $5)
        ORDER BY e.executed_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(user_id)
    .bind(since)
    .bind(follow_id)
    .bind(venue)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取某 (user, platform) 的凭证 blob 原始 JSON（account 服务解析非密字段给 delegation 视图）。
pub async fn get_credential_blob(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT encrypted_blob FROM account.user_venue_credentials WHERE user_id = $1 AND platform = $2",
    )
    .bind(user_id)
    .bind(platform)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(b,)| b))
}

// ── user_venue_credentials ──

/// upsert 凭证前：若已有行则整包写入 `credential_archives`（re-provision 可追溯）。
/// 无现有行时为 no-op。返回是否归档了一行。
pub async fn archive_credential_if_exists(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
) -> Result<bool, DbError> {
    let res = sqlx::query(
        r#"
        INSERT INTO account.credential_archives (
            user_id, platform, kind, encrypted_blob, proxy_address,
            revoked_at, revoked_by, original_created_at, original_updated_at
        )
        SELECT user_id, platform, kind, encrypted_blob, proxy_address,
               revoked_at, revoked_by, created_at, updated_at
        FROM account.user_venue_credentials
        WHERE user_id = $1 AND platform = $2
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// 列出用户某平台的凭证归档（最近优先）。含密文 blob，调用方勿回传前端。
pub async fn list_credential_archives(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
    limit: i64,
) -> Result<Vec<crate::CredentialArchive>, DbError> {
    let lim = limit.clamp(1, 50);
    let rows = sqlx::query_as::<_, crate::CredentialArchive>(
        r#"
        SELECT * FROM account.credential_archives
        WHERE user_id = $1 AND platform = $2
        ORDER BY archived_at DESC
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .bind(lim)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取单条归档（须属该用户）。含密文 blob。
pub async fn get_credential_archive(
    pool: &PgPool,
    user_id: Uuid,
    archive_id: i64,
) -> Result<Option<crate::CredentialArchive>, DbError> {
    let row = sqlx::query_as::<_, crate::CredentialArchive>(
        r#"
        SELECT * FROM account.credential_archives
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(archive_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 取某 (user, platform) 的完整凭证行（含 revoked_at），供 provision 闸门判断。
pub async fn get_credential(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
) -> Result<Option<UserVenueCredential>, DbError> {
    let row = sqlx::query_as::<_, UserVenueCredential>(
        "SELECT * FROM account.user_venue_credentials WHERE user_id = $1 AND platform = $2",
    )
    .bind(user_id)
    .bind(platform)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// upsert 某 (user, platform) 的加密凭证 blob。
/// `kind` 从 `encrypted_blob.kind` 提取（缺省 `unknown`），写入列级公开字段。
pub async fn upsert_credential(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
    encrypted_blob: &serde_json::Value,
) -> Result<UserVenueCredential, DbError> {
    let kind = credential_kind_from_blob(encrypted_blob);
    let row = sqlx::query_as::<_, UserVenueCredential>(
        r#"
        INSERT INTO account.user_venue_credentials (user_id, platform, kind, encrypted_blob)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, platform) DO UPDATE SET
            kind           = excluded.kind,
            encrypted_blob = excluded.encrypted_blob,
            updated_at     = now(),
            -- 安全修复 2.2：重新预配 = 新 owner key 新凭证，重置撤销态（旧 revoke 不可逆，
            -- 但新凭证是全新密钥，应可激活）。
            revoked_at      = NULL,
            revoked_by      = NULL
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .bind(&kind)
    .bind(encrypted_blob)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// upsert 凭证 + `proxy_address`（DepositWalletDelegated 时存 deposit wallet 地址）。
/// 对应 `docs/CHANNEL_A_SIGNING.md` §2.2 / §3.1 step 9。
pub async fn upsert_credential_with_proxy(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
    encrypted_blob: &serde_json::Value,
    proxy_address: Option<&str>,
) -> Result<UserVenueCredential, DbError> {
    let kind = credential_kind_from_blob(encrypted_blob);
    let row = sqlx::query_as::<_, UserVenueCredential>(
        r#"
        INSERT INTO account.user_venue_credentials (user_id, platform, kind, encrypted_blob, proxy_address)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, platform) DO UPDATE SET
            kind           = excluded.kind,
            encrypted_blob = excluded.encrypted_blob,
            proxy_address  = excluded.proxy_address,
            updated_at     = now(),
            -- 安全修复 2.2：重新预配 = 新 owner key 新凭证，重置撤销态。
            revoked_at      = NULL,
            revoked_by      = NULL
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .bind(&kind)
    .bind(encrypted_blob)
    .bind(proxy_address)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 从 blob JSON 取 kind（公开字段）；缺省 `unknown`。
fn credential_kind_from_blob(blob: &serde_json::Value) -> String {
    blob.get("kind")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

pub async fn list_credentials(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<UserVenueCredential>, DbError> {
    let rows = sqlx::query_as::<_, UserVenueCredential>(
        "SELECT * FROM account.user_venue_credentials WHERE user_id = $1 ORDER BY platform",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── 凭证撤销（revoke，安全修复 2.2）──

/// 撤销某 (user, platform) 凭证：置 revoked_at=now()、revoked_by=user_id。不可逆。
/// 仅对当前活跃（revoked_at IS NULL）的凭证生效；已撤销则幂等返回（行不变）。
/// 返回更新后的行（含 revoked_at）；无此凭证返回 None。
pub async fn revoke_credential(
    pool: &PgPool,
    user_id: Uuid,
    platform: &str,
) -> Result<Option<UserVenueCredential>, DbError> {
    let row = sqlx::query_as::<_, UserVenueCredential>(
        r#"
        UPDATE account.user_venue_credentials SET
            revoked_at = now(),
            revoked_by = $1,
            updated_at = now()
        WHERE user_id = $1 AND platform = $2 AND revoked_at IS NULL
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── 风控统计（copier 用）──

/// 某用户自 `since` 起的累计成交 notional（filled_size * filled_price）。
pub async fn sum_daily_notional(
    pool: &PgPool,
    user_id: Uuid,
    since: DateTime<Utc>,
) -> Result<f64, DbError> {
    let row: Option<(sqlx::types::Decimal,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(filled_size * filled_price), 0)::numeric
        FROM account.copy_execution
        WHERE user_id = $1 AND executed_at >= $2
        "#,
    )
    .bind(user_id)
    .bind(since)
    .fetch_optional(pool)
    .await?;
    let v = row
        .map(|(d,)| d)
        .unwrap_or_default()
        .try_into()
        .unwrap_or(0.0f64);
    Ok(v)
}

/// 某用户自 `since` 起**真实尝试下单**的 copy_order 数（rapid-flip 守卫用）。
///
/// 只计 pending/dispatched/submitted/filled（真实进入下单流程的），
/// 排除 skipped/failed/cancelled（风控主动放弃或未成交的终态）。
/// 否则 skipped 会累加触发守卫 → 后续信号全 skip → 雪崩式"越 skip 越跟不上"。
pub async fn count_recent_copy_orders(
    pool: &PgPool,
    user_id: Uuid,
    since: DateTime<Utc>,
) -> Result<i64, DbError> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM account.copy_order
        WHERE user_id = $1 AND enqueued_at >= $2
          AND status IN ('pending', 'dispatched', 'submitted', 'filled')
        "#,
    )
    .bind(user_id)
    .bind(since)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(c,)| c).unwrap_or(0))
}

/// 某用户当前在途 + 已成交的 copy_order 数（近似持仓数，风控 max_open_positions 用）。
/// 用户当前真实开仓数：按 token 聚合净持仓（buy filled − sell filled），仅净持仓 ≠ 0 的 token 计为 1 个开仓。
///
/// 安全修复 1.5：旧实现 `status IN ('pending','dispatched','filled')` 把所有历史 filled
/// 都算活跃 → 一轮往返（buy 后 sell）后 open_positions 永不减，`max_open_positions` 永久耗尽。
/// 改以 `copy_execution`（真实成交）净持仓为准：buy 10 + sell 10 同 token → net 0 → 不计。
pub async fn count_active_copy_orders(pool: &PgPool, user_id: Uuid) -> Result<i64, DbError> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT count(*) FROM (
            SELECT token_id,
                   SUM(CASE WHEN side = 'buy' THEN filled_size ELSE -filled_size END) AS net
            FROM account.copy_execution
            WHERE user_id = $1
            GROUP BY token_id
            HAVING SUM(CASE WHEN side = 'buy' THEN filled_size ELSE -filled_size END) <> 0
        ) AS open_tokens
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(c,)| c).unwrap_or(0))
}

/// 某用户当前活跃（未删除）跟随数。Free 档槽位上限后端强制用。
pub async fn count_active_follows_by_user(pool: &PgPool, user_id: Uuid) -> Result<i64, DbError> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM account.follow_relation WHERE user_id = $1 AND active AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(c,)| c).unwrap_or(0))
}

/// 某跟随关系自 `since` 起的累计成交 notional（per-follow daily_max_notional 强制用）。
pub async fn sum_daily_notional_for_follow(
    pool: &PgPool,
    follow_relation_id: Uuid,
    since: DateTime<Utc>,
) -> Result<f64, DbError> {
    let row: Option<(sqlx::types::Decimal,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(e.filled_size * e.filled_price), 0)::numeric
        FROM account.copy_execution e
        JOIN account.copy_order o ON o.id = e.copy_order_id
        WHERE o.follow_relation_id = $1 AND e.executed_at >= $2
        "#,
    )
    .bind(follow_relation_id)
    .bind(since)
    .fetch_optional(pool)
    .await?;
    let v = row
        .map(|(d,)| d)
        .unwrap_or_default()
        .try_into()
        .unwrap_or(0.0f64);
    Ok(v)
}

/// 某跟随关系当前在途 + 已成交的 copy_order 数（per-follow max_open_positions 强制用）。
/// 某跟随关系当前真实开仓数：按 token 聚合净持仓（buy − sell），净持仓 ≠ 0 的 token 计 1。
///
/// 安全修复 1.5：与 `count_active_copy_orders` 同口径，旧实现把历史 filled 全计活跃，
/// 往返后永不减。改以 `copy_execution` 净持仓为准（join copy_order 取 follow_relation_id）。
pub async fn count_active_copy_orders_for_follow(
    pool: &PgPool,
    follow_relation_id: Uuid,
) -> Result<i64, DbError> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT count(*) FROM (
            SELECT e.token_id,
                   SUM(CASE WHEN e.side = 'buy' THEN e.filled_size ELSE -e.filled_size END) AS net
            FROM account.copy_execution e
            JOIN account.copy_order o ON o.id = e.copy_order_id
            WHERE o.follow_relation_id = $1
            GROUP BY e.token_id
            HAVING SUM(CASE WHEN e.side = 'buy' THEN e.filled_size ELSE -e.filled_size END) <> 0
        ) AS open_tokens
        "#,
    )
    .bind(follow_relation_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(c,)| c).unwrap_or(0))
}

/// 取一条 copy_order（含归属校验用）。
pub async fn get_copy_order(pool: &PgPool, id: Uuid) -> Result<CopyOrderRow, DbError> {
    let row = sqlx::query_as::<_, CopyOrderRow>("SELECT * FROM account.copy_order WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| DbError::not_found(format!("copy_order {id}")))?;
    Ok(row)
}

/// 取某通道下所有用户的待派发指令（copier 通道 A worker 用）。
///
/// `FOR UPDATE SKIP LOCKED`：多 copier worker 并发轮询时，已被其他 worker 锁定的行跳过，
/// 减少重复抓取与风控白做。最终仍由 [`claim_copy_order`] 的 CAS `pending→dispatched`
/// 兜底防重复下单；本锁仅为减少并发竞争的优化，非正确性依赖。
pub async fn list_pending_copy_orders(
    pool: &PgPool,
    channel: &str,
    limit: i64,
) -> Result<Vec<CopyOrderRow>, DbError> {
    let rows = sqlx::query_as::<_, CopyOrderRow>(
        r#"
        SELECT * FROM account.copy_order
        WHERE channel = $1 AND status = 'pending'
        ORDER BY enqueued_at ASC
        LIMIT $2
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(channel)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取某用户最近 `limit` 条 copy_order 的 status（按时间倒序），用于连续亏损/失败熔断判定。
pub async fn recent_copy_order_statuses(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<String>, DbError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT status FROM account.copy_order
        WHERE user_id = $1
        ORDER BY enqueued_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

// ── 钱包登录（模型 A · 身份钱包）──

/// 按钱包地址查用户（地址须小写）。不存在返回 None。
pub async fn get_user_by_wallet(pool: &PgPool, address: &str) -> Result<Option<User>, DbError> {
    let row = sqlx::query_as::<_, User>(
        r#"
        SELECT u.* FROM account.users u
        JOIN account.user_wallets w ON w.user_id = u.id
        WHERE w.address = $1
        "#,
    )
    .bind(address)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 钱包登录 upsert：按地址查用户；不存在则建裸 user + 钱包行（事务）。
///
/// `address` 须已规范化为小写 0x hex。新用户首钱包 `is_primary=true`。
pub async fn upsert_wallet_user(pool: &PgPool, address: &str) -> Result<User, DbError> {
    if let Some(user) = get_user_by_wallet(pool, address).await? {
        return Ok(user);
    }
    let mut tx = pool.begin().await?;
    let user = sqlx::query_as::<_, User>("INSERT INTO account.users DEFAULT VALUES RETURNING *")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO account.user_wallets (user_id, address, is_primary)
           VALUES ($1, $2, true)"#,
    )
    .bind(user.id)
    .bind(address)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(user)
}

/// 已登录用户绑定第二个钱包（恢复因子）。`address` 须小写。
/// 唯一约束冲突（地址已被他人绑定）返回 `DbError::Conflict`。
pub async fn link_wallet(
    pool: &PgPool,
    user_id: Uuid,
    address: &str,
    label: Option<&str>,
) -> Result<UserWallet, DbError> {
    let row = sqlx::query_as::<_, UserWallet>(
        r#"
        INSERT INTO account.user_wallets (user_id, address, label, is_primary)
        VALUES ($1, $2, $3, false)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(address)
    .bind(label)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref de) if de.is_unique_violation() => {
            DbError::Conflict(format!("wallet {address} 已被绑定"))
        }
        other => other.into(),
    })?;
    Ok(row)
}

/// 列出用户所有钱包。
pub async fn list_wallets(pool: &PgPool, user_id: Uuid) -> Result<Vec<UserWallet>, DbError> {
    let rows = sqlx::query_as::<_, UserWallet>(
        "SELECT * FROM account.user_wallets WHERE user_id = $1 ORDER BY is_primary DESC, linked_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 解绑钱包（仅当属于该用户）。返回是否删除了一行。
/// 不允许删除最后一个 primary 钱包（应用层在 handler 判断）。
pub async fn unlink_wallet(pool: &PgPool, user_id: Uuid, address: &str) -> Result<bool, DbError> {
    let res = sqlx::query("DELETE FROM account.user_wallets WHERE user_id = $1 AND address = $2")
        .bind(user_id)
        .bind(address)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// 统计用户钱包数（用于禁止删除最后一个）。
pub async fn count_wallets(pool: &PgPool, user_id: Uuid) -> Result<i64, DbError> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM account.user_wallets WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(n)
}

// ── auth_nonces（SIWE 防重放）──

/// 签发一次性 nonce（address 须小写）。
pub async fn issue_nonce(pool: &PgPool, address: &str, nonce: &str) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO account.auth_nonces (address, nonce) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(address)
    .bind(nonce)
    .execute(pool)
    .await?;
    Ok(())
}

/// 原子消费 nonce：仅当未消费且未过期时标记 consumed_at 并返回 true。
/// 防重放——同一 (address, nonce) 第二次返回 false；超过 `max_age_secs` 的行不可消费。
pub async fn consume_nonce(
    pool: &PgPool,
    address: &str,
    nonce: &str,
    max_age_secs: i64,
) -> Result<bool, DbError> {
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs.max(0));
    let res = sqlx::query(
        r#"UPDATE account.auth_nonces
           SET consumed_at = now()
           WHERE address = $1 AND nonce = $2 AND consumed_at IS NULL
             AND issued_at > $3"#,
    )
    .bind(address)
    .bind(nonce)
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// 清理过期 / 陈旧 nonce 行（issued_at 早于 max_age）。best-effort，在签发时顺带调用。
pub async fn cleanup_stale_nonces(pool: &PgPool, max_age_secs: i64) -> Result<u64, DbError> {
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs.max(0));
    let res = sqlx::query("DELETE FROM account.auth_nonces WHERE issued_at < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

// ── jwt_denylist（JWT 吊销，对应安全修复 1.2）──

/// 吊销一个 JWT：写入其 jti。已存在则幂等（ON CONFLICT DO NOTHING）。
pub async fn revoke_jwt(pool: &PgPool, jti: &str, user_id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO account.jwt_denylist (jti, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(jti)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询 jti 是否已被吊销。命中返回 true。点查走主键索引。
pub async fn is_jwt_revoked(pool: &PgPool, jti: &str) -> Result<bool, DbError> {
    let res = sqlx::query("SELECT 1 FROM account.jwt_denylist WHERE jti = $1")
        .bind(jti)
        .fetch_optional(pool)
        .await?;
    Ok(res.is_some())
}

// ── withdrawals（提现审计）──
// 对应 docs/CHANNEL_A_SIGNING.md §4.1。提现是高敏操作，全量落库审计。

/// 插入一笔提现记录（status=pending）。返回插入行。
pub async fn insert_withdrawal(
    pool: &PgPool,
    user_id: Uuid,
    venue: &str,
    asset: &str,
    amount: rust_decimal::Decimal,
    to_address: &str,
    relayer_tx_id: Option<&str>,
) -> Result<Withdrawal, DbError> {
    let row = sqlx::query_as::<_, Withdrawal>(
        r#"
        INSERT INTO account.withdrawals (user_id, venue, asset, amount, to_address, relayer_tx_id, status)
        VALUES ($1, $2, $3, $4, $5, $6, 'pending')
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(venue)
    .bind(asset)
    .bind(amount)
    .bind(to_address)
    .bind(relayer_tx_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 更新提现状态（确认/失败时调用）。`tx_hash` 为链上哈希（mined 时填）。
pub async fn update_withdrawal_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    tx_hash: Option<&str>,
    note: Option<&str>,
) -> Result<Withdrawal, DbError> {
    let row = sqlx::query_as::<_, Withdrawal>(
        r#"
        UPDATE account.withdrawals
        SET status = $2,
            tx_hash = COALESCE($3, tx_hash),
            note   = COALESCE($4, note)
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(tx_hash)
    .bind(note)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 列出用户提现历史（最近优先）。
pub async fn list_withdrawals(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Withdrawal>, DbError> {
    let rows = sqlx::query_as::<_, Withdrawal>(
        "SELECT * FROM account.withdrawals WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 用户当日（UTC）已发起提现总额（人类单位，pending+mined 计入，failed 不计）。
/// 用于日上限风控。返回 amount 之和（None = 无记录）。
pub async fn daily_withdrawal_total(
    pool: &PgPool,
    user_id: Uuid,
    venue: &str,
) -> Result<Option<rust_decimal::Decimal>, DbError> {
    let (total,): (Option<rust_decimal::Decimal>,) = sqlx::query_as(
        r#"
        SELECT SUM(amount)
        FROM account.withdrawals
        WHERE user_id = $1
          AND venue = $2
          AND status IN ('pending', 'mined')
          AND created_at >= date_trunc('day', now())
        "#,
    )
    .bind(user_id)
    .bind(venue)
    .fetch_one(pool)
    .await?;
    Ok(total)
}

// ── redemptions（赎回审计）──
// 对应 docs/CHANNEL_A_SIGNING.md §4.2 与 migration 0025。赎回 = 已结算市场赢仓位换 pUSD。
// 自动 worker（source=auto）与手动端点（source=manual）共用本表。

/// 插入一笔赎回记录（status=pending）。唯一约束冲突（同 user+condition+outcome+deposit_wallet
/// 已有 pending/mined）返回 `DbError::Conflict`，调用方据此跳过重复赎回。
pub async fn insert_redemption(
    pool: &PgPool,
    user_id: Uuid,
    venue: &str,
    condition_id: &str,
    outcome: &str,
    token_id: &str,
    amount: rust_decimal::Decimal,
    source: &str,
    deposit_wallet: &str,
) -> Result<Redemption, DbError> {
    sqlx::query_as::<_, Redemption>(
        r#"
        INSERT INTO account.redemptions
            (user_id, venue, condition_id, outcome, token_id, amount, source, status, deposit_wallet)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(venue)
    .bind(condition_id)
    .bind(outcome)
    .bind(token_id)
    .bind(amount)
    .bind(source)
    .bind(deposit_wallet)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref de) if de.is_unique_violation() => DbError::Conflict(format!(
            "redemption {venue}/{condition_id}/{outcome}/{deposit_wallet} 已存在（pending/mined）"
        )),
        other => other.into(),
    })
}

/// 更新赎回状态（确认/失败时调用）。`tx_hash` 为链上哈希（mined 时填）。
pub async fn update_redemption_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    tx_hash: Option<&str>,
    note: Option<&str>,
) -> Result<Redemption, DbError> {
    let row = sqlx::query_as::<_, Redemption>(
        r#"
        UPDATE account.redemptions
        SET status = $2,
            tx_hash = COALESCE($3, tx_hash),
            note   = COALESCE($4, note)
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(tx_hash)
    .bind(note)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 列出可重试的 failed 赎回：attempts < 3 且 next_attempt_at <= now。
/// worker 每轮扫一批，对每条先查链上 balanceOf（0 → 已赎回，标 mined），
/// 否则改回 pending（唯一约束防并发）→ venue.redeem → 成功 mined / 失败 mark_retry_failed。
pub async fn list_retryable_failed_redemptions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<Redemption>, DbError> {
    let rows = sqlx::query_as::<_, Redemption>(
        r#"
        SELECT * FROM account.redemptions
        WHERE status = 'failed' AND attempts < 3
          AND next_attempt_at IS NOT NULL AND next_attempt_at <= now()
        ORDER BY next_attempt_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 标记赎回重试失败：attempts+1，置 next_attempt_at（指数退避：30s/120s/300s）。
/// attempts 达 3 则不再设 next_attempt_at（停止自动重试，交人工）。
pub async fn mark_redemption_retry_failed(
    pool: &PgPool,
    id: Uuid,
    note: &str,
) -> Result<Redemption, DbError> {
    let row = sqlx::query_as::<_, Redemption>(
        r#"
        UPDATE account.redemptions
        SET status = 'failed',
            attempts = attempts + 1,
            next_attempt_at = CASE
                WHEN attempts + 1 >= 3 THEN NULL
                ELSE now() + (INTERVAL '30 second' * (1 << (attempts + 1)))
            END,
            note = $2
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(note)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 把 failed 赎回改回 pending（重试前），供 venue.redeem 重新发起。
/// 唯一约束 WHERE status IN ('pending','mined') 会防并发重复。
pub async fn revive_redemption_to_pending(
    pool: &PgPool,
    id: Uuid,
) -> Result<Redemption, DbError> {
    let row = sqlx::query_as::<_, Redemption>(
        r#"
        UPDATE account.redemptions
        SET status = 'pending'
        WHERE id = $1 AND status = 'failed'
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
pub async fn list_redemptions(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Redemption>, DbError> {
    let rows = sqlx::query_as::<_, Redemption>(
        "SELECT * FROM account.redemptions WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取用户在某市场某 outcome、某 deposit_wallet 是否已有 pending/mined 赎回。
pub async fn redemption_exists_active(
    pool: &PgPool,
    user_id: Uuid,
    condition_id: &str,
    outcome: &str,
    deposit_wallet: &str,
) -> Result<bool, DbError> {
    let (exists,): (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM account.redemptions
            WHERE user_id = $1 AND condition_id = $2 AND outcome = $3
              AND deposit_wallet = $4
              AND status IN ('pending', 'mined')
        )
        "#,
    )
    .bind(user_id)
    .bind(condition_id)
    .bind(outcome)
    .bind(deposit_wallet)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 取在某市场（condition_id）有跟单成交的全部去重 user_id。
/// 赎回自动 worker 候选集：这些用户可能持有该市场仓位（链上 balanceOf 兜底确认）。
pub async fn list_users_for_market(
    pool: &PgPool,
    venue: &str,
    condition_id: &str,
) -> Result<Vec<Uuid>, DbError> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT user_id
        FROM account.copy_execution
        WHERE venue = $1 AND market_id = $2
        "#,
    )
    .bind(venue)
    .bind(condition_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

// ── watchlist ──
// 用户观察名单（纯收藏，不进执行路径）。对应 Watchlist 功能规划。
// 与 follow_relation 物理隔离：信号派生（list_follows_of_*）不查询本表。

/// 收藏单 Venue trader。唯一约束冲突（同用户已收藏同目标）返回 `DbError::Conflict`。
pub async fn create_watchlist_trader(
    pool: &PgPool,
    user_id: Uuid,
    watch_platform: &str,
    watch_address: &str,
) -> Result<Watchlist, DbError> {
    let row = sqlx::query_as::<_, Watchlist>(
        r#"
        INSERT INTO account.watchlist (user_id, watch_platform, watch_address)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(watch_platform)
    .bind(watch_address)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref de) if de.is_unique_violation() => {
            DbError::Conflict(format!("watchlist {watch_platform}/{watch_address} 已收藏"))
        }
        other => other.into(),
    })?;
    Ok(row)
}

/// 收藏跨 Venue identity。唯一约束冲突返回 `DbError::Conflict`。
pub async fn create_watchlist_identity(
    pool: &PgPool,
    user_id: Uuid,
    watch_identity_id: Uuid,
) -> Result<Watchlist, DbError> {
    let row = sqlx::query_as::<_, Watchlist>(
        r#"
        INSERT INTO account.watchlist (user_id, watch_identity_id)
        VALUES ($1, $2)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(watch_identity_id)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref de) if de.is_unique_violation() => {
            DbError::Conflict(format!("watchlist identity {watch_identity_id} 已收藏"))
        }
        other => other.into(),
    })?;
    Ok(row)
}

pub async fn get_watchlist(pool: &PgPool, id: Uuid) -> Result<Watchlist, DbError> {
    let row = sqlx::query_as::<_, Watchlist>("SELECT * FROM account.watchlist WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| DbError::not_found(format!("watchlist {id}")))?;
    Ok(row)
}

/// 列出某用户的全部收藏（按收藏时间倒序）。
pub async fn list_watchlists_by_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<Watchlist>, DbError> {
    let rows = sqlx::query_as::<_, Watchlist>(
        "SELECT * FROM account.watchlist WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 用户收藏数（配额校验用）。
pub async fn count_watchlists_by_user(pool: &PgPool, user_id: Uuid) -> Result<i64, DbError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM account.watchlist WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// 删除收藏（硬删；升级为 Follow 时也走此函数消费掉本行）。
pub async fn delete_watchlist(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM account.watchlist WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!("watchlist {id}")));
    }
    Ok(())
}
