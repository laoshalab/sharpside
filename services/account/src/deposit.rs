//! Deposit Wallet 一次性预配：生成 owner EOA → KMS 加密 → CREATE2 派生 → Relayer 部署
//! → L1 派生 L2 凭证 → batch approve → 余额同步 → 入库。
//!
//! 对应 `docs/CHANNEL_A_SIGNING.md` §3.1 与 §7 待办「account 服务
//! `/me/deposit-wallet/provision` 端点」。
//!
//! ## 离线 / 在线双模式
//!
//! 真实部署需 Polymarket Relayer + CLOB + Builder API key + 网络。本机受限网络环境无法联调，
//! 故提供两模式：
//! - **离线（默认）**：完成 owner EOA 生成、KMS 加密、CREATE2 地址派生、L2 凭证占位生成、
//!   入库；跳过 Relayer 部署 / L1 deriveApiKey / batch approve / 余额同步。响应标注 `live=false`
//!   与跳过步骤。`COPIER_DRY_RUN` 路径可立即用此凭证跑闭环。
//! - **在线**：env `POLYMARKET_PROVISION_LIVE=1` + `POLYMARKET_BUILDER_API_KEY`
//!   + `POLYMARKET_BUILDER_SECRET` + `POLYMARKET_BUILDER_PASSPHRASE` + 网络可达 → 跑全流程。

use crate::error::ApiError;
use crate::state::AppState;
use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use rand::RngCore;
use serde::Serialize;
use sharpside_db::queries::account as acct;
use sharpside_venues_polymarket::{
    clob, deposit::derive_deposit_wallet_address, wallet_batch, L2Credentials, PolymarketClient,
    RelayerClient,
};
use std::str::FromStr;
use tracing::info;

/// `POST /me/deposit-wallet/provision` 响应。
#[derive(Debug, Serialize)]
pub struct ProvisionResponse {
    /// 是否完成在线全流程（false = 离线模式，跳过网络步骤）。
    pub live: bool,
    pub owner_address: String,
    pub deposit_wallet_address: String,
    /// 跳过的步骤（离线模式下含 relayer_deploy / l1_derive_api_key / batch_approve / balance_sync）。
    pub skipped: Vec<String>,
    /// 入库的 credential id（user_id + platform 复合主键）。
    pub user_id: uuid::Uuid,
    pub platform: String,
}

