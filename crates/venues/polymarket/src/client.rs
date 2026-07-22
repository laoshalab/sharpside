//! Polymarket 原始 HTTP 客户端。对应 `docs/DATA_SOURCES.md` §2-§4。
//!
//! 封装 Data / Gamma / CLOB 三套公开 API（读免鉴权）。
//! 限流由调用方（venue-hub worker）按 `trades_rpm`/`positions_rpm` 控制，本客户端不做。

use crate::dto::{BookDto, ClobMarketDto, LeaderboardEntry, MarketDto, PositionDto, TradeDto};
use reqwest::Client;
use serde::Deserialize;

/// 构造带可选代理的 reqwest 客户端。
///
/// reqwest `default-features=false` 不启用 `system-proxy`，不会自动读 `HTTP_PROXY`/`HTTPS_PROXY`。
/// 故显式读 `POLYMARKET_HTTP_PROXY`（回退 `POLYMARKET_RELAYER_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY`），
/// 设了就 `Proxy::all(url)`。中国等地区封锁 Polymarket 时须经代理（如 Clash `http://127.0.0.1:7890`）。
pub fn build_http_client(timeout: std::time::Duration) -> Client {
    let mut b = Client::builder().timeout(timeout);
    for key in [
        "POLYMARKET_HTTP_PROXY",
        "POLYMARKET_RELAYER_PROXY",
        "HTTPS_PROXY",
        "HTTP_PROXY",
    ] {
        if let Ok(p) = std::env::var(key) {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            match reqwest::Proxy::all(p) {
                Ok(proxy) => {
                    b = b.proxy(proxy);
                    break;
                }
                Err(e) => tracing::warn!(proxy = p, error = %e, "代理解析失败，直连"),
            }
        }
    }
    b.build().expect("reqwest client build")
}

/// L2 CLOB 凭证（HMAC 鉴权用）。由 L1 EIP-712 签名派生。
#[derive(Debug, Clone, serde::Serialize)]
pub struct L2Credentials {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

/// 默认 API base。对应 `docs/VENUE_DESIGN.md` §5 `[venues.polymarket]`。
pub const DATA_API_DEFAULT: &str = "https://data-api.polymarket.com";
pub const GAMMA_API_DEFAULT: &str = "https://gamma-api.polymarket.com";
pub const CLOB_API_DEFAULT: &str = "https://clob.polymarket.com";

/// Polymarket 三 API 客户端。
#[derive(Clone)]
pub struct PolymarketClient {
    data_api: String,
    gamma_api: String,
    clob_api: String,
    http: Client,
}

impl PolymarketClient {
    /// 用默认 base URL 构造。
    pub fn new() -> Self {
        Self::with_urls(DATA_API_DEFAULT, GAMMA_API_DEFAULT, CLOB_API_DEFAULT)
    }

    /// 用自定义 base URL 构造（测试 / 自托管代理用）。
    pub fn with_urls(data_api: &str, gamma_api: &str, clob_api: &str) -> Self {
        Self {
            data_api: data_api.trim_end_matches('/').into(),
            gamma_api: gamma_api.trim_end_matches('/').into(),
            clob_api: clob_api.trim_end_matches('/').into(),
            http: build_http_client(std::time::Duration::from_secs(15)),
        }
    }

    /// Gamma API base（只读探针/测试用）。
    pub fn gamma_api(&self) -> &str {
        &self.gamma_api
    }

