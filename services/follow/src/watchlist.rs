//! Watchlist（观察名单）HTTP 端点。对应 Watchlist 功能规划。
//!
//! 与 `/follows` 同构的"二选一目标"（trader / identity），但**不进执行路径**：
//! - 无 execute_venue / channel / config —— 仅观察。
//! - 无 botfilter / identity manual_verified 门控（观察不等于跟单）。
//! - 配额按 `subscription_tier` 差异化（free 20 / pro_plus 200）。
//!
//! `POST /watchlists/:id/upgrade` 一键升级为 Follow：在事务内 INSERT follow_relation +
//! DELETE watchlist（消费式升级）。升级时**进入执行路径**，故 botfilter / identity
//! `manual_verified` 门控必须生效（与 `create_follow` 一致）——门控失败时 watchlist 保留。
//!
//! 路由注册见 `routes::router`。

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::routes::{ensure_venue_allowed_for_user, map_db, validate_channel, validate_platform};
use crate::state::AppState;
use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use sharpside_db::models::{FollowRelation, Watchlist};
use sharpside_db::queries::account as acct;
use sharpside_db::queries::identities as identity_q;
use sharpside_db::queries::perf as perf_q;
use sharpside_shared::{watchlist_limit, Platform, WatchlistCreate, WatchlistUpgrade};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/watchlists",
            post(create_watchlist).get(list_my_watchlists),
        )
        .route("/me/watchlists", get(list_my_watchlists))
        .route("/me/watchlists/:id", get(get_my_watchlist))
        .route("/watchlists/:id", axum::routing::delete(delete_watchlist))
        .route("/watchlists/:id/upgrade", post(upgrade_watchlist))
}

#[derive(Debug, Serialize)]
pub struct WatchlistOut {
    #[serde(flatten)]
    pub row: Watchlist,
}

// ── 创建 ──

async fn create_watchlist(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<WatchlistCreate>,
) -> Result<Json<WatchlistOut>, ApiError> {
    // 配额校验：按用户档位差异化上限。
    let user = acct::get_user(&state.db, auth.user_id)
        .await
        .map_err(map_db)?;
    let count = acct::count_watchlists_by_user(&state.db, auth.user_id)
        .await
        .map_err(map_db)?;
    let limit = watchlist_limit(&user.subscription_tier);
    if count >= limit {
        return Err(ApiError::TooManyRequests(format!(
            "watchlist 配额已满（{count}/{limit}，{tier} 档位）",
            tier = user.subscription_tier
        )));
    }

    let row = match body {
        WatchlistCreate::Trader {
            watch_platform,
            watch_address,
        } => {
            validate_platform(&watch_platform)?;
            acct::create_watchlist_trader(&state.db, auth.user_id, &watch_platform, &watch_address)
                .await
                .map_err(map_db)?
        }
        WatchlistCreate::Identity { watch_identity_id } => {
            // 观察无门控：identity 不须 manual_verified（区别于 follow）。
            // 仅校验 identity 存在（避免收藏不存在的 identity）。
            identity_q::get_identity(&state.db, watch_identity_id)
                .await
                .map_err(map_db)?;
            acct::create_watchlist_identity(&state.db, auth.user_id, watch_identity_id)
                .await
                .map_err(map_db)?
        }
    };
    Ok(Json(WatchlistOut { row }))
}

// ── 列出 / 单条 ──

