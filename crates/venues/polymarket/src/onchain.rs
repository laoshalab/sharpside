//! 链上余额兜底查询：Polygon JSON-RPC `eth_call` 读 pUSD（ERC-20）`balanceOf`。
//!
//! 用途：离线预配（`provision_live=false`）时 CLOB `/balance-allowance` 不可用，
//! 但 `deposit_wallet_address` 已由 CREATE2 确定性派生，可直接链上读 ERC-20 余额展示。
//!
//! 口径差异：
//! - CLOB `/balance-allowance.balance` = Deposit Wallet 的 pUSD collateral（可用现金）。
//! - 链上 `balanceOf(deposit_wallet)` = Deposit Wallet 持有的 pUSD ERC-20 原始余额。
//! - 两者通常一致（Polymarket 的 collateral 即 pUSD 本体）；仅在订单锁仓中间态可能略有差异，
//!   故前端标注「链上余额（RPC 兜底）」以示区别，不冒充 CLOB 实时可用资金。
//!
//! 网络：自建 HTTP 客户端，**不**继承 `POLYMARKET_HTTP_PROXY`。
//! Polymarket 域名常被墙，但 Polygon 公共 RPC（如 publicnode）多数地区可直连；
//! 若强制走已挂掉的 Clash 代理，余额展示会整体失败。需要代理时单独设 `POLYGON_RPC_PROXY`。
//! RPC URL 由 env `POLYGON_RPC_URL` 覆盖。

#![forbid(unsafe_code)]

use alloy_primitives::{Address, U256};
use reqwest::Client;
use serde::Deserialize;

/// 默认 Polygon mainnet 公共 RPC。可由 env `POLYGON_RPC_URL` 覆盖。
/// 选用 publicnode：比 polygon-rpc.com 更稳，且多数地区可直连（无需 Clash）。
pub const POLYGON_RPC_DEFAULT: &str = "https://polygon-bor.publicnode.com";

/// ERC-20 `balanceOf(address)` selector = `0x70a08231`。
const SELECTOR_BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

/// ERC-1155 `balanceOf(address,uint256)` selector = `0x00fdd58e`。
/// 用于读 CTF outcome token 余额（赎回前校验有无可赎回量）。
const SELECTOR_ERC1155_BALANCE_OF: [u8; 4] = [0x00, 0xfd, 0xd5, 0x8e];

/// `eth_call` 读余额的超时（秒）。归档列表会并发打多次，留足余量。
const RPC_TIMEOUT_SECS: u64 = 12;

/// 主 URL 失败时轮询的公共 Polygon RPC（直连、多数地区可达）。
const RPC_FALLBACKS: &[&str] = &[
    "https://polygon-bor.publicnode.com",
    "https://polygon-rpc.com",
    "https://1rpc.io/matic",
];

/// 解析 RPC 候选列表：`rpc_url` → `POLYGON_RPC_URLS` 额外 → 内置 fallback（去重）。
fn rpc_endpoints(primary: &str) -> Vec<String> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<String>, raw: &str| {
        let u = raw.trim().trim_end_matches('/');
        if !u.is_empty() && !out.iter().any(|x| x == u) {
            out.push(u.to_string());
        }
    };
    push(&mut out, primary);
    if let Ok(extra) = std::env::var("POLYGON_RPC_URLS") {
        for part in extra.split(',') {
            push(&mut out, part);
        }
    }
    for f in RPC_FALLBACKS {
        push(&mut out, f);
    }
    out
}

/// 链上 RPC 专用 HTTP 客户端：默认直连；仅当显式设置 `POLYGON_RPC_PROXY` 时走代理。
fn build_rpc_http_client(timeout: std::time::Duration) -> Client {
    let mut b = Client::builder().timeout(timeout);
    if let Ok(p) = std::env::var("POLYGON_RPC_PROXY") {
        let p = p.trim();
        if !p.is_empty() {
            match reqwest::Proxy::all(p) {
                Ok(proxy) => {
                    b = b.proxy(proxy);
                }
                Err(e) => tracing::warn!(proxy = p, error = %e, "POLYGON_RPC_PROXY 解析失败，直连"),
            }
        }
    }
    b.build().expect("reqwest rpc client build")
}