    /// 通用 GET → JSON（只读探针/测试用，复用带代理的 `http` 客户端）。
    pub async fn http_get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        self.http
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())
    }

    /// `GET /data/order/{id}`（CLOB L2 HMAC）：查单笔订单状态（LIVE/MATCHED/CANCELLED 等）。
    pub async fn get_order_l2(
        &self,
        order_id: &str,
        l2_api_key: &str,
        l2_secret: &str,
        l2_passphrase: &str,
        poly_address: alloy_primitives::Address,
    ) -> Result<serde_json::Value, String> {
        let path = format!("/data/order/{order_id}");
        let url = format!("{}{path}", self.clob_api);
        let headers = crate::clob::l2_headers(
            poly_address,
            l2_secret,
            l2_api_key,
            l2_passphrase,
            "GET",
            &path,
            "",
        );
        let mut req = self.http.get(&url).header("Accept", "application/json");
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "get /data/order {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| format!("parse order: {e}"))
    }

    /// `DELETE /order`（CLOB L2 HMAC）：撤单。body `{"orderID": "<id>"}`，L2 HMAC 签该 body。
    pub async fn cancel_order_l2(
        &self,
        order_id: &str,
        l2_api_key: &str,
        l2_secret: &str,
        l2_passphrase: &str,
        poly_address: alloy_primitives::Address,
    ) -> Result<serde_json::Value, String> {
        let path = "/order";
        let url = format!("{}{path}", self.clob_api);
        let body_json = serde_json::json!({ "orderID": order_id });
        let body_str = serde_json::to_string(&body_json).unwrap_or_default();
        let headers = crate::clob::l2_headers(
            poly_address,
            l2_secret,
            l2_api_key,
            l2_passphrase,
            "DELETE",
            path,
            &body_str,
        );
        let mut req = self
            .http
            .request(reqwest::Method::DELETE, &url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body_str);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "delete /order {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| format!("parse cancel: {e}"))
    }

    /// `GET /v1/leaderboard`（Data API）。对应 `docs/DATA_SOURCES.md` §3.1。
    /// 真实端点为 `/v1/leaderboard`（`/leaderboard` 返回 404）；positions/trades 仍走根路径。
    pub async fn leaderboard(
        &self,
        category: &str,
        time_period: &str,
        order_by: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<LeaderboardEntry>, reqwest::Error> {
        let url = format!("{}/v1/leaderboard", self.data_api);
        get_json(
            &self.http,
            &url,
            &[
                ("category", category),
                ("timePeriod", time_period),
                ("orderBy", order_by),
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
            ],
        )
        .await
    }

    /// `GET /positions?user={addr}`（Data API）。
    pub async fn positions(&self, user: &str) -> Result<Vec<PositionDto>, reqwest::Error> {
        let url = format!("{}/positions", self.data_api);
        get_json(&self.http, &url, &[("user", user)]).await
    }

    /// `GET /value?user={addr}`（Data API）。当前持仓总 USD 估值（快照，非时间序列）。
    /// worker 周期快照积累历史后按周期算 delta，近似非榜地址的官方盈亏。
    pub async fn value(&self, user: &str) -> Result<Vec<crate::dto::ValueDto>, reqwest::Error> {
        let url = format!("{}/value", self.data_api);
        get_json(&self.http, &url, &[("user", user)]).await
    }

    /// `GET /trades`（Data API）。最新优先，分页。
    pub async fn trades(
        &self,
        user: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<TradeDto>, reqwest::Error> {
        let url = format!("{}/trades", self.data_api);
        get_json(
            &self.http,
            &url,
            &[
                ("user", user),
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
            ],
        )
        .await
    }

    /// `GET /markets`（Gamma API）。
    pub async fn markets(&self, limit: u32, offset: u32) -> Result<Vec<MarketDto>, reqwest::Error> {
        let url = format!("{}/markets", self.gamma_api);
        get_json(
            &self.http,
            &url,
            &[
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
                ("active", "true"),
            ],
        )
        .await
    }

    /// `GET /book?market={condition_id}&asset={token_id}`（CLOB API）。
    pub async fn book(&self, _market_id: &str, token_id: &str) -> Result<BookDto, reqwest::Error> {
        // CLOB `/book` 只收 `token_id`（`?market=&asset=` 会返回 "Invalid token id"）。
        // `market_id` 参数保留以兼容 venue trait 签名；响应里的 `market` 字段即 condition_id。
        let url = format!("{}/book", self.clob_api);
        get_json(&self.http, &url, &[("token_id", token_id)]).await
    }

    /// `GET /markets/{condition_id}` → CLOB 市场元数据（含 `neg_risk`、`active`、`accepting_orders`）。
    pub async fn clob_market(&self, condition_id: &str) -> Result<ClobMarketDto, reqwest::Error> {
        let url = format!("{}/markets/{}", self.clob_api, condition_id);
        self.http.get(&url).send().await?.json().await
    }

    /// 按 token_id 解析 neg-risk 标志（决定 V2 Order verifyingContract）。
    ///
    /// CLOB `/book` 不含 `negRisk`，故两步：先 `GET /book?token_id={token_id}` 取
    /// `market`(condition_id)，再 `GET /markets/{condition_id}` 读 `neg_risk`。
    /// 任一步失败/缺省回退 `false`（standard exchange）并 warn——dry-sign / 离线路径不阻塞。
    pub async fn resolve_neg_risk(&self, token_id: &str) -> bool {
        let book_url = format!("{}/book", self.clob_api);
        let condition_id = match get_json::<BookDto>(
            &self.http,
            &book_url,
            &[("token_id", token_id)],
        )
        .await
        {
            Ok(b) => match b.market {
                Some(c) => c,
                None => {
                    tracing::warn!(
                        token_id = token_id,
                        "/book 无 market(condition_id)，回退 false(standard)"
                    );
                    return false;
                }
            },
            Err(e) => {
                tracing::warn!(token_id = token_id, error = %e, "/book 拉取失败，neg_risk 回退 false(standard)");
                return false;
            }
        };
        match self.clob_market(&condition_id).await {
            Ok(m) => m.neg_risk,
            Err(e) => {
                tracing::warn!(condition_id = condition_id, error = %e, "/markets 拉取失败，neg_risk 回退 false(standard)");
                false
            }
        }
    }

    /// `POST /order`（CLOB API，需 EIP-712 签名）。
    ///
    /// 提交已签名订单到 Polymarket CLOB。返回 CLOB 订单 id。仅 `POLYMARKET_CLOB_POST=1` 时调用；
    /// 离线/无网络环境会在此处报 reqwest 错误（预期）。
    pub async fn post_order(
        &self,
        signed: &crate::clob::SignedOrder,
    ) -> Result<String, reqwest::Error> {
        let url = format!("{}/order", self.clob_api);
        let side_str = if signed.side == 0 { "BUY" } else { "SELL" };
        let body = serde_json::json!({
            "order": {
                "salt": signed.salt.to_string(),
                "maker": signed.maker_address.to_string(),
                "signer": signed.signer_address.to_string(),
                "taker": "0x0000000000000000000000000000000000000000",
                "tokenId": signed.token_id.to_string(),
                "makerAmount": signed.maker_amount.to_string(),
                "takerAmount": signed.taker_amount.to_string(),
                "expiration": "0",
                "nonce": "0",
                "feeRateBps": "0",
                "side": side_str,
                "signatureType": signed.signature_type,
            },
            "signature": signed.signature,
            "owner": signed.signer_address.to_string(),
            "orderType": "GTC",
        });
        let resp: serde_json::Value = self.http.post(url).json(&body).send().await?.json().await?;
        Ok(resp
            .get("orderID")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string())
    }

    /// `POST /order` 带 L2 HMAC 鉴权 + Builder 归因（FrenFlow 式 Deposit Wallet 委托路径）。
    ///
    /// 对应 `docs/CHANNEL_A_SIGNING.md` §3.2。在 `post_order` 基础上附加 5 个 `POLY_*` L2 header
    /// 与 `X-Builder-Code` 归因 header。
    ///
    /// **POLY_1271 真实联调修正**：L2 HMAC 的 POLY_ADDRESS = `l2_poly_address`（= owner EOA = L2 凭证所属
    /// = API key 属主），**非** `signed.signer_address`（= deposit wallet）。wire `order.signer` = deposit wallet
    /// （clob-auth 硬约束 maker==signer=DW）；服务端对 signature_type=3 靠 owner EOA→DW 映射校验 order.signer。
    /// 映射由「充 pUSD + approve + balance sync(sigType=3)」建立；未充值时映射缺失 → `/order` 报
    /// 「the order signer address has to be the address of the API KEY」（Stage 3 阻塞，需真实充值）。
    pub async fn post_order_l2(
        &self,
        signed: &crate::clob::SignedOrder,
        l2_api_key: &str,
        l2_secret: &str,
        l2_passphrase: &str,
        l2_poly_address: alloy_primitives::Address,
    ) -> Result<String, String> {
        let path = "/order";
        let url = format!("{}{}", self.clob_api, path);
        let side_str = if signed.side == 0 { "BUY" } else { "SELL" };
        // V2 wire body 对齐官方 clob-client-v2 `orderToJsonV2`：
        // - `signature` 在 `order` 内（非顶层）
        // - `salt` 为 JSON 整数（`parseInt`），≤2^53 安全
        // - `expiration: "0"`（GTC 无过期）；签名 EIP-712 struct 不含 expiration/taker，但 wire body 含 expiration
        // - 顶层 `owner` = L2 API key（非 signer 地址；官方测试断言 `owner=="api-key-uuid"`）
        // - 顶层 `deferExec`/`postOnly`；无 `taker`（OrderV2 无此字段，JSON 丢弃）
        let salt_int = u64::try_from(signed.salt).unwrap_or(0);
        let body_json = serde_json::json!({
            "order": {
                "salt": salt_int,
                "maker": signed.maker_address.to_string(),
                "signer": signed.signer_address.to_string(),
                "tokenId": signed.token_id.to_string(),
                "makerAmount": signed.maker_amount.to_string(),
                "takerAmount": signed.taker_amount.to_string(),
                "side": side_str,
                "signatureType": signed.signature_type,
                "timestamp": signed.timestamp_ms.to_string(),
                "expiration": "0",
                "metadata": format!("{:x}", signed.metadata),
                "builder": format!("{:x}", signed.builder),
                "signature": signed.signature,
            },
            "owner": l2_api_key,
            "orderType": "GTC",
            "deferExec": false,
            "postOnly": false,
        });
        let body_str = serde_json::to_string(&body_json).unwrap_or_default();
        let headers = crate::clob::l2_headers(
            l2_poly_address,
            l2_secret,
            l2_api_key,
            l2_passphrase,
            "POST",
            path,
            &body_str,
        );
        let mut req = self
            .http
            .post(url)
            .body(body_str)
            .header("Content-Type", "application/json");
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        if let Some(bc) = &signed.builder_code {
            req = req.header("X-Builder-Code", bc);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "post /order {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        let resp: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok(resp
            .get("orderID")
            .or_else(|| resp.get("order_id"))
            .or_else(|| resp.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string())
    }
    /// `POST /auth/api-key`（CLOB API，L1 EIP-712 签名 → 派生 L2 凭证）。
    ///
    /// 对应 `docs/CHANNEL_A_SIGNING.md` §3.1 step 6。owner EOA 签 [`crate::clob::build_l1_auth_signature`]
    /// 后 POST，CLOB 返回 `{ apiKey, secret, passphrase }`（L2 HMAC 凭证）。
    /// 离线/无网络环境会在此处报 reqwest 错误（预期）。
    /// L1：先 `GET /auth/derive-api-key`（派生已有），失败再 `POST /auth/api-key`（创建新）。
    ///
    /// 对齐 `~/文档/sharpside/crates/clob-client/src/lib.rs` `create_or_derive_api_key`：
    /// 鉴权用**头** `POLY_ADDRESS/POLY_SIGNATURE/POLY_TIMESTAMP/POLY_NONCE`（非 body），body `{}`，
    /// **无 signatureType**（服务端按地址自动识别 wallet 类型）。`nonce` 与 L1 ClobAuth 签名里的 nonce 一致（0）。
    ///
    /// **POLY_ADDRESS = owner EOA**（= L2 凭证所属地址）。POLY_1271 下 signature_type=3 让服务端
    /// 把 owner EOA 映射到 CREATE2 deposit wallet；order.signer 须=owner EOA（=本地址）才通过
    /// 「order signer address has to be the address of the API KEY」校验。
    pub async fn derive_api_key_l1(
        &self,
        owner: alloy_primitives::Address,
        signature: &str,
        timestamp: i64,
    ) -> Result<L2Credentials, String> {
        let addr = owner.to_string();
        let nonce = 0u64;
        // 1) GET /auth/derive-api-key
        let derive_url = format!("{}/auth/derive-api-key", self.clob_api);
        let resp = self
            .http
            .get(&derive_url)
            .header("POLY_ADDRESS", &addr)
            .header("POLY_SIGNATURE", signature)
            .header("POLY_TIMESTAMP", timestamp.to_string())
            .header("POLY_NONCE", nonce.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let (status, text) = (resp.status(), resp.text().await.unwrap_or_default());
        let creds_json: serde_json::Value = if status.is_success() {
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
        } else {
            // 2) POST /auth/api-key（创建）
            let create_url = format!("{}/auth/api-key", self.clob_api);
            let resp = self
                .http
                .post(&create_url)
                .header("POLY_ADDRESS", &addr)
                .header("POLY_SIGNATURE", signature)
                .header("POLY_TIMESTAMP", timestamp.to_string())
                .header("POLY_NONCE", nonce.to_string())
                .header("Content-Type", "application/json")
                .body("{}")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(format!(
                    "create api-key {status}: {}",
                    text.chars().take(400).collect::<String>()
                ));
            }
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
        };
        Ok(L2Credentials {
            api_key: creds_json
                .get("apiKey")
                .or_else(|| creds_json.get("api_key"))
                .or_else(|| creds_json.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            secret: creds_json
                .get("secret")
                .or_else(|| creds_json.get("api_secret"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            passphrase: creds_json
                .get("passphrase")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
        })
    }

    /// `GET /balance-allowance/update`（CLOB API，L2 HMAC + signatureType=3）。
    ///
    /// 通知 CLOB 同步 deposit wallet 的 pUSD 余额与 approve 状态（setup 末步）。
    /// 对应 `docs/CHANNEL_A_SIGNING.md` §3.1 step 8「CLOB update_balance_allowance(signature_type=3)」。
    /// 离线/无网络环境会在此处报 reqwest 错误（预期）。
    ///
    /// **POLY_ADDRESS = owner EOA**（= L2 凭证所属地址）；`signature_type=3` 让服务端映射 owner → deposit wallet。
    pub async fn update_balance_allowance(
        &self,
        signer_address: alloy_primitives::Address,
        l2_api_key: &str,
        l2_secret: &str,
        l2_passphrase: &str,
    ) -> Result<serde_json::Value, reqwest::Error> {
        // 官方 clob-client-v2：GET /balance-allowance/update?asset_type=COLLATERAL&signature_type=3，
        // L2 HMAC method=GET、path=/balance-allowance/update（GET 不签 query/body）。
        // POLY_ADDRESS = owner EOA（L2 凭证所属），signature_type=3 → 服务端映射到 deposit wallet。
        let path = "/balance-allowance/update";
        let url = format!(
            "{}{}?asset_type=COLLATERAL&signature_type={}",
            self.clob_api,
            path,
            crate::clob::sig_type::POLY_1271
        );
        let headers = crate::clob::l2_headers(
            signer_address,
            l2_secret,
            l2_api_key,
            l2_passphrase,
            "GET",
            path,
            "",
        );
        let mut req = self.http.get(&url).header("Accept", "application/json");
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        req.send().await?.error_for_status()?.json().await
    }

    /// `GET /balance-allowance`（CLOB API，L2 HMAC + signatureType=3）。
    ///
    /// 查询 deposit wallet 当前 pUSD 余额（不触发同步，纯读）。
    /// 返回 `{ "balance": "<usdc 字符串>", "allowance": "<...>" }`（COLLATERAL = pUSD）。
    /// 离线/无网络环境会在此处报 reqwest 错误（预期）。
    ///
    /// **POLY_ADDRESS = owner EOA**（= L2 凭证所属地址）；`signature_type=3` 让服务端映射 owner → deposit wallet。
    pub async fn get_balance_allowance(
        &self,
        signer_address: alloy_primitives::Address,
        l2_api_key: &str,
        l2_secret: &str,
        l2_passphrase: &str,
    ) -> Result<serde_json::Value, String> {
        // 官方 clob-client-v2：GET /balance-allowance?asset_type=COLLATERAL&signature_type=3，
        // L2 HMAC method=GET、path=/balance-allowance（GET 不签 query/body）。
        let path = "/balance-allowance";
        let url = format!(
            "{}{}?asset_type=COLLATERAL&signature_type={}",
            self.clob_api,
            path,
            crate::clob::sig_type::POLY_1271
        );
        let headers = crate::clob::l2_headers(
            signer_address,
            l2_secret,
            l2_api_key,
            l2_passphrase,
            "GET",
            path,
            "",
        );
        let mut req = self.http.get(&url).header("Accept", "application/json");
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "get /balance-allowance {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| {
            format!(
                "balance-allowance 反序列化失败: {e}; body={}",
                text.chars().take(200).collect::<String>()
            )
        })
    }
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn get_json<T: for<'de> Deserialize<'de>>(
    http: &Client,
    url: &str,
    params: &[(&str, &str)],
) -> Result<T, reqwest::Error> {
    http.get(url).query(params).send().await?.json().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_urls() {
        let c = PolymarketClient::new();
        assert_eq!(c.data_api, DATA_API_DEFAULT);
        assert_eq!(c.gamma_api, GAMMA_API_DEFAULT);
        assert_eq!(c.clob_api, CLOB_API_DEFAULT);
    }

    #[test]
    fn trims_trailing_slash() {
        let c = PolymarketClient::with_urls(
            "https://data-api.polymarket.com/",
            "https://gamma-api.polymarket.com/",
            "https://clob.polymarket.com/",
        );
        assert!(!c.data_api.ends_with('/'));
        assert!(!c.gamma_api.ends_with('/'));
        assert!(!c.clob_api.ends_with('/'));
    }

    /// 真实联调只读探针（`#[ignore]`，不进常规 CI）。
    ///
    /// 跑法（需代理 + full_network，中国等地区封锁 Polymarket 时）：
    /// ```bash
    /// POLYMARKET_HTTP_PROXY=http://127.0.0.1:7890 \
    ///   cargo test -p sharpside-venues-polymarket --offline client::tests::live_read_probe -- --ignored --nocapture
    /// ```
    ///
    /// 验证：Gamma `/markets` 拿到一个活跃 market → 取其 `clobTokenIds` 的一个 token →
    /// CLOB `/book?token_id=` 返回非空订单簿 → `resolve_neg_risk` 返回 bool。
    /// 全程只读、无鉴权、无资金、可逆。证明网络通路 + 读客户端 + neg_risk 解析端到端可用。
    #[tokio::test]
    #[ignore]
    async fn live_read_probe() {
        let c = PolymarketClient::new();
        // 1) Gamma /markets：取一个活跃且有 clobTokenIds（字符串化数组）的 market。
        let url = format!("{}/markets?limit=20&active=true&closed=false", c.gamma_api);
        let markets: serde_json::Value =
            c.http.get(&url).send().await.unwrap().json().await.unwrap();
        let arr = markets.as_array().expect("/markets 返回数组");
        let pick = arr
            .iter()
            .find(|m| {
                m.get("clobTokenIds")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty() && s != "[]")
                    .unwrap_or(false)
            })
            .expect("至少一个 market 带 clobTokenIds");
        let market_id = pick.get("id").and_then(|v| v.as_str()).unwrap().to_string();
        let condition_id = pick
            .get("conditionId")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let token_id = serde_json::from_str::<Vec<String>>(
            pick.get("clobTokenIds").and_then(|v| v.as_str()).unwrap(),
        )
        .unwrap()
        .remove(0);
        eprintln!(
            "market: id={} condition={} q={:?}",
            market_id,
            condition_id,
            pick.get("question").and_then(|v| v.as_str())
        );
        eprintln!("token_id={}", token_id);

        // 2) CLOB /book?token_id= → 非空订单簿（响应含 market=condition_id）。
        let book = c.book(&condition_id, &token_id).await.unwrap();
        eprintln!(
            "book bids={} asks={} market={:?}",
            book.bids.len(),
            book.asks.len(),
            book.market
        );
        assert!(
            !book.bids.is_empty() || !book.asks.is_empty(),
            "订单簿不应全空"
        );
        assert_eq!(
            book.market.as_deref(),
            Some(condition_id.as_str()),
            "book.market 须=condition_id"
        );

        // 3) resolve_neg_risk：/book?token_id= → market → /markets/{condition_id} → neg_risk。
        let nr = c.resolve_neg_risk(&token_id).await;
        eprintln!("resolve_neg_risk={}", nr);
        // 4) 直连 /markets/{condition_id} 交叉验证。
        let cm = c.clob_market(&condition_id).await.unwrap();
        eprintln!(
            "clob_market neg_risk={} active={} accepting={}",
            cm.neg_risk, cm.active, cm.accepting_orders
        );
        assert_eq!(
            nr, cm.neg_risk,
            "resolve_neg_risk 须与 /markets/{condition_id} 的 neg_risk 一致"
        );
    }

    /// 真实联调 neg-risk=true 分支探针（`#[ignore]`）。
    ///
    /// 从 Gamma 找一个 `negRisk=true` 的活跃市场 → 取其首个 token →
    /// `resolve_neg_risk` 须返回 `true`，且与 `/markets/{condition_id}` 的 `neg_risk` 一致。
    /// 跑法同 `live_read_probe`（需 `POLYMARKET_HTTP_PROXY` + full_network）。
    #[tokio::test]
    #[ignore]
    async fn live_read_probe_neg_risk() {
        let c = PolymarketClient::new();
        let url = format!(
            "{}/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false",
            c.gamma_api
        );
        let markets: serde_json::Value =
            c.http.get(&url).send().await.unwrap().json().await.unwrap();
        let pick = markets
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                m.get("negRisk").and_then(|v| v.as_bool()).unwrap_or(false)
                    && m.get("clobTokenIds")
                        .and_then(|v| v.as_str())
                        .map(|s| !s.is_empty() && s != "[]")
                        .unwrap_or(false)
            })
            .expect("至少一个活跃 neg-risk market");
        let condition_id = pick
            .get("conditionId")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let token_id = serde_json::from_str::<Vec<String>>(
            pick.get("clobTokenIds").and_then(|v| v.as_str()).unwrap(),
        )
        .unwrap()
        .remove(0);
        eprintln!(
            "neg-risk market: id={:?} condition={}",
            pick.get("id").and_then(|v| v.as_str()),
            condition_id
        );

        let nr = c.resolve_neg_risk(&token_id).await;
        eprintln!("resolve_neg_risk={}", nr);
        assert!(nr, "neg-risk market 的 resolve_neg_risk 须为 true");
        let cm = c.clob_market(&condition_id).await.unwrap();
        assert!(
            cm.neg_risk,
            "/markets/{{condition_id}} 的 neg_risk 须为 true"
        );
        assert_eq!(nr, cm.neg_risk);
    }
}