async fn list_my_watchlists(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Vec<Watchlist>>, ApiError> {
    let rows = acct::list_watchlists_by_user(&state.db, auth.user_id)
        .await
        .map_err(map_db)?;
    Ok(Json(rows))
}

async fn get_my_watchlist(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WatchlistOut>, ApiError> {
    let row = acct::get_watchlist(&state.db, id).await.map_err(map_db)?;
    if row.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权查看他人 watchlist".into()));
    }
    Ok(Json(WatchlistOut { row }))
}

// ── 删除 ──

async fn delete_watchlist(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = acct::get_watchlist(&state.db, id).await.map_err(map_db)?;
    if existing.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权删除他人 watchlist".into()));
    }
    acct::delete_watchlist(&state.db, id)
        .await
        .map_err(map_db)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

// ── 一键升级为 Follow ──
//
// 事务内：INSERT follow_relation + DELETE watchlist（消费式升级）。
// 升级即进入执行路径，故门控与 `create_follow` 一致：
//   - trader：botfilter 标 bot → 拒绝（watchlist 保留）
//   - identity：须 manual_verified=true → 否则拒绝（watchlist 保留）
// 门控失败时**不进事务**，watchlist 原样保留，返回 4xx 让用户看到拒绝原因。

#[derive(Debug, Serialize)]
pub struct UpgradeOut {
    pub watchlist_id: Uuid,
    #[serde(flatten)]
    pub follow: FollowRelation,
}

async fn upgrade_watchlist(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WatchlistUpgrade>,
) -> Result<Json<UpgradeOut>, ApiError> {
    validate_platform(&body.execute_venue)?;
    validate_channel(&body.channel)?;
    // 管辖域校验：升级即进入执行路径，execute_venue 须被用户 jurisdiction 允许。
    ensure_venue_allowed_for_user(&state, auth.user_id, &body.execute_venue).await?;
    if body.config.execute_venue
        != body
            .execute_venue
            .parse::<Platform>()
            .ok()
            .unwrap_or(Platform::Polymarket)
    {
        // 宽容：以 body.execute_venue 为准，与 create_follow 一致不强制报错。
    }
    let config_json = serde_json::to_value(&body.config)
        .map_err(|e| ApiError::BadRequest(format!("config 序列化失败: {e}")))?;

    // 1. 取 watchlist 并校验归属
    let wl = acct::get_watchlist(&state.db, id).await.map_err(map_db)?;
    if wl.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权升级他人 watchlist".into()));
    }

    // 2. 门控（读，事务前）：trader → botfilter；identity → manual_verified
    match (
        wl.watch_platform.as_deref(),
        wl.watch_address.as_deref(),
        wl.watch_identity_id,
    ) {
        (Some(platform), Some(address), None) => {
            let tags = perf_q::get_trader_tag(&state.db, platform, address)
                .await
                .map_err(map_db)?;
            if tags.iter().any(|t| t == "bot") {
                return Err(ApiError::BadRequest(format!(
                    "拒绝升级：该交易者被 botfilter 标记为机器人（{platform}/{address}）"
                )));
            }
        }
        (None, None, Some(identity_id)) => {
            let identity = identity_q::get_identity(&state.db, identity_id)
                .await
                .map_err(map_db)?;
            if !identity.manual_verified {
                return Err(ApiError::BadRequest(
                    "升级为 Follow 须 identity manual_verified=true".into(),
                ));
            }
        }
        _ => return Err(ApiError::Internal("watchlist 目标二选一约束损坏".into())),
    }

    // 3. 事务：INSERT follow_relation + DELETE watchlist
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiError::Internal(format!("开启事务失败: {e}")))?;

    let follow: FollowRelation = match (
        wl.watch_platform.as_deref(),
        wl.watch_address.as_deref(),
        wl.watch_identity_id,
    ) {
        (Some(platform), Some(address), None) => {
            let row = sqlx::query_as::<_, FollowRelation>(
                r#"
                INSERT INTO account.follow_relation
                    (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only)
                VALUES ($1,$2,$3,$4,$5,$6,$7)
                RETURNING *
                "#,
            )
            .bind(auth.user_id)
            .bind(platform)
            .bind(address)
            .bind(body.execute_venue)
            .bind(body.channel)
            .bind(&config_json)
            .bind(body.config.same_venue_only)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| map_db(e.into()))?;
            row
        }
        (None, None, Some(identity_id)) => {
            let row = sqlx::query_as::<_, FollowRelation>(
                r#"
                INSERT INTO account.follow_relation
                    (user_id, follow_identity_id, execute_venue, channel, config, same_venue_only)
                VALUES ($1,$2,$3,$4,$5,$6)
                RETURNING *
                "#,
            )
            .bind(auth.user_id)
            .bind(identity_id)
            .bind(body.execute_venue)
            .bind(body.channel)
            .bind(&config_json)
            .bind(body.config.same_venue_only)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| map_db(e.into()))?;
            row
        }
        _ => return Err(ApiError::Internal("watchlist 目标二选一约束损坏".into())),
    };

    // 删除 watchlist（消费式升级）
    let deleted = sqlx::query("DELETE FROM account.watchlist WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("删除 watchlist 失败: {e}")))?;
    if deleted.rows_affected() == 0 {
        // 并发：watchlist 已被删（如用户同时点了删除）。回滚 follow，避免悬空 follow。
        return Err(ApiError::NotFound(format!("watchlist {id} 已不存在")));
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(format!("提交事务失败: {e}")))?;

    tracing::info!(watchlist_id = %id, follow_id = %follow.id, "watchlist 升级为 follow");
    Ok(Json(UpgradeOut {
        watchlist_id: id,
        follow,
    }))
}

// 校验/映射 helper（validate_platform / validate_channel / map_db / ensure_venue_allowed_for_user）
// 已收敛到 `crate::routes`，本模块直接 import，避免重复。
