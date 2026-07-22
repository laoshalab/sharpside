//! `trader_hub.raw_trades` / `raw_markades` 查询。对应 `docs/VENUEHUB_STORAGE.md` §2。
//!
//! 原始层保留各 Venue API 原貌，ingest worker 写入，perf worker 读取重算。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::error::DbError;
use crate::models::{RawMarket, RawTrade};

/// 插入一条原始成交。链上 Venue 用 `tx_hash` 去重，玩钱/KYC 用 `trade_id`。
///
/// `ON CONFLICT DO NOTHING`：去重索引命中时跳过，保证幂等（worker 重跑不产生重复）。
#[allow(clippy::too_many_arguments)]
pub async fn insert_raw_trade(
    pool: &PgPool,
    platform: &str,
    address: &str,
    token_id: &str,
    condition_id: Option<&str>,
    side: &str,
    price: Decimal,
    size: Decimal,
    ts: DateTime<Utc>,
    tx_hash: Option<&str>,
    trade_id: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO trader_hub.raw_trades
            (platform, address, token_id, condition_id, side, price, size, ts, tx_hash, trade_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(token_id)
    .bind(condition_id)
    .bind(side)
    .bind(price)
    .bind(size)
    .bind(ts)
    .bind(tx_hash)
    .bind(trade_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// upsert 原始市场元数据。`raw_json` 保留官方原貌便于回溯。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_raw_market(
    pool: &PgPool,
    platform: &str,
    venue_market_id: &str,
    title: &str,
    slug: Option<&str>,
    tags: &[String],
    category: Option<&str>,
    end_date: Option<DateTime<Utc>>,
    outcome_yes: Option<f64>,
    outcome_no: Option<f64>,
    raw_json: Option<&serde_json::Value>,
    closed: Option<bool>,
) -> Result<(), DbError> {
    let outcome_yes = outcome_yes
        .map(Decimal::try_from)
        .transpose()
        .map_err(|e| DbError::Invalid(e.to_string()))?;
    let outcome_no = outcome_no
        .map(Decimal::try_from)
        .transpose()
        .map_err(|e| DbError::Invalid(e.to_string()))?;
    let closed = closed.unwrap_or(false);
    sqlx::query(
        r#"
        INSERT INTO trader_hub.raw_markets
            (platform, venue_market_id, title, slug, tags, category, end_date,
             outcome_yes, outcome_no, raw_json, closed)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (platform, venue_market_id) DO UPDATE SET
            title       = excluded.title,
            slug        = COALESCE(excluded.slug, raw_markets.slug),
            tags        = excluded.tags,
            category    = COALESCE(excluded.category, raw_markets.category),
            end_date    = COALESCE(excluded.end_date, raw_markets.end_date),
            outcome_yes = COALESCE(excluded.outcome_yes, raw_markets.outcome_yes),
            outcome_no  = COALESCE(excluded.outcome_no, raw_markets.outcome_no),
            raw_json    = COALESCE(excluded.raw_json, raw_markets.raw_json),
            -- closed 一旦为 true 不回退（市场结算不可逆）；resolved_at 首次置 true 时填 now()。
            closed      = raw_markets.closed OR excluded.closed,
            resolved_at = COALESCE(raw_markets.resolved_at, CASE WHEN excluded.closed AND raw_markets.resolved_at IS NULL THEN now() END),
            fetched_at  = now()
        "#,
    )
    .bind(platform)
    .bind(venue_market_id)
    .bind(title)
    .bind(slug)
    .bind(tags)
    .bind(category)
    .bind(end_date)
    .bind(outcome_yes)
    .bind(outcome_no)
    .bind(raw_json)
    .bind(closed)
    .execute(pool)
    .await?;
    Ok(())
}

/// 列出某 `(platform, address)` 的全部原始成交，按时间升序（perf worker 重建仓位时间线用）。
pub async fn list_raw_trades_for_trader(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Vec<RawTrade>, DbError> {
    let rows = sqlx::query_as::<_, RawTrade>(
        r#"
        SELECT * FROM trader_hub.raw_trades
        WHERE platform = $1 AND address = $2
        ORDER BY ts ASC
        "#,
    )
    .bind(platform)
    .bind(address)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出某 Venue 的全部原始市场（mapping worker 跨 Venue 匹配用）。
pub async fn list_raw_markets(pool: &PgPool, platform: &str) -> Result<Vec<RawMarket>, DbError> {
    let rows = sqlx::query_as::<_, RawMarket>(
        "SELECT * FROM trader_hub.raw_markets WHERE platform = $1 ORDER BY venue_market_id",
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出某 Venue 自 `since` 以来新结算的市场（closed=true 且 resolved_at > since）。
/// 赎回自动 worker 游标推进用：每轮取新结算市场，对其中跟单用户发起赎回。
/// `since=None` 取全部已结算市场（首跑/回填用）。
pub async fn list_resolved_markets(
    pool: &PgPool,
    platform: &str,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<RawMarket>, DbError> {
    let rows = sqlx::query_as::<_, RawMarket>(
        r#"
        SELECT * FROM trader_hub.raw_markets
        WHERE platform = $1 AND closed = true
          AND ($2::timestamptz IS NULL OR resolved_at > $2)
        ORDER BY resolved_at ASC NULLS LAST
        "#,
    )
    .bind(platform)
    .bind(since)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取某 Venue 下指定 `venue_market_id` 集合的 `(venue_market_id, category)` 映射。
///
/// perf worker 用它把 trader 的 `raw_trades.condition_id` 归一到 `raw_markets.category`，
/// 从而按分类切片重算绩效。未命中的 market 返回的 category 为 None（归入 OVERALL）。
pub async fn map_market_categories(
    pool: &PgPool,
    platform: &str,
    market_ids: &[String],
) -> Result<std::collections::HashMap<String, Option<String>>, DbError> {
    if market_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT venue_market_id, category FROM trader_hub.raw_markets \
         WHERE platform = $1 AND venue_market_id = ANY($2)",
    )
    .bind(platform)
    .bind(market_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}
