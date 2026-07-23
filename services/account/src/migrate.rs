//! 归档 Deposit Wallet → 当前活跃 DW 的资金迁移。
//!
//! 重新预配会换新 owner/DW，旧密文进 `credential_archives`。本模块用归档密文：
//! 1. 若旧 DW 未部署 → Relayer `WALLET-CREATE`
//! 2. 链上读 pUSD 余额 → owner 签 WALLET batch `transfer` 到当前 DW
//!
//! 密钥不离开服务端；对外只返回地址 / 金额 / tx。

use crate::error::ApiError;
use crate::state::AppState;
use alloy_primitives::{Address, U256};
use alloy_signer_local::PrivateKeySigner;
use serde::Serialize;
use sharpside_db::queries::account as acct;
use sharpside_venues_polymarket::{onchain, wallet_batch, RelayerClient};
use std::str::FromStr;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ArchiveView {
    pub id: i64,
    pub platform: String,
    pub kind: String,
    pub deposit_wallet_address: Option<String>,
    pub owner_address: Option<String>,
    pub provision_live: Option<bool>,
    pub archived_at: chrono::DateTime<chrono::Utc>,
    /// 链上 pUSD 余额（人类单位）；RPC 失败时为 null。
    pub onchain_balance: Option<f64>,
    pub balance_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MigrateResponse {
    pub archive_id: i64,
    pub from_deposit_wallet: String,
    pub to_deposit_wallet: String,
    pub amount: f64,
    pub deployed: bool,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
}

/// 列出归档（非密字段 + 链上余额）。
pub async fn list_archives(state: &AppState, user_id: Uuid) -> Result<Vec<ArchiveView>, ApiError> {
    let rows = acct::list_credential_archives(&state.db, user_id, "polymarket", 20).await?;
    let pusd: Address = wallet_batch::contracts::COLLATERAL
        .parse()
        .map_err(|e| ApiError::Internal(format!("COLLATERAL 解析失败: {e}")))?;
    let rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| onchain::POLYGON_RPC_DEFAULT.to_string());

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let blob = &row.encrypted_blob;
        let deposit_wallet_address = blob
            .get("deposit_wallet_address")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| row.proxy_address.clone());
        let owner_address = blob
            .get("owner_address")
            .and_then(|v| v.as_str())
            .map(String::from);
        let provision_live = blob.get("provision_live").and_then(|v| v.as_bool());

        let (onchain_balance, balance_note) = match deposit_wallet_address.as_deref() {
            Some(dw) => match dw.parse::<Address>() {
                Ok(addr) => match onchain::pusd_balance_of(&rpc, pusd, addr).await {
                    Ok(b) => (Some(b), None),
                    Err(e) => (None, Some(format!("链上余额不可查: {e}"))),
                },
                Err(_) => (None, Some("deposit_wallet_address 非法".into())),
            },
            None => (None, Some("无 deposit_wallet_address".into())),
        };

        out.push(ArchiveView {
            id: row.id,
            platform: row.platform,
            kind: row.kind,
            deposit_wallet_address,
            owner_address,
            provision_live,
            archived_at: row.archived_at,
            onchain_balance,
            balance_note,
        });
    }
    Ok(out)
}

