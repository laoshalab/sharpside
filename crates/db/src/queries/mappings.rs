//! `trader_hub.market_mappings` 查询。对应 `docs/VENUE_DESIGN.md` §6.3。
//!
//! 跨 Venue 跟单只读 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射。

use sqlx::PgPool;

use crate::error::DbError;
use crate::models::MarketMapping;

/// 跨 Venue 跟单时翻译 source_market_id → execute_market_id。
///
/// 仅返回 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射，
/// 按 confidence 降序取第一条。无 verified 映射时返回 `NoVerifiedMapping`，
/// Copier 据此跳过该 copy_order 并标记 `skipped`（对应 `docs/VENUE_DESIGN.md` §10）。
pub async fn resolve_mapping(
    pool: &PgPool,
    from_platform: &str,
    from_market_id: &str,
    to_platform: &str,
) -> Result<MarketMapping, DbError> {
    let row = sqlx::query_as::<_, MarketMapping>(
        r#"
        SELECT * FROM trader_hub.market_mappings
        WHERE from_platform = $1
          AND from_market_id = $2
          AND to_platform = $3
          AND manual_verified = true
          AND resolution_verified = true
          AND status = 'active'
        ORDER BY confidence DESC
        LIMIT 1
        "#,
    )
    .bind(from_platform)
    .bind(from_market_id)
    .bind(to_platform)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::not_found(format!(
            "no verified mapping {from_platform}/{from_market_id} -> {to_platform}"
        ))
    })?;
    Ok(row)
}

/// 列出某 source 的所有 active 映射（admin 审核队列用）。
pub async fn list_mappings_from(
    pool: &PgPool,
    from_platform: &str,
    from_market_id: &str,
) -> Result<Vec<MarketMapping>, DbError> {
    let rows = sqlx::query_as::<_, MarketMapping>(
        r#"
        SELECT * FROM trader_hub.market_mappings
        WHERE from_platform = $1 AND from_market_id = $2 AND status = 'active'
        ORDER BY confidence DESC
        "#,
    )
    .bind(from_platform)
    .bind(from_market_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出待审核的候选映射（`manual_verified=false`）。
pub async fn list_pending_mappings(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<MarketMapping>, DbError> {
    let rows = sqlx::query_as::<_, MarketMapping>(
        r#"
        SELECT * FROM trader_hub.market_mappings
        WHERE manual_verified = false AND status = 'active'
        ORDER BY confidence DESC, created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 运营确认映射：置 `manual_verified=true` + `resolution_verified=true`，
/// 标注 `direction_flip` / `resolution_notes` / `min_notional`。
/// 对应 `docs/FLOWS.md` §2 人工校对流程。
#[allow(clippy::too_many_arguments)]
pub async fn verify_mapping(
    pool: &PgPool,
    from_platform: &str,
    from_market_id: &str,
    to_platform: &str,
    to_market_id: &str,
    direction_flip: bool,
    resolution_notes: Option<&str>,
    min_notional: Option<f64>,
    verified_by: &str,
) -> Result<MarketMapping, DbError> {
    let row = sqlx::query_as::<_, MarketMapping>(
        r#"
        UPDATE trader_hub.market_mappings SET
            manual_verified     = true,
            resolution_verified = true,
            direction_flip       = $5,
            resolution_notes     = $6,
            min_notional         = $7,
            verified_by          = $8,
            verified_at          = now()
        WHERE from_platform = $1 AND from_market_id = $2
          AND to_platform = $3 AND to_market_id = $4
        RETURNING *
        "#,
    )
    .bind(from_platform)
    .bind(from_market_id)
    .bind(to_platform)
    .bind(to_market_id)
    .bind(direction_flip)
    .bind(resolution_notes)
    .bind(
        min_notional
            .map(rust_decimal::Decimal::try_from)
            .transpose()
            .map_err(|e| DbError::Invalid(e.to_string()))?,
    )
    .bind(verified_by)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found("mapping not found".to_string()))?;
    Ok(row)
}

/// 失效映射：市场下架/重映射时置 `status='retired'`。
pub async fn retire_mapping(
    pool: &PgPool,
    from_platform: &str,
    from_market_id: &str,
    to_platform: &str,
    to_market_id: &str,
) -> Result<(), DbError> {
    let res = sqlx::query(
        r#"
        UPDATE trader_hub.market_mappings
        SET status = 'retired', retired_at = now()
        WHERE from_platform = $1 AND from_market_id = $2
          AND to_platform = $3 AND to_market_id = $4
        "#,
    )
    .bind(from_platform)
    .bind(from_market_id)
    .bind(to_platform)
    .bind(to_market_id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found("mapping not found".to_string()));
    }
    Ok(())
}

/// 插入启发式候选映射（mapping worker 用）。
///
/// `ON CONFLICT DO NOTHING`：同一 `(from, to)` 已存在则跳过，不覆盖人工已校对的结果。
/// 新候选默认 `manual_verified=false`，进 admin 审核队列。
pub async fn insert_candidate(
    pool: &PgPool,
    from_platform: &str,
    from_market_id: &str,
    to_platform: &str,
    to_market_id: &str,
    confidence: f64,
) -> Result<(), DbError> {
    let confidence =
        rust_decimal::Decimal::try_from(confidence).map_err(|e| DbError::Invalid(e.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO trader_hub.market_mappings
            (from_platform, from_market_id, to_platform, to_market_id, confidence)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (from_platform, from_market_id, to_platform, to_market_id) DO NOTHING
        "#,
    )
    .bind(from_platform)
    .bind(from_market_id)
    .bind(to_platform)
    .bind(to_market_id)
    .bind(confidence)
    .execute(pool)
    .await?;
    Ok(())
}
