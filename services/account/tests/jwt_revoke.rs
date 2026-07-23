//! JWT 吊销（denylist）集成测试。对应安全修复 1.2。
//!
//! 需要运行中的 account 服务 + 已迁移 0035 的 PG。标记 `#[ignore]`，由 e2e 显式调用：
//!   cargo test -p sharpside-account --test jwt_revoke -- --ignored --nocapture
//!
//! 流程：钱包登录 → GET /me 200 → POST /auth/logout → GET /me 401（吊销立即生效）。
//! 另测：无 jti 的旧 token 应被拒（强制重新登录）。

use alloy_primitives::utils::eip191_hash_message;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
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
         Sign in to Sharpside\n\n\
         URI: {uri}\nVersion: 1\nChain ID: {chain_id}\n\
         Nonce: {nonce}\nIssued At: {issued_at}\nExpiration Time: {expiration}"
    )
}

#[derive(Deserialize)]
struct AuthResp { token: String }

#[tokio::test]
#[ignore]
async fn logout_revokes_token_immediately() {
    let c = client();
    let signer = PrivateKeySigner::random();
    let address = format!("0x{:x}", signer.address());

    // 登录
    let nonce_resp: Value = c
        .get(format!("{ACCT}/auth/wallet/nonce?address={address}"))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let nonce = nonce_resp["nonce"].as_str().unwrap().to_string();
    let domain = nonce_resp["domain"].as_str().unwrap().to_string();
    let chain_id = nonce_resp["chain_id"].as_u64().unwrap();
    let issued_at = nonce_resp["issued_at"].as_str().unwrap().to_string();
    let expiration = (chrono::Utc::now() + chrono::Duration::minutes(5))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let msg = build_siwe(&domain, &address, "http://localhost", chain_id, &nonce, &issued_at, &expiration);
    let sig = format!("0x{}", hex::encode(signer.sign_hash_sync(&eip191_hash_message(msg.as_bytes())).unwrap().as_bytes()));
    let auth: AuthResp = c.post(format!("{ACCT}/auth/wallet/token")).json(&serde_json::json!({ "message": msg, "signature": sig }))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let bearer = format!("Bearer {}", auth.token);

    // 登录后 /me 可用
    let me = c.get(format!("{ACCT}/me")).header("Authorization", &bearer).send().await.unwrap();
    assert_eq!(me.status().as_u16(), 200, "登录后 /me 应 200");

    // 登出
    let out = c.post(format!("{ACCT}/auth/logout")).header("Authorization", &bearer).send().await.unwrap();
    assert_eq!(out.status().as_u16(), 200, "登出应 200");
    println!("✅ 登出成功，token 已入 denylist");

    // 旧 token 立即失效
    let me2 = c.get(format!("{ACCT}/me")).header("Authorization", &bearer).send().await.unwrap();
    assert_eq!(me2.status().as_u16(), 401, "登出后旧 token 应 401（吊销立即生效）");
    println!("✅ 旧 token 登出后立即 401");
}

#[tokio::test]
#[ignore]
async fn token_without_jti_rejected() {
    let c = client();
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string());
    // 构造无 jti 的旧式 token（合法签名 + 未过期）
    let exp = (chrono::Utc::now().timestamp() + 3600) as usize;
    let claims = serde_json::json!({ "sub": uuid::Uuid::new_v4().to_string(), "exp": exp });
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    ).unwrap();
    let me = c.get(format!("{ACCT}/me")).header("Authorization", format!("Bearer {token}")).send().await.unwrap();
    assert_eq!(me.status().as_u16(), 401, "无 jti 的旧 token 应被拒（强制重新登录）");
    println!("✅ 无 jti token 被拒");
}
