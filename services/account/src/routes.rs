//! 路由聚合。对应 `docs/ARCHITECTURE.md` §6.4 对外 API。
//!
//! 身份方式：钱包登录（SIWE / EIP-4361）或 TG 登录。邮箱认证已移除。
//!
//! - `POST /auth/tg`（TG bot 代签）
//! - `GET  /auth/wallet/nonce`（钱包登录：签发一次性 nonce）
//! - `POST /auth/wallet`（浏览器：SIWE → cookie-only，body 不含 token）
//! - `POST /auth/wallet/token`（程序化：SIWE → body 含 token + cookie）
//! - `GET  /me`
//! - `POST /me/subscription`
//! - `POST /me/billing/invoices` · `GET /me/billing/invoices/active`
//! - `POST /me/billing/invoices/{id}/submit-tx` · `GET /me/billing/history`
//! - `POST /internal/billing/confirm`（须 X-Internal-Secret；gateway 屏蔽）
//! - `GET  /me/venue-credentials`（列表；密文不回传）
//! - `POST /internal/venue-credentials/{user_id}/{platform}`（内部/运维 upsert，须 X-Internal-Secret）
//! - `POST /me/daemon-api-key`（轮换，返回明文一次）
//! - `GET/POST/DELETE /me/wallets`（已登录用户多钱包管理 / 恢复因子）
//! - `GET /healthz` / `GET /readyz`

use crate::auth::{hash_password, issue_jwt, AuthUser};
use crate::error::ApiError;
use crate::siwe;
use crate::state::AppState;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sharpside_db::queries::account as acct;
use sharpside_db::UserWallet;
use uuid::Uuid;

pub fn router(state: AppState) -> Router {
    // /auth/* 路由组：单独挂限流中间件（按 IP，防暴力撞库 / 注册刷量）。
    // from_fn_with_state 显式注入 AppState，使中间件可取 state.auth_limiter。
    let auth_routes = Router::new()
        .route("/auth/tg", post(tg_login))
        .route("/auth/wallet/nonce", get(wallet_nonce))
        .route("/auth/wallet", post(wallet_login))
        .route("/auth/wallet/token", post(wallet_login_token))
        .route("/auth/logout", post(logout))
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
        .route(
            "/internal/venue-credentials/:user_id/:platform",
            post(internal_upsert_credential),
        )
        .route("/me/delegation", get(get_delegation))
        .route("/me/delegation/archives", get(list_delegation_archives))
        .route("/me/daemon-api-key", post(rotate_daemon_key))
        .route("/me/wallets", get(list_wallets).post(link_wallet))
        .route("/me/wallets/:address", axum::routing::delete(unlink_wallet))
        .route(
            "/me/deposit-wallet/provision",
            post(provision_deposit_wallet),
        )
        .route("/me/deposit-wallet/revoke", post(revoke_deposit_wallet))
        .route(
            "/me/deposit-wallet/migrate-archive",
            post(migrate_archive),
        )
        .route(
            "/me/deposit-wallet/archives/:id/redeemable",
            get(list_archive_redeemable),
        )
        .route(
            "/me/deposit-wallet/archives/:id/redeem",
            post(redeem_archive),
        )
        .merge(crate::billing::routes::router())
        .with_state(state)
}

/// `POST /me/deposit-wallet/provision` —— 通道 A 一次性预配 deposit wallet。
///
/// 对应 `docs/CHANNEL_A_SIGNING.md` §3.1。生成 owner EOA → KMS 加密 → CREATE2 派生
/// → Relayer 部署 → L1 派生 L2 凭证 → batch approve → 余额同步 → 入库。
/// 离线模式（默认）跳过网络步骤，仅完成本地可闭环部分；在线模式需 env
/// `POLYMARKET_PROVISION_LIVE=1` + `POLYMARKET_DEPOSIT_INIT_CODE_HASH` + `POLYMARKET_BUILDER_API_KEY`。
///
/// 若已有**活跃**凭证，须 `confirm_replace: true`，否则 409；替换前旧密文写入
/// `credential_archives`。
#[derive(Debug, Deserialize)]
pub struct ProvisionBody {
    /// Polymarket Builder Code（归因 + 免 gas + fee）。默认 `sharpside-builder`。
    #[serde(default = "default_builder_code")]
    pub builder_code: String,
    /// 显式确认替换已有活跃凭证（否则 409）。已撤销凭证可直接重新预配。
    #[serde(default)]
    pub confirm_replace: bool,
}

fn default_builder_code() -> String {
    "sharpside-builder".into()
}

