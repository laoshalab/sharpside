//! Deposit Wallet `WALLET` batch approve：构造 approve calldata + owner EIP-712 `Batch` 签名。
//!
//! 对齐 `docs/CHANNEL_A_SIGNING.md` §3.1 step 7 与 Polymarket Deposit Wallet Guide：
//! - `WALLET` batch 用**普通 65 字节 EIP-712 签名**（非 ERC-7739 wrap）over `DepositWallet` `Batch` 类型。
//! - domain = `{name:"DepositWallet", version:"1", chainId:137, verifyingContract: depositWallet}`。
//! - types = `Call[{target,value,data}]`, `Batch[{wallet,nonce,deadline,calls:Call[]}]`。
//! - approve 必须从 deposit wallet 发起（owner EOA 的 approve 不算），故走 relayer `WALLET` batch。

#![forbid(unsafe_code)]

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, Eip712Domain, SolStruct};

/// Polygon mainnet（137）合约地址（取自官方 `@polymarket/clob-client-v2` config）。
pub mod contracts {
    /// pUSD（collateral，ERC-20）。
    pub const COLLATERAL: &str = "0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB";
    /// Conditional Tokens（ERC-1155）。
    pub const CONDITIONAL_TOKENS: &str = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";
    /// 标准（非 neg-risk）CTF Exchange V2（= clob-auth `EXCHANGE_STANDARD`）。
    pub const CTF_EXCHANGE_V2: &str = "0xE111180000d2663C0091e4f400237545B87B996B";
    /// neg-risk CTF Exchange V2（= clob-auth `EXCHANGE_NEG_RISK`）。
    pub const NEGRISK_EXCHANGE_V2: &str = "0xe2222d279d744050d28e00520010520000310F59";
    /// neg-risk adapter（pUSD approve 用）。
    pub const NEGRISK_ADAPTER: &str = "0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296";
}

const CHAIN_ID: u64 = 137;
const DEPOSIT_WALLET_NAME: &str = "DepositWallet";
const DEPOSIT_WALLET_VERSION: &str = "1";

/// ERC-20 `approve(address spender, uint256 amount)` selector = `0x095ea7b3`。
const SELECTOR_APPROVE: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];
/// ERC-1155 `setApprovalForAll(address operator, bool approved)` selector = `0xa22cb465`。
const SELECTOR_SET_APPROVAL_FOR_ALL: [u8; 4] = [0xa2, 0x2c, 0xb4, 0x65];
/// ERC-20 `transfer(address to, uint256 amount)` selector = `0xa9059cbb`。
const SELECTOR_TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

sol! {
    /// `DepositWallet` batch 单笔调用。
    struct Call {
        address target;
        uint256 value;
        bytes data;
    }
    /// `DepositWallet` batch EIP-712 主类型。
    struct Batch {
        address wallet;
        uint256 nonce;
        uint256 deadline;
        Call[] calls;
    }
}

/// 一笔 wallet call（target/value/data），便于构造与序列化。
#[derive(Debug, Clone)]
pub struct WalletCall {
    pub target: Address,
    pub value: U256,
    pub data: Bytes,
}

impl WalletCall {
    fn to_sol(&self) -> Call {
        Call {
            target: self.target,
            value: self.value,
            data: self.data.clone(),
        }
    }
}

fn addr(s: &str) -> Address {
    s.parse::<Address>()
        .expect("invalid contract address constant")
}

/// ERC-20 `approve(spender, amount)` calldata。
pub fn approve_calldata(spender: Address, amount: U256) -> Bytes {
    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&SELECTOR_APPROVE);
    out.extend_from_slice(&spender.left_pad_32()[..]);
    out.extend_from_slice(&amount.left_pad_32()[..]);
    Bytes::from(out)
}

/// ERC-20 `transfer(to, amount)` calldata。提现用：从 deposit wallet 转出 pUSD 到外部地址。
pub fn transfer_calldata(to: Address, amount: U256) -> Bytes {
    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&SELECTOR_TRANSFER);
    out.extend_from_slice(&to.left_pad_32()[..]);
    out.extend_from_slice(&amount.left_pad_32()[..]);
    Bytes::from(out)
}

/// ERC-1155 `setApprovalForAll(operator, approved)` calldata。
pub fn set_approval_for_all_calldata(operator: Address, approved: bool) -> Bytes {
    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&SELECTOR_SET_APPROVAL_FOR_ALL);
    out.extend_from_slice(&operator.left_pad_32()[..]);
    // bool 右对齐 32 字节
    let mut b = [0u8; 32];
    b[31] = if approved { 1 } else { 0 };
    out.extend_from_slice(&b);
    Bytes::from(out)
}

