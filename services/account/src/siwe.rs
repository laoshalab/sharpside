//! SIWE（EIP-4361）钱包登录验签。对应安全审计钱包登录方案（模型 A · 身份钱包）。
//!
//! 仅做 EOA EIP-191 验签：用 `signinwithethereum` crate（不启用 alloy feature，
//! 自带 k256/sha3 做 ecrecover，与本项目 alloy 栈解耦，避免版本冲突）。
//!
//! 校验项：domain 绑定（防钓鱼）、URI 白名单（防同域跨页签名）、chainId 白名单、
//! 时间有效（valid_at）、issued_at 新鲜度（防陈旧重放）、EIP-191 验签
//!（crate 内部已比对恢复地址 == msg.address，不符返回 `VerificationError::Signer`）。

use crate::config::normalize_uri;
use crate::error::ApiError;
use hex::FromHex;
use signinwithethereum::Message;
use time::OffsetDateTime;

/// 解析 + 校验 + 验签 SIWE 消息。
///
/// 成功返回已认证的 `Message`，其 `address` 字段即被证明的签名者地址。
/// 调用方据此做 nonce 消费 + 用户 upsert。
pub fn verify_and_validate(
    message_text: &str,
    signature_hex: &str,
    expected_domain: &str,
    allowed_uris: &[String],
    allowed_chains: &[u64],
    max_age_secs: i64,
) -> Result<Message, ApiError> {
    // 去掉首尾空白与多余尾部换行：前端/钱包偶发带 trailing `\n`，
    // signinwithethereum 会把空行当成 Unexpected Content。
    let message_text = message_text.trim_end_matches(['\r', '\n']).trim();
    let msg: Message = message_text
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("invalid siwe message: {e}")))?;

    // domain 绑定（防钓鱼跨站签名）
    if msg.domain.as_str() != expected_domain {
        return Err(ApiError::Unauthorized("siwe domain mismatch".into()));
    }
    // URI 白名单（防同域跨页/错误 origin 签名）
    let uri = normalize_uri(msg.uri.as_str());
    if allowed_uris.is_empty() || !allowed_uris.iter().any(|u| u == &uri) {
        return Err(ApiError::Unauthorized(format!(
            "siwe uri not allowed: {uri}"
        )));
    }
    // chainId 白名单
    if !allowed_chains.contains(&msg.chain_id) {
        return Err(ApiError::Unauthorized(format!(
            "chain {} not allowed",
            msg.chain_id
        )));
    }
    // 时间有效（not_before / expiration_time）
    let now = OffsetDateTime::now_utc();
    if !msg.valid_at(&now) {
        return Err(ApiError::Unauthorized(
            "siwe expired or not yet valid".into(),
        ));
    }
    // 新鲜度：issued_at 距今不超过 max_age_secs（防陈旧消息被重放）；
    // 允许 60s 时钟偏移（issued_at 略晚于 now）。
    let issued_ts = msg.issued_at.as_ref().unix_timestamp();
    let now_ts = now.unix_timestamp();
    if now_ts - issued_ts > max_age_secs || issued_ts - now_ts > 60 {
        return Err(ApiError::Unauthorized("siwe message stale".into()));
    }

    // EIP-191 验签：65 字节 r||s||v（v 为 27/28 或 0/1，crate 内部 % 27 处理）
    let sig_hex = signature_hex.strip_prefix("0x").unwrap_or(signature_hex);
    let sig_bytes = <[u8; 65]>::from_hex(sig_hex)
        .map_err(|e| ApiError::BadRequest(format!("invalid signature: {e}")))?;
    msg.verify_eip191(&sig_bytes)
        .map_err(|e| ApiError::Unauthorized(format!("signature verify failed: {e}")))?;

    Ok(msg)
}