async fn provision_deposit_wallet(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<ProvisionBody>,
) -> Result<Json<crate::deposit::ProvisionResponse>, ApiError> {
    if let Some(existing) = acct::get_credential(&state.db, auth.user_id, "polymarket").await? {
        if existing.revoked_at.is_none() && !body.confirm_replace {
            return Err(ApiError::Conflict(
                "已有活跃 polymarket 凭证：须 confirm_replace=true 以替换（旧密文将归档；旧 Deposit Wallet 资金可能需手动迁移）".into(),
            ));
        }
    }
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

/// 浏览器会话响应：仅 `user` + HttpOnly cookie，body **不含** token。
#[derive(Debug, Serialize)]
pub struct SessionResponse {
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
) -> Result<axum::response::Response, ApiError> {
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
    Ok(auth_cookie_and_token(
        token,
        user,
        state.config.cookie_secure,
        state.config.jwt_ttl_seconds,
    ))
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
    /// 加密凭证 blob（须为服务端/运维构造的 KMS 密文形态；本端点不做客户端开放写入）。
    pub encrypted_blob: serde_json::Value,
    /// 可选：Deposit Wallet / proxy 地址。
    pub proxy_address: Option<String>,
}

/// `POST /internal/venue-credentials/{user_id}/{platform}` —— 内部/运维 upsert。
///
/// 须 `X-Internal-Secret` 匹配 `ACCOUNT_INTERNAL_SECRET`。gateway 对
/// `/api/account/internal/*` 直接 404，仅私网可达。覆盖前自动归档旧密文。
async fn internal_upsert_credential(
    state: AppState,
    headers: HeaderMap,
    Path((user_id, platform)): Path<(Uuid, String)>,
    Json(body): Json<CredentialBody>,
) -> Result<Json<sharpside_db::UserVenueCredential>, ApiError> {
    let secret = state.config.internal_secret.trim();
    if secret.is_empty() {
        return Err(ApiError::Unauthorized(
            "ACCOUNT_INTERNAL_SECRET 未配置，拒绝内部写凭证".into(),
        ));
    }
    let got = headers
        .get("x-internal-secret")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !sharpside_shared::secrets::constant_time_eq(got.as_bytes(), secret.as_bytes()) {
        return Err(ApiError::Unauthorized("internal secret 不匹配".into()));
    }
    if platform.trim().is_empty() {
        return Err(ApiError::BadRequest("platform 不能为空".into()));
    }
    // 覆盖前归档（与 provision 路径一致）。
    acct::archive_credential_if_exists(&state.db, user_id, &platform).await?;
    let cred = acct::upsert_credential_with_proxy(
        &state.db,
        user_id,
        &platform,
        &body.encrypted_blob,
        body.proxy_address.as_deref(),
    )
    .await?;
    tracing::info!(
        op = "internal_upsert_credential",
        %user_id,
        platform = %platform,
        "内部凭证 upsert"
    );
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
    // 顺带清理过期 nonce（best-effort，失败不阻塞签发）。
    let _ = acct::cleanup_stale_nonces(&state.db, state.config.siwe_max_age_secs).await;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    acct::issue_nonce(&state.db, &address, &nonce).await?;
    Ok(Json(serde_json::json!({
        "nonce": nonce,
        "domain": state.config.public_domain,
        "uri": state.config.siwe_preferred_uri,
        "chain_id": state.config.siwe_allowed_chains.first().copied().unwrap_or(137),
        "issued_at": chrono::Utc::now(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct WalletLoginBody {
    pub message: String,
    pub signature: String,
}

/// `POST /auth/wallet { message, signature }` — 浏览器路径：SIWE → cookie-only。
/// body 仅含 `user`，**不含** token（防 XSS/响应截获读 JWT）。
async fn wallet_login(
    state: AppState,
    Json(body): Json<WalletLoginBody>,
) -> Result<axum::response::Response, ApiError> {
    let (user, token) = wallet_login_core(&state, &body).await?;
    Ok(auth_cookie_only(
        token,
        user,
        state.config.cookie_secure,
        state.config.jwt_ttl_seconds,
    ))
}

/// `POST /auth/wallet/token { message, signature }` — 程序化路径：SIWE → body 含 token。
/// 供集成测试 / 脚本 / 非浏览器客户端；同时写 HttpOnly cookie。
async fn wallet_login_token(
    state: AppState,
    Json(body): Json<WalletLoginBody>,
) -> Result<axum::response::Response, ApiError> {
    let (user, token) = wallet_login_core(&state, &body).await?;
    Ok(auth_cookie_and_token(
        token,
        user,
        state.config.cookie_secure,
        state.config.jwt_ttl_seconds,
    ))
}

/// SIWE 验签 → 消费 nonce → upsert → 签发 JWT。login / token 两入口共用。
async fn wallet_login_core(
    state: &AppState,
    body: &WalletLoginBody,
) -> Result<(sharpside_db::User, String), ApiError> {
    let msg = siwe::verify_and_validate(
        &body.message,
        &body.signature,
        &state.config.public_domain,
        &state.config.siwe_allowed_uris,
        &state.config.siwe_allowed_chains,
        state.config.siwe_max_age_secs,
    )?;
    let address = siwe::address_hex(&msg);
    if !acct::consume_nonce(
        &state.db,
        &address,
        &msg.nonce,
        state.config.siwe_max_age_secs,
    )
    .await?
    {
        return Err(ApiError::Unauthorized(
            "nonce invalid, expired, or already used".into(),
        ));
    }
    let user = acct::upsert_wallet_user(&state.db, &address).await?;
    let token = issue_jwt(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_ttl_seconds,
    )?;
    Ok((user, token))
}

/// `POST /auth/logout` — 吊销当前 JWT（写 jti 入 denylist）。
///
/// 从 `Authorization: Bearer <token>` 取 token，验签拿 jti + user_id，写 denylist。
/// 此后该 token 在任何校验点（account / copier）立即 401。幂等：重复登出同 jti 不报错。
async fn logout(
    state: AppState,
    headers: HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    // 安全修复 3.1：优先从 cookie 取 token，回退 Bearer。
    let token = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(sharpside_shared::session::extract_token_from_cookie_header)
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(|s| s.trim().to_string())
        })
        .ok_or_else(|| ApiError::Unauthorized("missing session token".into()))?;
    let claims = crate::auth::verify_jwt(&token, &state.config.jwt_secret)?;
    let user_id = claims
        .sub
        .parse::<uuid::Uuid>()
        .map_err(|_| ApiError::Unauthorized("invalid subject".into()))?;
    acct::revoke_jwt(&state.db, &claims.jti, user_id).await?;
    // 清 cookie + 200。
    let cookie = sharpside_shared::session::clear_set_cookie(state.config.cookie_secure);
    let mut resp = axum::Json(serde_json::json!({ "ok": true })).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
    }
    Ok(resp)
}

/// 浏览器路径：JSON `{ user }` + `Set-Cookie`（HttpOnly），body **不含** token。
fn auth_cookie_only(
    token: String,
    user: sharpside_db::User,
    secure: bool,
    ttl_seconds: i64,
) -> axum::response::Response {
    let cookie = sharpside_shared::session::build_set_cookie(&token, ttl_seconds, secure);
    let mut resp = axum::Json(SessionResponse { user }).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
    }
    resp
}

/// 程序化路径：JSON `{ token, user }` + `Set-Cookie`（兼容脚本 / TG 以外的 Bearer 客户端）。
fn auth_cookie_and_token(
    token: String,
    user: sharpside_db::User,
    secure: bool,
    ttl_seconds: i64,
) -> axum::response::Response {
    let cookie = sharpside_shared::session::build_set_cookie(&token, ttl_seconds, secure);
    let mut resp = axum::Json(AuthResponse { token, user }).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(axum::http::header::SET_COOKIE, v);
    }
    resp
}

// ── 已登录用户多钱包管理（恢复因子）──

#[derive(Debug, Deserialize)]
pub struct LinkWalletBody {
    /// SIWE 消息文本（EIP-4361）。前端先 `GET /auth/wallet/nonce?address=<待绑钱包>`
    /// 取 nonce，拼入消息后由待绑钱包私钥 EIP-191 签名。
    pub message: String,
    /// 对应 EIP-191 签名（0x + 130 hex）。
    pub signature: String,
    pub label: Option<String>,
}

/// `POST /me/wallets` — 绑定第二个钱包（恢复因子）。
///
/// **安全**：必须由待绑钱包私钥签名证明所有权（SIWE），地址从验签消息权威导出，
/// 不信任客户端传入的 `address`。nonce 原子消费防重放。这堵死「偷 JWT 即可绑
/// 任意地址 → 提现到该地址」的资金流失向量（提现目标白名单 = 已绑钱包）。
async fn link_wallet(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<LinkWalletBody>,
) -> Result<Json<UserWallet>, ApiError> {
    let msg = siwe::verify_and_validate(
        &body.message,
        &body.signature,
        &state.config.public_domain,
        &state.config.siwe_allowed_uris,
        &state.config.siwe_allowed_chains,
        state.config.siwe_max_age_secs,
    )?;
    let address = siwe::address_hex(&msg);
    // 原子消费 nonce（防重放）：与 wallet_login 同机制。
    if !acct::consume_nonce(
        &state.db,
        &address,
        &msg.nonce,
        state.config.siwe_max_age_secs,
    )
    .await?
    {
        return Err(ApiError::Unauthorized(
            "nonce invalid, expired, or already used".into(),
        ));
    }
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
    /// 安全修复 2.2：撤销时间。`None` = 活跃；`Some` = 已撤销（前端 stepper 显示已撤销锁）。
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 是否可撤销（活跃凭证 = true；已撤销 = false）。
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
    // 安全修复 2.2：读完整行（含 revoked_at），而非仅 blob。
    let rows = acct::list_credentials(&state.db, auth.user_id).await?;
    let row = rows
        .into_iter()
        .find(|c| c.platform == "polymarket")
        .ok_or_else(|| ApiError::NotFound("polymarket 凭证未预配".into()))?;
    let blob = &row.encrypted_blob;
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

    let revoked_at = row.revoked_at;
    let can_revoke = revoked_at.is_none();

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
        created_at: Some(row.created_at),
        revoked_at,
        can_revoke,
    }))
}

