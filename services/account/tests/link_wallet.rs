//! 绑定第二个钱包 SIWE 验签集成测试。对应安全修复 1.1（堵死「偷 JWT 即可绑任意地址 → 提现」向量）。
//!
//! 需要运行中的 account 服务 + 已迁移的 PG。标记 `#[ignore]`，由 e2e 显式调用：
//!   cargo test -p sharpside-account --test link_wallet -- --ignored --nocapture
//!
//! 流程：钱包 A 登录 → 无签名绑 B 应失败 → A 合法签名绑 B 成功 → B 出现在列表 → nonce 重放被拒。

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

fn build_siwe(domain: &str, address: &str, uri: &str, chain_id: u64, nonce: &str, issued_at: &str, expiration: &str) -> String {
    format!(
        "{domain} wants you to sign in with your Ethereum account:\n{address}\n\n\
         Connect wallet to Sharpside\n\n\
         URI: {uri}\nVersion: 1\nChain ID: {chain_id}\n\
         Nonce: {nonce}\nIssued At: {issued_at}\nExpiration Time: {expiration}"
    )
}

#[derive(Deserialize)]
struct AuthResp { token: String }

#[tokio::test]
#[ignore]
async fn link_wallet_requires_siwe_and_rejects_replay() {
    let c = client();
    // 钱包 A（登录身份）
    let signer_a = PrivateKeySigner::random();
    let addr_a = format!("0x{:x}", signer_a.address());
    // 钱包 B（待绑）
    let signer_b = PrivateKeySigner::random();
    let addr_b = format!("0x{:x}", signer_b.address());

    // 1) A 登录
    let nonce_resp: Value = c
        .get(format!("{ACCT}/auth/wallet/nonce?address={addr_a}"))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let nonce = nonce_resp["nonce"].as_str().unwrap().to_string();
    let domain = nonce_resp["domain"].as_str().unwrap().to_string();
    let chain_id = nonce_resp["chain_id"].as_u64().unwrap();
    let issued_at = nonce_resp["issued_at"].as_str().unwrap().to_string();
    let expiration = (chrono::Utc::now() + chrono::Duration::minutes(5))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let msg_a = build_siwe(&domain, &addr_a, "http://localhost", chain_id, &nonce, &issued_at, &expiration);
    let sig_a = format!("0x{}", hex::encode(signer_a.sign_hash_sync(&eip191_hash_message(msg_a.as_bytes())).unwrap().as_bytes()));
    let auth: AuthResp = c.post(format!("{ACCT}/auth/wallet/token")).json(&serde_json::json!({ "message": msg_a, "signature": sig_a }))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let bearer = format!("Bearer {}", auth.token);

    // 2) 无签名绑 B 应失败（400 缺字段 或 401）
    let no_sig = c.post(format!("{ACCT}/me/wallets")).header("Authorization", &bearer)
        .json(&serde_json::json!({ "label": "B" })).send().await.unwrap();
    assert!(no_sig.status().is_client_error(), "无签名绑钱包应被拒，got {}", no_sig.status());

    // 3) A 试图用 B 的地址但无 B 签名 → 应失败（body 无 message/signature）
    //    （已由上一步覆盖：缺签名即拒）

    // 4) B 合法签名绑 B：先取 B 的 nonce
    let nonce_b_resp: Value = c
        .get(format!("{ACCT}/auth/wallet/nonce?address={addr_b}"))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let nonce_b = nonce_b_resp["nonce"].as_str().unwrap().to_string();
    let msg_b = build_siwe(&domain, &addr_b, "http://localhost", chain_id, &nonce_b, &issued_at, &expiration);
    let sig_b = format!("0x{}", hex::encode(signer_b.sign_hash_sync(&eip191_hash_message(msg_b.as_bytes())).unwrap().as_bytes()));
    let linked: Value = c.post(format!("{ACCT}/me/wallets")).header("Authorization", &bearer)
        .json(&serde_json::json!({ "message": msg_b, "signature": sig_b, "label": "B" }))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(linked["address"].as_str().unwrap(), addr_b, "绑定返回的地址须为验签地址 B");
    println!("✅ 绑定钱包 B 成功 {addr_b}");

    // 5) B 出现在列表
    let wallets: Value = c.get(format!("{ACCT}/me/wallets")).header("Authorization", &bearer)
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let addrs: Vec<&str> = wallets.as_array().unwrap().iter().map(|w| w["address"].as_str().unwrap()).collect();
    assert!(addrs.contains(&addr_b.as_str()), "B 应在已绑钱包列表");
    assert!(addrs.contains(&addr_a.as_str()), "A 仍在列表");
    println!("✅ 钱包列表含 A 与 B");

    // 6) nonce 重放：同一 B 签名再绑应失败（nonce 已消费）
    let replay = c.post(format!("{ACCT}/me/wallets")).header("Authorization", &bearer)
        .json(&serde_json::json!({ "message": msg_b, "signature": sig_b, "label": "B2" }))
        .send().await.unwrap();
    assert_eq!(replay.status().as_u16(), 401, "nonce 重放绑钱包应被拒（401）");
    println!("✅ nonce 重放被拒");
}
