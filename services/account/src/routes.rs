//! 路由聚合。对应 `docs/ARCHITECTURE.md` §6.4 对外 API。
//!
//! 身份方式：钱包登录（SIWE / EIP-4361）或 TG 登录。邮箱认证已移除。
//!
//! - `POST /auth/tg`（TG bot 代签）
//! - `GET  /auth/wallet/nonce`（钱包登录：签发一次性 nonce）
//! - `POST /auth/wallet`（钱包登录：SIWE 验签 → upsert → 发 JWT）
//! - `GET  /me`
//! - `POST /me/subscription`
//! - `POST /me/venue-credentials/{platform}`
//! - `GET  /me/venue-credentials`
//! - `POST /me/daemon-api-key`（轮换，返回明文一次）
//! - `GET/POST/DELETE /me/wallets`（已登录用户多钱包管理 / 恢复因子）
//! - `GET /healthz` / `GET /readyz`

use crate::auth::{hash_password, issue_jwt, AuthUser};
use crate::error::ApiError;
use crate::siwe;
use crate::state::AppState;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sharpside_db::queries::account as acct;
use sharpside_db::UserWallet;

pub fn router(state: AppState) -> Router {
    // /auth/* 路由组：单独挂限流中间件（按 IP，防暴力撞库 / 注册刷量）。
    // from_fn_with_state 显式注入 AppState，使中间件可取 state.auth_limiter。
    let auth_routes = Router::new()
        .route("/auth/tg", post(tg_login))
        .route("/auth/wallet/nonce", get(wallet_nonce))
        .route("/auth/wallet", post(wallet_login))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::rate_limit::auth_middleware,
        ));

    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .merge(auth_routes)
        .route("/me", get(me))
        .route("/me/subscription", post(update_subscription))
        .route("/me/venue-credentials", get(list_credentials))
        .route("/me/venue-credentials/:platform", post(upsert_credential))
        .route("/me/delegation", get(get_delegation))
        .route("/me/daemon-api-key", post(rotate_daemon_key))
        .route("/me/wallets", get(list_wallets).post(link_wallet))
        .route("/me/wallets/:address", axum::routing::delete(unlink_wallet))
        .route(
            "/me/deposit-wallet/provision",
            post(provision_deposit_wallet),
        )
        .with_state(state)
}

/// `POST /me/deposit-wallet/provision` —— 通道 A 一次性预配 deposit wallet。
///
/// 对应 `docs/CHANNEL_A_SIGNING.md` §3.1。生成 owner EOA → KMS 加密 → CREATE2 派生
/// → Relayer 部署 → L1 派生 L2 凭证 → batch approve → 余额同步 → 入库。
/// 离线模式（默认）跳过网络步骤，仅完成本地可闭环部分；在线模式需 env
/// `POLYMARKET_PROVISION_LIVE=1` + `POLYMARKET_DEPOSIT_INIT_CODE_HASH` + `POLYMARKET_BUILDER_API_KEY`。
#[derive(Debug, Deserialize)]
pub struct ProvisionBody {
    /// Polymarket Builder Code（归因 + 免 gas + fee）。默认 `sharpside-builder`。
    #[serde(default = "default_builder_code")]
    pub builder_code: String,
}

fn default_builder_code() -> String {
    "sharpside-builder".into()
}

async fn provision_deposit_wallet(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<ProvisionBody>,
) -> Result<Json<crate::deposit::ProvisionResponse>, ApiError> {
    let resp = crate::deposit::provision(state, auth.user_id, body.builder_code).await?;
    Ok(Json(resp))
}

async fn readyz(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    sharpside_db::ping(&state.db).await?;
    Ok(Json(serde_json::json!({ "db": "ok" })))
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: sharpside_db::User,
}

