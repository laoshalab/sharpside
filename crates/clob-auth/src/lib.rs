//! Polymarket CLOB V2 鉴权（task 5.1）：L1（EIP-712 钱包签名，一次性派生 API 凭证）
//! + L2（HMAC-SHA256，每个鉴权请求）+ V2 Order EIP-712 签名。
//!
//! 算法对照 py-clob-client-v2 / Polymarket 官方文档（2026-04-28 CLOB V2 cutover）：
//! - **L1 ClobAuthDomain**：`name="ClobAuthDomain"`,`version="1"`,`chainId=137`（无 verifyingContract）。
//!   struct `ClobAuth{address,timestamp(string),nonce(uint256),message(string)}`。
//! - **L2 HMAC**：`base64url(HMAC_SHA256(base64url_decode(secret), "{ts}{METHOD}{path}{body}"))`，
//!   五个 `POLY_*` 头：`POLY_ADDRESS/POLY_API_KEY/POLY_PASSPHRASE/POLY_TIMESTAMP/POLY_SIGNATURE`。
//!   `POLY_TIMESTAMP` 单位 = 秒（与 Order.timestamp 的毫秒区分）。
//!   HMAC 的 `path` **不含 query string**（`/data/orders?id=` 只签 `/data/orders`）。
//! - **V2 Order EIP-712**：Exchange domain `name="Polymarket CTF Exchange"`,`version="2"`,`chainId=137`,
//!   `verifyingContract` 按 market `neg_risk` 取 standard / neg-risk 地址。
//!   struct 去掉 `taker/expiration/nonce/feeRateBps`，加 `timestamp(ms)/metadata(bytes32)/builder(bytes32)`。
//!
//! 单测对照策略：
//! - L2 HMAC：用 openssl/Python 独立计算的固定向量断言（见 `l2_hmac_matches_independent_vector`）。
//! - L1 / V2 Order：用固定私钥签名 → `recover_address == 钱包地址`（eth-account 同款正确性判据；
//!   EIP-712 哈希错则恢复失败）。`metadata/builder` 零值默认。

use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolStruct};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

/// Assist / executor / fill-worker 共用：由 intent 派生稳定 salt（32 bytes）。
///
/// CLOB OpenAPI 要求 wire `salt` 为 JSON **integer**；超过 JS 安全整数
/// （2^53−1）或改成字符串都会 `Invalid order payload`。因此只保留 keccak 低 53 位。
pub fn salt_from_intent(intent_id: i64, user_id: Uuid, source: &str) -> [u8; 32] {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&intent_id.to_be_bytes());
    buf.extend_from_slice(user_id.as_bytes());
    buf.extend_from_slice(source.as_bytes());
    let h = keccak256(&buf).0;
    const MAX_SAFE: u64 = (1u64 << 53) - 1;
    let mut n = u64::from_be_bytes(h[24..32].try_into().expect("8 bytes")) & MAX_SAFE;
    if n == 0 {
        n = 1;
    }
    U256::from(n).to_be_bytes()
}

/// 比较 CLOB 回传的 salt 字符串（十进制或 0x hex）与期望字节。
pub fn salt_str_matches(salt_str: &str, expected: &[u8; 32]) -> bool {
    let want = U256::from_be_bytes(*expected);
    let t = salt_str.trim();
    if t.is_empty() {
        return false;
    }
    if let Ok(got) = U256::from_str(t) {
        return got == want;
    }
    if let Ok(got) = U256::from_str_radix(t.trim_start_matches("0x"), 16) {
        return got == want;
    }
    false
}

/// 期望 salt 的十进制字符串（对账单测 / 日志）。
pub fn salt_decimal_string(expected: &[u8; 32]) -> String {
    U256::from_be_bytes(*expected).to_string()
}

/// Polygon mainnet chain id。
pub const CHAIN_ID: u64 = 137;

/// 标准（非 neg-risk）CTF Exchange V2 地址（Polygon）。
pub const EXCHANGE_STANDARD: &str = "0xE111180000d2663C0091e4f400237545B87B996B";
/// neg-risk CTF Exchange V2 地址（Polygon）。
pub const EXCHANGE_NEG_RISK: &str = "0xe2222d279d744050d28e00520010520000310F59";
const EXCHANGE_DOMAIN_NAME: &str = "Polymarket CTF Exchange";
const EXCHANGE_DOMAIN_VERSION: &str = "2";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid key: {0}")]
    BadKey(String),
    #[error("invalid address: {0}")]
    BadAddress(String),
    #[error("signing failed: {0}")]
    Sign(String),
    #[error("base64 decode failed: {0}")]
    B64(#[from] base64::DecodeError),
}

type HmacSha256 = Hmac<Sha256>;

sol! {
    struct ClobAuth {
        address address;
        string timestamp;
        uint256 nonce;
        string message;
    }
    /// Polymarket CTF Exchange V2 EIP-712 primary type — **must** be named `Order`
    /// (`Order(uint256 salt,...)` typehash). Renaming breaks CLOB verify →
    /// `invalid order version, please use the latest clob-client`.
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint8 side;
        uint8 signatureType;
        uint256 timestamp;
        bytes32 metadata;
        bytes32 builder;
    }
    /// ERC-7739 nested TypedDataSign (Deposit Wallet / POLY_1271).
    /// Outer EIP-712 domain stays the CTF Exchange; wallet identity lives in the message.
    struct TypedDataSign {
        Order contents;
        string name;
        string version;
        uint256 chainId;
        address verifyingContract;
        bytes32 salt;
    }
}

/// Canonical Order type string used in ERC-7739 contentsType suffix (186 bytes).
pub const ORDER_TYPE_STRING: &str = "Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)";
const DEPOSIT_WALLET_NAME: &str = "DepositWallet";
const DEPOSIT_WALLET_VERSION: &str = "1";