/// 对单个 endpoint 发一次 `eth_call`，成功返回 result hex 字符串。
async fn eth_call_once(
    http: &Client,
    url: &str,
    to: Address,
    data: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": to.to_string(), "data": data },
            "latest"
        ]
    });
    let resp = http
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}: {}",
            text.chars().take(160).collect::<String>()
        ));
    }
    let parsed: RpcResp = serde_json::from_str(&text).map_err(|e| {
        format!(
            "响应解析失败: {e}（{}）",
            text.chars().take(120).collect::<String>()
        )
    })?;
    if let Some(err) = parsed.error {
        return Err(format!(
            "RPC error: {}",
            err.message.unwrap_or_else(|| "unknown".into())
        ));
    }
    parsed
        .result
        .ok_or_else(|| "响应缺 result".to_string())
}

/// `eth_call`：主 URL + 备用节点，每节点最多 3 次短重试。
async fn eth_call_hex(rpc_url: &str, to: Address, data: &str) -> Result<String, String> {
    let http = build_rpc_http_client(std::time::Duration::from_secs(RPC_TIMEOUT_SECS));
    let endpoints = rpc_endpoints(rpc_url);
    let mut last_err = String::from("无可用 RPC");
    for url in &endpoints {
        for attempt in 0u32..3 {
            match eth_call_once(&http, url, to, data).await {
                Ok(hex) => return Ok(hex),
                Err(e) => {
                    last_err = format!("{url} (try {}): {e}", attempt + 1);
                    tracing::warn!(url = %url, attempt, error = %e, "Polygon eth_call 失败，将重试/换节点");
                    if attempt + 1 < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt + 1) as u64))
                            .await;
                    }
                }
            }
        }
    }
    Err(format!("链上余额暂不可查（RPC 繁忙或网络波动，请稍后刷新）: {last_err}"))
}

/// JSON-RPC 响应（只取 result 字段）。
#[derive(Debug, Deserialize)]
struct RpcResp {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    #[serde(default)]
    message: Option<String>,
}

/// `eth_getTransactionReceipt` / 含 object result 的通用包装。
#[derive(Debug, Deserialize)]
struct RpcRespVal<T> {
    /// 可为 null（未上链）。不加 `default`，避免泛型 T: Default 约束。
    result: Option<T>,
    #[serde(default)]
    error: Option<RpcError>,
}

/// 交易回执日志（精简字段，供 ERC-20 Transfer 匹配）。
#[derive(Debug, Clone, Deserialize)]
pub struct TxLog {
    /// 合约地址（代币）。
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    #[serde(default, rename = "logIndex")]
    pub log_index: Option<String>,
    /// `eth_getLogs` 才有；receipt 内嵌 log 通常无此字段。
    #[serde(default, rename = "transactionHash")]
    pub transaction_hash: Option<String>,
    #[serde(default, rename = "blockNumber")]
    pub block_number: Option<String>,
    #[serde(default)]
    pub removed: Option<bool>,
}

/// `eth_getTransactionReceipt` 回执。
#[derive(Debug, Clone, Deserialize)]
pub struct TxReceipt {
    /// `0x1` 成功 / `0x0` 失败。
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "blockNumber")]
    pub block_number: Option<String>,
    #[serde(default)]
    pub logs: Vec<TxLog>,
}

impl TxReceipt {
    /// 交易是否成功（缺 status 的老链视为未知 → false）。
    pub fn is_success(&self) -> bool {
        matches!(
            self.status.as_deref().map(|s| s.trim().to_lowercase()).as_deref(),
            Some("0x1") | Some("1")
        )
    }

    pub fn block_number_u64(&self) -> Option<u64> {
        self.block_number.as_deref().and_then(parse_hex_u64)
    }
}

impl TxLog {
    pub fn log_index_i32(&self) -> Option<i32> {
        let n = self.log_index.as_deref().and_then(parse_hex_u64)?;
        i32::try_from(n).ok()
    }