/// 将归档 DW 上的全部 pUSD 迁到当前活跃凭证的 Deposit Wallet。
pub async fn migrate_archive(
    state: &AppState,
    user_id: Uuid,
    archive_id: i64,
) -> Result<MigrateResponse, ApiError> {
    let archive = acct::get_credential_archive(&state.db, user_id, archive_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("归档凭证不存在".into()))?;
    if archive.platform != "polymarket" {
        return Err(ApiError::BadRequest("仅支持 polymarket 归档迁移".into()));
    }

    let active = acct::get_credential(&state.db, user_id, "polymarket")
        .await?
        .ok_or_else(|| ApiError::BadRequest("尚无活跃凭证：请先完成在线预配".into()))?;
    let to_dw = active
        .encrypted_blob
        .get("deposit_wallet_address")
        .and_then(|v| v.as_str())
        .or(active.proxy_address.as_deref())
        .ok_or_else(|| ApiError::BadRequest("活跃凭证缺少 deposit_wallet_address".into()))?
        .to_string();
    let to_addr: Address = to_dw
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("当前 DW 地址非法: {e}")))?;

    let blob = &archive.encrypted_blob;
    let from_dw = blob
        .get("deposit_wallet_address")
        .and_then(|v| v.as_str())
        .or(archive.proxy_address.as_deref())
        .ok_or_else(|| ApiError::BadRequest("归档缺少 deposit_wallet_address".into()))?
        .to_string();
    let from_addr: Address = from_dw
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("归档 DW 地址非法: {e}")))?;
    if from_addr == to_addr {
        return Err(ApiError::BadRequest("归档 DW 与当前 DW 相同，无需迁移".into()));
    }

    let owner_str = blob
        .get("owner_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("归档缺少 owner_address".into()))?;
    let owner_addr: Address = owner_str
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("owner_address 非法: {e}")))?;
    let encrypted_owner_key = blob
        .get("encrypted_owner_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("归档缺少 encrypted_owner_key".into()))?;

    let plaintext = state
        .kms
        .decrypt(encrypted_owner_key)
        .map_err(|e| ApiError::Internal(format!("KMS 解密归档 owner 失败: {e}")))?;
    let signer = PrivateKeySigner::from_str(plaintext.trim())
        .map_err(|e| ApiError::Internal(format!("归档 owner 私钥解析失败: {e}")))?;
    if signer.address() != owner_addr {
        return Err(ApiError::Internal(
            "归档 owner_address 与解出的 EOA 不一致".into(),
        ));
    }

    let pusd: Address = wallet_batch::contracts::COLLATERAL
        .parse()
        .map_err(|e| ApiError::Internal(format!("COLLATERAL 解析失败: {e}")))?;
    let rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| onchain::POLYGON_RPC_DEFAULT.to_string());
    let bal = onchain::pusd_balance_of(&rpc, pusd, from_addr)
        .await
        .map_err(|e| ApiError::Internal(format!("读旧 DW 余额失败: {e}")))?;
    if bal <= 0.0 {
        return Err(ApiError::BadRequest("旧 Deposit Wallet 余额为 0，无需迁移".into()));
    }
    let raw = (bal * 1_000_000.0).round() as u128;
    if raw == 0 {
        return Err(ApiError::BadRequest("旧 Deposit Wallet 余额过小".into()));
    }
    let amount_u256 = U256::from(raw);

    let relayer = RelayerClient::new();
    let mut deployed = false;
    match relayer.is_deployed(&from_dw).await {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            // 离线预配常见：地址有余额但合约未部署。
            let submit = relayer
                .wallet_create(owner_addr)
                .await
                .map_err(|e| ApiError::Internal(format!("WALLET-CREATE 失败: {e}")))?;
            info!(
                archive_id,
                owner = %owner_addr,
                tx_id = ?submit.transaction_id,
                "归档 DW WALLET-CREATE 已提交"
            );
            if let Some(tx_id) = submit.transaction_id.as_deref().filter(|s| !s.is_empty()) {
                let _ = relayer.poll_confirmed(tx_id).await;
            }
            deployed = true;
        }
    }

    let calls = vec![wallet_batch::WalletCall {
        target: pusd,
        value: U256::ZERO,
        data: wallet_batch::transfer_calldata(to_addr, amount_u256),
    }];
    let nonce = relayer
        .wallet_nonce(owner_addr)
        .await
        .map_err(|e| ApiError::Internal(format!("Relayer /nonce 失败: {e}")))?;
    let deadline = (chrono::Utc::now().timestamp() + 3600).max(0) as u64;
    let nonce_u256 = U256::from_str_radix(nonce.trim(), 10).unwrap_or(U256::ZERO);
    let sig = wallet_batch::sign_wallet_batch(
        &signer,
        from_addr,
        nonce_u256,
        U256::from(deadline),
        &calls,
    )
    .map_err(|e| ApiError::Internal(format!("WALLET batch 签名失败: {e}")))?;
    let batch = relayer
        .wallet_batch(
            owner_addr,
            from_addr,
            &nonce,
            &deadline.to_string(),
            &sig,
            &calls,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("WALLET batch 失败: {e}")))?;

    let mut tx_hash = batch.transaction_hash.clone();
    let relayer_tx_id = batch.transaction_id.clone();
    if let Some(tx_id) = relayer_tx_id.as_deref().filter(|s| !s.is_empty()) {
        match relayer.poll_confirmed(tx_id).await {
            Ok(row) => {
                if tx_hash.is_none() {
                    tx_hash = row.transaction_hash;
                }
            }
            Err(e) => {
                tracing::warn!(
                    archive_id,
                    error = %e,
                    "迁移 transfer 轮询未确认（可能仍在上链）"
                );
            }
        }
    }

    info!(
        archive_id,
        from = %from_dw,
        to = %to_dw,
        amount = bal,
        "归档 DW 资金迁移已提交"
    );

    Ok(MigrateResponse {
        archive_id,
        from_deposit_wallet: from_dw,
        to_deposit_wallet: to_dw,
        amount: bal,
        deployed,
        tx_hash,
        relayer_tx_id,
    })
}

#[derive(Debug, Serialize)]
pub struct ArchiveRedeemableItem {
    pub condition_id: String,
    pub title: String,
    pub outcome: String,
    pub token_id: String,
    pub amount: f64,
    pub estimated_pusd: f64,
    pub already_redeemed: bool,
}

