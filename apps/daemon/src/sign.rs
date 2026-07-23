//! 通道 B 本地 EIP-712 签名（复用 `sharpside-venues-polymarket::clob`）。
//!
//! 平台零钥：私钥仅存在于 daemon 进程（env `POLYMARKET_PRIVATE_KEY` /
//! `POLYMARKET_DEV_PRIVATE_KEY`），copier 永不接触。
//!
//! - 默认 dry-sign：真签名但不提交 CLOB，回报 `tx_hash`=签名 hex。
//! - `POLYMARKET_CLOB_POST=1`：签名后 POST CLOB `/order`（需网络可达）。

use sharpside_shared::Side;
use sharpside_venues_polymarket::OrderType;
use sharpside_venues_polymarket::clob::{self, SignedOrder};
use sharpside_venues_polymarket::PolymarketClient;

/// 本地下单结果（回传 copier）。
#[derive(Debug, Clone)]
pub struct LocalFill {
    pub venue_order_id: String,
    pub filled_size: f64,
    pub filled_price: f64,
    pub fee: f64,
    pub tx_hash: String,
    /// true = 仅签名未提交 CLOB
    pub dry_sign: bool,
}

/// 从环境解析私钥（优先 `POLYMARKET_PRIVATE_KEY`，回退 `POLYMARKET_DEV_PRIVATE_KEY`）。
pub fn load_private_key_from_env() -> Option<String> {
    for key in ["POLYMARKET_PRIVATE_KEY", "POLYMARKET_DEV_PRIVATE_KEY"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// 是否真提交 CLOB（`POLYMARKET_CLOB_POST=1`）。
pub fn clob_post_enabled() -> bool {
    matches!(
        std::env::var("POLYMARKET_CLOB_POST").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Polymarket 本地签名（可选提交）。
///
/// `token_id` 为 CLOB ERC1155 token id（十进制字符串）；`side` 为 `"buy"`/`"sell"`。
pub async fn execute_polymarket(
    token_id: &str,
    side: &str,
    price: f64,
    size: f64,
) -> Result<LocalFill, String> {
    let key = load_private_key_from_env().ok_or_else(|| {
        "未配置 POLYMARKET_PRIVATE_KEY（或 POLYMARKET_DEV_PRIVATE_KEY）".to_string()
    })?;
    let signer = clob::signer_from_hex(&key)?;
    let side = match side.to_ascii_lowercase().as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        other => return Err(format!("未知 side: {other}")),
    };
    // neg_risk 按 market metadata（CLOB /book 的 negRisk）选 V2 verifyingContract。
    // 真实提交（clob_post）才发 /book 解析；dry-sign 离线默认 false（standard）。
    let neg_risk = if clob_post_enabled() {
        PolymarketClient::new().resolve_neg_risk(token_id).await
    } else {
        false
    };
    let signed: SignedOrder =
        clob::sign_clob_order(&signer, side, token_id, price, size, neg_risk, None, None).await?;

    if clob_post_enabled() {
        let client = PolymarketClient::new();
        let order_id = client
            .post_order(&signed, OrderType::Gtc, None, false)
            .await
            .map_err(|e| format!("CLOB POST 失败: {e}"))?;
        Ok(LocalFill {
            venue_order_id: order_id,
            filled_size: size,
            filled_price: price,
            fee: 0.0,
            tx_hash: signed.signature,
            dry_sign: false,
        })
    } else {
        Ok(LocalFill {
            venue_order_id: format!(
                "dry-sign-{}",
                &signed.signature[..8.min(signed.signature.len())]
            ),
            filled_size: size,
            filled_price: price,
            fee: 0.0,
            tx_hash: signed.signature,
            dry_sign: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    // 这两个测试都读写全局 env（POLYMARKET_PRIVATE_KEY / POLYMARKET_DEV_PRIVATE_KEY），
    // 并行跑会互相污染（一个设、一个删）。用 tokio Mutex 串行化本模块的 env 依赖测试
    // （guard 可跨 await 持有，std Mutex 不行）。
    static ENV_GUARD: Mutex<()> = Mutex::const_new(());

    // 公开测试私钥（Anvil #0），勿用于生产。
    const TEST_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    #[tokio::test]
    async fn dry_sign_produces_recoverable_signature() {
        let _guard = ENV_GUARD.lock().await;
        std::env::set_var("POLYMARKET_PRIVATE_KEY", TEST_KEY);
        std::env::remove_var("POLYMARKET_CLOB_POST");
        let fill = execute_polymarket("12345", "buy", 0.5, 10.0)
            .await
            .expect("sign");
        assert!(fill.dry_sign);
        assert!(fill.tx_hash.starts_with("0x"));
        // EOA 路径（sign_clob_order, signatureType=0）V2 EIP-712 签名 = 65 字节 = 130 hex + 0x。
        assert_eq!(fill.tx_hash.len(), 132);
        assert!(fill.venue_order_id.starts_with("dry-sign-"));
        std::env::remove_var("POLYMARKET_PRIVATE_KEY");
    }

    #[tokio::test]
    async fn missing_key_errors() {
        let _guard = ENV_GUARD.lock().await;
        std::env::remove_var("POLYMARKET_PRIVATE_KEY");
        std::env::remove_var("POLYMARKET_DEV_PRIVATE_KEY");
        let err = execute_polymarket("1", "buy", 0.5, 1.0).await.unwrap_err();
        assert!(err.contains("未配置"));
    }
}