    pub fn block_number_u64(&self) -> Option<u64> {
        self.block_number.as_deref().and_then(parse_hex_u64)
    }

    pub fn tx_hash_normalized(&self) -> Option<String> {
        let h = self.transaction_hash.as_deref()?.trim().to_lowercase();
        if h.starts_with("0x") && h.len() == 66 {
            Some(h)
        } else {
            None
        }
    }

    pub fn is_removed(&self) -> bool {
        self.removed.unwrap_or(false)
    }
}

/// 解析 `0x…` hex 无符号整数。
pub fn parse_hex_u64(s: &str) -> Option<u64> {
    let h = s.trim().trim_start_matches("0x");
    if h.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(h, 16).ok()
}

/// 地址 → 32 字节 topic（左填零）。非法地址返回 None。
pub fn address_to_topic(addr: &str) -> Option<String> {
    let a = addr.trim().trim_start_matches("0x").to_lowercase();
    if a.len() != 40 || !a.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{:0>64}", a))
}

fn u64_to_hex_qty(n: u64) -> String {
    format!("0x{n:x}")
}

async fn rpc_post_json_timeout<T: for<'de> Deserialize<'de>>(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
    timeout: std::time::Duration,
) -> Result<Option<T>, String> {
    let url = rpc_url.trim_end_matches('/');
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let http = build_rpc_http_client(timeout);
    let resp = http
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC {method} 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "RPC {method} HTTP {status}: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    let parsed: RpcRespVal<T> = serde_json::from_str(&text).map_err(|e| {
        format!(
            "RPC {method} 响应解析失败: {e}（原文: {}）",
            text.chars().take(200).collect::<String>()
        )
    })?;
    if let Some(err) = parsed.error {
        return Err(format!(
            "RPC {method} error: {}",
            err.message.unwrap_or_else(|| "unknown".into())
        ));
    }
    Ok(parsed.result)
}

