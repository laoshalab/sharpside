//! 路由聚合。对应 `docs/ARCHITECTURE.md` §6.2 对外 API。
//!
//! - `POST /follows`（跟随 trader 或 identity）
//! - `GET /me/follows`
//! - `PATCH /follows/{id}`
//! - `DELETE /follows/{id}`
//! - `POST /internal/signals`（venue-hub 检出仓位变化后调用，强制 `X-Internal-Secret` 鉴权）
//! - Watchlist 端点见 [`crate::watchlist`]（`/watchlists*` / `/me/watchlists*`）
//! - `GET /healthz` / `GET /readyz`

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::signal::{derive_copy_orders, SignalEvent};
use crate::state::AppState;
use crate::watchlist;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sharpside_db::models::FollowRelation;
use sharpside_db::queries::account as acct;
use sharpside_db::queries::identities as identity_q;
use sharpside_db::queries::perf as perf_q;
use sharpside_db::queries::traders as trader_q;
use sharpside_db::DbError;
use sharpside_shared::{allowed_execute_venues, Channel, FollowConfig, Platform};
use std::collections::HashSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/follows", post(create_follow).get(list_my_follows))
        .route("/follows/:id", patch(update_follow).delete(delete_follow))
        .route("/internal/signals", post(ingest_signal))
        // Watchlist（观察名单）：与 /follows 同构但不进执行路径。
        .merge(watchlist::router())
}

async fn readyz(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    sharpside_db::ping(&state.db).await?;
    Ok(Json(serde_json::json!({ "db": "ok" })))
}

// ── 跟随关系 CRUD ──

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FollowTarget {
    Trader {
        follow_platform: String,
        follow_address: String,
    },
    Identity {
        follow_identity_id: Uuid,
    },
}

#[derive(Debug, Deserialize)]
pub struct CreateFollowBody {
    #[serde(flatten)]
    pub target: FollowTarget,
    pub execute_venue: String,
    pub channel: String,
    pub config: FollowConfig,
}

#[derive(Debug, Serialize)]
pub struct FollowOut {
    #[serde(flatten)]
    pub relation: FollowRelation,
}

