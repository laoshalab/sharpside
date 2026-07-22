//! 一次性：查 live_post_order 返回的订单状态（L2 HMAC GET /data/order/{id}）。`#[ignore]`。
//!
//! 跑法：
//! ```bash
//! set -a; source .env.local; set +a
//! POLYMARKET_ORDER_ID=0x... cargo test -p sharpside-venues-polymarket --offline --test live_order_status -- --ignored --nocapture
//! ```

use alloy_signer_local::PrivateKeySigner;
use sharpside_venues_polymarket::{clob, PolymarketClient};
use std::str::FromStr;

#[tokio::test]
#[ignore]
async fn live_query_order_status() {
    let order_id = std::env::var("POLYMARKET_ORDER_ID").expect("需设 POLYMARKET_ORDER_ID");
    let owner_pk =
        std::env::var("POLYMARKET_TEST_OWNER_PK").expect("需设 POLYMARKET_TEST_OWNER_PK");
    let owner_signer = PrivateKeySigner::from_str(&owner_pk).unwrap();
    let owner_address = owner_signer.address();

    // L2 凭证（重新派生；createOrDeriveApiKey 幂等）
    let client = PolymarketClient::new();
    let ts = chrono::Utc::now().timestamp();
    let auth_sig = clob::build_l1_auth_signature(&owner_signer, ts).unwrap();
    let l2 = client
        .derive_api_key_l1(owner_address, &auth_sig, ts)
        .await
        .expect("L1 deriveApiKey 失败");

    let order = client
        .get_order_l2(
            &order_id,
            &l2.api_key,
            &l2.secret,
            &l2.passphrase,
            owner_address,
        )
        .await;
    match order {
        Ok(v) => {
            eprintln!("order {order_id} status: {v:#}");
            eprintln!("STAGE3_ORDER_STATUS=OK");
        }
        Err(e) => {
            eprintln!("order {order_id} 查询失败: {e}");
            eprintln!("STAGE3_ORDER_STATUS=QUERY_FAILED");
        }
    }
}