async fn rpc_post_json<T: for<'de> Deserialize<'de>>(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<Option<T>, String> {
    rpc_post_json_timeout(
        rpc_url,
        method,
        params,
        std::time::Duration::from_secs(RPC_TIMEOUT_SECS),
    )
    .await
}

/// `eth_blockNumber` → 最新块高。
pub async fn eth_block_number(rpc_url: &str) -> Result<u64, String> {
    let hex: Option<String> = rpc_post_json(rpc_url, "eth_blockNumber", serde_json::json!([])).await?;
    let hex = hex.ok_or_else(|| "eth_blockNumber 缺 result".to_string())?;
    parse_hex_u64(&hex).ok_or_else(|| format!("非法 blockNumber: {hex}"))
}

/// `eth_getTransactionReceipt`。未上链时 result 为 null → `Ok(None)`。
pub async fn eth_get_transaction_receipt(
    rpc_url: &str,
    tx_hash: &str,
) -> Result<Option<TxReceipt>, String> {
    rpc_post_json(
        rpc_url,
        "eth_getTransactionReceipt",
        serde_json::json!([tx_hash]),
    )
    .await
}

/// `eth_getLogs`：按合约 + topics 拉日志（用于无 submit-tx 时认领入账）。
///
/// `topics` 中可用 `serde_json::Value::Null` 表示任意。
pub async fn eth_get_logs(
    rpc_url: &str,
    address: &str,
    topics: Vec<serde_json::Value>,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<TxLog>, String> {
    if from_block > to_block {
        return Ok(vec![]);
    }
    let filter = serde_json::json!({
        "fromBlock": u64_to_hex_qty(from_block),
        "toBlock": u64_to_hex_qty(to_block),
        "address": address,
        "topics": topics,
    });
    let logs: Option<Vec<TxLog>> = rpc_post_json_timeout(
        rpc_url,
        "eth_getLogs",
        serde_json::json!([filter]),
        std::time::Duration::from_secs(20),
    )
    .await?;
    Ok(logs.unwrap_or_default())
}

/// 分块拉 logs，规避公共 RPC 块范围限制。
pub async fn eth_get_logs_chunked(
    rpc_url: &str,
    address: &str,
    topics: Vec<serde_json::Value>,
    from_block: u64,
    to_block: u64,
    chunk_size: u64,
) -> Result<Vec<TxLog>, String> {
    let chunk = chunk_size.max(1);
    let mut out = Vec::new();
    let mut start = from_block;
    while start <= to_block {
        let end = start.saturating_add(chunk - 1).min(to_block);
        let mut part = eth_get_logs(rpc_url, address, topics.clone(), start, end).await?;
        out.append(&mut part);
        if end == to_block {
            break;
        }
        start = end + 1;
    }
    Ok(out)
}

/// 构造 `balanceOf(address)` calldata（4 + 32 字节），返回 `0x` 前缀 hex。
fn balance_of_calldata(who: Address) -> String {
    let mut out = Vec::with_capacity(4 + 32);
    out.extend_from_slice(&SELECTOR_BALANCE_OF);
    let mut pad = [0u8; 32];
    pad[12..].copy_from_slice(who.as_slice());
    out.extend_from_slice(&pad);
    format!("0x{}", hex::encode(&out))
}

/// 读 `deposit_wallet` 在 pUSD（COLLATERAL）合约上的 ERC-20 余额，归一为美元（6 decimals）。
///
/// - `rpc_url`：Polygon JSON-RPC endpoint（如 `https://polygon-bor.publicnode.com`）。
/// - `collateral`：pUSD 合约地址（取自 [`crate::wallet_batch::contracts::COLLATERAL`]）。
/// - 失败（网络 / RPC error / 非法 hex）返回 Err 字符串，调用方降级为「余额不可查」。
pub async fn pusd_balance_of(
    rpc_url: &str,
    collateral: Address,
    deposit_wallet: Address,
) -> Result<f64, String> {
    let data = balance_of_calldata(deposit_wallet);
    let hex_result = eth_call_hex(rpc_url, collateral, &data).await?;
    let hex_str = hex_result.trim().trim_start_matches("0x");
    if hex_str.is_empty() {
        return Ok(0.0);
    }
    let bytes = hex::decode(hex_str).map_err(|e| format!("RPC result hex 解码失败: {e}"))?;
    // 右对齐到 32 字节（balanceOf 返回 uint256；不足高位补零）
    let mut buf = [0u8; 32];
    if bytes.len() >= 32 {
        buf.copy_from_slice(&bytes[bytes.len() - 32..]);
    } else {
        buf[32 - bytes.len()..].copy_from_slice(&bytes);
    }
    let raw = U256::from_be_bytes::<32>(buf);
    // pUSD = USDC，6 decimals。U256 → u128 不会溢出（远小于 2^128）。
    let raw_u128: u128 = raw
        .try_into()
        .map_err(|_| "余额超出 uint128 范围".to_string())?;
    Ok(raw_u128 as f64 / 1_000_000.0)
}

/// 计算 CTF outcome token 的 ERC-1155 positionId（token id）。
///
/// 对应 Gnosis ConditionalTokens 合约：
///   `collectionId = keccak256(abi.encodePacked(conditionId, indexSet))`
///   `positionId   = uint256(keccak256(abi.encodePacked(collateralToken, collectionId)))`
/// 其中 `parentCollectionId = bytes32(0)`（顶层市场），XOR 0 即原值。
///
/// - `collateral`：pUSD 地址。
/// - `condition_id`：市场 conditionId（0x hex，bytes32）。
/// - `index_set`：二元市场 YES=2、NO=1（CTF outcome slot bitmask）。
pub fn ctf_position_id(collateral: Address, condition_id: &str, index_set: u64) -> U256 {
    let cond = parse_bytes32_hex(condition_id);
    // collectionId = keccak256(conditionId || uint256(indexSet))
    let mut packed = [0u8; 64];
    packed[..32].copy_from_slice(&cond);
    packed[32..].copy_from_slice(&U256::from(index_set).to_be_bytes::<32>());
    let collection_id = alloy_primitives::keccak256(packed);

    // positionId = uint256(keccak256(collateralToken(20) || collectionId(32)))
    let mut packed2 = [0u8; 52];
    packed2[..20].copy_from_slice(collateral.as_slice());
    packed2[20..].copy_from_slice(collection_id.as_slice());
    let position_id = alloy_primitives::keccak256(packed2);
    U256::from_be_bytes(position_id.0)
}

/// 解析 0x 前缀 hex conditionId 为 bytes32。非法输入返回全 0（由调用方兜底校验）。
fn parse_bytes32_hex(s: &str) -> [u8; 32] {
    let hex_s = s.trim().trim_start_matches("0x");
    let bytes = match hex::decode(hex_s) {
        Ok(b) => b,
        Err(_) => return [0u8; 32],
    };
    let mut out = [0u8; 32];
    if bytes.len() == 32 {
        out.copy_from_slice(&bytes);
    } else if bytes.len() < 32 {
        out[32 - bytes.len()..].copy_from_slice(&bytes);
    } else {
        out.copy_from_slice(&bytes[bytes.len() - 32..]);
    }
    out
}

/// 构造 ERC-1155 `balanceOf(address,uint256)` calldata（4 + 32 + 32 = 68 字节），返回 `0x` hex。
fn erc1155_balance_of_calldata(account: Address, id: U256) -> String {
    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&SELECTOR_ERC1155_BALANCE_OF);
    out.extend_from_slice(&account.left_pad_bytes::<32>()[..]);
    out.extend_from_slice(&id.to_be_bytes::<32>());
    format!("0x{}", hex::encode(&out))
}

/// 读 CTF（ERC-1155）outcome token 余额。返回人类单位（CTF token 1:1 collateral，6 decimals）。
///
/// - `rpc_url`：Polygon JSON-RPC endpoint。
/// - `ctf`：ConditionalTokens 合约地址（`wallet_batch::contracts::CONDITIONAL_TOKENS`）。
/// - `deposit_wallet`：持有 outcome token 的 deposit wallet 地址。
/// - `position_id`：outcome token 的 ERC-1155 id（由 [`ctf_position_id`] 计算）。
pub async fn ctf_balance_of(
    rpc_url: &str,
    ctf: Address,
    deposit_wallet: Address,
    position_id: U256,
) -> Result<f64, String> {
    let data = erc1155_balance_of_calldata(deposit_wallet, position_id);
    let hex_result = eth_call_hex(rpc_url, ctf, &data).await?;
    let hex_str = hex_result.trim().trim_start_matches("0x");
    if hex_str.is_empty() {
        return Ok(0.0);
    }
    let bytes = hex::decode(hex_str).map_err(|e| format!("RPC result hex 解码失败: {e}"))?;
    let mut buf = [0u8; 32];
    if bytes.len() >= 32 {
        buf.copy_from_slice(&bytes[bytes.len() - 32..]);
    } else {
        buf[32 - bytes.len()..].copy_from_slice(&bytes);
    }
    let raw = U256::from_be_bytes::<32>(buf);
    let raw_u128: u128 = raw
        .try_into()
        .map_err(|_| "余额超出 uint128 范围".to_string())?;
    Ok(raw_u128 as f64 / 1_000_000.0)
}

// 左填充 address 到 32 字节（ABI encode）。
trait LeftPadBytes {
    fn left_pad_bytes<const N: usize>(&self) -> [u8; N];
}
impl LeftPadBytes for Address {
    fn left_pad_bytes<const N: usize>(&self) -> [u8; N] {
        let mut b = [0u8; N];
        b[N - 20..].copy_from_slice(self.as_slice());
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_of_calldata_selector_and_padding() {
        let who: Address = "0x13146Acc5fC55Ca91F5d7bA16cf2f16F86f4532D"
            .parse()
            .unwrap();
        let cd = balance_of_calldata(who);
        assert!(cd.starts_with("0x70a08231"));
        // 4 + 32 = 36 字节 = 72 hex + 0x
        assert_eq!(cd.len(), 2 + 72);
        // 地址左填充 12 字节零后右对齐在末 40 hex（20 字节）
        let addr_hex = &cd[cd.len() - 40..];
        assert_eq!(
            addr_hex.to_lowercase(),
            "13146acc5fc55ca91f5d7ba16cf2f16f86f4532d"
        );
        // 前 24 hex（12 字节）应为零
        assert_eq!(&cd[2 + 8..2 + 8 + 24], "000000000000000000000000");
    }

    #[test]
    fn parse_hex_result_zero() {
        // 模拟 RPC 返回全零
        let hex = "0x0000000000000000000000000000000000000000000000000000000000000000";
        let bytes = hex::decode(hex.trim_start_matches("0x")).unwrap();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes[bytes.len() - 32..]);
        let raw = U256::from_be_bytes::<32>(buf);
        assert!(raw.is_zero());
    }

    #[test]
    fn parse_hex_result_seven_usd() {
        // 7.0 pUSD = 7_000_000 raw = 0x6ACFC0
        let hex = "0x00000000000000000000000000000000000000000000000000000000006acfc0";
        let bytes = hex::decode(hex.trim_start_matches("0x")).unwrap();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes[bytes.len() - 32..]);
        let raw_u128: u128 = U256::from_be_bytes::<32>(buf).try_into().unwrap();
        assert_eq!(raw_u128, 7_000_000);
        assert!((raw_u128 as f64 / 1_000_000.0 - 7.0).abs() < 1e-9);
    }

    #[test]
    fn parse_short_hex_result_right_aligned() {
        // 短返回（< 32 字节）应右对齐补零
        let hex = "0x6acfc0";
        let bytes = hex::decode(hex.trim_start_matches("0x")).unwrap();
        let mut buf = [0u8; 32];
        buf[32 - bytes.len()..].copy_from_slice(&bytes);
        let raw_u128: u128 = U256::from_be_bytes::<32>(buf).try_into().unwrap();
        assert_eq!(raw_u128, 7_000_000);
    }

    #[test]
    fn parse_bytes32_hex_round_trip() {
        let s = "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c";
        let b = parse_bytes32_hex(s);
        let expect = hex::decode(s.trim_start_matches("0x")).unwrap();
        assert_eq!(&b[..], expect.as_slice());
    }

    #[test]
    fn ctf_position_id_yes_no_differ_and_stable() {
        // YES(indexSet=2) 与 NO(indexSet=1) 的 positionId 应不同且稳定。
        let pusd: Address = "0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB"
            .parse()
            .unwrap();
        let cond = "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c";
        let yes = ctf_position_id(pusd, cond, 2);
        let no = ctf_position_id(pusd, cond, 1);
        assert_ne!(yes, no);
        assert!(!yes.is_zero());
        assert!(!no.is_zero());
        // 稳定性：同输入同输出
        assert_eq!(ctf_position_id(pusd, cond, 2), yes);
    }

    #[test]
    fn erc1155_balance_of_calldata_shape() {
        let who: Address = "0x13146Acc5fC55Ca91F5d7bA16cf2f16F86f4532D"
            .parse()
            .unwrap();
        let id = U256::from(12345u64);
        let cd = erc1155_balance_of_calldata(who, id);
        assert!(cd.starts_with("0x00fdd58e"));
        // 4 + 32 + 32 = 68 字节 = 136 hex + 0x
        assert_eq!(cd.len(), 2 + 136);
    }

    #[test]
    fn parse_hex_u64_and_receipt_success() {
        assert_eq!(parse_hex_u64("0x10"), Some(16));
        assert_eq!(parse_hex_u64("0x0"), Some(0));
        let ok = TxReceipt {
            status: Some("0x1".into()),
            block_number: Some("0xff".into()),
            logs: vec![],
        };
        assert!(ok.is_success());
        assert_eq!(ok.block_number_u64(), Some(255));
        let bad = TxReceipt {
            status: Some("0x0".into()),
            block_number: None,
            logs: vec![],
        };
        assert!(!bad.is_success());
    }
}