async fn create_follow(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<CreateFollowBody>,
) -> Result<Json<FollowOut>, ApiError> {
    validate_platform(&body.execute_venue)?;
    validate_channel(&body.channel)?;
    // 已接入执行 adapter 校验：execute_venue 须已在 copier build_registry 接入（H3 修复）。
    // 否则建出的跟随每个信号都会被 copier 因无 adapter 静默跳过。与管辖域校验互补：
    // 管辖域管「法域允许」，本校验管「工程已接入」。
    let exec_venue = body
        .execute_venue
        .parse::<Platform>()
        .map_err(|_| ApiError::BadRequest(format!("未知 platform: {}", body.execute_venue)))?;
    if !sharpside_shared::is_implemented_venue(exec_venue) {
        return Err(ApiError::BadRequest(format!(
            "execute_venue {} 暂未接入执行 adapter（当前可用：{}）",
            body.execute_venue,
            sharpside_shared::jurisdiction::implemented_execute_venues()
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    // 管辖域校验：execute_venue 须被用户 jurisdiction 允许。
    // 早拒绝，避免创建出"每个信号都被 copier 跳过"的静默失效跟随。
    ensure_venue_allowed_for_user(&state, auth.user_id, &body.execute_venue).await?;
    // 档位槽位校验：Free 档最多 FREE_FOLLOW_SLOTS 个活跃跟随；Pro+ 不限。
    // 此前仅前端展示，后端不拦，可绕过 UI 无限建跟随。此处后端强制。
    let user = acct::get_user(&state.db, auth.user_id).await?;
    if user.subscription_tier == "free" {
        let used = acct::count_active_follows_by_user(&state.db, auth.user_id).await?;
        if used >= FREE_FOLLOW_SLOTS {
            return Err(ApiError::BadRequest(format!(
                "Free 档最多 {FREE_FOLLOW_SLOTS} 个活跃跟随（当前 {used}），升级 Pro+ 解锁无限槽位"
            )));
        }
    }
    // config 中的 execute_venue/channel 须与列一致
    if body.config.execute_venue
        != body
            .execute_venue
            .parse::<Platform>()
            .ok()
            .unwrap_or(Platform::Polymarket)
    {
        // 宽容：以 body.execute_venue 为准，不强制报错
    }
    let config_json = serde_json::to_value(&body.config)
        .map_err(|e| ApiError::BadRequest(format!("config 序列化失败: {e}")))?;
    let rel = match body.target {
        FollowTarget::Trader {
            follow_platform,
            follow_address,
        } => {
            validate_platform(&follow_platform)?;
            // trader 存在性校验：拒绝跟随 trader_hub.traders 中不存在的地址，
            // 否则会创建出永不命中信号、且无任何提示的静默失效跟随。
            match trader_q::get_trader(&state.db, &follow_platform, &follow_address).await {
                Ok(_) => {}
                Err(DbError::NotFound(_)) => {
                    return Err(ApiError::BadRequest(format!(
                        "交易者不存在：{follow_platform}/{follow_address}"
                    )));
                }
                Err(e) => return Err(e.into()),
            }
            // bot 门控：被 botfilter 标记为机器人的交易者禁止跟随。
            // `bot` 标签由 venue-hub perf worker 调 `crates/botfilter` 产出，写入 `trader_tag.tags`。
            // 未算过标签（无 trader_tag 行）视为 clean，放行。
            let tags = perf_q::get_trader_tag(&state.db, &follow_platform, &follow_address).await?;
            if tags.iter().any(|t| t == "bot") {
                return Err(ApiError::BadRequest(format!(
                    "拒绝跟随：该交易者被 botfilter 标记为机器人（{follow_platform}/{follow_address}）"
                )));
            }
            // 唯一性预检：每个用户对同一 trader 仅允许一条 active follow，
            // 避免每信号派生多笔 copy_order 重复下单。DB 侧部分唯一索引为并发兜底。
            if acct::find_active_follow_trader(
                &state.db,
                auth.user_id,
                &follow_platform,
                &follow_address,
            )
            .await?
            .is_some()
            {
                return Err(ApiError::Conflict(format!(
                    "已跟随该交易者（{follow_platform}/{follow_address}）"
                )));
            }
            acct::create_follow_trader(
                &state.db,
                auth.user_id,
                &follow_platform,
                &follow_address,
                &body.execute_venue,
                &body.channel,
                &config_json,
                body.config.same_venue_only,
            )
            .await?
        }
        FollowTarget::Identity { follow_identity_id } => {
            // 校验 identity 存在且 manual_verified（跟随 identity 须人工校对）
            let identity = identity_q::get_identity(&state.db, follow_identity_id).await?;
            if !identity.manual_verified {
                return Err(ApiError::BadRequest(
                    "跟随 identity 须 manual_verified=true".into(),
                ));
            }
            // 唯一性预检：每个用户对同一 identity 仅允许一条 active follow。
            if acct::find_active_follow_identity(&state.db, auth.user_id, follow_identity_id)
                .await?
                .is_some()
            {
                return Err(ApiError::Conflict(
                    "已跟随该身份（identity）".into(),
                ));
            }
            acct::create_follow_identity(
                &state.db,
                auth.user_id,
                follow_identity_id,
                &body.execute_venue,
                &body.channel,
                &config_json,
                body.config.same_venue_only,
            )
            .await?
        }
    };
    Ok(Json(FollowOut { relation: rel }))
}

async fn list_my_follows(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Vec<FollowRelation>>, ApiError> {
    let rows = acct::list_follows_by_user(&state.db, auth.user_id).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateFollowBody {
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub config: Option<FollowConfig>,
    #[serde(default)]
    pub execute_venue: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
}

async fn update_follow(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFollowBody>,
) -> Result<Json<FollowRelation>, ApiError> {
    let existing = acct::get_follow(&state.db, id).await?;
    if existing.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权修改他人跟随关系".into()));
    }
    // 恢复（active=false → true）时校验唯一性：避免与另一条已 active 的同目标跟随冲突
    // （用户暂停后又新建了一条 active 跟随，再恢复旧条会撞部分唯一索引）。
    if body.active == Some(true) && !existing.active {
        let dup = if let (Some(fp), Some(fa)) =
            (&existing.follow_platform, &existing.follow_address)
        {
            acct::find_active_follow_trader(&state.db, auth.user_id, fp, fa)
                .await?
                .filter(|r| r.id != id)
        } else if let Some(iid) = existing.follow_identity_id {
            acct::find_active_follow_identity(&state.db, auth.user_id, iid)
                .await?
                .filter(|r| r.id != id)
        } else {
            None
        };
        if dup.is_some() {
            return Err(ApiError::Conflict(
                "已存在一条启用中的同目标跟随，请先暂停或删除它再恢复".into(),
            ));
        }
    }
    if let Some(ev) = &body.execute_venue {
        validate_platform(ev)?;
    }
    if let Some(ch) = &body.channel {
        validate_channel(ch)?;
    }
    let config = match &body.config {
        Some(c) => Some(serde_json::to_value(c).map_err(|e| ApiError::BadRequest(e.to_string()))?),
        None => None,
    };
    let rel = acct::update_follow(
        &state.db,
        id,
        body.active,
        config.as_ref(),
        body.execute_venue.as_deref(),
        body.channel.as_deref(),
    )
    .await?;
    Ok(Json(rel))
}

async fn delete_follow(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = acct::get_follow(&state.db, id).await?;
    if existing.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权删除他人跟随关系".into()));
    }
    acct::delete_follow(&state.db, id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

// ── 信号派生 ──

#[derive(Debug, Serialize)]
pub struct IngestResult {
    pub matched_relations: usize,
    pub enqueued: usize,
    pub skipped: usize,
}

async fn ingest_signal(
    state: AppState,
    headers: HeaderMap,
    Json(event): Json<SignalEvent>,
) -> Result<Json<IngestResult>, ApiError> {
    // 0. 内部端点鉴权：INTERNAL_SIGNAL_SECRET 必须配置，且请求须携带匹配的 X-Internal-Secret。
    //    强制配置（空串即拒绝）——防止 follow 端口被误暴露公网时伪造仓位变化灌入 copy_order。
    //    生产部署必须设 INTERNAL_SIGNAL_SECRET；dev/e2e 设一个已知值（如 e2e-internal-secret）。
    let secret = state.config.internal_signal_secret.trim();
    if secret.is_empty() {
        tracing::error!("INTERNAL_SIGNAL_SECRET 未配置，拒绝接收信号（生产必须配置非空密钥）");
        return Err(ApiError::Unauthorized(
            "INTERNAL_SIGNAL_SECRET 未配置，拒绝接收信号".into(),
        ));
    }
    let got = headers
        .get("x-internal-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if got != secret {
        return Err(ApiError::Unauthorized("internal signal secret 不匹配".into()));
    }

    // 1. 收集匹配的活跃跟随关系（trader 跟随 + identity 跟随）
    let mut relations: Vec<FollowRelation> =
        acct::list_follows_of_trader(&state.db, event.platform.as_str(), &event.trader_id).await?;

    if let Some(identity_id) = event.identity_id {
        let id_rels = acct::list_follows_of_identity(&state.db, identity_id).await?;
        relations.extend(id_rels);
    }

    // 去重（同一 relation 可能被两条路径命中）
    relations.sort_by_key(|r| r.id);
    relations.dedup_by_key(|r| r.id);

    let matched = relations.len();

    // 2. 查 identity 跟随涉及的 identity 是否 manual_verified
    let mut verified: HashSet<Uuid> = HashSet::new();
    for rel in &relations {
        if let Some(identity_id) = rel.follow_identity_id {
            if verified.contains(&identity_id) {
                continue;
            }
            if let Ok(identity) = identity_q::get_identity(&state.db, identity_id).await {
                if identity.manual_verified {
                    verified.insert(identity_id);
                }
            }
        }
    }

    // 3. 派生 + 落库
    let derived = derive_copy_orders(&event, &relations, &verified);
    // 信号去重键：同一 (platform,trader,token,ts) 在 outbox 重发时产出同一 key，
    // 配合 copy_order (signal_id, follow_relation_id) 唯一约束，重发不重复下单。
    let sig_id = sharpside_shared::signal_id(
        event.platform.as_str(),
        &event.trader_id,
        &event.token_id,
        event.ts,
    );
    let mut enqueued = 0usize;
    let mut skipped = 0usize;
    for d in derived {
        let (status, skip_reason) = match d.skip_reason {
            Some(reason) => {
                skipped += 1;
                ("skipped", Some(reason))
            }
            None => {
                enqueued += 1;
                ("pending", None)
            }
        };
        match acct::enqueue_copy_order(
            &state.db,
            Uuid::new_v4(),
            d.follow_relation_id,
            d.user_id,
            d.source_venue.as_str(),
            d.execute_venue.as_str(),
            &d.source_market_id,
            &d.source_token_id,
            d.side.as_str(),
            d.price,
            d.size,
            d.channel.as_str(),
            d.signal_at,
            skip_reason.as_deref(),
            status,
            Some(&sig_id),
        )
        .await
        {
            Ok(_) => {}
            // 同一 signal_id 已派生过（outbox 重发命中唯一约束）：幂等跳过，不计错误。
            Err(sharpside_db::DbError::Conflict(_)) => {
                tracing::debug!(
                    signal_id = %sig_id,
                    follow = %d.follow_relation_id,
                    "信号已派生过，幂等跳过"
                );
            }
            Err(e) => return Err(e.into()),
        }
    }

    tracing::info!(matched, enqueued, skipped, "信号派生完成");
    Ok(Json(IngestResult {
        matched_relations: matched,
        enqueued,
        skipped,
    }))
}

pub(crate) fn validate_platform(p: &str) -> Result<(), ApiError> {
    p.parse::<Platform>()
        .map(|_| ())
        .map_err(|_| ApiError::BadRequest(format!("未知 platform: {p}")))
}

pub(crate) fn validate_channel(c: &str) -> Result<(), ApiError> {
    c.parse::<Channel>()
        .map(|_| ())
        .map_err(|_| ApiError::BadRequest(format!("未知 channel: {c}")))
}

/// Free 档活跃跟随槽位上限。Pro+ 不限。
const FREE_FOLLOW_SLOTS: i64 = 3;

/// 管辖域校验：用户 `jurisdiction` 须允许 `execute_venue`。
///
/// 在创建跟随 / 升级 watchlist 时**前置校验**，早拒绝（返回 400），避免创建出
/// "每个信号都被 copier 在执行时跳过"的静默失效跟随。copier 执行时仍会兜底校验
/// （防御纵深，应对 follow 创建后用户改了 jurisdiction 的场景）。
///
/// 共用于 `create_follow`（本模块）与 `upgrade_watchlist`（`watchlist` 模块）。
pub(crate) async fn ensure_venue_allowed_for_user(
    state: &AppState,
    user_id: Uuid,
    execute_venue: &str,
) -> Result<(), ApiError> {
    let user = acct::get_user(&state.db, user_id).await.map_err(map_db)?;
    let venue = execute_venue
        .parse::<Platform>()
        .map_err(|_| ApiError::BadRequest(format!("未知 platform: {execute_venue}")))?;
    if !allowed_execute_venues(&user.jurisdiction).contains(&venue) {
        return Err(ApiError::BadRequest(format!(
            "管辖域 {} 不允许 execute_venue {}（可用：{}）",
            user.jurisdiction,
            execute_venue,
            allowed_execute_venues(&user.jurisdiction)
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )));
    }
    Ok(())
}

/// 把 `DbError` 映射到带正确 HTTP 状态码的 `ApiError`（NotFound→404、Conflict→409、其余→500）。
pub(crate) fn map_db(e: sharpside_db::DbError) -> ApiError {
    match e {
        sharpside_db::DbError::NotFound(msg) => ApiError::NotFound(msg),
        sharpside_db::DbError::Conflict(msg) => ApiError::Conflict(msg),
        other => ApiError::Db(other),
    }
}
