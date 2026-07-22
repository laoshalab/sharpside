//! 路由聚合。对应 `docs/ARCHITECTURE.md` §14「Venue × 业务面」二维菜单。
//!
//! - 市场映射审核队列：`GET /mappings/pending`、`POST /mappings/verify`、`POST /mappings/retire`
//! - 身份审核队列：`GET /identities/pending`、`POST /identities/{id}/verify`、`DELETE /identities/{id}`
//! - 热钥 per Venue：`GET /hot-wallets?platform=`、`POST /hot-wallets`、`DELETE /hot-wallets`
//! - 标签阈值：`GET /tag-rules`、`PUT /tag-rules/{rule_id}`
//! - 分类映射：`GET /category-mappings`、`PUT /category-mappings`、`DELETE /category-mappings/{p}/{cat}`
//! - 交易者管控：`GET /traders`、`PATCH .../visibility`、`PATCH .../hot`、`PATCH .../alias`
//! - 影子阈值：`GET /audit-thresholds`、`PUT /audit-thresholds/{metric}`
//! - 数据健康：`GET /shadow-health/summary|heatmap|top-diffs|audits`
//! - `GET /healthz` / `GET /readyz`

use crate::error::ApiError;
use crate::state::{AdminAuth, AppState};
use axum::extract::{Path, Query};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sharpside_db::queries::{
    identities as id_q, mappings as map_q, monitor as mon_q, ops, shadow as sh_q, traders as tr_q,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        // 映射审核
        .route("/mappings/pending", get(list_pending_mappings))
        .route("/mappings/verify", post(verify_mapping))
        .route("/mappings/retire", post(retire_mapping))
        // 身份审核
        .route("/identities/pending", get(list_pending_identities))
        .route("/identities/:id/verify", post(verify_identity))
        .route("/identities/:id", delete(delete_identity))
        // 热钥
        .route("/hot-wallets", get(list_hot_wallets).post(upsert_hot_wallet))
        .route("/hot-wallets/:platform/:address", delete(delete_hot_wallet))
        // 标签阈值
        .route("/tag-rules", get(list_tag_rules))
        .route("/tag-rules/:rule_id", put(upsert_tag_rule))
        // 分类映射
        .route(
            "/category-mappings",
            get(list_category_mappings).put(upsert_category_mapping),
        )
        .route(
            "/category-mappings/:platform/:official_category",
            delete(delete_category_mapping),
        )
        // 交易者管控
        .route("/traders", get(list_traders))
        .route("/traders/:platform/:address/visibility", patch(set_visibility))
        .route("/traders/:platform/:address/hot", patch(set_hot))
        .route("/traders/:platform/:address/alias", patch(set_alias))
        // 影子阈值
        .route("/audit-thresholds", get(list_audit_thresholds))
        .route("/audit-thresholds/:metric", put(upsert_audit_threshold))
        // 数据健康（影子审计报表）
        .route("/shadow-health/summary", get(shadow_summary))
        .route("/shadow-health/heatmap", get(shadow_heatmap))
        .route("/shadow-health/top-diffs", get(shadow_top_diffs))
        .route("/shadow-health/audits", get(shadow_audits))
}

async fn readyz(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    sharpside_db::ping(&state.db).await?;
    Ok(Json(serde_json::json!({ "db": "ok" })))
}

// ── 映射审核 ──