/// L1 派生凭证用输入。`message` 固定为 Polymarket 的 attestation 字符串。
#[derive(Debug, Clone)]
pub struct L1Input<'a> {
    pub address: &'a str,
    pub timestamp: u64,
    pub nonce: u64,
    pub message: &'a str,
}

/// L2 HMAC 凭证（来自 `POST /auth/api-key` 的响应）。
#[derive(Debug, Clone)]
pub struct ApiCreds {
    pub api_key: String,
    /// base64url 编码的 HMAC secret（L1 派生返回）。
    pub api_secret_b64: String,
    pub passphrase: String,
}

/// 用 EIP-712 私钥签名 L1 ClobAuth，返回 `0x{65 bytes r||s||v}` hex（填 `POLY_SIGNATURE`）。
pub fn sign_l1(pk: &str, input: &L1Input<'_>) -> Result<String, AuthError> {
    let signer = PrivateKeySigner::from_str(pk).map_err(|e| AuthError::BadKey(e.to_string()))?;
    let address: Address = input
        .address
        .parse::<Address>()
        .map_err(|e| AuthError::BadAddress(e.to_string()))?;
    let msg = ClobAuth {
        address,
        timestamp: input.timestamp.to_string(),
        nonce: U256::from(input.nonce),
        message: input.message.to_string(),
    };
    // ClobAuthDomain：name=ClobAuthDomain / version=1 / chainId=137（无 verifyingContract/salt）。
    let domain = clob_auth_domain();
    let digest = typed_data_digest(&domain, &msg);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// 同 [`sign_l1`]，但直接接收已构造的 `PrivateKeySigner`（避免调用方反复从 pk hex 重建）。
/// 复用同一 digest 逻辑（`clob_auth_domain` + `ClobAuth`），签名结果与 [`sign_l1`] 一致。
pub fn sign_l1_with_signer(
    signer: &PrivateKeySigner,
    input: &L1Input<'_>,
) -> Result<String, AuthError> {
    let address: Address = input
        .address
        .parse::<Address>()
        .map_err(|e| AuthError::BadAddress(e.to_string()))?;
    let msg = ClobAuth {
        address,
        timestamp: input.timestamp.to_string(),
        nonce: U256::from(input.nonce),
        message: input.message.to_string(),
    };
    let domain = clob_auth_domain();
    let digest = typed_data_digest(&domain, &msg);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// 构造 L2 HMAC 五个 `POLY_*` 头。`body` 为请求体字符串（GET 传 `""`）。
/// 返回 `(POLY_ADDRESS, POLY_API_KEY, POLY_PASSPHRASE, POLY_TIMESTAMP, POLY_SIGNATURE)`。
pub fn l2_headers(
    address: &str,
    creds: &ApiCreds,
    method: &str,
    path: &str,
    body: &str,
    timestamp: u64,
) -> Result<[(&'static str, String); 5], AuthError> {
    let sig = l2_signature(&creds.api_secret_b64, timestamp, method, path, body)?;
    Ok([
        ("POLY_ADDRESS", address.to_string()),
        ("POLY_API_KEY", creds.api_key.clone()),
        ("POLY_PASSPHRASE", creds.passphrase.clone()),
        ("POLY_TIMESTAMP", timestamp.to_string()),
        ("POLY_SIGNATURE", sig),
    ])
}

/// Builder Relayer HMAC 头（`POST /submit` 等）。对齐 `~/文档/sharpside/bins/api/src/poly_relayer.rs`。
///
/// 与 [`l2_headers`] 同 HMAC 算法（复用 [`l2_signature`]），但：
/// - 头名前缀 `POLY_BUILDER_*`（`POLY_BUILDER_API_KEY/PASSPHRASE/TIMESTAMP/SIGNATURE`）
/// - 无 `POLY_ADDRESS`（relayer 按 builder 平台凭证鉴权，非按用户地址）
/// - 凭证是平台 builder 账户的 API key/secret/passphrase（env `POLYMARKET_BUILDER_*`），
///   非 CLOB L2 的 per-user `deriveApiKey` 凭证。
pub fn builder_headers(
    creds: &ApiCreds,
    method: &str,
    path: &str,
    body: &str,
    timestamp: u64,
) -> Result<[(&'static str, String); 4], AuthError> {
    let sig = l2_signature(&creds.api_secret_b64, timestamp, method, path, body)?;
    Ok([
        ("POLY_BUILDER_API_KEY", creds.api_key.clone()),
        ("POLY_BUILDER_PASSPHRASE", creds.passphrase.clone()),
        ("POLY_BUILDER_TIMESTAMP", timestamp.to_string()),
        ("POLY_BUILDER_SIGNATURE", sig),
    ])
}

/// 仅算 HMAC 签名（base64url，含 padding，对齐 Python `base64.urlsafe_b64encode`）。
///
/// CLOB L2 对 GET 只签 **path**（不含 `?query`）。调用方可以传入完整 URL path+query；
/// 本函数会在签名前剥离 query，避免 `/data/orders?id=…` 一类请求 401 Invalid api key。
pub fn l2_signature(
    secret_b64: &str,
    timestamp: u64,
    method: &str,
    path: &str,
    body: &str,
) -> Result<String, AuthError> {
    // Polymarket secret 可能是 base64url（- _）或 base64 standard（+ /）；两种都接受。
    let key = base64::engine::general_purpose::URL_SAFE
        .decode(secret_b64.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(secret_b64.as_bytes()))?;
    let path_for_sig = path.split_once('?').map(|(p, _)| p).unwrap_or(path);
    let msg = format!(
        "{}{}{}{}",
        timestamp,
        method.to_uppercase(),
        path_for_sig,
        body
    );
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|e| AuthError::Sign(e.to_string()))?;
    mac.update(msg.as_bytes());
    let bytes = mac.finalize().into_bytes();
    Ok(base64::engine::general_purpose::URL_SAFE.encode(bytes))
}

/// V2 Order 签名输入。`maker/signer` = 钱包地址（EOA 直签二者相同）。
/// `maker_amount/taker_amount` 为 6 位小数整数；`timestamp` 为毫秒；`metadata/builder` 默认零。
#[derive(Debug, Clone)]
pub struct V2OrderInput {
    pub salt: U256,
    pub maker: Address,
    pub signer: Address,
    pub token_id: U256,
    pub maker_amount: U256,
    pub taker_amount: U256,
    /// 0=BUY 1=SELL
    pub side: u8,
    /// 0=EOA 1=Poly Proxy 2=Safe（产品仅用 2）
    pub signature_type: u8,
    pub timestamp_ms: U256,
    pub metadata: B256,
    pub builder: B256,
}

impl V2OrderInput {
    /// metadata/builder 全零的便捷构造（EOA：maker == signer）。
    #[allow(clippy::too_many_arguments)]
    pub fn zero_meta(
        salt: U256,
        maker: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        side: u8,
        signature_type: u8,
        timestamp_ms: U256,
    ) -> Self {
        Self {
            salt,
            maker,
            signer: maker,
            token_id,
            maker_amount,
            taker_amount,
            side,
            signature_type,
            timestamp_ms,
            metadata: B256::ZERO,
            builder: B256::ZERO,
        }
    }

    /// Safe Session：maker=Safe（funder），signer=Session EOA，signature_type=2。
    #[allow(clippy::too_many_arguments)]
    pub fn safe_session(
        salt: U256,
        maker_safe: Address,
        signer_eoa: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        side: u8,
        timestamp_ms: U256,
    ) -> Self {
        Self::safe_session_with_builder(
            salt,
            maker_safe,
            signer_eoa,
            token_id,
            maker_amount,
            taker_amount,
            side,
            timestamp_ms,
            B256::ZERO,
        )
    }

    /// Safe Session + Builder 归因码（bytes32）。
    #[allow(clippy::too_many_arguments)]
    pub fn safe_session_with_builder(
        salt: U256,
        maker_safe: Address,
        signer_eoa: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        side: u8,
        timestamp_ms: U256,
        builder: B256,
    ) -> Self {
        Self {
            salt,
            maker: maker_safe,
            signer: signer_eoa,
            token_id,
            maker_amount,
            taker_amount,
            side,
            signature_type: 2,
            timestamp_ms,
            metadata: B256::ZERO,
            builder,
        }
    }

    /// POLY_1271 Deposit Wallet：maker = signer = DW，signature_type = 3。
    /// 链上校验走 EIP-1271；提交前签名须经 ERC-7739 TypedDataSign 包裹（verifyingContract=DW）。
    #[allow(clippy::too_many_arguments)]
    pub fn poly_1271(
        salt: U256,
        deposit_wallet: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        side: u8,
        timestamp_ms: U256,
        builder: B256,
    ) -> Self {
        Self {
            salt,
            maker: deposit_wallet,
            signer: deposit_wallet,
            token_id,
            maker_amount,
            taker_amount,
            side,
            signature_type: 3,
            timestamp_ms,
            metadata: B256::ZERO,
            builder,
        }
    }
}

/// 解析 `COPY_BUILDER_CODE` / `POLY_BUILDER_CODE`（0x + 64 hex 或纯 64 hex）→ bytes32。
pub fn parse_builder_code(raw: &str) -> Option<B256> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let hex_s = t.trim_start_matches("0x");
    if hex_s.len() != 64 {
        return None;
    }
    let bytes = hex::decode(hex_s).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(B256::from(arr))
}

/// L1 ClobAuth 固定 attestation 文案（Polymarket 官方）。
pub const CLOB_AUTH_MESSAGE: &str = "This message attests that I control the given wallet";

/// 导出 L1 `ClobAuth` typed data（Privy / wagmi 签后填 `POLY_SIGNATURE`）。
pub fn clob_auth_typed_data(
    address: Address,
    timestamp_secs: u64,
    nonce: u64,
) -> Result<serde_json::Value, AuthError> {
    Ok(serde_json::json!({
        "domain": {
            "name": "ClobAuthDomain",
            "version": "1",
            "chainId": CHAIN_ID,
        },
        "types": {
            "ClobAuth": [
                {"name": "address", "type": "address"},
                {"name": "timestamp", "type": "string"},
                {"name": "nonce", "type": "uint256"},
                {"name": "message", "type": "string"},
            ]
        },
        "primaryType": "ClobAuth",
        "message": {
            "address": format!("{address}"),
            "timestamp": timestamp_secs.to_string(),
            "nonce": nonce.to_string(),
            "message": CLOB_AUTH_MESSAGE,
        }
    }))
}

fn parse_exchange(neg_risk: bool) -> Result<Address, AuthError> {
    let s = if neg_risk {
        EXCHANGE_NEG_RISK
    } else {
        EXCHANGE_STANDARD
    };
    s.parse::<Address>()
        .map_err(|e| AuthError::BadAddress(e.to_string()))
}

fn order_from_input(input: &V2OrderInput) -> Order {
    Order {
        salt: input.salt,
        maker: input.maker,
        signer: input.signer,
        tokenId: input.token_id,
        makerAmount: input.maker_amount,
        takerAmount: input.taker_amount,
        side: input.side,
        signatureType: input.signature_type,
        timestamp: input.timestamp_ms,
        metadata: input.metadata,
        builder: input.builder,
    }
}

fn order_message_json(order: &Order) -> serde_json::Value {
    serde_json::json!({
        "salt": order.salt.to_string(),
        "maker": format!("{}", order.maker),
        "signer": format!("{}", order.signer),
        "tokenId": order.tokenId.to_string(),
        "makerAmount": order.makerAmount.to_string(),
        "takerAmount": order.takerAmount.to_string(),
        "side": order.side,
        "signatureType": order.signatureType,
        "timestamp": order.timestamp.to_string(),
        "metadata": format!("{}", order.metadata),
        "builder": format!("{}", order.builder),
    })
}

fn order_types_json() -> serde_json::Value {
    serde_json::json!([
        {"name": "salt", "type": "uint256"},
        {"name": "maker", "type": "address"},
        {"name": "signer", "type": "address"},
        {"name": "tokenId", "type": "uint256"},
        {"name": "makerAmount", "type": "uint256"},
        {"name": "takerAmount", "type": "uint256"},
        {"name": "side", "type": "uint8"},
        {"name": "signatureType", "type": "uint8"},
        {"name": "timestamp", "type": "uint256"},
        {"name": "metadata", "type": "bytes32"},
        {"name": "builder", "type": "bytes32"},
    ])
}

/// CT-2 Assist：导出浏览器 `signTypedData` 用的 EIP-712 typed data（与 `sign_v2_order` 同域同类型）。
pub fn v2_order_typed_data(
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<serde_json::Value, AuthError> {
    let exchange = parse_exchange(neg_risk)?;
    let order = order_from_input(input);
    // alloy returns the full canonical root type string, while JSON
    // `primaryType` below must be the struct name only.
    debug_assert!(Order::eip712_root_type().starts_with("Order("));
    Ok(serde_json::json!({
        "domain": {
            "name": EXCHANGE_DOMAIN_NAME,
            "version": EXCHANGE_DOMAIN_VERSION,
            "chainId": CHAIN_ID,
            "verifyingContract": format!("{exchange}"),
        },
        "types": {
            "EIP712Domain": [
                {"name": "name", "type": "string"},
                {"name": "version", "type": "string"},
                {"name": "chainId", "type": "uint256"},
                {"name": "verifyingContract", "type": "address"},
            ],
            "Order": order_types_json(),
        },
        "primaryType": "Order",
        "message": order_message_json(&order),
    }))
}

/// POLY_1271 Privy/EOA typed data：ERC-7739 nested `TypedDataSign`（verifyingContract in message = DW）。
pub fn poly_1271_typed_data_sign(
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<serde_json::Value, AuthError> {
    if input.signature_type != 3 {
        return Err(AuthError::Sign(
            "poly_1271_typed_data_sign requires signature_type=3".into(),
        ));
    }
    if input.maker != input.signer {
        return Err(AuthError::Sign(
            "poly_1271 requires maker == signer == deposit wallet".into(),
        ));
    }
    let exchange = parse_exchange(neg_risk)?;
    let order = order_from_input(input);
    Ok(serde_json::json!({
        "domain": {
            "name": EXCHANGE_DOMAIN_NAME,
            "version": EXCHANGE_DOMAIN_VERSION,
            "chainId": CHAIN_ID,
            "verifyingContract": format!("{exchange}"),
        },
        "types": {
            "EIP712Domain": [
                {"name": "name", "type": "string"},
                {"name": "version", "type": "string"},
                {"name": "chainId", "type": "uint256"},
                {"name": "verifyingContract", "type": "address"},
            ],
            "TypedDataSign": [
                {"name": "contents", "type": "Order"},
                {"name": "name", "type": "string"},
                {"name": "version", "type": "string"},
                {"name": "chainId", "type": "uint256"},
                {"name": "verifyingContract", "type": "address"},
                {"name": "salt", "type": "bytes32"},
            ],
            "Order": order_types_json(),
        },
        "primaryType": "TypedDataSign",
        "message": {
            "contents": order_message_json(&order),
            "name": DEPOSIT_WALLET_NAME,
            "version": DEPOSIT_WALLET_VERSION,
            "chainId": CHAIN_ID.to_string(),
            "verifyingContract": format!("{}", input.signer),
            "salt": format!("{}", B256::ZERO),
        }
    }))
}

/// Wrap a 65-byte ECDSA over TypedDataSign into the ERC-7739 POLY_1271 wire signature.
///
/// Layout (matches `@polymarket/clob-client-v2` ExchangeOrderBuilderV2):
/// `innerSig || appDomainSeparator || contentsHash || OrderTypeString || u16be(len)`.
pub fn wrap_poly_1271_signature(
    inner_sig_hex: &str,
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<String, AuthError> {
    let hex_s = inner_sig_hex.trim().trim_start_matches("0x");
    let inner = hex::decode(hex_s).map_err(|e| AuthError::Sign(format!("bad sig hex: {e}")))?;
    if inner.len() != 65 {
        return Err(AuthError::Sign(format!(
            "poly_1271 inner signature must be 65 bytes, got {}",
            inner.len()
        )));
    }
    let exchange = parse_exchange(neg_risk)?;
    let order = order_from_input(input);
    let app_domain_sep = exchange_domain(exchange).separator();
    let contents_hash = struct_hash(&order);
    debug_assert_eq!(ORDER_TYPE_STRING.len(), 186);
    let mut out = Vec::with_capacity(65 + 32 + 32 + ORDER_TYPE_STRING.len() + 2);
    out.extend_from_slice(&inner);
    out.extend_from_slice(app_domain_sep.as_slice());
    out.extend_from_slice(contents_hash.as_slice());
    out.extend_from_slice(ORDER_TYPE_STRING.as_bytes());
    out.extend_from_slice(&(ORDER_TYPE_STRING.len() as u16).to_be_bytes());
    Ok(format!("0x{}", hex::encode(out)))
}

fn typed_data_sign_from_input(input: &V2OrderInput) -> TypedDataSign {
    TypedDataSign {
        contents: order_from_input(input),
        name: DEPOSIT_WALLET_NAME.to_string(),
        version: DEPOSIT_WALLET_VERSION.to_string(),
        chainId: U256::from(CHAIN_ID),
        verifyingContract: input.signer,
        salt: B256::ZERO,
    }
}

/// Recover the EOA that signed the ERC-7739 TypedDataSign (inner 65-byte ECDSA).
pub fn recover_poly_1271_inner_signer(
    signature_hex: &str,
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<Address, AuthError> {
    let exchange = parse_exchange(neg_risk)?;
    let nested = typed_data_sign_from_input(input);
    let digest = typed_data_digest(&exchange_domain(exchange), &nested);
    let hex_s = signature_hex.trim().trim_start_matches("0x");
    let bytes = hex::decode(hex_s).map_err(|e| AuthError::Sign(format!("bad sig hex: {e}")))?;
    let sig = alloy_primitives::Signature::try_from(bytes.as_slice())
        .map_err(|e| AuthError::Sign(format!("bad sig: {e}")))?;
    sig.recover_address_from_prehash(&digest)
        .map_err(|e| AuthError::Sign(format!("recover failed: {e}")))
}

/// Local-key POLY_1271 sign + ERC-7739 wrap (golden vectors / daemon offline path).
pub fn sign_poly_1271_order(
    pk: &str,
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<String, AuthError> {
    if input.signature_type != 3 || input.maker != input.signer {
        return Err(AuthError::Sign(
            "sign_poly_1271_order requires signature_type=3 and maker==signer".into(),
        ));
    }
    let exchange = parse_exchange(neg_risk)?;
    let nested = typed_data_sign_from_input(input);
    let domain = exchange_domain(exchange);
    let digest = typed_data_digest(&domain, &nested);
    let signer = PrivateKeySigner::from_str(pk).map_err(|e| AuthError::BadKey(e.to_string()))?;
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    wrap_poly_1271_signature(
        &format!("0x{}", hex::encode(sig.as_bytes())),
        input,
        neg_risk,
    )
}

/// 从 L1 ClobAuth EIP-712 签名恢复地址（Assist 中继派生 L2 前验签）。
pub fn recover_clob_auth_signer(
    signature_hex: &str,
    address: Address,
    timestamp_secs: u64,
    nonce: u64,
) -> Result<Address, AuthError> {
    let msg = ClobAuth {
        address,
        timestamp: timestamp_secs.to_string(),
        nonce: U256::from(nonce),
        message: CLOB_AUTH_MESSAGE.to_string(),
    };
    let digest = typed_data_digest(&clob_auth_domain(), &msg);
    let hex_s = signature_hex.trim().trim_start_matches("0x");
    let bytes = hex::decode(hex_s).map_err(|e| AuthError::Sign(format!("bad sig hex: {e}")))?;
    let sig = alloy_primitives::Signature::try_from(bytes.as_slice())
        .map_err(|e| AuthError::Sign(format!("bad sig: {e}")))?;
    sig.recover_address_from_prehash(&digest)
        .map_err(|e| AuthError::Sign(format!("recover failed: {e}")))
}

/// 从 V2 Order EIP-712 签名恢复 signer 地址（Assist POST 验签用）。
pub fn recover_v2_order_signer(
    signature_hex: &str,
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<Address, AuthError> {
    let exchange: Address = if neg_risk {
        EXCHANGE_NEG_RISK
    } else {
        EXCHANGE_STANDARD
    }
    .parse::<Address>()
    .map_err(|e| AuthError::BadAddress(e.to_string()))?;
    let order = Order {
        salt: input.salt,
        maker: input.maker,
        signer: input.signer,
        tokenId: input.token_id,
        makerAmount: input.maker_amount,
        takerAmount: input.taker_amount,
        side: input.side,
        signatureType: input.signature_type,
        timestamp: input.timestamp_ms,
        metadata: input.metadata,
        builder: input.builder,
    };
    let digest = typed_data_digest(&exchange_domain(exchange), &order);
    let hex_s = signature_hex.trim().trim_start_matches("0x");
    let bytes = hex::decode(hex_s).map_err(|e| AuthError::Sign(format!("bad sig hex: {e}")))?;
    let sig = alloy_primitives::Signature::try_from(bytes.as_slice())
        .map_err(|e| AuthError::Sign(format!("bad sig: {e}")))?;
    sig.recover_address_from_prehash(&digest)
        .map_err(|e| AuthError::Sign(format!("recover failed: {e}")))
}

/// 用 `neg_risk` 选择 verifyingContract，EIP-712 签 V2 Order，返回 `0x{r||s||v}` hex。
pub fn sign_v2_order(pk: &str, input: &V2OrderInput, neg_risk: bool) -> Result<String, AuthError> {
    let exchange: Address = if neg_risk {
        EXCHANGE_NEG_RISK
    } else {
        EXCHANGE_STANDARD
    }
    .parse::<Address>()
    .map_err(|e| AuthError::BadAddress(e.to_string()))?;
    sign_v2_order_with_contract(pk, input, exchange)
}

/// 显式 verifyingContract 版本（调用方已从 `/book` 读到地址）。
pub fn sign_v2_order_with_contract(
    pk: &str,
    input: &V2OrderInput,
    exchange: Address,
) -> Result<String, AuthError> {
    let signer = PrivateKeySigner::from_str(pk).map_err(|e| AuthError::BadKey(e.to_string()))?;
    let order = Order {
        salt: input.salt,
        maker: input.maker,
        signer: input.signer,
        tokenId: input.token_id,
        makerAmount: input.maker_amount,
        takerAmount: input.taker_amount,
        side: input.side,
        signatureType: input.signature_type,
        timestamp: input.timestamp_ms,
        metadata: input.metadata,
        builder: input.builder,
    };
    let domain = exchange_domain(exchange);
    let digest = typed_data_digest(&domain, &order);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// 同 [`sign_v2_order_with_contract`]，但直接接收 `PrivateKeySigner`。
pub fn sign_v2_order_with_signer(
    signer: &PrivateKeySigner,
    input: &V2OrderInput,
    exchange: Address,
) -> Result<String, AuthError> {
    let order = order_from_input(input);
    let domain = exchange_domain(exchange);
    let digest = typed_data_digest(&domain, &order);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// 同 [`sign_poly_1271_order`]，但直接接收 `PrivateKeySigner`（复用同一 ERC-7739 wrap 逻辑）。
pub fn sign_poly_1271_order_with_signer(
    signer: &PrivateKeySigner,
    input: &V2OrderInput,
    neg_risk: bool,
) -> Result<String, AuthError> {
    if input.signature_type != 3 || input.maker != input.signer {
        return Err(AuthError::Sign(
            "sign_poly_1271_order_with_signer requires signature_type=3 and maker==signer".into(),
        ));
    }
    let exchange = parse_exchange(neg_risk)?;
    let nested = typed_data_sign_from_input(input);
    let domain = exchange_domain(exchange);
    let digest = typed_data_digest(&domain, &nested);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| AuthError::Sign(e.to_string()))?;
    wrap_poly_1271_signature(
        &format!("0x{}", hex::encode(sig.as_bytes())),
        input,
        neg_risk,
    )
}

/// ClobAuthDomain 构造（name=ClobAuthDomain / version=1 / chainId=137）。
fn clob_auth_domain() -> alloy_sol_types::Eip712Domain {
    alloy_sol_types::Eip712Domain::new(
        Some("ClobAuthDomain".into()),
        Some("1".into()),
        Some(U256::from(CHAIN_ID)),
        None,
        None,
    )
}

/// Exchange domain（name=Polymarket CTF Exchange / version=2 / chainId=137 / verifyingContract）。
fn exchange_domain(exchange: Address) -> alloy_sol_types::Eip712Domain {
    alloy_sol_types::Eip712Domain::new(
        Some(EXCHANGE_DOMAIN_NAME.into()),
        Some(EXCHANGE_DOMAIN_VERSION.into()),
        Some(U256::from(CHAIN_ID)),
        Some(exchange),
        None,
    )
}

/// EIP-712 typed-data digest = keccak256("\x19\x01" || domainSeparator || structHash)。
/// structHash = keccak256(typeHash || abi.encode(struct))。
fn typed_data_digest<T: SolStruct>(domain: &alloy_sol_types::Eip712Domain, value: &T) -> B256 {
    let sep = domain.separator();
    let struct_hash = struct_hash(value);
    let mut buf = [0u8; 2 + 32 + 32];
    buf[0] = 0x19;
    buf[1] = 0x01;
    buf[2..34].copy_from_slice(sep.as_slice());
    buf[34..66].copy_from_slice(struct_hash.as_slice());
    alloy_primitives::keccak256(buf)
}

/// keccak256(typeHash || encodeData) —— EIP-712 hashStruct。
fn struct_hash<T: SolStruct>(value: &T) -> B256 {
    let type_hash = value.eip712_type_hash();
    let encode_data = value.eip712_encode_data();
    let mut buf = Vec::with_capacity(32 + encode_data.len());
    buf.extend_from_slice(type_hash.as_slice());
    buf.extend_from_slice(&encode_data);
    alloy_primitives::keccak256(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, B256, U256};

    // 固定测试私钥（仅测试用，公开无价值）。
    const PK: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    // 上述私钥对应的地址（hardhat #0）。
    const ADDR: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";

    fn addr() -> Address {
        ADDR.parse().unwrap()
    }

    #[test]
    fn l2_hmac_matches_independent_vector() {
        // 由 Python `hmac` + `base64.urlsafe_b64encode` 独立计算（非本 crate 算法）。
        let secret_b64 = "czRTeHh5SjZQc2xJOERBd2V0M0x6N3NWSzVxQw==";
        let sig =
            l2_signature(secret_b64, 1713398400, "POST", "/order", r#"{"foo":"bar"}"#).unwrap();
        assert_eq!(sig, "8y8ZMu3smg_pH0hrtcDDMG1YSh4C8bj-GHrwiVHNU8k=");
    }

    #[test]
    fn l2_headers_shape() {
        let creds = ApiCreds {
            api_key: "k".into(),
            api_secret_b64: "czRTeHh5SjZQc2xJOERBd2V0M0x6N3NWSzVxQw==".into(),
            passphrase: "p".into(),
        };
        let h = l2_headers(ADDR, &creds, "get", "/data/orders", "", 1713398400).unwrap();
        let map: std::collections::HashMap<&str, &str> =
            h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(map["POLY_ADDRESS"], ADDR);
        assert_eq!(map["POLY_API_KEY"], "k");
        assert_eq!(map["POLY_PASSPHRASE"], "p");
        assert_eq!(map["POLY_TIMESTAMP"], "1713398400");
        assert!(map["POLY_SIGNATURE"].ends_with('=') || !map["POLY_SIGNATURE"].is_empty());
        // GET 路径与方法大小写无关：内部统一大写。
        let sig_get =
            l2_signature(&creds.api_secret_b64, 1713398400, "get", "/data/orders", "").unwrap();
        assert_eq!(map["POLY_SIGNATURE"], sig_get);
        // Query string must not change the HMAC (Polymarket L2 signs path only).
        let sig_with_query = l2_signature(
            &creds.api_secret_b64,
            1713398400,
            "GET",
            "/data/orders?id=0xabc",
            "",
        )
        .unwrap();
        assert_eq!(sig_get, sig_with_query);
    }

    #[test]
    fn l1_sign_recovers_to_wallet() {
        let input = L1Input {
            address: ADDR,
            timestamp: 1713398400,
            nonce: 0,
            message: "This message attests that I control the given wallet",
        };
        let sig_hex = sign_l1(PK, &input).unwrap();
        let bytes = hex::decode(&sig_hex[2..]).unwrap();
        assert_eq!(bytes.len(), 65);
        let sig = alloy_primitives::Signature::try_from(&bytes[..]).unwrap();
        // 构造相同 digest 以恢复。
        let domain = clob_auth_domain();
        let value = ClobAuth {
            address: addr(),
            timestamp: "1713398400".to_string(),
            nonce: U256::ZERO,
            message: input.message.to_string(),
        };
        let digest = typed_data_digest(&domain, &value);
        let recovered = sig.recover_address_from_prehash(&digest).unwrap();
        assert_eq!(recovered, addr(), "L1 EIP-712 签名必须恢复出原钱包地址");
        let via_helper = recover_clob_auth_signer(&sig_hex, addr(), 1713398400, 0).unwrap();
        assert_eq!(via_helper, addr());
    }

    #[test]
    fn v2_order_recovers_to_wallet_standard() {
        let order = V2OrderInput::zero_meta(
            U256::from(1u64),
            addr(),
            U256::from(102936u64),
            U256::from(45_000_000u64), // 45 USDC @ 6 decimals
            U256::from(100_000_000u64),
            0,                            // BUY
            0,                            // EOA
            U256::from(1713398400000u64), // ms
        );
        let sig_hex = sign_v2_order(PK, &order, false).unwrap();
        let bytes = hex::decode(&sig_hex[2..]).unwrap();
        assert_eq!(bytes.len(), 65);
        let sig = alloy_primitives::Signature::try_from(&bytes[..]).unwrap();
        let domain = exchange_domain(EXCHANGE_STANDARD.parse::<Address>().unwrap());
        let v = Order {
            salt: order.salt,
            maker: order.maker,
            signer: order.signer,
            tokenId: order.token_id,
            makerAmount: order.maker_amount,
            takerAmount: order.taker_amount,
            side: order.side,
            signatureType: order.signature_type,
            timestamp: order.timestamp_ms,
            metadata: order.metadata,
            builder: order.builder,
        };
        let digest = typed_data_digest(&domain, &v);
        let recovered = sig.recover_address_from_prehash(&digest).unwrap();
        assert_eq!(
            recovered,
            addr(),
            "V2 Order EIP-712 签名必须恢复出原钱包地址"
        );
        assert_eq!(
            recover_v2_order_signer(&sig_hex, &order, false).unwrap(),
            addr()
        );
    }

    #[test]
    fn v2_order_standard_vs_neg_risk_differ() {
        let order = V2OrderInput::zero_meta(
            U256::from(7u64),
            addr(),
            U256::from(999u64),
            U256::from(10_000_000u64),
            U256::from(20_000_000u64),
            1,
            0,
            U256::from(1713398400000u64),
        );
        let a = sign_v2_order(PK, &order, false).unwrap();
        let b = sign_v2_order(PK, &order, true).unwrap();
        assert_ne!(a, b, "neg_risk 切换 verifyingContract 后签名必须不同");
    }

    #[test]
    fn v2_order_metadata_builder_affect_sig() {
        let mut order = V2OrderInput::zero_meta(
            U256::from(1u64),
            addr(),
            U256::from(1u64),
            U256::from(1u64),
            U256::from(1u64),
            0,
            0,
            U256::from(1u64),
        );
        let a = sign_v2_order(PK, &order, false).unwrap();
        order.metadata = B256::from(alloy_primitives::keccak256("meta"));
        let b = sign_v2_order(PK, &order, false).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn bad_private_key_rejected() {
        let input = L1Input {
            address: ADDR,
            timestamp: 1,
            nonce: 0,
            message: "x",
        };
        assert!(sign_l1("0xnope", &input).is_err());
    }

    #[test]
    fn bad_secret_b64_rejected() {
        assert!(l2_signature("!!!notbase64!!!", 1, "GET", "/", "").is_err());
    }

    #[test]
    fn clob_auth_typed_data_has_primary() {
        let td = clob_auth_typed_data(addr(), 1_700_000_000, 0).unwrap();
        assert_eq!(td["primaryType"], "ClobAuth");
        assert_eq!(td["domain"]["name"], "ClobAuthDomain");
        assert_eq!(td["message"]["message"], CLOB_AUTH_MESSAGE);
    }

    #[test]
    fn v2_order_typed_data_primary_is_order_not_v2order() {
        let order = V2OrderInput::zero_meta(
            U256::from(1u64),
            addr(),
            U256::from(102936u64),
            U256::from(45_000_000u64),
            U256::from(100_000_000u64),
            0,
            0,
            U256::from(1713398400000u64),
        );
        let td = v2_order_typed_data(&order, false).unwrap();
        assert_eq!(td["primaryType"], "Order");
        assert!(Order::eip712_root_type().starts_with("Order("));
        assert!(td["types"].get("Order").is_some());
        assert!(td["types"].get("V2Order").is_none());
        assert_eq!(td["domain"]["version"], "2");
    }

    #[test]
    fn parse_builder_code_roundtrip() {
        let b = parse_builder_code(
            "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        assert_ne!(b, B256::ZERO);
        assert!(parse_builder_code("short").is_none());
        assert!(parse_builder_code("").is_none());
    }

    #[test]
    fn safe_session_sets_sigtype2_and_split_maker_signer() {
        let maker = "0x00000000000000000000000000000000000000aa"
            .parse::<Address>()
            .unwrap();
        let signer = "0x00000000000000000000000000000000000000bb"
            .parse::<Address>()
            .unwrap();
        let o = V2OrderInput::safe_session(
            U256::from(1u64),
            maker,
            signer,
            U256::from(2u64),
            U256::from(3u64),
            U256::from(4u64),
            0,
            U256::from(5u64),
        );
        assert_eq!(o.signature_type, 2);
        assert_eq!(o.maker, maker);
        assert_eq!(o.signer, signer);
        assert_ne!(o.maker, o.signer);
    }

    #[test]
    fn poly_1271_sets_sigtype3_and_maker_eq_signer() {
        let dw = "0x00000000000000000000000000000000000000cc"
            .parse::<Address>()
            .unwrap();
        let o = V2OrderInput::poly_1271(
            U256::from(1u64),
            dw,
            U256::from(2u64),
            U256::from(3u64),
            U256::from(4u64),
            0,
            U256::from(5u64),
            B256::ZERO,
        );
        assert_eq!(o.signature_type, 3);
        assert_eq!(o.maker, dw);
        assert_eq!(o.signer, dw);
    }

    /// Dual §8.2 `type3_golden_vectors`: Rust ↔ official TS SDK byte-exact ERC-7739 wrap.
    #[test]
    fn type3_golden_vectors() {
        poly_1271_erc7739_matches_official_ts_sdk_vector_inner();
    }

    #[test]
    fn poly_1271_erc7739_matches_official_ts_sdk_vector() {
        poly_1271_erc7739_matches_official_ts_sdk_vector_inner();
    }

    fn poly_1271_erc7739_matches_official_ts_sdk_vector_inner() {
        // Fixture owner = anvil key 0x01; signature produced by clob-client-v2@1.1.0.
        const PK1: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let vector: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/type3_sdk_sign_only.json"))
                .unwrap();
        let order = &vector["signed_order"];
        let dw: Address = order["maker"].as_str().unwrap().parse().unwrap();
        let input = V2OrderInput {
            salt: U256::from_str(order["salt"].as_str().unwrap()).unwrap(),
            maker: dw,
            signer: dw,
            token_id: U256::from_str(order["tokenId"].as_str().unwrap()).unwrap(),
            maker_amount: U256::from_str(order["makerAmount"].as_str().unwrap()).unwrap(),
            taker_amount: U256::from_str(order["takerAmount"].as_str().unwrap()).unwrap(),
            side: 0, // BUY
            signature_type: 3,
            timestamp_ms: U256::from_str(order["timestamp"].as_str().unwrap()).unwrap(),
            metadata: order["metadata"].as_str().unwrap().parse().unwrap(),
            builder: order["builder"].as_str().unwrap().parse().unwrap(),
        };
        let wrapped = sign_poly_1271_order(PK1, &input, false).unwrap();
        assert_eq!(
            wrapped.to_ascii_lowercase(),
            order["signature"].as_str().unwrap().to_ascii_lowercase()
        );
        assert_eq!((wrapped.len() - 2) / 2, 317);
        assert_eq!(ORDER_TYPE_STRING.len(), 186);

        // Negative: maker ≠ signer must fail closed for type-3 helper.
        let mut bad = input.clone();
        bad.signer = Address::ZERO;
        assert!(sign_poly_1271_order(PK1, &bad, false).is_err());
        assert!(poly_1271_typed_data_sign(&bad, false).is_err());
    }

    #[test]
    fn salt_from_intent_stable_and_comparable() {
        let u = Uuid::nil();
        let a = salt_from_intent(1, u, "0xabc");
        let b = salt_from_intent(1, u, "0xabc");
        assert_eq!(a, b);
        assert_ne!(salt_from_intent(2, u, "0xabc"), a);
        let n = U256::from_be_bytes(a);
        assert!(n > U256::ZERO);
        assert!(n < U256::from(1u64 << 53));
        let dec = n.to_string();
        assert!(salt_str_matches(&dec, &a));
        assert!(!salt_str_matches("0", &a));
    }

    #[test]
    fn sign_l1_with_signer_matches_sign_l1() {
        let signer = PrivateKeySigner::from_str(PK).unwrap();
        let input = L1Input {
            address: ADDR,
            timestamp: 1713398400,
            nonce: 0,
            message: CLOB_AUTH_MESSAGE,
        };
        let via_pk = sign_l1(PK, &input).unwrap();
        let via_signer = sign_l1_with_signer(&signer, &input).unwrap();
        assert_eq!(via_pk, via_signer);
    }

    #[test]
    fn sign_v2_order_with_signer_matches_pk_path() {
        let signer = PrivateKeySigner::from_str(PK).unwrap();
        let order = V2OrderInput::zero_meta(
            U256::from(42u64),
            addr(),
            U256::from(102936u64),
            U256::from(45_000_000u64),
            U256::from(100_000_000u64),
            0,
            0,
            U256::from(1713398400000u64),
        );
        let exchange: Address = EXCHANGE_STANDARD.parse().unwrap();
        let via_pk = sign_v2_order_with_contract(PK, &order, exchange).unwrap();
        let via_signer = sign_v2_order_with_signer(&signer, &order, exchange).unwrap();
        assert_eq!(via_pk, via_signer);
    }

    #[test]
    fn sign_poly_1271_with_signer_matches_pk_path() {
        let dw: Address = "0x73310C2E333fB7fb177Dcb43e164958a9D443d6f"
            .parse()
            .unwrap();
        let signer = PrivateKeySigner::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let input = V2OrderInput::poly_1271(
            U256::from(123u64),
            dw,
            U256::from(2u64),
            U256::from(3u64),
            U256::from(4u64),
            0,
            U256::from(5u64),
            B256::ZERO,
        );
        let via_pk = sign_poly_1271_order(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
            &input,
            false,
        )
        .unwrap();
        let via_signer = sign_poly_1271_order_with_signer(&signer, &input, false).unwrap();
        assert_eq!(via_pk, via_signer);
    }
}
