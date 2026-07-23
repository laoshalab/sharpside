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

/// 列出某 Venue **到期**的信号监控目标：热钥 ∪ 活跃直接跟随 ∪ 活跃 identity 跟随下的各 trader。
///
/// 自适应扫描（Phase B）：只返回"到期"的目标——`last_scanned_at IS NULL`（从未扫描，bootstrap）
/// 或 `last_scanned_at + interval_secs <= now()`。`last_scanned_at` 派生自 `trader_positions_snapshot`
/// 的 `max(captured_at)`（每次扫描都写快照，故可作 last_scanned 代理，且对热钥/跟随统一生效，无需新列）。
/// `interval_secs`：热钥取 `hot_wallets.scan_interval_secs`，跟随类取 `$2`（全局 `follow_scan_secs`）。
/// `due_cap`（$3）限制每 tick 最多取多少到期目标，防 bootstrap 风暴 + 平滑突发。
/// 同一 address 去重；附 `identity_id`（来自 trader_hub.traders）以让 identity 跟随命中。
pub async fn list_due_signal_targets(
    pool: &PgPool,
    platform: &str,
    follow_interval_secs: i32,
    due_cap: i64,
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
            SELECT address, scan_interval_secs FROM trader_hub.hot_wallets WHERE platform = $1 AND enabled = true
        ),
        all_addr AS (
            SELECT address FROM direct
            UNION
            SELECT address FROM ident_traders
            UNION
            SELECT address FROM hot
        ),
        addr_meta AS (
            SELECT a.address,
                   COALESCE(h.scan_interval_secs, $2) AS interval_secs,
                   (
                       SELECT MAX(s.captured_at)
                       FROM trader_hub.trader_positions_snapshot s
                       WHERE s.platform = $1 AND s.address = a.address
                   ) AS last_scanned_at
            FROM all_addr a
            LEFT JOIN trader_hub.hot_wallets h
              ON h.platform = $1 AND h.address = a.address
        )
        SELECT m.address, t.identity_id, m.interval_secs, m.last_scanned_at
        FROM addr_meta m
        LEFT JOIN trader_hub.traders t
          ON t.platform = $1 AND t.address = m.address
        WHERE m.last_scanned_at IS NULL
           OR m.last_scanned_at + m.interval_secs * interval '1 second' <= now()
        ORDER BY m.last_scanned_at NULLS FIRST
        LIMIT $3
        "#,
    )
    .bind(platform)
    .bind(follow_interval_secs)
    .bind(due_cap)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 第 3 层：列出某 Venue 的**全部**信号监控目标（热钥 ∪ 活跃直接跟随 ∪ 活跃 identity 跟随下的 trader），
/// **不做 due/interval 过滤**——trade_watch 每 tick 全量轮询，由 Venue client governor 限流。
///
/// 与 `list_due_signal_targets` 的差异：后者按 `last_scanned_at + interval` 到期过滤（positions diff 节流用）；
/// 本函数返回全部目标，trade_watch 用 `latest_trade_ts` 作增量游标，不依赖 position 快照时间。
pub async fn list_all_signal_targets(
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
        SELECT a.address, t.identity_id,
               COALESCE((SELECT scan_interval_secs FROM trader_hub.hot_wallets
                         WHERE platform = $1 AND address = a.address), 30) AS interval_secs,
               NULL::timestamptz AS last_scanned_at
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

/// R1-C 重跟兜底：取某 (platform, address, token_id) 最近一次快照的净仓 size（带符号，
/// 正=多 负=空）。无快照返回 None（调用方 fail-open，不阻断跟单）。
///
/// 用于 copier 下单前校验 copy 用户净仓不超过源 trader 净仓 × ratio（Proportional 同 venue）。
pub async fn latest_snapshot_size_for_token(
    pool: &PgPool,
    platform: &str,
    address: &str,
    token_id: &str,
) -> Result<Option<f64>, DbError> {
    let row: Option<(Option<f64>,)> = sqlx::query_as(
        r#"
        SELECT size FROM trader_hub.trader_positions_snapshot
        WHERE platform = $1 AND address = $2 AND token_id = $3
        ORDER BY captured_at DESC
        LIMIT 1
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(token_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(s,)| s))
}

/// 第 3 层：某 `(platform, address)` 在 `before` 时刻**之前**最近一轮扫描的每 token 快照。
///
/// diff 对账用"落后一轮闭合窗口"：用 `latest_snapshots`（上一轮 T_prev）减去本函数返回的
/// 再上一轮（T_prev_prev），Δ 落在已闭合区间 [T_prev_prev, T_prev] 内，trade_watch 此时
/// 早已轮询过该区间（间隔远小于 diff），故覆盖查询无竞态、无双计。
///
/// `before` 传上一轮的 `captured_at`：`DISTINCT ON (token_id)` 取 `captured_at < before` 的最新行。
pub async fn latest_snapshots_before(
    pool: &PgPool,
    platform: &str,
    address: &str,
    before: DateTime<Utc>,
) -> Result<Vec<PositionSnapshot>, DbError> {
    let rows = sqlx::query_as::<_, PositionSnapshot>(
        r#"
        SELECT DISTINCT ON (token_id) * FROM trader_hub.trader_positions_snapshot
        WHERE platform = $1 AND address = $2 AND captured_at < $3
        ORDER BY token_id, captured_at DESC
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(before)
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
