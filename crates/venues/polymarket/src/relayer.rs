//! Polymarket Relayer 客户端：gasless 部署 deposit wallet（`WALLET-CREATE`）。
//!
//! 对齐 `~/文档/sharpside/bins/api/src/poly_relayer.rs`（生产参考）与
//! `docs/CHANNEL_A_SIGNING.md` §3.1 一次性 setup。
//!
//! Relayer 是 Polymarket 提供的免 gas 中继服务（builder 归因）：
//! - **`WALLET-CREATE`**：`POST /submit` body `{"type":"WALLET-CREATE","from":owner,"to":FACTORY}`，
//!   **无需用户签名**（builder HMAC 鉴权即可），Relayer 部署 deposit wallet（CREATE2 确定性地址）。
//!   返回 `transactionID`，轮询 `GET /transaction?id=` 至 `STATE_CONFIRMED`/`MINED`。
//! - **`WALLET` batch**：`POST /submit` body `{"type":"WALLET","from":owner,"to":FACTORY,"nonce":...,
//!   "signature":"0x65ByteSig","depositWalletParams":{depositWallet,deadline,calls[]}}`，
//!   owner EIP-712 签 `DepositWallet` `Batch`（普通 65 字节，非 ERC-7739），用于 approve/transfer/split/merge。
//!   approve calldata + Batch 签名见 [`crate::wallet_batch`]。
//! - **`GET /deployed?address=`**：查地址是否已部署（无鉴权）。
//! - **`GET /nonce?address=&type=WALLET`**：取 owner 当前 WALLET batch nonce（无鉴权）。
//!
//! **认证**：Builder HMAC 头（`POLY_BUILDER_API_KEY/PASSPHRASE/TIMESTAMP/SIGNATURE`），
//! 算法同 CLOB L2 HMAC（复用 [`sharpside_clob_auth::builder_headers`]）。
//! 凭证是平台 builder 账户的 API key/secret/passphrase（env `POLYMARKET_BUILDER_*`），
//! 非 CLOB L2 的 per-user `deriveApiKey` 凭证。

#![forbid(unsafe_code)]

use alloy_primitives::Address;
use reqwest::Client;
use serde::Deserialize;
use sharpside_clob_auth as clob_auth;

/// 默认 Relayer base。可由 env `POLYMARKET_RELAYER_URL` 覆盖。
pub const RELAYER_URL_DEFAULT: &str = "https://relayer-v2.polymarket.com";

/// Path F Deposit Wallet factory（Polygon mainnet）。`WALLET-CREATE` 的 `to` 字段。
pub const DEPOSIT_WALLET_FACTORY: &str = "0x00000000000Fb5C9ADea0298D729A0CB3823Cc07";

/// Relayer 客户端。
#[derive(Clone)]
pub struct RelayerClient {
    base: String,
    /// 平台 builder 凭证（env `POLYMARKET_BUILDER_*`）。缺则 `submit` 会 401。
    builder_creds: Option<clob_auth::ApiCreds>,
    http: Client,
}

/// `POST /submit` 响应。
#[derive(Debug, Deserialize)]
pub struct SubmitResp {
    #[serde(rename = "transactionID", alias = "transactionId")]
    pub transaction_id: Option<String>,
    #[serde(rename = "transactionHash", alias = "txHash")]
    pub transaction_hash: Option<String>,
}

/// `GET /transaction?id=` 单行（轮询确认用）。
#[derive(Debug, Deserialize)]
pub struct TxRow {
    pub state: Option<String>,
    #[serde(rename = "transactionHash", alias = "txHash")]
    pub transaction_hash: Option<String>,
    /// Relayer 历史用 `proxyAddress`；WALLET-CREATE 可能省略或用别名。
    #[serde(
        rename = "proxyAddress",
        alias = "proxy_address",
        alias = "proxyWallet",
        alias = "depositWallet",
        alias = "wallet"
    )]
    pub proxy_address: Option<String>,
}