/// CTF `redeemPositions` selector = `0x01b7037c`（标准市场赎回）。
/// 函数签名：`redeemPositions(address,bytes32,bytes32,uint256[])`。
/// = keccak256("redeemPositions(address,bytes32,bytes32,uint256[])")[..4]。
const SELECTOR_REDEEM_POSITIONS: [u8; 4] = [0x01, 0xb7, 0x03, 0x7c];

/// CTF `redeemPositions(address collateralToken, bytes32 parentCollectionId,
/// bytes32 conditionId, uint256[] indexSets)` calldata。
///
/// 标准市场赎回：把已结算市场的赢仓位 CTF token 换回 pUSD。
/// 对应 `docs/CHANNEL_A_SIGNING.md` §4.2 与 Polymarket 官方 `conditional-token-examples/redeem.ts`。
///
/// - `collateral`：pUSD 地址（`contracts::COLLATERAL`）。
/// - `condition_id`：市场 conditionId（0x 前缀 hex，bytes32）。
/// - `index_sets`：二元市场固定 `[1, 2]`（NO=1, YES=2）。合约自动烧输方 token、
///   按赢方余额 1:1 付 pUSD；余额为 0 的那边自动忽略，故一次调用赎两边。
///
/// ABI 编码（含动态数组参数）：
///   selector | addr | bytes32 | bytes32 | offset(0x80) | len | elem0 | elem1
pub fn redeem_positions_calldata(
    collateral: Address,
    condition_id: &str,
    index_sets: &[u64],
) -> Bytes {
    let cond = parse_bytes32(condition_id);
    let mut out = Vec::with_capacity(4 + 4 * 32 + 32 + index_sets.len() * 32);
    out.extend_from_slice(&SELECTOR_REDEEM_POSITIONS);
    // arg0: collateralToken（address，左填充 32）
    out.extend_from_slice(&collateral.left_pad_32()[..]);
    // arg1: parentCollectionId（bytes32 全 0）
    out.extend_from_slice(&[0u8; 32]);
    // arg2: conditionId（bytes32）
    out.extend_from_slice(&cond);
    // arg3: offset 到 indexSets 数据 = 4*32 = 128 = 0x80
    let offset = U256::from(4u64 * 32u64);
    out.extend_from_slice(&offset.left_pad_32()[..]);
    // 动态数据：数组长度 + 各元素
    out.extend_from_slice(&U256::from(index_sets.len()).left_pad_32()[..]);
    for s in index_sets {
        out.extend_from_slice(&U256::from(*s).left_pad_32()[..]);
    }
    Bytes::from(out)
}

/// 解析 0x 前缀 hex conditionId 为 bytes32。长度不足/超长返回全 0（链上会拒，由调用方兜底）。
fn parse_bytes32(s: &str) -> [u8; 32] {
    let hex_s = s.trim().trim_start_matches("0x");
    let bytes = match hex::decode(hex_s) {
        Ok(b) => b,
        Err(_) => return [0u8; 32],
    };
    let mut out = [0u8; 32];
    if bytes.len() == 32 {
        out.copy_from_slice(&bytes);
    } else if bytes.len() < 32 {
        // 右对齐（conditionId 是高位有意义）
        out[32 - bytes.len()..].copy_from_slice(&bytes);
    }
    // 长度 > 32：取末 32 字节（异常输入兜底）
    if bytes.len() > 32 {
        out.copy_from_slice(&bytes[bytes.len() - 32..]);
    }
    out
}

/// 构造交易所需的全套 approve calls（DW 作为发起方）：
/// - pUSD → CTF Exchange V2 / NegRisk Exchange V2 / NegRisk Adapter（ERC-20 approve，max）
/// - Conditional Tokens → CTF Exchange V2 / NegRisk Exchange V2（ERC-1155 setApprovalForAll）
///
/// 覆盖标准与 neg-risk 两类市场；重复 approve 幂等，可多次提交。
pub fn trading_approves() -> Vec<WalletCall> {
    let max = U256::MAX;
    let pusd = addr(contracts::COLLATERAL);
    let ct = addr(contracts::CONDITIONAL_TOKENS);
    let ex = addr(contracts::CTF_EXCHANGE_V2);
    let nrex = addr(contracts::NEGRISK_EXCHANGE_V2);
    let nrad = addr(contracts::NEGRISK_ADAPTER);
    vec![
        WalletCall {
            target: pusd,
            value: U256::ZERO,
            data: approve_calldata(ex, max),
        },
        WalletCall {
            target: pusd,
            value: U256::ZERO,
            data: approve_calldata(nrex, max),
        },
        WalletCall {
            target: pusd,
            value: U256::ZERO,
            data: approve_calldata(nrad, max),
        },
        WalletCall {
            target: ct,
            value: U256::ZERO,
            data: set_approval_for_all_calldata(ex, true),
        },
        WalletCall {
            target: ct,
            value: U256::ZERO,
            data: set_approval_for_all_calldata(nrex, true),
        },
    ]
}