/// 预配 deposit wallet。对应 `docs/CHANNEL_A_SIGNING.md` §3.1 step 1-9。
pub async fn provision(
    state: AppState,
    user_id: uuid::Uuid,
    builder_code: String,
) -> Result<ProvisionResponse, ApiError> {
    let live = std::env::var("POLYMARKET_PROVISION_LIVE").ok().as_deref() == Some("1");

    // step 1: 生成 owner EOA 私钥（32 字节随机 → hex → PrivateKeySigner）。
    let mut key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key_bytes);
    let owner_key_hex = format!("0x{}", alloy_primitives::hex::encode(key_bytes));
    let owner_signer = PrivateKeySigner::from_str(&owner_key_hex)
        .map_err(|e| ApiError::Internal(format!("owner 私钥构造失败: {e}")))?;
    let owner_address = owner_signer.address();

    // step 2: KMS 加密 owner EOA 私钥 → encrypted_owner_key。
    // 用 AppState 注入的 KMS（main.rs 构造：LocalKms 生产 / DevKms dev）。copier 须注入同一 KMS 解密。
    let kms = &state.kms;
    let encrypted_owner_key = kms
        .encrypt(&owner_key_hex)
        .map_err(|e| ApiError::Internal(format!("KMS 加密 owner key 失败: {e}")))?;

    // step 3: CREATE2 派生 deposit wallet 地址（Solady ERC1967 beacon clone，从 beacon 地址算 init code hash；
    // 无需 env）。移植自 ~/文档/sharpside/crates/poly-relayer/src/derive.rs。
    let deposit_wallet_address = derive_deposit_wallet_address(owner_address);

    let mut skipped: Vec<String> = Vec::new();
    let l2: L2Credentials;

    if live {
        // step 4: Relayer WALLET-CREATE（部署 deposit wallet，gasless，无需用户签名）。
        // POST /submit {type:WALLET-CREATE, from:owner, to:DEPOSIT_WALLET_FACTORY}，builder HMAC 鉴权。
        let relayer = RelayerClient::new();
        let submit = relayer
            .wallet_create(owner_address)
            .await
            .map_err(|e| ApiError::Internal(format!("Relayer WALLET-CREATE 失败: {e}")))?;
        info!(owner = %owner_address, tx_id = ?submit.transaction_id, "Relayer WALLET-CREATE 已提交");
        if let Some(tx_id) = submit.transaction_id.as_deref() {
            if !tx_id.is_empty() {
                match relayer.poll_confirmed(tx_id).await {
                    Ok(row) => {
                        info!(owner = %owner_address, state = ?row.state, tx = ?row.transaction_hash, "Relayer 部署已确认")
                    }
                    Err(e) => {
                        tracing::warn!(owner = %owner_address, error = %e, "Relayer 轮询未确认（可能仍在上链）")
                    }
                }
            }
        }

        // step 5: ClobClient L1：owner EOA 签 EIP-712 → createOrDeriveApiKey → L2 凭证。
        // POLY_ADDRESS = owner EOA（= L2 凭证所属）；signature_type=3 让服务端映射 owner → deposit wallet。
        let client = PolymarketClient::new();
        let ts = chrono::Utc::now().timestamp();
        let auth_sig = clob::build_l1_auth_signature(&owner_signer, ts)
            .map_err(|e| ApiError::Internal(format!("L1 auth 签名失败: {e}")))?;
        l2 = client
            .derive_api_key_l1(owner_address, &auth_sig, ts)
            .await
            .map_err(|e| ApiError::Internal(format!("L1 deriveApiKey 失败: {e}")))?;
        info!(owner = %owner_address, "L2 凭证已派生");

        // step 6: KMS 加密 L2 secret。
        let encrypted_l2_secret = kms
            .encrypt(&l2.secret)
            .map_err(|e| ApiError::Internal(format!("KMS 加密 L2 secret 失败: {e}")))?;

        // step 7: Relayer WALLET batch（approve pUSD → CTF Exchange/NegRisk/Adapter；CT → Exchange setApprovalForAll）。
        // approve 必须从 deposit wallet 发起（owner EOA 的 approve 不算），故走 relayer `WALLET` batch：
        // 取 WALLET nonce → owner EIP-712 签 `Batch`（普通 65 字节，非 ERC-7739）→ POST /submit type=WALLET → 轮询确认。
        // approve 与余额无关，可在充值前提交（DW 部署后即可）；后续充值 pUSD 即可直接交易。
        let approve_calls = wallet_batch::trading_approves();
        let nonce = relayer
            .wallet_nonce(owner_address)
            .await
            .map_err(|e| ApiError::Internal(format!("Relayer /nonce 失败: {e}")))?;
        let deadline = (chrono::Utc::now().timestamp() + 600).max(0) as u64;
        let deadline_u256 = alloy_primitives::U256::from(deadline);
        let nonce_u256 = alloy_primitives::U256::from_str_radix(nonce.trim(), 10)
            .unwrap_or(alloy_primitives::U256::ZERO);
        let sig = wallet_batch::sign_wallet_batch(
            &owner_signer,
            deposit_wallet_address,
            nonce_u256,
            deadline_u256,
            &approve_calls,
        )
        .map_err(|e| ApiError::Internal(format!("WALLET batch 签名失败: {e}")))?;
        let batch_submit = relayer
            .wallet_batch(
                owner_address,
                deposit_wallet_address,
                &nonce,
                &deadline.to_string(),
                &sig,
                &approve_calls,
            )
            .await
            .map_err(|e| ApiError::Internal(format!("Relayer WALLET batch 失败: {e}")))?;
        info!(owner = %owner_address, tx_id = ?batch_submit.transaction_id, "WALLET batch approve 已提交");
        if let Some(tx_id) = batch_submit
            .transaction_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            match relayer.poll_confirmed(tx_id).await {
                Ok(row) => {
                    info!(owner = %owner_address, state = ?row.state, tx = ?row.transaction_hash, "WALLET batch 已确认")
                }
                Err(e) => {
                    tracing::warn!(owner = %owner_address, error = %e, "WALLET batch 轮询未确认（可能仍在上链）")
                }
            }
        }

        // step 8: CLOB update_balance_allowance(signature_type=3) 余额同步。
        // POLY_ADDRESS = owner EOA（L2 凭证所属），signature_type=3 → 服务端映射到 deposit wallet。
        client
            .update_balance_allowance(owner_address, &l2.api_key, &l2.secret, &l2.passphrase)
            .await
            .map_err(|e| ApiError::Internal(format!("update_balance_allowance 失败: {e}")))?;

        store_credential(
            &state,
            user_id,
            &deposit_wallet_address,
            owner_address,
            &encrypted_owner_key,
            &l2,
            &encrypted_l2_secret,
            &builder_code,
            true, // live
            &[],  // no skipped
        )
        .await?;
    } else {
        // 离线模式：L2 凭证占位生成（dev），跳过网络步骤。
        l2 = L2Credentials {
            api_key: format!("dev-{}", uuid::Uuid::new_v4()),
            secret: format!("dev-secret-{}", uuid::Uuid::new_v4()),
            passphrase: format!("dev-pass-{}", uuid::Uuid::new_v4().as_simple()),
        };
        let encrypted_l2_secret = kms
            .encrypt(&l2.secret)
            .map_err(|e| ApiError::Internal(format!("KMS 加密 L2 secret 失败: {e}")))?;
        skipped.extend([
            "relayer_deploy".into(),
            "l1_derive_api_key".into(),
            "batch_approve".into(),
            "balance_sync".into(),
        ]);
        store_credential(
            &state,
            user_id,
            &deposit_wallet_address,
            owner_address,
            &encrypted_owner_key,
            &l2,
            &encrypted_l2_secret,
            &builder_code,
            false, // offline
            &skipped,
        )
        .await?;
        info!(user_id = %user_id, "离线预配完成（dry_run 闭环可用）");
    }

    Ok(ProvisionResponse {
        live,
        owner_address: owner_address.to_string(),
        deposit_wallet_address: deposit_wallet_address.to_string(),
        skipped,
        user_id,
        platform: "polymarket".into(),
    })
}

