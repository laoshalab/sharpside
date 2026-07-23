//! 链上 USDC Transfer 匹配与支付确认。
//!
//! 两条路径：
//! 1. `submit-tx` → `eth_getTransactionReceipt`
//! 2. 无 submit-tx → `eth_getLogs`（Transfer to treasury）按金额认领

use std::collections::HashSet;

use rust_decimal::Decimal;
use sharpside_db::queries::billing as bill;
use sharpside_db::{BillingInvoice, BillingPayment};
use sharpside_venues_polymarket::onchain::{
    address_to_topic, eth_block_number, eth_get_logs_chunked, eth_get_transaction_receipt, TxLog,
    TxReceipt, POLYGON_RPC_DEFAULT,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;

/// ERC-20 `Transfer(address,address,uint256)` topic0。
pub const ERC20_TRANSFER_TOPIC0: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedTransfer {
    pub from_address: String,
    pub to_address: String,
    pub amount_raw: Decimal,
    pub log_index: i32,
    pub block_number: i64,
    pub tx_hash: Option<String>,
}

#[derive(Debug)]
pub enum ConfirmOutcome {
    /// 尚未上链 / 确认数不足，继续等。
    Pending(&'static str),
    Confirmed {
        invoice_id: Uuid,
        user_id: Uuid,
        tx_hash: String,
    },
    Rejected {
        payment_id: Uuid,
        reason: String,
    },
}

pub fn polygon_rpc_url() -> String {
    std::env::var("POLYGON_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| POLYGON_RPC_DEFAULT.to_string())
}

/// 处理一笔 submitted payment。
pub async fn try_confirm_submitted(
    state: &AppState,
    payment: &BillingPayment,
) -> Result<ConfirmOutcome, String> {
    let inv = bill::get_invoice(&state.db, payment.invoice_id)
        .await
        .map_err(|e| e.to_string())?;
    if inv.status != "pending" {
        let _ = bill::reject_payment(
            &state.db,
            payment.id,
            &format!("发票已为 {}，放弃确认", inv.status),
        )
        .await;
        return Ok(ConfirmOutcome::Rejected {
            payment_id: payment.id,
            reason: format!("invoice status {}", inv.status),
        });
    }

    let rpc = polygon_rpc_url();
    let receipt = eth_get_transaction_receipt(&rpc, &payment.tx_hash).await?;
    let Some(receipt) = receipt else {
        return Ok(ConfirmOutcome::Pending("tx not mined"));
    };

    if !receipt.is_success() {
        let reason = "链上交易失败（receipt.status != 0x1）".to_string();
        let _ = bill::reject_payment(&state.db, payment.id, &reason).await;
        return Ok(ConfirmOutcome::Rejected {
            payment_id: payment.id,
            reason,
        });
    }

    let block = receipt
        .block_number_u64()
        .ok_or_else(|| "receipt 缺 blockNumber".to_string())?;
    let latest = eth_block_number(&rpc).await?;
    let confs = latest.saturating_sub(block).saturating_add(1);
    let need = u64::from(state.config.billing_confirmations.max(1));
    if confs < need {
        return Ok(ConfirmOutcome::Pending("insufficient confirmations"));
    }

    let Some(matched) = find_matching_transfer(
        &receipt,
        &inv.token_address,
        &inv.treasury_address,
        inv.amount_raw,
        block,
    ) else {
        let reason = format!(
            "回执中无匹配的 USDC Transfer（token={} to={} amount_raw={}）",
            inv.token_address, inv.treasury_address, inv.amount_raw
        );
        let _ = bill::reject_payment(&state.db, payment.id, &reason).await;
        return Ok(ConfirmOutcome::Rejected {
            payment_id: payment.id,
            reason,
        });
    };

    if state.config.billing_require_linked_wallet {
        let wallets = sharpside_db::queries::account::list_wallets(&state.db, inv.user_id)
            .await
            .map_err(|e| e.to_string())?;
        if !wallets
            .iter()
            .any(|w| w.address == matched.from_address)
        {
            let reason = format!(
                "付款地址 {} 未绑定到该用户（BILLING_REQUIRE_LINKED_WALLET）",
                matched.from_address
            );
            let _ = bill::reject_payment(&state.db, payment.id, &reason).await;
            return Ok(ConfirmOutcome::Rejected {
                payment_id: payment.id,
                reason,
            });
        }
    }

    finalize_confirm(
        state,
        &inv,
        &payment.tx_hash,
        &matched,
        confs,
        "billing_rpc_confirm",
    )
    .await
}

async fn finalize_confirm(
    state: &AppState,
    inv: &BillingInvoice,
    tx_hash: &str,
    matched: &MatchedTransfer,
    confs: u64,
    op: &'static str,
) -> Result<ConfirmOutcome, String> {
    // linked-wallet 校验由调用方完成（submit 路径会 reject payment；getLogs 路径会 skip）
    let input = bill::ConfirmPaymentInput {
        invoice_id: inv.id,
        tx_hash: tx_hash.to_string(),
        log_index: matched.log_index,
        from_address: matched.from_address.clone(),
        to_address: matched.to_address.clone(),
        amount_raw: matched.amount_raw,
        block_number: matched.block_number,
        chain_id: inv.chain_id,
    };

    match bill::confirm_payment(&state.db, &input).await {
        Ok((invoice, _pay, user)) => {
            info!(
                op,
                invoice_id = %invoice.id,
                user_id = %user.id,
                tx_hash = %tx_hash,
                confirmations = confs,
                "Pro+ 链上支付已确认"
            );
            Ok(ConfirmOutcome::Confirmed {
                invoice_id: invoice.id,
                user_id: user.id,
                tx_hash: tx_hash.to_string(),
            })
        }
        Err(sharpside_db::DbError::Conflict(msg)) => {
            info!(tx_hash = %tx_hash, %msg, "billing confirm 幂等命中");
            Ok(ConfirmOutcome::Confirmed {
                invoice_id: inv.id,
                user_id: inv.user_id,
                tx_hash: tx_hash.to_string(),
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 在回执 logs 中找精确匹配的 ERC-20 Transfer。
pub fn find_matching_transfer(
    receipt: &TxReceipt,
    token_address: &str,
    treasury_address: &str,
    amount_raw: Decimal,
    block_number: u64,
) -> Option<MatchedTransfer> {
    let token = token_address.trim().to_lowercase();
    let treasury = treasury_address.trim().to_lowercase();
    for (i, log) in receipt.logs.iter().enumerate() {
        if let Some(m) = parse_erc20_transfer(log) {
            if m.token != token {
                continue;
            }
            if m.to_address != treasury {
                continue;
            }
            if m.amount_raw != amount_raw {
                continue;
            }
            let log_index = log.log_index_i32().unwrap_or(i as i32);
            return Some(MatchedTransfer {
                from_address: m.from_address,
                to_address: m.to_address,
                amount_raw: m.amount_raw,
                log_index,
                block_number: i64::try_from(block_number).unwrap_or(0),
                tx_hash: log.tx_hash_normalized(),
            });
        }
    }
    None
}

#[derive(Debug)]
struct ParsedTransfer {
    token: String,
    from_address: String,
    to_address: String,
    amount_raw: Decimal,
}

fn parse_erc20_transfer(log: &TxLog) -> Option<ParsedTransfer> {
    if log.topics.len() < 3 {
        return None;
    }
    let topic0 = log.topics[0].trim().to_lowercase();
    if topic0 != ERC20_TRANSFER_TOPIC0 {
        return None;
    }
    let from_address = topic_to_address(&log.topics[1])?;
    let to_address = topic_to_address(&log.topics[2])?;
    let amount_raw = data_to_decimal(&log.data)?;
    Some(ParsedTransfer {
        token: log.address.trim().to_lowercase(),
        from_address,
        to_address,
        amount_raw,
    })
}

fn topic_to_address(topic: &str) -> Option<String> {
    let h = topic.trim().trim_start_matches("0x").to_lowercase();
    if h.len() < 40 {
        return None;
    }
    let addr = &h[h.len() - 40..];
    if !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{addr}"))
}

fn data_to_decimal(data: &str) -> Option<Decimal> {
    let h = data.trim().trim_start_matches("0x");
    if h.is_empty() {
        return Some(Decimal::ZERO);
    }
    let bytes = hex::decode(h).ok()?;
    let mut buf = [0u8; 32];
    if bytes.len() >= 32 {
        buf.copy_from_slice(&bytes[bytes.len() - 32..]);
    } else {
        buf[32 - bytes.len()..].copy_from_slice(&bytes);
    }
    let mut acc: u128 = 0;
    for b in buf {
        acc = acc.checked_shl(8)?.checked_add(u128::from(b))?;
    }
    Decimal::from_str_exact(&acc.to_string()).ok()
}

/// 从 getLogs 结果中，为发票列表认领匹配（纯逻辑，便于单测）。
///
/// 规则：按 log 顺序；金额精确匹配；同金额取列表中最先出现的未认领发票（调用方应按 created_at ASC）。
pub fn claim_invoices_from_logs(
    invoices: &[BillingInvoice],
    logs: &[TxLog],
    already_claimed_invoice_ids: &HashSet<Uuid>,
) -> Vec<(Uuid, MatchedTransfer, String)> {
    let mut claimed = already_claimed_invoice_ids.clone();
    let mut out = Vec::new();

    for (i, log) in logs.iter().enumerate() {
        if log.is_removed() {
            continue;
        }
        let Some(parsed) = parse_erc20_transfer(log) else {
            continue;
        };
        let Some(tx_hash) = log.tx_hash_normalized() else {
            continue;
        };
        let Some(block) = log.block_number_u64() else {
            continue;
        };
        let log_index = log.log_index_i32().unwrap_or(i as i32);

        let Some(inv) = invoices.iter().find(|inv| {
            !claimed.contains(&inv.id)
                && inv.status == "pending"
                && inv.amount_raw == parsed.amount_raw
                && inv.token_address == parsed.token
                && inv.treasury_address == parsed.to_address
        }) else {
            continue;
        };

        claimed.insert(inv.id);
        out.push((
            inv.id,
            MatchedTransfer {
                from_address: parsed.from_address,
                to_address: parsed.to_address,
                amount_raw: parsed.amount_raw,
                log_index,
                block_number: i64::try_from(block).unwrap_or(0),
                tx_hash: Some(tx_hash.clone()),
            },
            tx_hash,
        ));
    }
    out
}

/// 无 submit-tx：`eth_getLogs` 扫转入 treasury 的 USDC Transfer，按金额认领 pending 发票。
pub async fn process_get_logs_claim_batch(state: &AppState) -> Result<(), String> {
    if !state.config.billing_enabled() {
        return Ok(());
    }

    let invoices = bill::list_pending_invoices(&state.db, 100)
        .await
        .map_err(|e| e.to_string())?;
    if invoices.is_empty() {
        return Ok(());
    }

    let rpc = polygon_rpc_url();
    let latest = eth_block_number(&rpc).await?;
    let need = u64::from(state.config.billing_confirmations.max(1));
    // 只认领已达确认数的块
    let to_block = latest.saturating_sub(need.saturating_sub(1));
    if to_block == 0 && latest < need {
        return Ok(());
    }
    let lookback = state.config.billing_logs_lookback_blocks.max(1);
    let from_block = to_block.saturating_sub(lookback.saturating_sub(1));

    let treasury_topic = address_to_topic(&state.config.billing_treasury_address)
        .ok_or_else(|| "BILLING_TREASURY_ADDRESS 非法".to_string())?;
    let topics = vec![
        serde_json::json!(ERC20_TRANSFER_TOPIC0),
        serde_json::Value::Null,
        serde_json::json!(treasury_topic),
    ];

    let logs = eth_get_logs_chunked(
        &rpc,
        &state.config.billing_usdc_address,
        topics,
        from_block,
        to_block,
        state.config.billing_logs_chunk_blocks.max(1),
    )
    .await?;

    if logs.is_empty() {
        tracing::debug!(
            from_block,
            to_block,
            pending_invoices = invoices.len(),
            "billing getLogs：无 Transfer"
        );
        return Ok(());
    }

    info!(
        from_block,
        to_block,
        logs = logs.len(),
        pending_invoices = invoices.len(),
        "billing getLogs：扫描认领"
    );

    let claims = claim_invoices_from_logs(&invoices, &logs, &HashSet::new());
    let confs = need;

    for (invoice_id, matched, tx_hash) in claims {
        let log_index = matched.log_index;
        if bill::payment_log_confirmed(
            &state.db,
            state.config.billing_chain_id,
            &tx_hash,
            log_index,
        )
        .await
        .map_err(|e| e.to_string())?
        {
            continue;
        }

        let inv = match invoices.iter().find(|i| i.id == invoice_id) {
            Some(i) => i.clone(),
            None => continue,
        };

        if state.config.billing_require_linked_wallet {
            let wallets = sharpside_db::queries::account::list_wallets(&state.db, inv.user_id)
                .await
                .map_err(|e| e.to_string())?;
            if !wallets
                .iter()
                .any(|w| w.address == matched.from_address)
            {
                warn!(
                    invoice_id = %inv.id,
                    from = %matched.from_address,
                    "getLogs 认领跳过：付款地址未绑定"
                );
                continue;
            }
        }

        match finalize_confirm(
            state,
            &inv,
            &tx_hash,
            &matched,
            confs,
            "billing_getlogs_claim",
        )
        .await
        {
            Ok(ConfirmOutcome::Confirmed {
                invoice_id,
                user_id,
                tx_hash,
            }) => {
                info!(%invoice_id, %user_id, %tx_hash, "billing getLogs 认领成功");
            }
            Ok(other) => {
                warn!(?other, "billing getLogs 认领非预期结果");
            }
            Err(e) => {
                warn!(
                    invoice_id = %inv.id,
                    %tx_hash,
                    error = %e,
                    "billing getLogs 认领失败"
                );
            }
        }
    }

    Ok(())
}

/// submitted receipt 确认 + getLogs 认领。
pub async fn process_confirm_batch(state: &AppState) -> Result<(), sharpside_db::DbError> {
    process_submitted_batch(state).await?;
    if let Err(e) = process_get_logs_claim_batch(state).await {
        warn!(error = %e, "billing getLogs 认领失败");
    }
    Ok(())
}

/// 批量处理 submitted；错误记 warn 不中断整批。
pub async fn process_submitted_batch(state: &AppState) -> Result<(), sharpside_db::DbError> {
    if !state.config.billing_enabled() {
        return Ok(());
    }
    let pending = bill::list_submitted_payments(&state.db, 50).await?;
    if pending.is_empty() {
        return Ok(());
    }
    info!(count = pending.len(), "billing：扫描 submitted 链上确认");
    for p in &pending {
        match try_confirm_submitted(state, p).await {
            Ok(ConfirmOutcome::Pending(why)) => {
                tracing::debug!(
                    payment_id = %p.id,
                    tx_hash = %p.tx_hash,
                    reason = why,
                    "billing confirm 等待中"
                );
            }
            Ok(ConfirmOutcome::Confirmed {
                invoice_id,
                user_id,
                tx_hash,
            }) => {
                info!(%invoice_id, %user_id, %tx_hash, "billing RPC 确认成功");
            }
            Ok(ConfirmOutcome::Rejected { payment_id, reason }) => {
                warn!(%payment_id, %reason, "billing 支付已拒绝");
            }
            Err(e) => {
                warn!(
                    payment_id = %p.id,
                    tx_hash = %p.tx_hash,
                    error = %e,
                    "billing RPC 确认失败"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use sharpside_venues_polymarket::onchain::TxLog;

    fn transfer_log(
        token: &str,
        from: &str,
        to: &str,
        amount_hex: &str,
        idx: &str,
        tx: &str,
        block: &str,
    ) -> TxLog {
        let pad = |addr: &str| {
            let a = addr.trim_start_matches("0x").to_lowercase();
            format!("0x{:0>64}", a)
        };
        TxLog {
            address: token.to_string(),
            topics: vec![
                ERC20_TRANSFER_TOPIC0.to_string(),
                pad(from),
                pad(to),
            ],
            data: amount_hex.to_string(),
            log_index: Some(idx.to_string()),
            transaction_hash: Some(tx.to_string()),
            block_number: Some(block.to_string()),
            removed: Some(false),
        }
    }

    fn sample_invoice(id: Uuid, amount: u64, created_offset_secs: i64) -> BillingInvoice {
        let token = "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359".to_string();
        let treasury = "0x1111111111111111111111111111111111111111".to_string();
        BillingInvoice {
            id,
            user_id: Uuid::nil(),
            plan: "pro_plus".into(),
            period_days: 30,
            amount_usdc: Decimal::from(amount) / Decimal::from(1_000_000),
            amount_raw: Decimal::from(amount),
            chain_id: 137,
            token_address: token,
            treasury_address: treasury,
            status: "pending".into(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            paid_at: None,
            created_at: Utc::now() + chrono::Duration::seconds(created_offset_secs),
        }
    }

    #[test]
    fn match_exact_usdc_transfer() {
        let token = "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359";
        let treasury = "0x1111111111111111111111111111111111111111";
        let from = "0x2222222222222222222222222222222222222222";
        let receipt = TxReceipt {
            status: Some("0x1".into()),
            block_number: Some("0x100".into()),
            logs: vec![transfer_log(
                token,
                from,
                treasury,
                "0x0000000000000000000000000000000000000000000000000000000001c9c380",
                "0x3",
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "0x100",
            )],
        };
        let m = find_matching_transfer(
            &receipt,
            token,
            treasury,
            Decimal::from(30_000_000u64),
            256,
        )
        .expect("should match");
        assert_eq!(m.from_address, from);
        assert_eq!(m.to_address, treasury);
        assert_eq!(m.amount_raw, Decimal::from(30_000_000u64));
        assert_eq!(m.log_index, 3);
        assert_eq!(m.block_number, 256);
    }

    #[test]
    fn reject_wrong_amount() {
        let token = "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359";
        let treasury = "0x1111111111111111111111111111111111111111";
        let from = "0x2222222222222222222222222222222222222222";
        let receipt = TxReceipt {
            status: Some("0x1".into()),
            block_number: Some("0x100".into()),
            logs: vec![transfer_log(
                token,
                from,
                treasury,
                "0x0000000000000000000000000000000000000000000000000000000001ba8140",
                "0x0",
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "0x100",
            )],
        };
        assert!(find_matching_transfer(
            &receipt,
            token,
            treasury,
            Decimal::from(30_000_000u64),
            256,
        )
        .is_none());
    }

    #[test]
    fn topic_address_padding() {
        assert_eq!(
            topic_to_address(
                "0x000000000000000000000000aabbccddeeff00112233445566778899aabbccdd"
            )
            .unwrap(),
            "0xaabbccddeeff00112233445566778899aabbccdd"
        );
    }

    #[test]
    fn get_logs_claim_fifo_same_amount() {
        let id1 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let id2 = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let invoices = vec![
            sample_invoice(id1, 30_000_000, -100),
            sample_invoice(id2, 30_000_000, -50),
        ];
        let token = "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359";
        let treasury = "0x1111111111111111111111111111111111111111";
        let from = "0x2222222222222222222222222222222222222222";
        let logs = vec![transfer_log(
            token,
            from,
            treasury,
            "0x0000000000000000000000000000000000000000000000000000000001c9c380",
            "0x1",
            "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "0x200",
        )];
        let claims = claim_invoices_from_logs(&invoices, &logs, &HashSet::new());
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].0, id1);
        assert_eq!(
            claims[0].2,
            "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        );
    }

    #[test]
    fn get_logs_skips_removed() {
        let id1 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let invoices = vec![sample_invoice(id1, 30_000_000, -10)];
        let token = "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359";
        let treasury = "0x1111111111111111111111111111111111111111";
        let from = "0x2222222222222222222222222222222222222222";
        let mut log = transfer_log(
            token,
            from,
            treasury,
            "0x0000000000000000000000000000000000000000000000000000000001c9c380",
            "0x1",
            "0xdddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "0x200",
        );
        log.removed = Some(true);
        let claims = claim_invoices_from_logs(&invoices, &[log], &HashSet::new());
        assert!(claims.is_empty());
    }
}