#[derive(Debug, Serialize)]
pub struct ArchiveRedeemResponse {
    pub archive_id: i64,
    pub deposit_wallet: String,
    pub id: Uuid,
    pub status: String,
    pub condition_id: String,
    pub outcome: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
    pub note: Option<String>,
}

fn archive_cred_and_dw(
    archive: &sharpside_db::CredentialArchive,
) -> Result<(sharpside_venues_core::Credential, String, Address), ApiError> {
    let blob = &archive.encrypted_blob;
    let from_dw = blob
        .get("deposit_wallet_address")
        .and_then(|v| v.as_str())
        .or(archive.proxy_address.as_deref())
        .ok_or_else(|| ApiError::BadRequest("归档缺少 deposit_wallet_address".into()))?
        .to_string();
    let from_addr: Address = from_dw
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("归档 DW 地址非法: {e}")))?;
    let cred: sharpside_venues_core::Credential = serde_json::from_value(blob.clone())
        .map_err(|e| ApiError::Internal(format!("归档凭证反序列化失败: {e}")))?;
    match &cred {
        sharpside_venues_core::Credential::DepositWalletDelegated { .. } => {}
        _ => {
            return Err(ApiError::BadRequest(
                "归档非 DepositWalletDelegated，无法赎回".into(),
            ));
        }
    }
    Ok((cred, from_dw, from_addr))
}

async fn ensure_dw_deployed(owner: Address, from_dw: &str) -> Result<bool, ApiError> {
    let relayer = RelayerClient::new();
    match relayer.is_deployed(from_dw).await {
        Ok(true) => Ok(false),
        Ok(false) | Err(_) => {
            let submit = relayer
                .wallet_create(owner)
                .await
                .map_err(|e| ApiError::Internal(format!("WALLET-CREATE 失败: {e}")))?;
            if let Some(tx_id) = submit.transaction_id.as_deref().filter(|s| !s.is_empty()) {
                let _ = relayer.poll_confirmed(tx_id).await;
            }
            Ok(true)
        }
    }
}

/// 列出归档 DW 上已结算可赎回仓位。
pub async fn list_archive_redeemable(
    state: &AppState,
    user_id: Uuid,
    archive_id: i64,
) -> Result<Vec<ArchiveRedeemableItem>, ApiError> {
    use sharpside_db::queries::raw;

    let archive = acct::get_credential_archive(&state.db, user_id, archive_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("归档凭证不存在".into()))?;
    let (_cred, from_dw, from_addr) = archive_cred_and_dw(&archive)?;

    let markets = raw::list_resolved_markets(&state.db, "polymarket", None).await?;
    let rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| onchain::POLYGON_RPC_DEFAULT.to_string());
    let pusd: Address = wallet_batch::contracts::COLLATERAL
        .parse()
        .map_err(|e| ApiError::Internal(format!("COLLATERAL 解析失败: {e}")))?;
    let ctf: Address = wallet_batch::contracts::CONDITIONAL_TOKENS
        .parse()
        .map_err(|e| ApiError::Internal(format!("CTF 解析失败: {e}")))?;

    let mut items = Vec::new();
    for m in markets {
        let (outcome, index_set) = match (
            m.outcome_yes
                .and_then(|d| d.to_string().parse::<f64>().ok()),
            m.outcome_no.and_then(|d| d.to_string().parse::<f64>().ok()),
        ) {
            (Some(1.0), _) => ("YES", 2u64),
            (_, Some(1.0)) => ("NO", 1u64),
            _ => continue,
        };
        let position_id =
            onchain::ctf_position_id(pusd, &m.venue_market_id, index_set);
        let balance = match onchain::ctf_balance_of(&rpc, ctf, from_addr, position_id).await {
            Ok(b) => b,
            Err(_) => continue,
        };
        if balance <= 0.0 {
            continue;
        }
        let already_redeemed = acct::redemption_exists_active(
            &state.db,
            user_id,
            &m.venue_market_id,
            outcome,
            &from_dw,
        )
        .await
        .unwrap_or(false);
        items.push(ArchiveRedeemableItem {
            condition_id: m.venue_market_id,
            title: m.title,
            outcome: outcome.to_string(),
            token_id: position_id.to_string(),
            amount: balance,
            estimated_pusd: balance,
            already_redeemed,
        });
    }
    Ok(items)
}