impl RelayerClient {
    pub fn new() -> Self {
        let base = std::env::var("POLYMARKET_RELAYER_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| RELAYER_URL_DEFAULT.to_string());
        Self::with_base(base)
    }

    pub fn with_base(base: String) -> Self {
        let base = base.trim_end_matches('/').to_string();
        // 平台 builder 凭证（HMAC 鉴权用）。优先 POLYMARKET_BUILDER_*；api_key 回退 POLYMARKET_RELAYER_API_KEY。
        let api_key = std::env::var("POLYMARKET_BUILDER_API_KEY")
            .ok()
            .or_else(|| std::env::var("POLYMARKET_RELAYER_API_KEY").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let secret = std::env::var("POLYMARKET_BUILDER_SECRET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let passphrase = std::env::var("POLYMARKET_BUILDER_PASSPHRASE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let builder_creds = match (api_key, secret, passphrase) {
            (Some(k), Some(s), Some(p)) => Some(clob_auth::ApiCreds {
                api_key: k,
                api_secret_b64: s,
                passphrase: p,
            }),
            _ => None,
        };
        Self {
            base,
            builder_creds,
            http: crate::client::build_http_client(std::time::Duration::from_secs(30)),
        }
    }

    /// `WALLET-CREATE`：部署 deposit wallet（gasless，无需用户签名）。
    ///
    /// `POST /submit` body `{"type":"WALLET-CREATE","from":owner,"to":DEPOSIT_WALLET_FACTORY}`，
    /// builder HMAC 鉴权。返回 `transactionID`（用 [`poll_confirmed`] 轮询至确认）。
    pub async fn wallet_create(&self, owner: Address) -> Result<SubmitResp, String> {
        const PATH: &str = "/submit";
        let url = format!("{}{}", self.base, PATH);
        let body = serde_json::json!({
            "type": "WALLET-CREATE",
            "from": owner.to_string(),
            "to": DEPOSIT_WALLET_FACTORY,
        });
        self.authed_post(PATH, &url, &body).await
    }

    /// `GET /nonce?address=<owner>&type=WALLET`：取 owner 当前 `WALLET` batch nonce（无鉴权）。
    pub async fn wallet_nonce(&self, owner: Address) -> Result<String, String> {
        let url = format!("{}/nonce?address={}&type=WALLET", self.base, owner);
        let v: serde_json::Value = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| format!("relayer /nonce: {e}"))?
            .json()
            .await
            .map_err(|e| format!("relayer /nonce parse: {e}"))?;
        // 兼容 {"nonce":"0"} / {"nonce":0} / 直接字符串
        v.get("nonce")
            .and_then(|n| {
                n.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| n.as_u64().map(|u| u.to_string()))
            })
            .or_else(|| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| format!("relayer /nonce 缺 nonce 字段: {v}"))
    }

    /// `WALLET` batch：提交 owner 签好的 `Batch`（approve / transfer / split / merge 等）。
    ///
    /// `POST /submit` body 对齐 Deposit Wallet Guide：
    /// ```json
    /// { "type":"WALLET", "from":owner, "to":DEPOSIT_WALLET_FACTORY,
    ///   "nonce":"<nonce>", "signature":"0x65ByteSig",
    ///   "depositWalletParams":{ "depositWallet":"<dw>", "deadline":"<deadline>",
    ///     "calls":[{"target":"<addr>","value":"0","data":"0xCalldata"}] } }
    /// ```
    pub async fn wallet_batch(
        &self,
        owner: Address,
        deposit_wallet: Address,
        nonce: &str,
        deadline: &str,
        signature: &str,
        calls: &[crate::wallet_batch::WalletCall],
    ) -> Result<SubmitResp, String> {
        const PATH: &str = "/submit";
        let url = format!("{}{}", self.base, PATH);
        let calls_json: Vec<serde_json::Value> = calls
            .iter()
            .map(|c| {
                serde_json::json!({
                    "target": c.target.to_string(),
                    "value": c.value.to_string(),
                    "data": format!("0x{}", hex::encode(&c.data)),
                })
            })
            .collect();
        let body = serde_json::json!({
            "type": "WALLET",
            "from": owner.to_string(),
            "to": DEPOSIT_WALLET_FACTORY,
            "nonce": nonce,
            "signature": signature,
            "depositWalletParams": {
                "depositWallet": deposit_wallet.to_string(),
                "deadline": deadline,
                "calls": calls_json,
            }
        });
        self.authed_post(PATH, &url, &body).await
    }

    /// `GET /deployed?address=`：查地址是否已部署（无鉴权）。
    pub async fn is_deployed(&self, address: &str) -> Result<bool, String> {
        let url = format!("{}/deployed?address={}", self.base, address);
        let v: serde_json::Value = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(v.get("deployed").and_then(|x| x.as_bool()).unwrap_or(false))
    }

    /// `GET /transaction?id=`：拉一笔 relayer 交易的状态行（无鉴权）。
    pub async fn transaction(&self, tx_id: &str) -> Result<Vec<TxRow>, String> {
        let url = format!("{}/transaction?id={}", self.base, tx_id);
        let v: serde_json::Value = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(if v.is_array() {
            serde_json::from_value(v).unwrap_or_default()
        } else if let Some(arr) = v.get("transactions").or_else(|| v.get("data")) {
            serde_json::from_value(arr.clone()).unwrap_or_default()
        } else {
            serde_json::from_value(serde_json::json!([v])).unwrap_or_default()
        })
    }

    /// 轮询 `GET /transaction?id=` 至 `STATE_CONFIRMED`/`MINED`（或失败 bail）。
    /// 最多 ~90s（45 × 2s），对齐生产 `poll_confirmed`。
    pub async fn poll_confirmed(&self, tx_id: &str) -> Result<TxRow, String> {
        for _ in 0..45 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Ok(rows) = self.transaction(tx_id).await {
                if let Some(row) = rows.into_iter().next() {
                    let st = row.state.as_deref().unwrap_or("");
                    if st.contains("CONFIRM") || st == "STATE_CONFIRMED" || st.contains("MINED") {
                        return Ok(row);
                    }
                    if st.contains("FAIL") || st.contains("INVALID") || st.contains("CANCEL") {
                        return Err(format!("relayer tx failed: {st}"));
                    }
                }
            }
        }
        Err(format!("relayer tx poll timeout for {tx_id}"))
    }

