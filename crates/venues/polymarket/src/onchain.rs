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

/// `eth_call` 读余额的超时（秒）。短超时防阻塞 portfolio 页面。
const RPC_TIMEOUT_SECS: u64 = 5;

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
    let url = rpc_url.trim_end_matches('/');
    let data = balance_of_calldata(deposit_wallet);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": collateral.to_string(), "data": data },
            "latest"
        ]
    });
    // 直连公共 RPC；不走 POLYMARKET_HTTP_PROXY（避免 Clash 未开时余额全挂）。
    let http = build_rpc_http_client(std::time::Duration::from_secs(RPC_TIMEOUT_SECS));
    let resp = http
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC eth_call 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "RPC eth_call HTTP {status}: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    let parsed: RpcResp = serde_json::from_str(&text).map_err(|e| {
        format!(
            "RPC 响应解析失败: {e}（原文: {}）",
            text.chars().take(200).collect::<String>()
        )
    })?;
    if let Some(err) = parsed.error {
        return Err(format!(
            "RPC error: {}",
            err.message.unwrap_or_else(|| "unknown".into())
        ));
    }
    let hex_result = parsed
        .result
        .ok_or_else(|| "RPC 响应缺 result".to_string())?;
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
    let url = rpc_url.trim_end_matches('/');
    let data = erc1155_balance_of_calldata(deposit_wallet, position_id);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": ctf.to_string(), "data": data },
            "latest"
        ]
    });
    let http = build_rpc_http_client(std::time::Duration::from_secs(RPC_TIMEOUT_SECS));
    let resp = http
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC eth_call 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "RPC eth_call HTTP {status}: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    let parsed: RpcResp = serde_json::from_str(&text).map_err(|e| {
        format!(
            "RPC 响应解析失败: {e}（原文: {}）",
            text.chars().take(200).collect::<String>()
        )
    })?;
    if let Some(err) = parsed.error {
        return Err(format!(
            "RPC error: {}",
            err.message.unwrap_or_else(|| "unknown".into())
        ));
    }
    let hex_result = parsed
        .result
        .ok_or_else(|| "RPC 响应缺 result".to_string())?;
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
    // CTF token 1:1 collateral（pUSD，6 decimals）。
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
}