fn deposit_wallet_domain(deposit_wallet: Address) -> Eip712Domain {
    Eip712Domain::new(
        Some(DEPOSIT_WALLET_NAME.into()),
        Some(DEPOSIT_WALLET_VERSION.into()),
        Some(U256::from(CHAIN_ID)),
        Some(deposit_wallet),
        None,
    )
}

/// EIP-712 digest = `keccak256("\x19\x01" ‖ domainSeparator ‖ structHash(Batch))`。
pub fn batch_digest(
    deposit_wallet: Address,
    nonce: U256,
    deadline: U256,
    calls: &[WalletCall],
) -> B256 {
    let domain = deposit_wallet_domain(deposit_wallet);
    let sol_calls: Vec<Call> = calls.iter().map(WalletCall::to_sol).collect();
    let batch = Batch {
        wallet: deposit_wallet,
        nonce,
        deadline,
        calls: sol_calls,
    };
    let sep = domain.separator();
    let type_hash = batch.eip712_type_hash();
    let enc = batch.eip712_encode_data();
    let mut buf = Vec::with_capacity(32 + enc.len());
    buf.extend_from_slice(type_hash.as_slice());
    buf.extend_from_slice(&enc);
    let struct_hash = keccak256(&buf);
    let mut digest = [0u8; 2 + 32 + 32];
    digest[0] = 0x19;
    digest[1] = 0x01;
    digest[2..34].copy_from_slice(sep.as_slice());
    digest[34..66].copy_from_slice(struct_hash.as_slice());
    keccak256(digest)
}

/// owner EOA 对 `Batch` 签 EIP-712 → 65 字节 sig（`0x` 前缀 hex）。
pub fn sign_wallet_batch(
    signer: &PrivateKeySigner,
    deposit_wallet: Address,
    nonce: U256,
    deadline: U256,
    calls: &[WalletCall],
) -> Result<String, String> {
    let digest = batch_digest(deposit_wallet, nonce, deadline, calls);
    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|e| format!("wallet batch 签名失败: {e}"))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// 从 65 字节 sig 恢复 signer 地址（验签用）。
pub fn recover_wallet_batch_signer(
    signature_hex: &str,
    deposit_wallet: Address,
    nonce: U256,
    deadline: U256,
    calls: &[WalletCall],
) -> Result<Address, String> {
    let digest = batch_digest(deposit_wallet, nonce, deadline, calls);
    let hex_s = signature_hex.trim().trim_start_matches("0x");
    let bytes = hex::decode(hex_s).map_err(|e| format!("bad sig hex: {e}"))?;
    let sig = alloy_primitives::Signature::try_from(bytes.as_slice())
        .map_err(|e| format!("bad sig: {e}"))?;
    sig.recover_address_from_prehash(&digest)
        .map_err(|e| format!("recover failed: {e}"))
}