/// 把 `Message.address`（`[u8;20]`）转为小写 `0x` hex 字符串（DB 统一存小写）。
pub fn address_hex(msg: &Message) -> String {
    format!("0x{}", hex::encode(msg.address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;

    fn allowed_uris(domain: &str) -> Vec<String> {
        vec![
            format!("https://{domain}"),
            format!("http://{domain}"),
            format!("http://{domain}:8070"),
        ]
    }

    /// 构造一条合法 SIWE 消息文本（EIP-4361 格式）。
    fn build_siwe(
        domain: &str,
        address: &str,
        nonce: &str,
        issued_at: &str,
        expiration: &str,
    ) -> String {
        // 末行不加尾部换行（与前端 lib/siwe.js 一致）
        format!(
            "{domain} wants you to sign in with your Ethereum account:\n{address}\n\n\
             Sign in to Sharpside\n\n\
             URI: https://{domain}\n\
             Version: 1\n\
             Chain ID: 137\n\
             Nonce: {nonce}\n\
             Issued At: {issued_at}\n\
             Expiration Time: {expiration}"
        )
    }

    #[test]
    fn verify_round_trip_accepts_valid_signature() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let now = OffsetDateTime::now_utc();
        let issued = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let exp = (now + time::Duration::minutes(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let nonce = "testnonce1234567";
        let msg_text = build_siwe("localhost", &address.to_string(), nonce, &issued, &exp);

        // alloy 用 EIP-191 personal_sign 哈希签名（sign_hash 对 eip191_hash 签）
        let digest = alloy_primitives::utils::eip191_hash_message(msg_text.as_bytes());
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));

        let verified = verify_and_validate(
            &msg_text,
            &sig_hex,
            "localhost",
            &allowed_uris("localhost"),
            &[137, 1],
            300,
        );
        assert!(
            verified.is_ok(),
            "valid signature should verify: {:?}",
            verified.err()
        );
        let m = verified.unwrap();
        assert_eq!(address_hex(&m), address.to_string().to_lowercase());
    }

    #[test]
    fn verify_rejects_tampered_message() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let now = OffsetDateTime::now_utc();
        let issued = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let exp = (now + time::Duration::minutes(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let msg_text = build_siwe("localhost", &address.to_string(), "n1", &issued, &exp);
        let digest = alloy_primitives::utils::eip191_hash_message(msg_text.as_bytes());
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));

        // 篡改 nonce
        let tampered = msg_text.replace("n1", "n2");
        assert!(verify_and_validate(
            &tampered,
            &sig_hex,
            "localhost",
            &allowed_uris("localhost"),
            &[137, 1],
            300
        )
        .is_err());
    }

    #[test]
    fn verify_rejects_domain_mismatch() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let now = OffsetDateTime::now_utc();
        let issued = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let exp = (now + time::Duration::minutes(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let msg_text = build_siwe("evil.com", &address.to_string(), "n1", &issued, &exp);
        let digest = alloy_primitives::utils::eip191_hash_message(msg_text.as_bytes());
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
        // 期望 domain=localhost，消息 domain=evil.com → 拒绝
        assert!(verify_and_validate(
            &msg_text,
            &sig_hex,
            "localhost",
            &allowed_uris("localhost"),
            &[137, 1],
            300
        )
        .is_err());
    }

    #[test]
    fn verify_rejects_uri_not_in_allowlist() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let now = OffsetDateTime::now_utc();
        let issued = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let exp = (now + time::Duration::minutes(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let msg_text = format!(
            "localhost wants you to sign in with your Ethereum account:\n{address}\n\n\
             Sign in to Sharpside\n\n\
             URI: https://evil.example/phish\n\
             Version: 1\n\
             Chain ID: 137\n\
             Nonce: nuri0001\n\
             Issued At: {issued}\n\
             Expiration Time: {exp}"
        );
        let digest = alloy_primitives::utils::eip191_hash_message(msg_text.as_bytes());
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
        let err = verify_and_validate(
            &msg_text,
            &sig_hex,
            "localhost",
            &allowed_uris("localhost"),
            &[137, 1],
            300,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("uri not allowed"),
            "got: {err}"
        );
    }

    #[test]
    fn verify_rejects_disallowed_chain() {
        let signer = PrivateKeySigner::random();
        let address = signer.address();
        let now = OffsetDateTime::now_utc();
        let issued = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let exp = (now + time::Duration::minutes(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        // Chain ID: 999（不在白名单）
        let msg_text = format!(
            "localhost wants you to sign in with your Ethereum account:\n{address}\n\n\
             x\n\nURI: https://localhost\nVersion: 1\nChain ID: 999\n\
             Nonce: n1\nIssued At: {issued}\nExpiration Time: {exp}"
        );
        let digest = alloy_primitives::utils::eip191_hash_message(msg_text.as_bytes());
        let sig = signer.sign_hash_sync(&digest).unwrap();
        let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
        assert!(verify_and_validate(
            &msg_text,
            &sig_hex,
            "localhost",
            &allowed_uris("localhost"),
            &[137, 1],
            300
        )
        .is_err());
    }
}
