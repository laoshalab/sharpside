//! 一次性：撤掉 live_post_order 下的真实挂单（L2 HMAC DELETE /order）。`#[ignore]`。
//!
//! 跑法：
//! ```bash
//! set -a; source .env.local; set +a
//! POLYMARKET_ORDER_ID=0x... cargo test -p sharpside-venues-polymarket --offline --test live_cancel_order -- --ignored --nocapture
//! ```

use alloy_signer_local::PrivateKeySigner;
use sharpside_venues_polymarket::{clob, deposit::derive_deposit_wallet_address, PolymarketClient};
use std::str::FromStr;

#[tokio::test]
#[ignore]
async fn live_cancel_placed_order() {
    let order_id = std::env::var("POLYMARKET_ORDER_ID").expect("需设 POLYMARKET_ORDER_ID");
    let owner_pk =
        std::env::var("POLYMARKET_TEST_OWNER_PK").expect("需设 POLYMARKET_TEST_OWNER_PK");
    let owner_signer = PrivateKeySigner::from_str(&owner_pk).unwrap();
    let owner_address = owner_signer.address();
    let _dw = derive_deposit_wallet_address(owner_address);

    let client = PolymarketClient::new();
    let ts = chrono::Utc::now().timestamp();
    let auth_sig = clob::build_l1_auth_signature(&owner_signer, ts).unwrap();
    let l2 = client
        .derive_api_key_l1(owner_address, &auth_sig, ts)
        .await
        .expect("L1 deriveApiKey 失败");

    eprintln!("撤单 orderID={order_id}");
    match client
        .cancel_order_l2(
            &order_id,
            &l2.api_key,
            &l2.secret,
            &l2.passphrase,
            owner_address,
        )
        .await
    {
        Ok(v) => {
            eprintln!("撤单响应: {v:#}");
            eprintln!("STAGE3_CANCEL=OK");
        }
        Err(e) => {
            eprintln!("撤单失败: {e}");
            eprintln!("STAGE3_CANCEL=FAILED");
        }
    }

    // 复查订单状态，确认已 CANCELLED
    match client
        .get_order_l2(
            &order_id,
            &l2.api_key,
            &l2.secret,
            &l2.passphrase,
            owner_address,
        )
        .await
    {
        Ok(v) => eprintln!("撤单后状态: {v:#}"),
        Err(e) => eprintln!("撤单后状态查询失败: {e}"),
    }
}