// 左填充 address / U256 到 32 字节（ABI encode）。
trait LeftPad32 {
    fn left_pad_32(&self) -> [u8; 32];
}
impl LeftPad32 for Address {
    fn left_pad_32(&self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[12..].copy_from_slice(self.as_slice());
        b
    }
}
impl LeftPad32 for U256 {
    fn left_pad_32(&self) -> [u8; 32] {
        self.to_be_bytes::<32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_addr() -> Address {
        "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap()
    }

    #[test]
    fn approve_calldata_selector_and_args() {
        let spender = test_addr();
        let cd = approve_calldata(spender, U256::MAX);
        assert_eq!(&cd[..4], &SELECTOR_APPROVE);
        assert_eq!(cd.len(), 4 + 32 + 32);
    }

    #[test]
    fn transfer_calldata_selector_and_args() {
        let to = test_addr();
        let cd = transfer_calldata(to, U256::from(1_000_000u64));
        assert_eq!(&cd[..4], &SELECTOR_TRANSFER);
        assert_eq!(cd.len(), 4 + 32 + 32);
        // amount 在末 32 字节，1_000_000 = 0xF4240
        let amt_be = &cd[cd.len() - 32..];
        assert_eq!(U256::from_be_slice(amt_be), U256::from(1_000_000u64));
    }

    #[test]
    fn set_approval_for_all_calldata_shape() {
        let op = test_addr();
        let cd = set_approval_for_all_calldata(op, true);
        assert_eq!(&cd[..4], &SELECTOR_SET_APPROVAL_FOR_ALL);
        assert_eq!(cd.len(), 4 + 32 + 32);
        assert_eq!(cd[cd.len() - 1], 1);
    }

    #[test]
    fn trading_approves_covers_standard_and_negrisk() {
        let calls = trading_approves();
        assert_eq!(calls.len(), 5);
        assert!(calls.iter().all(|c| c.value == U256::ZERO));
    }

    #[test]
    fn redeem_positions_calldata_shape_and_encoding() {
        // 标准市场赎回 calldata：redeemPositions(pUSD, 0, conditionId, [1,2])。
        let pusd = addr(contracts::COLLATERAL);
        let cond = "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c";
        let cd = redeem_positions_calldata(pusd, cond, &[1, 2]);
        // selector
        assert_eq!(&cd[..4], &SELECTOR_REDEEM_POSITIONS);
        // 总长 = 4 + 4*32 (args+offset) + 32 (len) + 2*32 (elems) = 228
        assert_eq!(cd.len(), 4 + 4 * 32 + 32 + 2 * 32);
        // arg0: collateralToken 左填充 32（末 20 字节 = pUSD 地址）
        assert_eq!(&cd[4 + 12..4 + 32], pusd.as_slice());
        // arg1: parentCollectionId 全 0
        assert_eq!(&cd[4 + 32..4 + 64], &[0u8; 32]);
        // arg2: conditionId（bytes32，原样）
        let cond_bytes = hex::decode(cond.trim_start_matches("0x")).unwrap();
        assert_eq!(&cd[4 + 64..4 + 96], cond_bytes.as_slice());
        // arg3: offset = 0x80 = 128
        assert_eq!(
            U256::from_be_slice(&cd[4 + 96..4 + 128]),
            U256::from(128u64)
        );
        // 数组长度 = 2
        assert_eq!(U256::from_be_slice(&cd[4 + 128..4 + 160]), U256::from(2u64));
        // elem0 = 1, elem1 = 2
        assert_eq!(U256::from_be_slice(&cd[4 + 160..4 + 192]), U256::from(1u64));
        assert_eq!(U256::from_be_slice(&cd[4 + 192..4 + 224]), U256::from(2u64));
    }

    #[test]
    fn redeem_positions_calldata_bad_condition_id_zeros() {
        // 非法 conditionId → bytes32 全 0（链上会拒，由调用方兜底校验）。
        let pusd = addr(contracts::COLLATERAL);
        let cd = redeem_positions_calldata(pusd, "not-hex", &[1, 2]);
        assert_eq!(&cd[4 + 64..4 + 96], &[0u8; 32]);
    }

    #[test]
    fn parse_bytes32_round_trip() {
        let s = "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c";
        let b = parse_bytes32(s);
        let expect = hex::decode(s.trim_start_matches("0x")).unwrap();
        assert_eq!(&b[..], expect.as_slice());
        // 无 0x 前缀也应解析
        assert_eq!(parse_bytes32(&s[2..]), b);
    }

    #[test]
    fn batch_digest_stable_and_recovers_to_owner() {
        let signer = PrivateKeySigner::from_str(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let dw = test_addr();
        let calls = trading_approves();
        let nonce = U256::from(0u64);
        let deadline = U256::from(1760000000u64);
        let sig = sign_wallet_batch(&signer, dw, nonce, deadline, &calls).unwrap();
        let recovered = recover_wallet_batch_signer(&sig, dw, nonce, deadline, &calls).unwrap();
        assert_eq!(recovered, signer.address());
    }

    #[test]
    fn batch_digest_changes_with_nonce() {
        let dw = test_addr();
        let calls = trading_approves();
        let d0 = batch_digest(dw, U256::from(0u64), U256::from(1760000000u64), &calls);
        let d1 = batch_digest(dw, U256::from(1u64), U256::from(1760000000u64), &calls);
        assert_ne!(d0, d1);
    }
}
