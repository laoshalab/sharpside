//! Billing HTTP 路由。生产升档只经 `confirm_payment`（internal / 未来 worker），
//! 不开放 `POST /me/subscription` → pro_plus。

use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sharpside_db::queries::billing as bill;
use sharpside_db::{BillingInvoice, BillingPayment, User};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/billing/invoices", post(create_invoice))
        .route("/me/billing/invoices/active", get(active_invoice))
        .route(
            "/me/billing/invoices/:id/submit-tx",
            post(submit_tx),
        )
        .route("/me/billing/history", get(history))
        .route("/internal/billing/confirm", post(internal_confirm))
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceBody {
    /// 订阅周期天数：30 或 90。默认 30。
    #[serde(default = "default_period_days")]
    pub period_days: i32,
}

fn default_period_days() -> i32 {
    30
}

#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    #[serde(flatten)]
    pub invoice: BillingInvoice,
    /// 前端展示用：链名提示。
    pub chain_label: &'static str,
    pub instructions: String,
}

fn invoice_response(inv: BillingInvoice) -> InvoiceResponse {
    let instructions = format!(
        "向 {} 转入恰好 {} USDC（raw={}），代币 {}，chainId={}。发票 {} 前有效。",
        inv.treasury_address,
        inv.amount_usdc,
        inv.amount_raw,
        inv.token_address,
        inv.chain_id,
        inv.expires_at.to_rfc3339()
    );
    InvoiceResponse {
        invoice: inv,
        chain_label: "polygon",
        instructions,
    }
}

/// `POST /me/billing/invoices` — 创建或返回活跃 pending 发票。
async fn create_invoice(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<CreateInvoiceBody>,
) -> Result<Json<InvoiceResponse>, ApiError> {
    let cfg = &state.config;
    if !cfg.billing_enabled() {
        return Err(ApiError::BadRequest(
            "计费未配置：须设置 BILLING_TREASURY_ADDRESS 与 BILLING_USDC_ADDRESS".into(),
        ));
    }
    let period_days = body.period_days;
    if period_days != 30 && period_days != 90 {
        return Err(ApiError::BadRequest("period_days 须为 30 或 90".into()));
    }
    let amount_usdc = if period_days == 90 {
        cfg.billing_price_90d_usdc
    } else {
        cfg.billing_price_30d_usdc
    };
    let amount_raw = usdc_to_raw(amount_usdc)?;

    let inv = bill::create_or_get_pending_invoice(
        &state.db,
        auth.user_id,
        "pro_plus",
        period_days,
        amount_usdc,
        amount_raw,
        cfg.billing_chain_id,
        &cfg.billing_usdc_address,
        &cfg.billing_treasury_address,
        cfg.billing_invoice_ttl_secs,
    )
    .await;
    let inv = match inv {
        Ok(i) => i,
        Err(sharpside_db::DbError::Conflict(_)) => bill::get_pending_invoice(&state.db, auth.user_id)
            .await?
            .ok_or_else(|| ApiError::Conflict("已有未支付发票".into()))?,
        Err(e) => return Err(e.into()),
    };
    Ok(Json(invoice_response(inv)))
}

