//! Deposit Wallet (POLY_1271) 专用：CREATE2 地址派生。
//!
//! 对应 `docs/CHANNEL_A_SIGNING.md` §3 / §7「deriveDepositWalletAddress Rust 实现」。
//!
//! ## ERC-7739 包装签名
//!
//! ERC-7739-wrapped POLY_1271 签名（owner EOA 签嵌套 `TypedDataSign` under CTF Exchange domain，
//! wire 签名为 `innerSig || appDomainSep || contentsHash || OrderTypeString || u16be(len)`）
//! 已移至独立 crate [`sharpside_clob_auth`]（`sign_poly_1271_order` / `sign_poly_1271_order_with_signer`），
//! 对齐官方 `@polymarket/clob-client-v2@1.1.0`，经 golden vector 字节级验证。本模块不再含签名逻辑。
//!
//! ## CREATE2 地址派生
//!
//! init code hash 从 beacon 地址按 Solady `LibClone.initCodeHashERC1967Beacon` 计算（非固定常量）；
//! salt = `keccak256(abi.encode(factory, bytes32(owner)))`。Polygon mainnet 走 beacon clone 路径。
//! 真实链上 canary 向量：owner `0x7b51...1a8a` → `0xa7a8...3711`（见 [`derive_deposit_wallet_address`] 测试）。

#![forbid(unsafe_code)]

use alloy_primitives::{address, keccak256, Address, B256};

/// Polymarket deposit wallet 工厂（CREATE2 部署 deposit wallet 的工厂合约）。
/// 见 `docs/CHANNEL_A_SIGNING.md` §7。
pub const DEPOSIT_WALLET_FACTORY: Address = address!("00000000000Fb5C9ADea0298D729A0CB3823Cc07");
/// Polygon mainnet Deposit Wallet beacon（`factory.beacon()` / selector `0x49493a4d`）。
/// Relayer WALLET-CREATE 当前部署此 beacon clone（非 UUPS）。
pub const DEPOSIT_WALLET_BEACON_POLYGON: Address =
    address!("7a18edfe055488a3128f01f563e5b479d92ffc3a");
/// Polygon mainnet Deposit Wallet implementation（UUPS 路径，当前未用）。
pub const DEPOSIT_WALLET_IMPLEMENTATION: Address =
    address!("58CA52ebe0DadfdF531Cde7062e76746de4Db1eB");

// Solady LibClone.initCodeHashERC1967 / ERC1967Beacon 常量（v0.1.26）。
// 移植自 ~/文档/sharpside/crates/poly-relayer/src/derive.rs。
const ERC1967_CONST1: [u8; 32] =
    hex_literal_32("cc3735a920a3ca505d382bbc545af43d6000803e6038573d6000fd5b3d6000f3");
const ERC1967_CONST2: [u8; 32] =
    hex_literal_32("5155f3363d3d373d3d363d7f360894a13ba1a3210667c828492db98dca3e2076");
const ERC1967_PREFIX: u128 = 0x6100_3d3d_8160_233d_3973;
const ERC1967_BEACON_CONST1: [u8; 32] =
    hex_literal_32("b3582b35133d50545afa5036515af43d6000803e604d573d6000fd5b3d6000f3");
const ERC1967_BEACON_CONST2: [u8; 32] =
    hex_literal_32("1b60e01b36527fa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6c");
const ERC1967_BEACON_CONST3: [u8; 23] =
    hex_literal_n("60195155f3363d3d373d3d363d602036600436635c60da");
const ERC1967_BEACON_PREFIX: u128 = 0x6100_523d_8160_233d_3973;