async fn me(state: AppState, auth: AuthUser) -> Result<Json<sharpside_db::User>, ApiError> {
    match acct::get_user(&state.db, auth.user_id).await {
        Ok(user) => Ok(Json(user)),
        // JWT 有效但用户已不存在 → 当作会话失效，前端清 token 并重连钱包。
        Err(sharpside_db::DbError::NotFound(_)) => {
            Err(ApiError::Unauthorized("user no longer exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}

/// `POST /auth/tg` —— TG bot 代用户换 JWT。
///
/// 请求体 `{tg_id, username?}`；须带 `X-TG-Bot-Secret` 头匹配 `config.tg_bot_secret`。
/// 按 `tg_id` upsert 用户（首次自动建账，web 与 TG 共用身份），签发 JWT 返回。
#[derive(Debug, Deserialize)]
pub struct TgLoginBody {
    pub tg_id: i64,
}

async fn tg_login(
    state: AppState,
    headers: HeaderMap,
    Json(body): Json<TgLoginBody>,
) -> Result<Json<AuthResponse>, ApiError> {
    let provided = headers
        .get("X-TG-Bot-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !sharpside_shared::secrets::constant_time_eq(
        provided.as_bytes(),
        state.config.tg_bot_secret.as_bytes(),
    ) {
        return Err(ApiError::Unauthorized("invalid tg-bot secret".into()));
    }
    let user = acct::upsert_tg_user(&state.db, body.tg_id).await?;
    let token = issue_jwt(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_ttl_seconds,
    )?;
    Ok(Json(AuthResponse { token, user }))
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionBody {
    pub tier: String,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}

async fn update_subscription(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<SubscriptionBody>,
) -> Result<Json<sharpside_db::User>, ApiError> {
    if !matches!(body.tier.as_str(), "free" | "pro_plus") {
        return Err(ApiError::BadRequest("tier 必须为 free / pro_plus".into()));
    }
    // 安全：pro_plus 升级必须经支付/billing webhook，禁止自助升档（否则任意登录用户
    // 可白嫖 Pro+ 权益）。free（取消订阅）任何用户可自助。生产环境直接拒绝 pro_plus
    // 升级；dev/测试环境保留 F0「测试开通」能力（APP_ENV != production）。
    if body.tier == "pro_plus" && sharpside_shared::secrets::is_production() {
        return Err(ApiError::Forbidden(
            "pro_plus 升级需通过支付回调，禁止自助升档".into(),
        ));
    }
    let user = acct::update_subscription(&state.db, auth.user_id, &body.tier, body.until).await?;
    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
pub struct CredentialBody {
    /// 加密凭证 blob（account 服务只存密文，KMS 加密由调用方/上游完成）
    pub encrypted_blob: serde_json::Value,
}

async fn upsert_credential(
    state: AppState,
    auth: AuthUser,
    Path(platform): Path<String>,
    Json(body): Json<CredentialBody>,
) -> Result<Json<sharpside_db::UserVenueCredential>, ApiError> {
    let cred =
        acct::upsert_credential(&state.db, auth.user_id, &platform, &body.encrypted_blob).await?;
    Ok(Json(cred))
}

async fn list_credentials(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Vec<sharpside_db::UserVenueCredential>>, ApiError> {
    let rows = acct::list_credentials(&state.db, auth.user_id).await?;
    Ok(Json(rows))
}

/// daemon_api_key 轮换：生成新明文 key 一次返回，库内只存 hash。
async fn rotate_daemon_key(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let plain = uuid::Uuid::new_v4().to_string();
    let hash = hash_password(&plain)?;
    acct::set_daemon_api_key_hash(&state.db, auth.user_id, &hash).await?;
    Ok(Json(serde_json::json!({
        "daemon_api_key": plain,
        "note": "明文仅此一次返回，请妥善保存；库内仅存 hash"
    })))
}

// ── 钱包登录（模型 A · 身份钱包）──

/// 校验地址格式（0x + 40 hex），并规范化为小写。
fn normalize_address(addr: &str) -> Result<String, ApiError> {
    let a = addr.trim();
    if !a.starts_with("0x") || a.len() != 42 {
        return Err(ApiError::BadRequest("address 须为 0x + 40 hex".into()));
    }
    if !a[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("address 含非 hex 字符".into()));
    }
    Ok(a.to_lowercase())
}

#[derive(Debug, Deserialize)]
pub struct NonceQuery {
    pub address: String,
}

/// `GET /auth/wallet/nonce?address=0x...` — 签发一次性 nonce，供前端拼装 SIWE 消息。
async fn wallet_nonce(
    state: AppState,
    axum::extract::Query(q): axum::extract::Query<NonceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = normalize_address(&q.address)?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    acct::issue_nonce(&state.db, &address, &nonce).await?;
    Ok(Json(serde_json::json!({
        "nonce": nonce,
        "domain": state.config.public_domain,
        "chain_id": state.config.siwe_allowed_chains.first().copied().unwrap_or(137),
        "issued_at": chrono::Utc::now(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct WalletLoginBody {
    pub message: String,
    pub signature: String,
}

/// `POST /auth/wallet { message, signature }` — SIWE 验签 → 消费 nonce → upsert → 发 JWT。
async fn wallet_login(
    state: AppState,
    Json(body): Json<WalletLoginBody>,
) -> Result<Json<AuthResponse>, ApiError> {
    let msg = siwe::verify_and_validate(
        &body.message,
        &body.signature,
        &state.config.public_domain,
        &state.config.siwe_allowed_chains,
        state.config.siwe_max_age_secs,
    )?;
    let address = siwe::address_hex(&msg);
    // 原子消费 nonce（防重放）
    if !acct::consume_nonce(&state.db, &address, &msg.nonce).await? {
        return Err(ApiError::Unauthorized(
            "nonce invalid or already used".into(),
        ));
    }
    let user = acct::upsert_wallet_user(&state.db, &address).await?;
    let token = issue_jwt(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_ttl_seconds,
    )?;
    Ok(Json(AuthResponse { token, user }))
}

// ── 已登录用户多钱包管理（恢复因子）──

#[derive(Debug, Deserialize)]
pub struct LinkWalletBody {
    pub address: String,
    pub label: Option<String>,
}

/// `POST /me/wallets` — 绑定第二个钱包。地址须小写规范化。
async fn link_wallet(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<LinkWalletBody>,
) -> Result<Json<UserWallet>, ApiError> {
    let address = normalize_address(&body.address)?;
    let w = acct::link_wallet(&state.db, auth.user_id, &address, body.label.as_deref()).await?;
    Ok(Json(w))
}

/// `GET /me/wallets` — 列出用户所有钱包。
async fn list_wallets(state: AppState, auth: AuthUser) -> Result<Json<Vec<UserWallet>>, ApiError> {
    let rows = acct::list_wallets(&state.db, auth.user_id).await?;
    Ok(Json(rows))
}

/// `DELETE /me/wallets/:address` — 解绑钱包。禁止删除最后一个（避免账号无入口）。
async fn unlink_wallet(
    state: AppState,
    auth: AuthUser,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = normalize_address(&address)?;
    if acct::count_wallets(&state.db, auth.user_id).await? <= 1 {
        return Err(ApiError::BadRequest("不可删除最后一个钱包".into()));
    }
    let ok = acct::unlink_wallet(&state.db, auth.user_id, &address).await?;
    if !ok {
        return Err(ApiError::NotFound(format!("钱包 {address} 不属于当前用户")));
    }
    Ok(Json(serde_json::json!({ "unlinked": address })))
}

/// `GET /me/delegation` — 委托管理安全视图。对应 `docs/FRONTEND_DESIGN.md` §6.4。
///
/// 解析 polymarket 凭证 blob 的**非密字段**（deposit_wallet_address / owner_address /
/// l2_api_key / builder_code / kind），密钥（encrypted_owner_key / encrypted_l2_secret）
/// 绝不返回。无凭证时返 404，前端引导预配。
#[derive(Debug, Serialize)]
pub struct DelegationView {
    pub platform: String,
    pub custody_tier: String,
    pub custody_label: String,
    pub deposit_wallet_address: Option<String>,
    pub owner_address: Option<String>,
    pub builder_code: Option<String>,
    pub l2_api_key: Option<String>,
    /// 是否完成在线全流程（离线模式 = false，跳过 relayer/approve）。
    pub provision_live: bool,
    /// 预配步骤状态（8 步；done / skipped / pending / failed）。前端 stepper 用。
    pub provision_steps: Vec<StepStatus>,
    pub kms_key_id: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Phase 2 前不可自助撤销。
    pub can_revoke: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Done,
    Skipped,
    #[allow(dead_code)]
    Pending,
    #[allow(dead_code)]
    Failed,
}

async fn get_delegation(state: AppState, auth: AuthUser) -> Result<Json<DelegationView>, ApiError> {
    let blob = acct::get_credential_blob(&state.db, auth.user_id, "polymarket").await?;
    let blob = match blob {
        Some(b) => b,
        None => return Err(ApiError::NotFound("polymarket 凭证未预配".into())),
    };
    let _kind = blob
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let deposit_wallet_address = blob
        .get("deposit_wallet_address")
        .and_then(|v| v.as_str())
        .map(String::from);
    let owner_address = blob
        .get("owner_address")
        .and_then(|v| v.as_str())
        .map(String::from);
    let builder_code = blob
        .get("builder_code")
        .and_then(|v| v.as_str())
        .map(String::from);
    let l2_api_key = blob
        .get("l2_api_key")
        .and_then(|v| v.as_str())
        .map(String::from);

    // 优先读持久化字段；旧 blob 无字段时回退推断（兼容重新预配前的凭证）。
    let provision_live = blob
        .get("provision_live")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| {
            l2_api_key
                .as_deref()
                .map(|k| !k.starts_with("dev-"))
                .unwrap_or(false)
        });

    let provision_steps = parse_provision_steps(blob.get("provision_steps"))
        .unwrap_or_else(|| infer_provision_steps(provision_live));

    let kms_key_id = blob
        .get("kms_key_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| Some(state.kms.name().to_string()));

    Ok(Json(DelegationView {
        platform: "polymarket".into(),
        custody_tier: "delegated".into(),
        custody_label: "委托交易（未到完全非托管）".into(),
        deposit_wallet_address,
        owner_address,
        builder_code,
        l2_api_key,
        provision_live,
        provision_steps,
        kms_key_id,
        created_at: None,
        can_revoke: false,
    }))
}

/// 从 blob.provision_steps 解析 8 步；长度不对或非法值则返回 None（走推断兜底）。
fn parse_provision_steps(v: Option<&serde_json::Value>) -> Option<Vec<StepStatus>> {
    let arr = v?.as_array()?;
    if arr.len() != 8 {
        return None;
    }
    let mut out = Vec::with_capacity(8);
    for item in arr {
        let s = item.as_str()?;
        out.push(match s {
            "done" => StepStatus::Done,
            "skipped" => StepStatus::Skipped,
            "pending" => StepStatus::Pending,
            "failed" => StepStatus::Failed,
            _ => return None,
        });
    }
    Some(out)
}

/// 旧 blob 无 provision_steps 时的推断（与历史行为一致）。
fn infer_provision_steps(live: bool) -> Vec<StepStatus> {
    if live {
        vec![
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
        ]
    } else {
        vec![
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Done,
            StepStatus::Skipped,
            StepStatus::Skipped,
            StepStatus::Skipped,
            StepStatus::Skipped,
            StepStatus::Done,
        ]
    }
}