/// 在归档旧 DW 上赎回单市场赢仓位 → pUSD 留在旧 DW（再点「迁到当前钱包」）。
pub async fn redeem_archive(
    state: &AppState,
    user_id: Uuid,
    archive_id: i64,
    condition_id_raw: &str,
) -> Result<ArchiveRedeemResponse, ApiError> {
    use sharpside_db::queries::raw;
    use sharpside_venues_core::Venue;
    use sharpside_venues_polymarket::PolymarketVenue;

    let condition_id = condition_id_raw.trim().to_lowercase();
    let cond_hex = condition_id.trim_start_matches("0x");
    if cond_hex.len() != 64 || !cond_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest(
            "condition_id 须为 0x 前缀的 32 字节 hex（bytes32）".into(),
        ));
    }

    let archive = acct::get_credential_archive(&state.db, user_id, archive_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("归档凭证不存在".into()))?;
    let (cred, from_dw, from_addr) = archive_cred_and_dw(&archive)?;

    let owner_str = archive
        .encrypted_blob
        .get("owner_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("归档缺少 owner_address".into()))?;
    let owner_addr: Address = owner_str
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("owner_address 非法: {e}")))?;
    let _ = ensure_dw_deployed(owner_addr, &from_dw).await?;

    let market = raw::list_raw_markets(&state.db, "polymarket")
        .await?
        .into_iter()
        .find(|m| m.venue_market_id.eq_ignore_ascii_case(&condition_id))
        .ok_or_else(|| ApiError::NotFound("市场未在缓存中（condition_id 不匹配）".into()))?;
    if !market.closed {
        return Err(ApiError::BadRequest("市场尚未结算，无法赎回".into()));
    }
    let (outcome, index_set) = match (
        market
            .outcome_yes
            .and_then(|d| d.to_string().parse::<f64>().ok()),
        market
            .outcome_no
            .and_then(|d| d.to_string().parse::<f64>().ok()),
    ) {
        (Some(1.0), _) => ("YES", 2u64),
        (_, Some(1.0)) => ("NO", 1u64),
        _ => {
            return Err(ApiError::BadRequest(
                "市场已结算但赢方 outcome 不明确".into(),
            ));
        }
    };

    if acct::redemption_exists_active(&state.db, user_id, &condition_id, outcome, &from_dw).await?
    {
        return Err(ApiError::BadRequest(
            "该归档 DW 上此市场赢仓位已有进行中/完成的赎回".into(),
        ));
    }

    let rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| onchain::POLYGON_RPC_DEFAULT.to_string());
    let pusd: Address = wallet_batch::contracts::COLLATERAL
        .parse()
        .map_err(|e| ApiError::Internal(format!("COLLATERAL 解析失败: {e}")))?;
    let ctf: Address = wallet_batch::contracts::CONDITIONAL_TOKENS
        .parse()
        .map_err(|e| ApiError::Internal(format!("CTF 解析失败: {e}")))?;
    let position_id = onchain::ctf_position_id(pusd, &condition_id, index_set);
    let balance = onchain::ctf_balance_of(&rpc, ctf, from_addr, position_id)
        .await
        .map_err(ApiError::Internal)?;
    if balance <= 0.0 {
        return Err(ApiError::BadRequest(
            "归档 DW 链上无可赎回仓位（balanceOf=0）".into(),
        ));
    }

    let amount_dec = rust_decimal::Decimal::from_f64_retain(balance)
        .ok_or_else(|| ApiError::BadRequest("赎回数量精度异常".into()))?;
    let token_id_str = position_id.to_string();
    let pending = acct::insert_redemption(
        &state.db,
        user_id,
        "polymarket",
        &condition_id,
        outcome,
        &token_id_str,
        amount_dec,
        "archive_manual",
        &from_dw,
    )
    .await
    .map_err(|e| match e {
        sharpside_db::DbError::Conflict(msg) => ApiError::BadRequest(msg),
        other => ApiError::Internal(other.to_string()),
    })?;

    let venue = PolymarketVenue::new()
        .with_kms(state.kms.clone())
        .with_relayer(RelayerClient::new());
    let result = venue.redeem(&cred, &condition_id, balance).await;
    let row = match result {
        Ok(r) => {
            let status = if r.tx_hash.is_some() {
                "mined"
            } else {
                "pending"
            };
            acct::update_redemption_status(
                &state.db,
                pending.id,
                status,
                r.tx_hash.as_deref(),
                None,
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        }
        Err(e) => {
            let note = format!("{e}");
            acct::update_redemption_status(&state.db, pending.id, "failed", None, Some(&note))
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
        }
    };

    let amount_f = amount_dec
        .to_string()
        .parse::<f64>()
        .unwrap_or(balance);

    Ok(ArchiveRedeemResponse {
        archive_id,
        deposit_wallet: from_dw,
        id: row.id,
        status: row.status,
        condition_id: row.condition_id,
        outcome: row.outcome,
        amount: amount_f,
        tx_hash: row.tx_hash,
        relayer_tx_id: row.relayer_tx_id,
        note: row.note,
    })
}