/// `GET /me/billing/invoices/active`
async fn active_invoice(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Option<InvoiceResponse>>, ApiError> {
    let inv = bill::get_pending_invoice(&state.db, auth.user_id).await?;
    Ok(Json(inv.map(invoice_response)))
}

#[derive(Debug, Deserialize)]
pub struct SubmitTxBody {
    pub tx_hash: String,
}

/// `POST /me/billing/invoices/{id}/submit-tx`
async fn submit_tx(
    state: AppState,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitTxBody>,
) -> Result<Json<BillingPayment>, ApiError> {
    let tx_hash = normalize_tx_hash(&body.tx_hash)?;
    let payment = bill::submit_payment_tx(
        &state.db,
        id,
        auth.user_id,
        state.config.billing_chain_id,
        &tx_hash,
    )
    .await?;
    Ok(Json(payment))
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub invoices: Vec<BillingInvoice>,
    pub payments: Vec<BillingPayment>,
}

/// `GET /me/billing/history`
async fn history(state: AppState, auth: AuthUser) -> Result<Json<HistoryResponse>, ApiError> {
    let invoices = bill::list_invoices_for_user(&state.db, auth.user_id, 50).await?;
    let payments = bill::list_payments_for_user(&state.db, auth.user_id, 50).await?;
    Ok(Json(HistoryResponse { invoices, payments }))
}

#[derive(Debug, Deserialize)]
pub struct InternalConfirmBody {
    pub invoice_id: Uuid,
    pub tx_hash: String,
    pub log_index: i32,
    pub from_address: String,
    pub to_address: String,
    /// 链上整数（USDC 6 位）。可用字符串或数字。
    pub amount_raw: Decimal,
    pub block_number: i64,
    #[serde(default)]
    pub chain_id: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ConfirmResponse {
    pub invoice: BillingInvoice,
    pub payment: BillingPayment,
    pub user: User,
}

/// `POST /internal/billing/confirm` — worker / 运维幂等确认。
///
/// 须 `X-Internal-Secret`；gateway 对 `/api/account/internal/*` 返回 404。
async fn internal_confirm(
    state: AppState,
    headers: HeaderMap,
    Json(body): Json<InternalConfirmBody>,
) -> Result<Json<ConfirmResponse>, ApiError> {
    require_internal_secret(&state, &headers)?;

    let tx_hash = normalize_tx_hash(&body.tx_hash)?;
    let from_address = normalize_hex_address(&body.from_address)?;
    let to_address = normalize_hex_address(&body.to_address)?;
    let chain_id = body.chain_id.unwrap_or(state.config.billing_chain_id);

    if state.config.billing_require_linked_wallet {
        let wallets = sharpside_db::queries::account::list_wallets(&state.db, {
            let inv = bill::get_invoice(&state.db, body.invoice_id).await?;
            inv.user_id
        })
        .await?;
        if !wallets.iter().any(|w| w.address == from_address) {
            return Err(ApiError::BadRequest(
                "付款地址未绑定到该用户（BILLING_REQUIRE_LINKED_WALLET）".into(),
            ));
        }
    }

    let input = bill::ConfirmPaymentInput {
        invoice_id: body.invoice_id,
        tx_hash,
        log_index: body.log_index,
        from_address,
        to_address,
        amount_raw: body.amount_raw,
        block_number: body.block_number,
        chain_id,
    };
    let (invoice, payment, user) = bill::confirm_payment(&state.db, &input).await?;
    tracing::info!(
        op = "billing_confirm",
        invoice_id = %invoice.id,
        user_id = %user.id,
        tx_hash = %payment.tx_hash,
        "Pro+ 支付已确认"
    );
    Ok(Json(ConfirmResponse {
        invoice,
        payment,
        user,
    }))
}

fn require_internal_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let secret = state.config.internal_secret.trim();
    if secret.is_empty() {
        return Err(ApiError::Unauthorized(
            "ACCOUNT_INTERNAL_SECRET 未配置，拒绝内部确认".into(),
        ));
    }
    let got = headers
        .get("x-internal-secret")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !sharpside_shared::secrets::constant_time_eq(got.as_bytes(), secret.as_bytes()) {
        return Err(ApiError::Unauthorized("internal secret 不匹配".into()));
    }
    Ok(())
}

fn usdc_to_raw(amount: Decimal) -> Result<Decimal, ApiError> {
    let scale = Decimal::from(1_000_000u32);
    let raw = (amount * scale).trunc();
    if raw <= Decimal::ZERO {
        return Err(ApiError::BadRequest("amount_usdc 无效".into()));
    }
    Ok(raw)
}

fn normalize_tx_hash(h: &str) -> Result<String, ApiError> {
    let a = h.trim().to_lowercase();
    if !a.starts_with("0x") || a.len() != 66 {
        return Err(ApiError::BadRequest("tx_hash 须为 0x + 64 hex".into()));
    }
    if !a[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("tx_hash 含非 hex 字符".into()));
    }
    Ok(a)
}

fn normalize_hex_address(addr: &str) -> Result<String, ApiError> {
    let a = addr.trim().to_lowercase();
    if !a.starts_with("0x") || a.len() != 42 {
        return Err(ApiError::BadRequest("address 须为 0x + 40 hex".into()));
    }
    if !a[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("address 含非 hex 字符".into()));
    }
    Ok(a)
}
