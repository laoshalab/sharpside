//! 影子层查询：`trader_performance_third_party` / `metric_audit`。
//! 对应 `docs/VENUEHUB_STORAGE.md` §9 与 `docs/SHADOW_MODE.md` §5。
//!
//! 影子路径与生产展示链路物理隔离：第三方指标只写审计表 + 告警，永不进入用户界面。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::types::Decimal as SqlDecimal;
use sqlx::PgPool;

use crate::error::DbError;

/// `trader_hub.trader_performance_third_party` 行。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct ThirdPartyPerf {
    pub platform: String,
    pub address: String,
    pub source: String,
    pub period: String,
    pub roi: Option<Decimal>,
    pub win_rate: Option<Decimal>,
    pub realized_pnl: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub wins: Option<i32>,
    pub losses: Option<i32>,
    pub markets_count: Option<i32>,
    pub total_volume: Option<Decimal>,
    pub fetched_at: DateTime<Utc>,
}

/// `trader_hub.metric_audit` 行。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct MetricAudit {
    pub id: i64,
    pub platform: String,
    pub address: String,
    pub source: String,
    pub period: String,
    pub metric_name: String,
    pub self_value: Option<Decimal>,
    pub third_party_value: Option<Decimal>,
    pub diff_abs: Option<Decimal>,
    pub diff_pct: Option<Decimal>,
    pub status: String,
    pub audited_at: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_third_party_perf(
    pool: &PgPool,
    platform: &str,
    address: &str,
    source: &str,
    period: &str,
    roi: Option<f64>,
    win_rate: Option<f64>,
    realized_pnl: Option<f64>,
    unrealized_pnl: Option<f64>,
    wins: Option<i32>,
    losses: Option<i32>,
    markets_count: Option<i32>,
    total_volume: Option<f64>,
) -> Result<(), DbError> {
    let to_dec = |v: Option<f64>| -> Result<Option<SqlDecimal>, DbError> {
        v.map(|x| SqlDecimal::try_from(x).map_err(|e| DbError::Invalid(e.to_string())))
            .transpose()
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_performance_third_party
            (platform, address, source, period, roi, win_rate, realized_pnl,
             unrealized_pnl, wins, losses, markets_count, total_volume)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        ON CONFLICT (platform, address, source, period) DO UPDATE SET
            roi             = excluded.roi,
            win_rate        = excluded.win_rate,
            realized_pnl    = excluded.realized_pnl,
            unrealized_pnl  = excluded.unrealized_pnl,
            wins            = excluded.wins,
            losses          = excluded.losses,
            markets_count   = excluded.markets_count,
            total_volume    = excluded.total_volume,
            fetched_at      = now()
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(source)
    .bind(period)
    .bind(to_dec(roi)?)
    .bind(to_dec(win_rate)?)
    .bind(to_dec(realized_pnl)?)
    .bind(to_dec(unrealized_pnl)?)
    .bind(wins)
    .bind(losses)
    .bind(markets_count)
    .bind(to_dec(total_volume)?)
    .execute(pool)
    .await?;
    Ok(())
}

/// 取某 (platform, address, period) 的自算绩效（用于 diff）。
pub async fn get_self_perf(
    pool: &PgPool,
    platform: &str,
    address: &str,
    period: &str,
) -> Result<Option<crate::models::TraderPerformance>, DbError> {
    let row = sqlx::query_as::<_, crate::models::TraderPerformance>(
        "SELECT * FROM trader_hub.trader_performance \
         WHERE platform = $1 AND address = $2 AND period = $3",
    )
    .bind(platform)
    .bind(address)
    .bind(period)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_metric_audit(
    pool: &PgPool,
    platform: &str,
    address: &str,
    source: &str,
    period: &str,
    metric_name: &str,
    self_value: Option<f64>,
    third_party_value: Option<f64>,
    diff_abs: Option<f64>,
    diff_pct: Option<f64>,
    status: &str,
) -> Result<(), DbError> {
    let to_dec = |v: Option<f64>| -> Result<Option<SqlDecimal>, DbError> {
        v.map(|x| SqlDecimal::try_from(x).map_err(|e| DbError::Invalid(e.to_string())))
            .transpose()
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.metric_audit
            (platform, address, source, period, metric_name,
             self_value, third_party_value, diff_abs, diff_pct, status)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(source)
    .bind(period)
    .bind(metric_name)
    .bind(to_dec(self_value)?)
    .bind(to_dec(third_party_value)?)
    .bind(to_dec(diff_abs)?)
    .bind(to_dec(diff_pct)?)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// 取某 metric 的审计阈值（warn/alert）。缺省返回 None。
pub async fn get_audit_threshold(
    pool: &PgPool,
    metric_name: &str,
) -> Result<Option<crate::queries::ops::AuditThreshold>, DbError> {
    let row = sqlx::query_as::<_, crate::queries::ops::AuditThreshold>(
        "SELECT * FROM trader_hub.audit_thresholds WHERE metric_name = $1",
    )
    .bind(metric_name)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 最近 N 小时审计汇总（admin「数据健康」页）。对应 `docs/SHADOW_MODE.md` §8。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct MetricAuditSummary {
    pub total: i64,
    pub ok_count: i64,
    pub warn_count: i64,
    pub alert_count: i64,
}

/// metric × period 偏离热力（按 status 计数）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct MetricAuditHeatCell {
    pub metric_name: String,
    pub period: String,
    pub ok_count: i64,
    pub warn_count: i64,
    pub alert_count: i64,
}

/// 最近 N 小时汇总。
pub async fn metric_audit_summary(
    pool: &PgPool,
    hours: i32,
) -> Result<MetricAuditSummary, DbError> {
    let hours = hours.clamp(1, 24 * 30);
    let row = sqlx::query_as::<_, MetricAuditSummary>(
        r#"
        SELECT
            count(*)::bigint AS total,
            count(*) FILTER (WHERE status = 'ok')::bigint AS ok_count,
            count(*) FILTER (WHERE status = 'warn')::bigint AS warn_count,
            count(*) FILTER (WHERE status = 'alert')::bigint AS alert_count
        FROM trader_hub.metric_audit
        WHERE audited_at >= now() - make_interval(hours => $1)
        "#,
    )
    .bind(hours)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// metric × period 热力计数。
pub async fn metric_audit_heatmap(
    pool: &PgPool,
    hours: i32,
) -> Result<Vec<MetricAuditHeatCell>, DbError> {
    let hours = hours.clamp(1, 24 * 30);
    let rows = sqlx::query_as::<_, MetricAuditHeatCell>(
        r#"
        SELECT
            metric_name,
            period,
            count(*) FILTER (WHERE status = 'ok')::bigint AS ok_count,
            count(*) FILTER (WHERE status = 'warn')::bigint AS warn_count,
            count(*) FILTER (WHERE status = 'alert')::bigint AS alert_count
        FROM trader_hub.metric_audit
        WHERE audited_at >= now() - make_interval(hours => $1)
        GROUP BY metric_name, period
        ORDER BY alert_count DESC, warn_count DESC, metric_name ASC, period ASC
        "#,
    )
    .bind(hours)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Top 偏离（按 |diff_pct| 降序）。`status` 可筛 ok/warn/alert。
pub async fn list_top_metric_diffs(
    pool: &PgPool,
    hours: i32,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<MetricAudit>, DbError> {
    let hours = hours.clamp(1, 24 * 30);
    let limit = limit.clamp(1, 200);
    let rows = sqlx::query_as::<_, MetricAudit>(
        r#"
        SELECT *
        FROM trader_hub.metric_audit
        WHERE audited_at >= now() - make_interval(hours => $1)
          AND ($2::text IS NULL OR status = $2)
        ORDER BY abs(coalesce(diff_pct, 0)) DESC, audited_at DESC
        LIMIT $3
        "#,
    )
    .bind(hours)
    .bind(status)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 审计明细筛选（admin 报表）。
#[allow(clippy::too_many_arguments)]
pub async fn list_metric_audits(
    pool: &PgPool,
    platform: Option<&str>,
    address: Option<&str>,
    metric_name: Option<&str>,
    status: Option<&str>,
    hours: Option<i32>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MetricAudit>, DbError> {
    let limit = limit.clamp(1, 500);
    let offset = offset.max(0);
    let hours = hours.map(|h| h.clamp(1, 24 * 30));
    let rows = sqlx::query_as::<_, MetricAudit>(
        r#"
        SELECT *
        FROM trader_hub.metric_audit
        WHERE ($1::text IS NULL OR platform = $1)
          AND ($2::text IS NULL OR address ILIKE '%' || $2 || '%')
          AND ($3::text IS NULL OR metric_name = $3)
          AND ($4::text IS NULL OR status = $4)
          AND ($5::int IS NULL OR audited_at >= now() - make_interval(hours => $5))
        ORDER BY audited_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(metric_name)
    .bind(status)
    .bind(hours)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
