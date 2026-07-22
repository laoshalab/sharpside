//! `trader_hub.tag_rules` / `audit_thresholds` / `category_mapping` 查询。
//! 对应 `docs/VENUEHUB_STORAGE.md` §8。
//!
//! 运营后台可调的标签阈值、影子校验阈值与官方分类映射，零代码改动。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::types::Decimal as SqlDecimal;
use sqlx::PgPool;

use crate::error::DbError;

/// `trader_hub.tag_rules` 行。
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct TagRule {
    pub rule_id: String,
    pub params: serde_json::Value,
    pub enabled: bool,
    pub updated_by: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// `trader_hub.audit_thresholds` 行。
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AuditThreshold {
    pub metric_name: String,
    pub warn_pct: Decimal,
    pub warn_abs: Decimal,
    pub alert_pct: Decimal,
    pub alert_abs: Decimal,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// upsert 标签阈值规则。
pub async fn upsert_tag_rule(
    pool: &PgPool,
    rule_id: &str,
    params: &serde_json::Value,
    enabled: bool,
    updated_by: &str,
) -> Result<TagRule, DbError> {
    let row = sqlx::query_as::<_, TagRule>(
        r#"
        INSERT INTO trader_hub.tag_rules (rule_id, params, enabled, updated_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (rule_id) DO UPDATE SET
            params     = excluded.params,
            enabled    = excluded.enabled,
            updated_by = excluded.updated_by,
            updated_at = now()
        RETURNING *
        "#,
    )
    .bind(rule_id)
    .bind(params)
    .bind(enabled)
    .bind(updated_by)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list_tag_rules(pool: &PgPool) -> Result<Vec<TagRule>, DbError> {
    let rows = sqlx::query_as::<_, TagRule>("SELECT * FROM trader_hub.tag_rules ORDER BY rule_id")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 取单个 `tag_rules` 行（按 PK）。无则 None。
///
/// perf worker 用它读 `rule_id='botfilter'` 行，反序列化 `params` 为 `BotFilterConfig`。
pub async fn get_tag_rule(pool: &PgPool, rule_id: &str) -> Result<Option<TagRule>, DbError> {
    let row = sqlx::query_as::<_, TagRule>("SELECT * FROM trader_hub.tag_rules WHERE rule_id = $1")
        .bind(rule_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// upsert 影子校验阈值。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_audit_threshold(
    pool: &PgPool,
    metric_name: &str,
    warn_pct: f64,
    warn_abs: f64,
    alert_pct: f64,
    alert_abs: f64,
) -> Result<AuditThreshold, DbError> {
    let to_dec = |v: f64| -> Result<SqlDecimal, DbError> {
        SqlDecimal::try_from(v).map_err(|e| DbError::Invalid(e.to_string()))
    };
    let row = sqlx::query_as::<_, AuditThreshold>(
        r#"
        INSERT INTO trader_hub.audit_thresholds
            (metric_name, warn_pct, warn_abs, alert_pct, alert_abs)
        VALUES ($1,$2,$3,$4,$5)
        ON CONFLICT (metric_name) DO UPDATE SET
            warn_pct   = excluded.warn_pct,
            warn_abs   = excluded.warn_abs,
            alert_pct  = excluded.alert_pct,
            alert_abs  = excluded.alert_abs,
            updated_at = now()
        RETURNING *
        "#,
    )
    .bind(metric_name)
    .bind(to_dec(warn_pct)?)
    .bind(to_dec(warn_abs)?)
    .bind(to_dec(alert_pct)?)
    .bind(to_dec(alert_abs)?)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list_audit_thresholds(pool: &PgPool) -> Result<Vec<AuditThreshold>, DbError> {
    let rows = sqlx::query_as::<_, AuditThreshold>(
        "SELECT * FROM trader_hub.audit_thresholds ORDER BY metric_name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `trader_hub.category_mapping` 行。官方 category → 站内分类。
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct CategoryMapping {
    pub platform: String,
    pub official_category: String,
    pub site_category: String,
    pub display_name: Option<String>,
}

/// 列出分类映射；`platform` 为空则全量。
pub async fn list_category_mappings(
    pool: &PgPool,
    platform: Option<&str>,
) -> Result<Vec<CategoryMapping>, DbError> {
    let rows = sqlx::query_as::<_, CategoryMapping>(
        r#"
        SELECT * FROM trader_hub.category_mapping
        WHERE ($1::text IS NULL OR platform = $1)
        ORDER BY platform ASC, official_category ASC
        "#,
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// upsert 分类映射（PK = platform + official_category）。
pub async fn upsert_category_mapping(
    pool: &PgPool,
    platform: &str,
    official_category: &str,
    site_category: &str,
    display_name: Option<&str>,
) -> Result<CategoryMapping, DbError> {
    let row = sqlx::query_as::<_, CategoryMapping>(
        r#"
        INSERT INTO trader_hub.category_mapping
            (platform, official_category, site_category, display_name)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (platform, official_category) DO UPDATE SET
            site_category = excluded.site_category,
            display_name  = excluded.display_name
        RETURNING *
        "#,
    )
    .bind(platform)
    .bind(official_category)
    .bind(site_category)
    .bind(display_name)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 删除一条分类映射。
pub async fn delete_category_mapping(
    pool: &PgPool,
    platform: &str,
    official_category: &str,
) -> Result<(), DbError> {
    let res = sqlx::query(
        "DELETE FROM trader_hub.category_mapping \
         WHERE platform = $1 AND official_category = $2",
    )
    .bind(platform)
    .bind(official_category)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!(
            "category_mapping {platform}/{official_category}"
        )));
    }
    Ok(())
}