/// `GET /me/delegation/archives` — 历史 Deposit Wallet（重新预配归档）。
/// 只返非密字段 + 链上余额；encrypted_* 永不回传。
async fn list_delegation_archives(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Vec<crate::migrate::ArchiveView>>, ApiError> {
    let rows = crate::migrate::list_archives(&state, auth.user_id).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct MigrateArchiveBody {
    pub archive_id: i64,
}

/// `POST /me/deposit-wallet/migrate-archive` — 将归档 DW 上全部 pUSD 迁到当前活跃 DW。
/// 旧 DW 未部署时先 Relayer WALLET-CREATE，再 WALLET batch transfer。
async fn migrate_archive(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<MigrateArchiveBody>,
) -> Result<Json<crate::migrate::MigrateResponse>, ApiError> {
    let resp = crate::migrate::migrate_archive(&state, auth.user_id, body.archive_id).await?;
    Ok(Json(resp))
}

/// `GET /me/deposit-wallet/archives/:id/redeemable` — 归档旧 DW 可赎回列表。
async fn list_archive_redeemable(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<crate::migrate::ArchiveRedeemableItem>>, ApiError> {
    let rows = crate::migrate::list_archive_redeemable(&state, auth.user_id, id).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct ArchiveRedeemBody {
    pub condition_id: String,
}

/// `POST /me/deposit-wallet/archives/:id/redeem` — 在归档旧 DW 上赎回已结算仓位。
/// pUSD 留在旧 DW；再调用 migrate-archive 迁到当前钱包。
async fn redeem_archive(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<ArchiveRedeemBody>,
) -> Result<Json<crate::migrate::ArchiveRedeemResponse>, ApiError> {
    let resp =
        crate::migrate::redeem_archive(&state, auth.user_id, id, &body.condition_id).await?;
    Ok(Json(resp))
}

/// `POST /me/deposit-wallet/revoke` —— 撤销委托凭证（安全修复 2.2）。
///
/// 置 `user_venue_credentials.revoked_at=now()`、`revoked_by=user_id`，不可逆。
/// copier `load_credential` 读到 `revoked_at IS NOT NULL` 即拒下单（pull-based 停派发）。
/// 注意：仅撤销本平台凭证记录，不在链上撤销 Polymarket 委托（owner EOA 私钥仍由 KMS 托管，
/// 用户须另行在 Polymarket 官网/链上解除委托；本端点确保 Sharpside 不再代其下单）。
async fn revoke_deposit_wallet(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = acct::revoke_credential(&state.db, auth.user_id, "polymarket")
        .await?
        .ok_or_else(|| ApiError::NotFound("polymarket 凭证未预配".into()))?;
    tracing::info!(
        user_id = %auth.user_id,
        revoked_at = ?row.revoked_at,
        "deposit wallet 凭证已撤销（不可逆），copier 将停派发"
    );
    Ok(Json(serde_json::json!({
        "platform": "polymarket",
        "revoked_at": row.revoked_at,
        "revoked_by": row.revoked_by,
        "on_chain_revoked": false,
        "warning": "On-platform stop only: Sharpside will no longer place orders. Owner EOA remains in LocalKms; on-chain / Polymarket deposit-wallet delegation is NOT revoked—revoke separately on Polymarket / chain if needed.",
    })))
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
