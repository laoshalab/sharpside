//! 钱包登录端到端集成测试。对应安全审计钱包登录方案（模型 A · 身份钱包）。
//!
//! 需要运行中的 account 服务 + 已迁移 0014 的 PG。标记 `#[ignore]`，
//! 由 `infra/e2e.sh` 在服务就绪后显式调用：
//!   cargo test -p sharpside-account --test wallet_login -- --ignored --nocapture
//!
//! 流程：随机 EOA → GET /auth/wallet/nonce → 构造 SIWE → EIP-191 签名 →
//!       POST /auth/wallet/token → GET /me → GET /me/wallets 校验。

use alloy_primitives::utils::eip191_hash_message;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use serde::Deserialize;
use serde_json::Value;

const ACCT: &str = "http://127.0.0.1:8084";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
}

/// 构造 EIP-4361 SIWE 文本（与前端 lib/siwe.js buildSiwe 格式一致）。
fn build_siwe(
    domain: &str,
    address: &str,
    uri: &str,
    chain_id: u64,
    nonce: &str,
    issued_at: &str,
    expiration: &str,
) -> String {
    // 末行不加尾部换行：signinwithethereum 遇空行会报 Unexpected Content
    format!(
        "{domain} wants you to sign in with your Ethereum account:\n{address}\n\n\
         Sign in to Sharpside\n\n\
         URI: {uri}\n\
         Version: 1\n\
         Chain ID: {chain_id}\n\
         Nonce: {nonce}\n\
         Issued At: {issued_at}\n\
         Expiration Time: {expiration}"
    )
}

#[derive(Deserialize)]
struct AuthResp {
    token: String,
    user: Value,
}

#[tokio::test]
#[ignore]
async fn wallet_login_e2e() {
    let c = client();
    let signer = PrivateKeySigner::random();
    let address = format!("0x{:x}", signer.address());

    // 1) GET /auth/wallet/nonce
    let nonce_resp: Value = c
        .get(format!("{ACCT}/auth/wallet/nonce?address={address}"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let nonce = nonce_resp["nonce"].as_str().unwrap().to_string();
    let domain = nonce_resp["domain"].as_str().unwrap().to_string();
    let chain_id = nonce_resp["chain_id"].as_u64().unwrap();
    let issued_at = nonce_resp["issued_at"].as_str().unwrap().to_string();
    assert!(!nonce.is_empty(), "nonce 非空");
    assert!(!domain.is_empty(), "domain 非空");

    // 2) 构造 SIWE + EIP-191 签名
    let expiration = (chrono::Utc::now() + chrono::Duration::minutes(5))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let message = build_siwe(
        &domain,
        &address,
        "http://localhost",
        chain_id,
        &nonce,
        &issued_at,
        &expiration,
    );
    let digest = eip191_hash_message(message.as_bytes());
    let sig = signer.sign_hash_sync(&digest).unwrap();
    let signature = format!("0x{}", hex::encode(sig.as_bytes()));

    // 3) POST /auth/wallet/token（程序化路径：body 含 JWT）
    let auth: AuthResp = c
        .post(format!("{ACCT}/auth/wallet/token"))
        .json(&serde_json::json!({ "message": message, "signature": signature }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!auth.token.is_empty(), "JWT 非空");
    let user_id = auth.user["id"].as_str().unwrap().to_string();
    assert!(!user_id.is_empty(), "user_id 非空");
    println!("✅ 钱包登录成功 user_id={user_id} address={address}");

    // 3b) 浏览器路径 /auth/wallet 不应在 body 返回 token（需新 nonce，此处仅文档约定；
    //     完整 cookie-only 由单元/契约覆盖；本 e2e 校验 token 端点可用）。

    // 4) GET /me（用 JWT）
    let me: Value = c
        .get(format!("{ACCT}/me"))
        .header("Authorization", format!("Bearer {}", auth.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["id"].as_str().unwrap(), user_id, "/me 返回同一 user_id");
    println!("✅ /me 校验通过");

    // 5) GET /me/wallets（校验地址已绑）
    let wallets: Value = c
        .get(format!("{ACCT}/me/wallets"))
        .header("Authorization", format!("Bearer {}", auth.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = wallets.as_array().unwrap();
    assert_eq!(arr.len(), 1, "新用户应恰好 1 个钱包");
    assert_eq!(arr[0]["address"].as_str().unwrap(), address, "钱包地址匹配");
    assert!(arr[0]["is_primary"].as_bool().unwrap(), "首钱包为 primary");
    println!("✅ /me/wallets 校验通过（1 个 primary 钱包）");

    // 6) 重放防护：同一 nonce 二次登录应失败
    let replay = c
        .post(format!("{ACCT}/auth/wallet/token"))
        .json(&serde_json::json!({ "message": message, "signature": signature }))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status().as_u16(), 401, "nonce 重放应被拒（401）");
    println!("✅ nonce 重放被拒");
}
