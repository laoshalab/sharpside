//! Pro+ USDC 订阅账本查询。对应 migration 0040。
//!
//! 权益写入与 invoice/payment 状态变更在同一事务内完成（`confirm_payment`）。

use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::DbError;
use crate::models::{BillingInvoice, BillingPayment, User};

/// 创建应付单。若已有 pending，返回已有行（不新建）。
pub async fn create_or_get_pending_invoice(
    pool: &PgPool,
    user_id: Uuid,
    plan: &str,
    period_days: i32,
    amount_usdc: Decimal,
    amount_raw: Decimal,
    chain_id: i32,
    token_address: &str,
    treasury_address: &str,
    ttl_secs: i64,
) -> Result<BillingInvoice, DbError> {
    if let Some(existing) = get_pending_invoice(pool, user_id).await? {
        return Ok(existing);
    }
    let expires_at = Utc::now() + Duration::seconds(ttl_secs.max(60));
    let row = sqlx::query_as::<_, BillingInvoice>(
        r#"
        INSERT INTO account.billing_invoices (
            user_id, plan, period_days, amount_usdc, amount_raw,
            chain_id, token_address, treasury_address, status, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(plan)
    .bind(period_days)
    .bind(amount_usdc)
    .bind(amount_raw)
    .bind(chain_id)
    .bind(token_address)
    .bind(treasury_address)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(|e| map_unique_pending(e))?;
    Ok(row)
}

fn map_unique_pending(e: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db) = e {
        if db.constraint() == Some("uq_billing_invoices_user_pending") {
            return DbError::conflict("已有未支付发票");
        }
    }
    DbError::from(e)
}

pub async fn get_pending_invoice(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<BillingInvoice>, DbError> {
    let row = sqlx::query_as::<_, BillingInvoice>(
        r#"
        SELECT * FROM account.billing_invoices
        WHERE user_id = $1 AND status = 'pending'
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_invoice(
    pool: &PgPool,
    invoice_id: Uuid,
) -> Result<BillingInvoice, DbError> {
    sqlx::query_as::<_, BillingInvoice>(
        "SELECT * FROM account.billing_invoices WHERE id = $1",
    )
    .bind(invoice_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("invoice {invoice_id}")))
}

pub async fn list_invoices_for_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<BillingInvoice>, DbError> {
    let rows = sqlx::query_as::<_, BillingInvoice>(
        r#"
        SELECT * FROM account.billing_invoices
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_payments_for_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<BillingPayment>, DbError> {
    let rows = sqlx::query_as::<_, BillingPayment>(
        r#"
        SELECT * FROM account.billing_payments
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 用户粘贴 tx_hash：写入 submitted payment（同一 invoice+tx 可幂等返回已有行）。
pub async fn submit_payment_tx(
    pool: &PgPool,
    invoice_id: Uuid,
    user_id: Uuid,
    chain_id: i32,
    tx_hash: &str,
) -> Result<BillingPayment, DbError> {
    let inv = get_invoice(pool, invoice_id).await?;
    if inv.user_id != user_id {
        return Err(DbError::not_found(format!("invoice {invoice_id}")));
    }
    if inv.status != "pending" {
        return Err(DbError::Invalid(format!(
            "发票状态为 {}，不可提交支付",
            inv.status
        )));
    }
    if Utc::now() > inv.expires_at {
        return Err(DbError::Invalid("发票已过期".into()));
    }

    if let Some(existing) = sqlx::query_as::<_, BillingPayment>(
        r#"
        SELECT * FROM account.billing_payments
        WHERE invoice_id = $1 AND tx_hash = $2 AND status IN ('submitted', 'confirmed')
        LIMIT 1
        "#,
    )
    .bind(invoice_id)
    .bind(tx_hash)
    .fetch_optional(pool)
    .await?
    {
        return Ok(existing);
    }

    let row = sqlx::query_as::<_, BillingPayment>(
        r#"
        INSERT INTO account.billing_payments (
            invoice_id, user_id, chain_id, tx_hash, to_address, status
        )
        VALUES ($1, $2, $3, $4, $5, 'submitted')
        RETURNING *
        "#,
    )
    .bind(invoice_id)
    .bind(user_id)
    .bind(chain_id)
    .bind(tx_hash)
    .bind(&inv.treasury_address)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 列出未过期的 pending 发票（FIFO，供 getLogs 认领）。
pub async fn list_pending_invoices(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<BillingInvoice>, DbError> {
    let rows = sqlx::query_as::<_, BillingInvoice>(
        r#"
        SELECT * FROM account.billing_invoices
        WHERE status = 'pending' AND expires_at > now()
        ORDER BY created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 该 (chain, tx, log) 是否已有 confirmed 支付。
pub async fn payment_log_confirmed(
    pool: &PgPool,
    chain_id: i32,
    tx_hash: &str,
    log_index: i32,
) -> Result<bool, DbError> {
    let (n,): (i64,) = sqlx::query_as(
        r#"
        SELECT count(*)::bigint FROM account.billing_payments
        WHERE chain_id = $1 AND tx_hash = $2 AND log_index = $3 AND status = 'confirmed'
        "#,
    )
    .bind(chain_id)
    .bind(tx_hash)
    .bind(log_index)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

/// 列出待链上确认的 submitted payments（worker 用）。
pub async fn list_submitted_payments(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<BillingPayment>, DbError> {
    let rows = sqlx::query_as::<_, BillingPayment>(
        r#"
        SELECT p.* FROM account.billing_payments p
        JOIN account.billing_invoices i ON i.id = p.invoice_id
        WHERE p.status = 'submitted' AND i.status = 'pending'
        ORDER BY p.created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 过期 pending 发票。
pub async fn expire_stale_invoices(pool: &PgPool) -> Result<u64, DbError> {
    let res = sqlx::query(
        r#"
        UPDATE account.billing_invoices
        SET status = 'expired'
        WHERE status = 'pending' AND expires_at < now()
        "#,
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// 过期 Pro+：until + grace 已过 → free。
pub async fn expire_subscriptions(pool: &PgPool, grace_secs: i64) -> Result<u64, DbError> {
    let res = sqlx::query(
        r#"
        UPDATE account.users
        SET subscription_tier = 'free',
            subscription_until = NULL,
            updated_at = now()
        WHERE subscription_tier = 'pro_plus'
          AND subscription_until IS NOT NULL
          AND subscription_until + ($1 || ' seconds')::interval < now()
        "#,
    )
    .bind(grace_secs.max(0).to_string())
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// 链上确认入参（worker / internal API）。
#[derive(Debug, Clone)]
pub struct ConfirmPaymentInput {
    pub invoice_id: Uuid,
    pub tx_hash: String,
    pub log_index: i32,
    pub from_address: String,
    pub to_address: String,
    pub amount_raw: Decimal,
    pub block_number: i64,
    pub chain_id: i32,
}

/// 确认支付并开通/续期 Pro+（单事务，幂等）。
///
/// - 已 confirmed 的同一 (chain, tx, log) → 返回已有结果
/// - invoice 非 pending → Invalid
/// - amount/to 与发票不匹配 → Invalid
pub async fn confirm_payment(
    pool: &PgPool,
    input: &ConfirmPaymentInput,
) -> Result<(BillingInvoice, BillingPayment, User), DbError> {
    let mut tx = pool.begin().await?;

    // 幂等：同一 log 已确认
    if let Some(existing) = sqlx::query_as::<_, BillingPayment>(
        r#"
        SELECT * FROM account.billing_payments
        WHERE chain_id = $1 AND tx_hash = $2 AND log_index = $3 AND status = 'confirmed'
        "#,
    )
    .bind(input.chain_id)
    .bind(&input.tx_hash)
    .bind(input.log_index)
    .fetch_optional(&mut *tx)
    .await?
    {
        let inv = sqlx::query_as::<_, BillingInvoice>(
            "SELECT * FROM account.billing_invoices WHERE id = $1",
        )
        .bind(existing.invoice_id)
        .fetch_one(&mut *tx)
        .await?;
        let user = sqlx::query_as::<_, User>("SELECT * FROM account.users WHERE id = $1")
            .bind(existing.user_id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok((inv, existing, user));
    }

    let inv = sqlx::query_as::<_, BillingInvoice>(
        "SELECT * FROM account.billing_invoices WHERE id = $1 FOR UPDATE",
    )
    .bind(input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| DbError::not_found(format!("invoice {}", input.invoice_id)))?;

    if inv.status != "pending" {
        return Err(DbError::Invalid(format!(
            "发票状态为 {}，不可确认",
            inv.status
        )));
    }
    if input.chain_id != inv.chain_id {
        return Err(DbError::Invalid("chain_id 与发票不符".into()));
    }
    if input.to_address != inv.treasury_address {
        return Err(DbError::Invalid("收款地址与发票不符".into()));
    }
    if input.amount_raw != inv.amount_raw {
        return Err(DbError::Invalid("支付金额与发票不符".into()));
    }

    // 复用已有 submitted 行，或新建
    let payment = if let Some(p) = sqlx::query_as::<_, BillingPayment>(
        r#"
        SELECT * FROM account.billing_payments
        WHERE invoice_id = $1 AND tx_hash = $2 AND status = 'submitted'
        FOR UPDATE
        "#,
    )
    .bind(input.invoice_id)
    .bind(&input.tx_hash)
    .fetch_optional(&mut *tx)
    .await?
    {
        sqlx::query_as::<_, BillingPayment>(
            r#"
            UPDATE account.billing_payments SET
                log_index = $2,
                from_address = $3,
                to_address = $4,
                amount_raw = $5,
                block_number = $6,
                status = 'confirmed',
                confirmed_at = now(),
                note = NULL
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(p.id)
        .bind(input.log_index)
        .bind(&input.from_address)
        .bind(&input.to_address)
        .bind(input.amount_raw)
        .bind(input.block_number)
        .fetch_one(&mut *tx)
        .await?
    } else {
        sqlx::query_as::<_, BillingPayment>(
            r#"
            INSERT INTO account.billing_payments (
                invoice_id, user_id, chain_id, tx_hash, log_index,
                from_address, to_address, amount_raw, block_number,
                status, confirmed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'confirmed', now())
            RETURNING *
            "#,
        )
        .bind(input.invoice_id)
        .bind(inv.user_id)
        .bind(input.chain_id)
        .bind(&input.tx_hash)
        .bind(input.log_index)
        .bind(&input.from_address)
        .bind(&input.to_address)
        .bind(input.amount_raw)
        .bind(input.block_number)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db) = e {
                if db.constraint() == Some("uq_billing_payments_chain_tx_log")
                    || db.constraint() == Some("uq_billing_payments_invoice_confirmed")
                {
                    return DbError::conflict("支付已确认或 tx 已被占用");
                }
            }
            DbError::from(e)
        })?
    };

    let inv = sqlx::query_as::<_, BillingInvoice>(
        r#"
        UPDATE account.billing_invoices
        SET status = 'paid', paid_at = now()
        WHERE id = $1 AND status = 'pending'
        RETURNING *
        "#,
    )
    .bind(input.invoice_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| DbError::conflict("发票已被确认"))?;

    let user = sqlx::query_as::<_, User>(
        r#"
        UPDATE account.users SET
            subscription_tier = 'pro_plus',
            subscription_until = GREATEST(COALESCE(subscription_until, now()), now())
                + ($2 || ' days')::interval,
            updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(inv.user_id)
    .bind(inv.period_days.to_string())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((inv, payment, user))
}

/// 拒绝 submitted payment（校验失败时）。
pub async fn reject_payment(
    pool: &PgPool,
    payment_id: Uuid,
    note: &str,
) -> Result<BillingPayment, DbError> {
    let row = sqlx::query_as::<_, BillingPayment>(
        r#"
        UPDATE account.billing_payments
        SET status = 'rejected', note = $2
        WHERE id = $1 AND status = 'submitted'
        RETURNING *
        "#,
    )
    .bind(payment_id)
    .bind(note)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("payment {payment_id}")))?;
    Ok(row)
}
