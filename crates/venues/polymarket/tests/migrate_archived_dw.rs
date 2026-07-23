//! 一次性：部署已归档（未上链）的旧 Deposit Wallet，并把全部 pUSD 转到新 DW。
//!
//! 环境变量（勿打印私钥）：
//! - `MIGRATE_OWNER_PK`：旧 owner EOA 私钥（0x…）
//! - `MIGRATE_FROM_DW`：旧 deposit wallet 地址
//! - `MIGRATE_TO_DW`：新 deposit wallet 地址
//!
//! ```bash
//! set -a; source .env; set +a
//! POLYMARKET_HTTP_PROXY=http://127.0.0.1:7890 \
//!   cargo test -p sharpside-venues-polymarket --offline --test migrate_archived_dw -- --ignored --nocapture
//! ```

use alloy_primitives::{Address, U256};
use alloy_signer_local::PrivateKeySigner;
use sharpside_venues_polymarket::{
    deposit::derive_deposit_wallet_address, onchain, wallet_batch, RelayerClient,
};
use std::str::FromStr;

#[tokio::test]
#[ignore]
async fn migrate_archived_dw_to_new() {
    let pk = std::env::var("MIGRATE_OWNER_PK").expect("需设 MIGRATE_OWNER_PK");
    let from_dw = std::env::var("MIGRATE_FROM_DW").expect("需设 MIGRATE_FROM_DW");
    let to_dw = std::env::var("MIGRATE_TO_DW").expect("需设 MIGRATE_TO_DW");

    let signer = PrivateKeySigner::from_str(pk.trim()).expect("owner pk 解析失败");
    let owner = signer.address();
    let derived = derive_deposit_wallet_address(owner);
    let from: Address = from_dw.parse().expect("FROM_DW");
    let to: Address = to_dw.parse().expect("TO_DW");
    assert_eq!(
        derived, from,
        "归档 owner 的 CREATE2 地址与 MIGRATE_FROM_DW 不一致"
    );
    eprintln!("owner={owner}");
    eprintln!("from_dw={from}");
    eprintln!("to_dw={to}");

    let rpc = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| onchain::POLYGON_RPC_DEFAULT.to_string());
    let pusd: Address = wallet_batch::contracts::COLLATERAL.parse().unwrap();

    let bal0 = onchain::pusd_balance_of(&rpc, pusd, from)
        .await
        .expect("读旧 DW 余额失败");
    eprintln!("from balance before = {bal0} pUSD");
    assert!(bal0 > 0.0, "旧 DW 无余额，无需迁移");
    let raw = (bal0 * 1_000_000.0).round() as u128;
    let amount = U256::from(raw);

    // 1) 未部署则 Relayer WALLET-CREATE（gasless，仅需 owner 地址）
    let relayer = RelayerClient::new();
    let submit = relayer
        .wallet_create(owner)
        .await
        .unwrap_or_else(|e| panic!("WALLET-CREATE 失败: {e}"));
    eprintln!(
        "WALLET-CREATE tx_id={:?} hash={:?}",
        submit.transaction_id, submit.transaction_hash
    );
    if let Some(tx_id) = submit.transaction_id.as_deref().filter(|s| !s.is_empty()) {
        match relayer.poll_confirmed(tx_id).await {
            Ok(row) => eprintln!(
                "deploy confirmed state={:?} hash={:?}",
                row.state, row.transaction_hash
            ),
            Err(e) => eprintln!("deploy poll warn: {e}（可能仍在上链，继续尝试 transfer）"),
        }
    }

    // 2) WALLET batch：pUSD.transfer(to, all)
    let calls = vec![wallet_batch::WalletCall {
        target: pusd,
        value: U256::ZERO,
        data: wallet_batch::transfer_calldata(to, amount),
    }];
    let nonce = relayer
        .wallet_nonce(owner)
        .await
        .unwrap_or_else(|e| panic!("nonce 失败: {e}"));
    let deadline = (chrono::Utc::now().timestamp() + 3600).max(0) as u64;
    let nonce_u256 = U256::from_str_radix(nonce.trim(), 10).unwrap_or(U256::ZERO);
    let sig = wallet_batch::sign_wallet_batch(
        &signer,
        from,
        nonce_u256,
        U256::from(deadline),
        &calls,
    )
    .unwrap_or_else(|e| panic!("sign 失败: {e}"));
    let batch = relayer
        .wallet_batch(owner, from, &nonce, &deadline.to_string(), &sig, &calls)
        .await
        .unwrap_or_else(|e| panic!("WALLET batch 失败: {e}"));
    eprintln!(
        "transfer submitted tx_id={:?} hash={:?}",
        batch.transaction_id, batch.transaction_hash
    );
    if let Some(tx_id) = batch.transaction_id.as_deref().filter(|s| !s.is_empty()) {
        let row = relayer
            .poll_confirmed(tx_id)
            .await
            .unwrap_or_else(|e| panic!("transfer poll 失败: {e}"));
        eprintln!(
            "transfer confirmed state={:?} hash={:?}",
            row.state, row.transaction_hash
        );
    }

    // 3) 校验
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let bal_from = onchain::pusd_balance_of(&rpc, pusd, from)
        .await
        .expect("读旧余额");
    let bal_to = onchain::pusd_balance_of(&rpc, pusd, to)
        .await
        .expect("读新余额");
    eprintln!("from after={bal_from}  to after={bal_to}");
    assert!(bal_from < 0.000_001, "旧 DW 应清零");
    assert!(
        (bal_to - bal0).abs() < 0.000_001 || bal_to + 0.000_001 >= bal0,
        "新 DW 应收到资金"
    );
}
