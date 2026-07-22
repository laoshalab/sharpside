//! 真实联调 Stage 2：live provision（部署 deposit wallet）。`#[ignore]`，不进常规 CI。
//!
//! 直接跑通道 A 预配的链上步骤（跳过 DB 存储——正交），真打 Polymarket Relayer + CLOB：
//! 1. 生成 owner EOA（`PrivateKeySigner::random()`）
//! 2. CREATE2 派生 deposit wallet 地址
//! 3. Relayer `WALLET-CREATE`（**真实部署**，gasless 但链上不可逆）
//! 4. CLOB L1 `deriveApiKey` → L2 凭证
//! 5. CLOB `update_balance_allowance`(signatureType=3) 余额同步
//!
//! 跑法（需代理 + full_network + builder/relayer 凭证；中国等地区封锁 Polymarket 须代理）：
//! ```bash
//! POLYMARKET_HTTP_PROXY=http://127.0.0.1:7890 \
//!   cargo test -p sharpside-venues-polymarket --offline --test live_provision -- --ignored --nocapture
//! ```
//!
//! 注意：每次跑都在 Polygon 上部署一个**新** deposit wallet（新 owner EOA）。gasless（Relayer 出 gas），
//! 但合约真实上链、不可逆。失败信息即联调发现的真实对接差异。

use alloy_signer_local::PrivateKeySigner;
use sharpside_venues_polymarket::{
    clob, deposit::derive_deposit_wallet_address, PolymarketClient, RelayerClient,
};

#[tokio::test]
#[ignore]
async fn live_provision_deploys_deposit_wallet() {
    // step 1: 生成 owner EOA
    let owner_signer = PrivateKeySigner::random();
    let owner_address = owner_signer.address();
    eprintln!("step1 owner EOA: {owner_address}");

    // step 2: CREATE2 派生 deposit wallet 地址
    let deposit_wallet_address = derive_deposit_wallet_address(owner_address);
    eprintln!("step2 deposit wallet (CREATE2): {deposit_wallet_address}");

    // step 3: Relayer WALLET-CREATE（真实部署，无需用户签名）
    let relayer = RelayerClient::new();
    let resp = relayer.wallet_create(owner_address).await;
    let submit = match resp {
        Ok(r) => {
            eprintln!(
                "step3 WALLET-CREATE ok: tx_id={:?} tx_hash={:?}",
                r.transaction_id, r.transaction_hash
            );
            r
        }
        Err(e) => panic!("step3 WALLET-CREATE 失败（联调差异）: {e}"),
    };

    // step 3b: 轮询至确认（~90s）
    if let Some(tx_id) = submit.transaction_id.as_deref() {
        if !tx_id.is_empty() {
            match relayer.poll_confirmed(tx_id).await {
                Ok(row) => eprintln!(
                    "step3b 部署确认: state={:?} tx={:?} proxy={:?}",
                    row.state, row.transaction_hash, row.proxy_address
                ),
                Err(e) => eprintln!("step3b 轮询未确认（可能仍上链中）: {e}"),
            }
        }
    }
    // 校验 /deployed 认 CREATE2 派生地址
    match relayer
        .is_deployed(&deposit_wallet_address.to_string())
        .await
    {
        Ok(d) => eprintln!("step3c /deployed({}) = {}", deposit_wallet_address, d),
        Err(e) => eprintln!("step3c /deployed 查询失败: {e}"),
    }

    // step 4: CLOB L1 deriveApiKey → L2 凭证
    // POLY_ADDRESS = owner EOA（= L2 凭证所属）；signature_type=3 → 服务端映射 owner → deposit wallet。
    let client = PolymarketClient::new();
    let ts2 = chrono::Utc::now().timestamp();
    let auth_sig = clob::build_l1_auth_signature(&owner_signer, ts2).unwrap();
    let l2 = client
        .derive_api_key_l1(owner_address, &auth_sig, ts2)
        .await;
    let l2 = match l2 {
        Ok(c) => {
            eprintln!(
                "step4 L2 派生 ok: api_key={} secret_len={} passphrase_len={}",
                c.api_key,
                c.secret.len(),
                c.passphrase.len()
            );
            c
        }
        Err(e) => panic!("step4 L1 deriveApiKey 失败（联调差异）: {e:?}"),
    };

    // step 5: update_balance_allowance（signature_type=3；POLY_ADDRESS=owner EOA）
    match client
        .update_balance_allowance(owner_address, &l2.api_key, &l2.secret, &l2.passphrase)
        .await
    {
        Ok(v) => eprintln!("step5 balance-allowance ok: {v}"),
        Err(e) => {
            eprintln!("step5 update_balance_allowance 失败（可能需先充 pUSD；联调差异）: {e:?}")
        }
    }
}