/// step 9: 写 user_venue_credentials (kind=deposit_wallet_delegated, proxy_address=deposit wallet)。
///
/// 同时持久化 `provision_live` + `provision_steps`（8 步）+ `kms_key_id`，
/// 供 `GET /me/delegation` 直接读取（对应 `docs/FRONTEND_DESIGN.md` §11 provision 状态持久化）。
///
/// 8 步顺序对齐前端 stepper：
/// ①owner EOA ②KMS 加密 ③CREATE2 派生 ④Relayer 部署 ⑤L1 deriveApiKey
/// ⑥batch approve ⑦余额同步 ⑧入库。
async fn store_credential(
    state: &AppState,
    user_id: uuid::Uuid,
    deposit_wallet_address: &Address,
    owner_address: Address,
    encrypted_owner_key: &str,
    l2: &L2Credentials,
    encrypted_l2_secret: &str,
    builder_code: &str,
    live: bool,
    skipped: &[String],
) -> Result<(), ApiError> {
    let provision_steps = build_provision_steps(live, skipped);
    let blob = serde_json::json!({
        "kind": "deposit_wallet_delegated",
        "deposit_wallet_address": deposit_wallet_address.to_string(),
        "owner_address": owner_address.to_string(),
        "encrypted_owner_key": encrypted_owner_key,
        "l2_api_key": l2.api_key,
        "encrypted_l2_secret": encrypted_l2_secret,
        "l2_passphrase": l2.passphrase,
        "builder_code": builder_code,
        "provision_live": live,
        "provision_steps": provision_steps,
        "kms_key_id": state.kms.name(),
    });
    acct::upsert_credential_with_proxy(
        &state.db,
        user_id,
        "polymarket",
        &blob,
        Some(&deposit_wallet_address.to_string()),
    )
    .await?;
    Ok(())
}

/// 构造 8 步状态数组（字符串：done / skipped / pending / failed）。
fn build_provision_steps(live: bool, skipped: &[String]) -> Vec<&'static str> {
    if live {
        return vec![
            "done", "done", "done", "done", "done", "done", "done", "done",
        ];
    }
    // 离线：①②③ done；④relayer ⑤l1 ⑥approve ⑦balance skipped；⑧入库 done
    let skip_set: std::collections::HashSet<&str> = skipped.iter().map(|s| s.as_str()).collect();
    let step4 = if skip_set.contains("relayer_deploy") {
        "skipped"
    } else {
        "done"
    };
    let step5 = if skip_set.contains("l1_derive_api_key") {
        "skipped"
    } else {
        "done"
    };
    let step6 = if skip_set.contains("batch_approve") {
        "skipped"
    } else {
        "done"
    };
    let step7 = if skip_set.contains("balance_sync") {
        "skipped"
    } else {
        "done"
    };
    vec!["done", "done", "done", step4, step5, step6, step7, "done"]
}
