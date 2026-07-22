//! `trader_hub.identities` 查询。对应 `docs/VENUE_DESIGN.md` §7.1。
//!
//! 跨 Venue 身份聚合：identity worker 产候选 → 创建 identity + 链接 traders；
//! admin 审核置 `manual_verified=true`。GET /identities/{id} 读聚合详情。

use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::DbError;
use crate::models::{Identity, Trader};

/// 创建一个 identity 并把给定 traders 链接到它（单事务）。
///
/// `confidence` 取启发式最高分。traders 通过 `(platform, address)` 定位；
/// 不存在的 trader 跳过（不报错），便于 worker 在 ingest 未完成时部分链接。
pub async fn create_identity_with_links(
    pool: &PgPool,
    alias: Option<&str>,
    confidence: f64,
    trader_keys: &[(&str, &str)],
) -> Result<Uuid, DbError> {
    let confidence = Decimal::try_from(confidence).map_err(|e| DbError::Invalid(e.to_string()))?;

    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO trader_hub.identities (alias, confidence) VALUES ($1, $2) RETURNING id",
    )
    .bind(alias)
    .bind(confidence)
    .fetch_one(&mut *tx)
    .await?;

    for (platform, address) in trader_keys {
        sqlx::query(
            "UPDATE trader_hub.traders SET identity_id = $3 \
             WHERE platform = $1 AND address = $2",
        )
        .bind(*platform)
        .bind(*address)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// 取单个 identity。
pub async fn get_identity(pool: &PgPool, id: Uuid) -> Result<Identity, DbError> {
    let row = sqlx::query_as::<_, Identity>("SELECT * FROM trader_hub.identities WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| DbError::not_found(format!("identity {id}")))?;
    Ok(row)
}

/// 列出某 identity 下已链接的所有 trader（跨 Venue）。
pub async fn list_identity_traders(pool: &PgPool, id: Uuid) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        "SELECT * FROM trader_hub.traders WHERE identity_id = $1 ORDER BY platform",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 运营确认身份：置 `manual_verified=true`。对应 `docs/FLOWS.md` §3 人工校对。
pub async fn verify_identity(
    pool: &PgPool,
    id: Uuid,
    verified_by: &str,
) -> Result<Identity, DbError> {
    let row = sqlx::query_as::<_, Identity>(
        r#"
        UPDATE trader_hub.identities SET
            manual_verified = true,
            verified_by     = $2,
            verified_at     = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(verified_by)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("identity {id}")))?;
    Ok(row)
}

/// 待审核身份候选（`manual_verified=false`），admin 审核队列用。
pub async fn list_pending_identities(pool: &PgPool) -> Result<Vec<Identity>, DbError> {
    let rows = sqlx::query_as::<_, Identity>(
        r#"
        SELECT * FROM trader_hub.identities
        WHERE manual_verified = false
        ORDER BY confidence DESC, created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 已人工校对身份列表（`manual_verified=true`），用户端跨 Venue 跟随下拉用。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.10「身份下拉仅列 manual_verified」。
pub async fn list_verified_identities(pool: &PgPool) -> Result<Vec<Identity>, DbError> {
    let rows = sqlx::query_as::<_, Identity>(
        r#"
        SELECT * FROM trader_hub.identities
        WHERE manual_verified = true
        ORDER BY alias NULLS LAST, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 拒绝身份候选：删除（未链接到任何 follow_relation 时安全）。
pub async fn delete_identity(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM trader_hub.identities WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!("identity {id}")));
    }
    Ok(())
}
