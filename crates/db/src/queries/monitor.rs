//! `trader_hub.hot_wallets` / `trader_positions_snapshot` 查询。
//! 对应 `docs/VENUEHUB_STORAGE.md` §7 与 `docs/VENUE_DESIGN.md` §8。
//!
//! hot worker 读热钥清单 → 抓浮仓 → 写快照（自适应频率由 scan_interval_secs 控制）。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::error::DbError;
use crate::models::{HotWallet, PositionSnapshot, SignalTarget};

/// 列出某 Venue 已启用的热钥，按 priority 降序。
pub async fn list_enabled_hot_wallets(
    pool: &PgPool,
    platform: &str,
) -> Result<Vec<HotWallet>, DbError> {
    let rows = sqlx::query_as::<_, HotWallet>(
        "SELECT * FROM trader_hub.hot_wallets \
         WHERE platform = $1 AND enabled = true ORDER BY priority DESC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出某 Venue 的全部信号监控目标：热钥 ∪ 活跃直接跟随 ∪ 活跃 identity 跟随下的各 trader。
/// 同一 address 去重；附 `identity_id`（来自 trader_hub.traders）以让 identity 跟随命中。
pub async fn list_signal_targets(
    pool: &PgPool,
    platform: &str,
) -> Result<Vec<SignalTarget>, DbError> {
    let rows = sqlx::query_as::<_, SignalTarget>(
        r#"
        WITH direct AS (
            SELECT follow_address AS address
            FROM account.follow_relation
            WHERE active AND deleted_at IS NULL
              AND follow_platform = $1 AND follow_address IS NOT NULL
        ),
        ident_traders AS (
            SELECT t.address
            FROM account.follow_relation fr
            JOIN trader_hub.traders t
              ON t.identity_id = fr.follow_identity_id AND t.platform = $1
            WHERE fr.active AND fr.deleted_at IS NULL AND fr.follow_identity_id IS NOT NULL
        ),
        hot AS (
            SELECT address FROM trader_hub.hot_wallets WHERE platform = $1 AND enabled = true
        ),
        all_addr AS (
            SELECT address FROM direct
            UNION
            SELECT address FROM ident_traders
            UNION
            SELECT address FROM hot
        )
        SELECT a.address, t.identity_id
        FROM all_addr a
        LEFT JOIN trader_hub.traders t
          ON t.platform = $1 AND t.address = a.address
        "#,
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 写一条热钥浮仓快照（append-only，主键含 captured_at）。
#[allow(clippy::too_many_arguments)]
pub async fn insert_position_snapshot(
    pool: &PgPool,
    platform: &str,
    address: &str,
    token_id: &str,
    condition_id: Option<&str>,
    size: f64,
    avg_price: f64,
    current_price: f64,
    pnl: f64,
    captured_at: DateTime<Utc>,
) -> Result<(), DbError> {
    let to_dec = |v: f64| -> Result<Decimal, DbError> {
        Decimal::try_from(v).map_err(|e| DbError::Invalid(e.to_string()))
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_positions_snapshot
            (platform, address, token_id, condition_id, size, avg_price,
             current_price, pnl, captured_at)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(token_id)
    .bind(condition_id)
    .bind(to_dec(size)?)
    .bind(to_dec(avg_price)?)
    .bind(to_dec(current_price)?)
    .bind(to_dec(pnl)?)
    .bind(captured_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 列出某热钥最近一次抓取的浮仓快照（按 captured_at 降序取每个 token_id 最新）。
pub async fn latest_snapshots(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Vec<PositionSnapshot>, DbError> {
    let rows = sqlx::query_as::<_, PositionSnapshot>(
        r#"
        SELECT DISTINCT ON (token_id) * FROM trader_hub.trader_positions_snapshot
        WHERE platform = $1 AND address = $2
        ORDER BY token_id, captured_at DESC
        "#,
    )
    .bind(platform)
    .bind(address)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── admin: 热钥 per Venue 管理 ──

/// 新增热钥。`(platform, address)` 唯一；已存在则更新 priority/scan_interval/enabled。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_hot_wallet(
    pool: &PgPool,
    platform: &str,
    address: &str,
    added_by: &str,
    priority: i32,
    scan_interval_secs: i32,
    enabled: bool,
) -> Result<HotWallet, DbError> {
    let row = sqlx::query_as::<_, HotWallet>(
        r#"
        INSERT INTO trader_hub.hot_wallets
            (platform, address, added_by, priority, scan_interval_secs, enabled)
        VALUES ($1,$2,$3,$4,$5,$6)
        ON CONFLICT (platform, address) DO UPDATE SET
            priority           = excluded.priority,
            scan_interval_secs = excluded.scan_interval_secs,
            enabled            = excluded.enabled
        RETURNING *
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(added_by)
    .bind(priority)
    .bind(scan_interval_secs)
    .bind(enabled)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 删除热钥。
pub async fn delete_hot_wallet(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<(), DbError> {
    let res =
        sqlx::query("DELETE FROM trader_hub.hot_wallets WHERE platform = $1 AND address = $2")
            .bind(platform)
            .bind(address)
            .execute(pool)
            .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!(
            "hot_wallet {platform}/{address}"
        )));
    }
    Ok(())
}

/// 列出某 Venue 全部热钥（含禁用，admin 用）。
pub async fn list_all_hot_wallets(
    pool: &PgPool,
    platform: &str,
) -> Result<Vec<HotWallet>, DbError> {
    let rows = sqlx::query_as::<_, HotWallet>(
        "SELECT * FROM trader_hub.hot_wallets WHERE platform = $1 ORDER BY priority DESC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