    async fn authed_post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<T, String> {
        let body_text = serde_json::to_string(body).unwrap_or_default();
        let mut req = self
            .http
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body_text.clone());
        if let Some(creds) = &self.builder_creds {
            let ts = chrono::Utc::now().timestamp().max(0) as u64;
            match clob_auth::builder_headers(creds, "POST", path, &body_text, ts) {
                Ok(headers) => {
                    for (k, v) in headers.iter() {
                        req = req.header(*k, v);
                    }
                }
                Err(e) => tracing::warn!(error = %e, "builder HMAC 头构造失败"),
            }
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "relayer {path} {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| format!("submit parse: {e}"))
    }
}

impl Default for RelayerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 这两个测试都改/删 `POLYMARKET_BUILDER_*`，并行跑会竞态。用 mutex 串行化 env 敏感段。
    static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn clear_builder_env() {
        std::env::remove_var("POLYMARKET_BUILDER_API_KEY");
        std::env::remove_var("POLYMARKET_BUILDER_SECRET");
        std::env::remove_var("POLYMARKET_BUILDER_PASSPHRASE");
        std::env::remove_var("POLYMARKET_RELAYER_API_KEY");
    }

    #[test]
    fn default_base_trimmed() {
        let _g = ENV_GUARD.lock().unwrap();
        clear_builder_env();
        let c = RelayerClient::with_base("https://relayer-v2.polymarket.com/".into());
        assert!(!c.base.ends_with('/'));
        assert!(c.builder_creds.is_none());
    }

    #[test]
    fn builder_creds_from_env() {
        let _g = ENV_GUARD.lock().unwrap();
        clear_builder_env();
        std::env::set_var("POLYMARKET_BUILDER_API_KEY", "test-key");
        std::env::set_var("POLYMARKET_BUILDER_SECRET", "dGVzdC1zZWNyZXQ=");
        std::env::set_var("POLYMARKET_BUILDER_PASSPHRASE", "test-pass");
        let c = RelayerClient::new();
        let creds = c.builder_creds.expect("builder_creds");
        assert_eq!(creds.api_key, "test-key");
        assert_eq!(creds.passphrase, "test-pass");
        clear_builder_env();
    }

    #[test]
    fn builder_creds_partial_env_is_none() {
        let _g = ENV_GUARD.lock().unwrap();
        clear_builder_env();
        std::env::set_var("POLYMARKET_BUILDER_API_KEY", "test-key");
        // 缺 secret/passphrase → None（避免半凭证发请求）
        let c = RelayerClient::new();
        assert!(c.builder_creds.is_none());
        clear_builder_env();
    }

    #[test]
    fn deposit_wallet_factory_const_matches_production() {
        // 对齐 ~/文档/sharpside/bins/api/src/poly_relayer.rs DEPOSIT_WALLET_FACTORY。
        assert_eq!(
            DEPOSIT_WALLET_FACTORY,
            "0x00000000000Fb5C9ADea0298D729A0CB3823Cc07"
        );
    }
}