const fn hex_literal_32(s: &str) -> [u8; 32] {
    let bytes = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (hex_nibble(bytes[i * 2]) << 4) | hex_nibble(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn hex_literal_n(s: &str) -> [u8; 23] {
    let bytes = s.as_bytes();
    let mut out = [0u8; 23];
    let mut i = 0;
    while i < 23 {
        out[i] = (hex_nibble(bytes[i * 2]) << 4) | hex_nibble(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// `abi.encode(address factory, bytes32 walletId)`，walletId = 左补零的 owner。
fn deposit_wallet_args(owner: Address, factory: Address) -> [u8; 64] {
    let mut args = [0u8; 64];
    args[12..32].copy_from_slice(factory.as_slice());
    args[44..64].copy_from_slice(owner.as_slice());
    args
}

fn create2_address(factory: Address, salt: B256, init_code_hash: B256) -> Address {
    let mut buf = Vec::with_capacity(85);
    buf.push(0xff);
    buf.extend_from_slice(factory.as_slice());
    buf.extend_from_slice(salt.as_slice());
    buf.extend_from_slice(init_code_hash.as_slice());
    Address::from_slice(&keccak256(&buf)[12..])
}

/// viem: `toHex(PREFIX + (n << 56n), { size: 10 })`。
fn prefix10(base: u128, args_len: u64) -> [u8; 10] {
    let combined = base + ((args_len as u128) << 56);
    let mut out = [0u8; 10];
    for (i, b) in out.iter_mut().enumerate() {
        let shift = (9 - i) * 8;
        *b = ((combined >> shift) & 0xff) as u8;
    }
    out
}

fn init_code_hash_erc1967(implementation: Address, args: &[u8]) -> B256 {
    let mut buf = Vec::with_capacity(10 + 20 + 2 + 32 + 32 + args.len());
    buf.extend_from_slice(&prefix10(ERC1967_PREFIX, args.len() as u64));
    buf.extend_from_slice(implementation.as_slice());
    buf.extend_from_slice(&[0x60, 0x09]);
    buf.extend_from_slice(&ERC1967_CONST2);
    buf.extend_from_slice(&ERC1967_CONST1);
    buf.extend_from_slice(args);
    keccak256(buf)
}

fn init_code_hash_erc1967_beacon(beacon: Address, args: &[u8]) -> B256 {
    let mut buf = Vec::with_capacity(10 + 20 + 23 + 32 + 32 + args.len());
    buf.extend_from_slice(&prefix10(ERC1967_BEACON_PREFIX, args.len() as u64));
    buf.extend_from_slice(beacon.as_slice());
    buf.extend_from_slice(&ERC1967_BEACON_CONST3);
    buf.extend_from_slice(&ERC1967_BEACON_CONST2);
    buf.extend_from_slice(&ERC1967_BEACON_CONST1);
    buf.extend_from_slice(args);
    keccak256(buf)
}

/// UUPS 路径派生（当前 Relayer 未用，保留对齐 SDK 向量）。
pub fn derive_uups_deposit_wallet(
    owner: Address,
    factory: Address,
    implementation: Address,
) -> Address {
    let args = deposit_wallet_args(owner, factory);
    let salt = keccak256(args);
    let bytecode_hash = init_code_hash_erc1967(implementation, &args);
    create2_address(factory, salt, bytecode_hash)
}

/// Beacon 路径派生（Polygon mainnet 当前 Relayer WALLET-CREATE 走此路径）。
pub fn derive_beacon_deposit_wallet(owner: Address, factory: Address, beacon: Address) -> Address {
    let args = deposit_wallet_args(owner, factory);
    let salt = keccak256(args);
    let bytecode_hash = init_code_hash_erc1967_beacon(beacon, &args);
    create2_address(factory, salt, bytecode_hash)
}

/// Polygon mainnet 默认派生（beacon clone）。对应 `derive_polygon_deposit_wallet`。
/// 不再需要 env `POLYMARKET_DEPOSIT_INIT_CODE_HASH`——init code hash 从 beacon 地址按
/// Solady ERC1967 模式计算得出。
pub fn derive_deposit_wallet_address(owner: Address) -> Address {
    derive_beacon_deposit_wallet(owner, DEPOSIT_WALLET_FACTORY, DEPOSIT_WALLET_BEACON_POLYGON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_matches_canary_dw() {
        // 真实链上 canary 向量（移植自 ~/文档/sharpside/crates/poly-relayer/src/derive.rs）。
        // owner 0x7b51...1a8a → deposit wallet 0xa7a8...3711（Polygon beacon clone 路径）。
        let owner = address!("7b51078a7723c3b116f76cb060e567681b6b1a8a");
        let expected = address!("a7a8fb7b93d31363ec82a78a3e9db04f70473711");
        assert_eq!(derive_deposit_wallet_address(owner), expected);
    }

    #[test]
    fn uups_matches_sdk_vector() {
        // UUPS 路径对齐 @polymarket/builder-relayer-client SDK 向量。
        let owner = address!("7b51078a7723c3b116f76cb060e567681b6b1a8a");
        let got = derive_uups_deposit_wallet(
            owner,
            DEPOSIT_WALLET_FACTORY,
            DEPOSIT_WALLET_IMPLEMENTATION,
        );
        assert_eq!(got, address!("eb07b64e1901aa2df94cfc2795eae3cd6bb611e7"));
    }

    #[test]
    fn create2_address_deterministic() {
        let owner = Address::ZERO;
        let a = derive_deposit_wallet_address(owner);
        let b = derive_deposit_wallet_address(owner);
        assert_eq!(a, b);
    }

    #[test]
    fn create2_address_changes_with_owner() {
        let a = derive_deposit_wallet_address(Address::ZERO);
        let b = derive_deposit_wallet_address(address!("0000000000000000000000000000000000000001"));
        assert_ne!(a, b);
    }
}