async fn list_pending_mappings(
    state: AppState,
    _auth: AdminAuth,
) -> Result<Json<Vec<sharpside_db::MarketMapping>>, ApiError> {
    let rows = map_q::list_pending_mappings(&state.db, 200).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct VerifyMappingBody {
    pub from_platform: String,
    pub from_market_id: String,
    pub to_platform: String,
    pub to_market_id: String,
    pub direction_flip: bool,
    #[serde(default)]
    pub resolution_notes: Option<String>,
    #[serde(default)]
    pub min_notional: Option<f64>,
    pub verified_by: String,
}

async fn verify_mapping(
    state: AppState,
    _auth: AdminAuth,
    Json(body): Json<VerifyMappingBody>,
) -> Result<Json<sharpside_db::MarketMapping>, ApiError> {
    let m = map_q::verify_mapping(
        &state.db,
        &body.from_platform,
        &body.from_market_id,
        &body.to_platform,
        &body.to_market_id,
        body.direction_flip,
        body.resolution_notes.as_deref(),
        body.min_notional,
        &body.verified_by,
    )
    .await?;
    Ok(Json(m))
}

#[derive(Debug, Deserialize)]
pub struct RetireMappingBody {
    pub from_platform: String,
    pub from_market_id: String,
    pub to_platform: String,
    pub to_market_id: String,
}

async fn retire_mapping(
    state: AppState,
    _auth: AdminAuth,
    Json(body): Json<RetireMappingBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    map_q::retire_mapping(
        &state.db,
        &body.from_platform,
        &body.from_market_id,
        &body.to_platform,
        &body.to_market_id,
    )
    .await?;
    Ok(Json(serde_json::json!({ "retired": true })))
}

// ── 身份审核 ──

async fn list_pending_identities(
    state: AppState,
    _auth: AdminAuth,
) -> Result<Json<Vec<sharpside_db::Identity>>, ApiError> {
    let rows = id_q::list_pending_identities(&state.db).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct VerifyIdentityBody {
    pub verified_by: String,
}

async fn verify_identity(
    state: AppState,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<VerifyIdentityBody>,
) -> Result<Json<sharpside_db::Identity>, ApiError> {
    let idn = id_q::verify_identity(&state.db, id, &body.verified_by).await?;
    Ok(Json(idn))
}

async fn delete_identity(
    state: AppState,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    id_q::delete_identity(&state.db, id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

// ── 热钥 per Venue ──

#[derive(Debug, Deserialize)]
pub struct HotWalletQuery {
    pub platform: String,
}

async fn list_hot_wallets(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<HotWalletQuery>,
) -> Result<Json<Vec<sharpside_db::HotWallet>>, ApiError> {
    let rows = mon_q::list_all_hot_wallets(&state.db, &q.platform).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpsertHotWalletBody {
    pub platform: String,
    pub address: String,
    pub added_by: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_priority() -> i32 {
    100
}
fn default_scan_interval() -> i32 {
    30
}
fn default_true() -> bool {
    true
}

async fn upsert_hot_wallet(
    state: AppState,
    _auth: AdminAuth,
    Json(body): Json<UpsertHotWalletBody>,
) -> Result<Json<sharpside_db::HotWallet>, ApiError> {
    let w = mon_q::upsert_hot_wallet(
        &state.db,
        &body.platform,
        &body.address,
        &body.added_by,
        body.priority,
        body.scan_interval_secs,
        body.enabled,
    )
    .await?;
    Ok(Json(w))
}

async fn delete_hot_wallet(
    state: AppState,
    _auth: AdminAuth,
    Path((platform, address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    mon_q::delete_hot_wallet(&state.db, &platform, &address).await?;
    Ok(Json(
        serde_json::json!({ "deleted": format!("{platform}/{address}") }),
    ))
}

// ── 标签阈值 ──

async fn list_tag_rules(
    state: AppState,
    _auth: AdminAuth,
) -> Result<Json<Vec<ops::TagRule>>, ApiError> {
    let rows = ops::list_tag_rules(&state.db).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpsertTagRuleBody {
    pub params: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub updated_by: String,
}

async fn upsert_tag_rule(
    state: AppState,
    _auth: AdminAuth,
    Path(rule_id): Path<String>,
    Json(body): Json<UpsertTagRuleBody>,
) -> Result<Json<ops::TagRule>, ApiError> {
    let r = ops::upsert_tag_rule(
        &state.db,
        &rule_id,
        &body.params,
        body.enabled,
        &body.updated_by,
    )
    .await?;
    Ok(Json(r))
}

// ── 分类映射 ──

#[derive(Debug, Deserialize)]
pub struct CategoryMappingQuery {
    #[serde(default)]
    pub platform: Option<String>,
}

async fn list_category_mappings(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<CategoryMappingQuery>,
) -> Result<Json<Vec<ops::CategoryMapping>>, ApiError> {
    let rows = ops::list_category_mappings(&state.db, q.platform.as_deref()).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpsertCategoryMappingBody {
    pub platform: String,
    pub official_category: String,
    pub site_category: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

async fn upsert_category_mapping(
    state: AppState,
    _auth: AdminAuth,
    Json(body): Json<UpsertCategoryMappingBody>,
) -> Result<Json<ops::CategoryMapping>, ApiError> {
    if body.platform.trim().is_empty()
        || body.official_category.trim().is_empty()
        || body.site_category.trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "platform / official_category / site_category 不能为空".into(),
        ));
    }
    let row = ops::upsert_category_mapping(
        &state.db,
        body.platform.trim(),
        body.official_category.trim(),
        body.site_category.trim(),
        body.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()),
    )
    .await?;
    Ok(Json(row))
}

async fn delete_category_mapping(
    state: AppState,
    _auth: AdminAuth,
    Path((platform, official_category)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ops::delete_category_mapping(&state.db, &platform, &official_category).await?;
    Ok(Json(serde_json::json!({
        "deleted": format!("{platform}/{official_category}")
    })))
}

// ── 交易者管控 ──

/// admin 视角交易者列表（含 hidden/blocked）。对应 `docs/FRONTEND_DESIGN.md` §7.6。
#[derive(Debug, Deserialize)]
pub struct AdminTradersQuery {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default = "default_admin_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_admin_limit() -> i64 {
    100
}

async fn list_traders(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<AdminTradersQuery>,
) -> Result<Json<Vec<sharpside_db::Trader>>, ApiError> {
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);
    let rows = tr_q::list_all_traders(
        &state.db,
        q.platform.as_deref(),
        q.q.as_deref(),
        limit,
        offset,
    )
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct VisibilityBody {
    pub visibility: String,
}

async fn set_visibility(
    state: AppState,
    _auth: AdminAuth,
    Path((platform, address)): Path<(String, String)>,
    Json(body): Json<VisibilityBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !matches!(body.visibility.as_str(), "visible" | "hidden" | "blocked") {
        return Err(ApiError::BadRequest(
            "visibility 必须为 visible / hidden / blocked".into(),
        ));
    }
    tr_q::set_visibility(&state.db, &platform, &address, &body.visibility).await?;
    Ok(Json(
        serde_json::json!({ "platform": platform, "address": address, "visibility": body.visibility }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct HotBody {
    pub is_hot: bool,
}

async fn set_hot(
    state: AppState,
    _auth: AdminAuth,
    Path((platform, address)): Path<(String, String)>,
    Json(body): Json<HotBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tr_q::set_hot(&state.db, &platform, &address, body.is_hot).await?;
    Ok(Json(
        serde_json::json!({ "platform": platform, "address": address, "is_hot": body.is_hot }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct AliasBody {
    /// `null` 或空串清空 alias。
    #[serde(default)]
    pub alias: Option<String>,
}

async fn set_alias(
    state: AppState,
    _auth: AdminAuth,
    Path((platform, address)): Path<(String, String)>,
    Json(body): Json<AliasBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tr_q::set_alias(&state.db, &platform, &address, body.alias.as_deref()).await?;
    Ok(Json(serde_json::json!({
        "platform": platform,
        "address": address,
        "alias": body.alias.as_deref().map(str::trim).filter(|s| !s.is_empty()),
    })))
}

// ── 影子阈值 ──

async fn list_audit_thresholds(
    state: AppState,
    _auth: AdminAuth,
) -> Result<Json<Vec<ops::AuditThreshold>>, ApiError> {
    let rows = ops::list_audit_thresholds(&state.db).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpsertAuditThresholdBody {
    pub warn_pct: f64,
    pub warn_abs: f64,
    pub alert_pct: f64,
    pub alert_abs: f64,
}

async fn upsert_audit_threshold(
    state: AppState,
    _auth: AdminAuth,
    Path(metric): Path<String>,
    Json(body): Json<UpsertAuditThresholdBody>,
) -> Result<Json<ops::AuditThreshold>, ApiError> {
    let t = ops::upsert_audit_threshold(
        &state.db,
        &metric,
        body.warn_pct,
        body.warn_abs,
        body.alert_pct,
        body.alert_abs,
    )
    .await?;
    Ok(Json(t))
}

// ── 数据健康（影子审计报表）──

fn default_hours() -> i32 {
    24
}

#[derive(Debug, Deserialize)]
pub struct ShadowHoursQuery {
    #[serde(default = "default_hours")]
    pub hours: i32,
}

async fn shadow_summary(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<ShadowHoursQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let s = sh_q::metric_audit_summary(&state.db, q.hours).await?;
    let ok_rate = if s.total > 0 {
        (s.ok_count as f64) / (s.total as f64)
    } else {
        1.0
    };
    Ok(Json(serde_json::json!({
        "hours": q.hours.clamp(1, 24 * 30),
        "total": s.total,
        "ok_count": s.ok_count,
        "warn_count": s.warn_count,
        "alert_count": s.alert_count,
        "ok_rate": ok_rate,
        "target_ok_rate": 0.95,
    })))
}

async fn shadow_heatmap(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<ShadowHoursQuery>,
) -> Result<Json<Vec<sh_q::MetricAuditHeatCell>>, ApiError> {
    let rows = sh_q::metric_audit_heatmap(&state.db, q.hours).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct ShadowTopDiffsQuery {
    #[serde(default = "default_hours")]
    pub hours: i32,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default = "default_top_limit")]
    pub limit: i64,
}

fn default_top_limit() -> i64 {
    20
}

async fn shadow_top_diffs(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<ShadowTopDiffsQuery>,
) -> Result<Json<Vec<sh_q::MetricAudit>>, ApiError> {
    if let Some(ref s) = q.status {
        if !matches!(s.as_str(), "ok" | "warn" | "alert") {
            return Err(ApiError::BadRequest(
                "status 必须为 ok / warn / alert".into(),
            ));
        }
    }
    let rows =
        sh_q::list_top_metric_diffs(&state.db, q.hours, q.status.as_deref(), q.limit).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct ShadowAuditsQuery {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub hours: Option<i32>,
    #[serde(default = "default_admin_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

async fn shadow_audits(
    state: AppState,
    _auth: AdminAuth,
    Query(q): Query<ShadowAuditsQuery>,
) -> Result<Json<Vec<sh_q::MetricAudit>>, ApiError> {
    if let Some(ref s) = q.status {
        if !matches!(s.as_str(), "ok" | "warn" | "alert") {
            return Err(ApiError::BadRequest(
                "status 必须为 ok / warn / alert".into(),
            ));
        }
    }
    let rows = sh_q::list_metric_audits(
        &state.db,
        q.platform.as_deref(),
        q.address.as_deref(),
        q.metric.as_deref(),
        q.status.as_deref(),
        q.hours,
        q.limit,
        q.offset,
    )
    .await?;
    Ok(Json(rows))
}

// 抑制未使用导入警告（Serialize 在未来响应体扩展时使用）
#[allow(dead_code)]
fn _unused_serialize() -> impl Serialize {
    serde_json::json!({})
}
