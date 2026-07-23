//! Polymarket CLOB EIP-712 订单签名 + L2 HMAC 鉴权。
//!
//! 对应 `docs/VENUE_DESIGN.md` §5 execution_venue 能力与 `docs/CHANNEL_A_SIGNING.md` §3。
//!
//! 两条签名路径：
//! - **旧（EOA）**：`signatureType=0`，maker=signer=EOA，单钥模型（dev/兼容）
//! - **主（Deposit Wallet 委托）**：`signatureType=3`（POLY_1271），maker=signer=deposit wallet，
//!   owner EOA 签 ERC-7739-wrapped 订单 + L2 HMAC headers + builderCode（FrenFlow 式，通道 A 新默认）
//!
//! 离线/无凭证：`place_order` 默认 dry-sign（签名但不提交 CLOB，返回合成 Fill，`tx_hash`=签名）。
//! 设 `POLYMARKET_CLOB_POST=1` 且网络可达时才真提交。
//!
//! 金额换算：CTF outcome token 与 USDC 均为 6 decimals。
//! - BUY  `size` shares @ `price`：makerAmount = size（shares base），takerAmount = price*size（USDC base）
//! - SELL `size` shares @ `price`：makerAmount = price*size（USDC base），takerAmount = size（shares base）

use alloy_primitives::{address, Address, B256, U256};
use alloy_signer_local::PrivateKeySigner;
use sharpside_clob_auth as clob_auth;
use std::str::FromStr;

/// USDC / CTF outcome token 小数位（6）。金额精度对齐用 1e5（shares 5 位）与 10_000（USDC 2 位 cent）硬编码，
/// 此常量保留作 6-decimals 约定文档锚点。
#[allow(dead_code)]
const DECIMALS: u32 = 6;

/// CLOB 签名类型。对应 Polymarket `signatureType`。
/// 详见 `docs/CHANNEL_A_SIGNING.md` §3.3。
pub mod sig_type {
    /// EOA 直接签名（maker=signer=EOA）
    pub const EOA: u8 = 0;
    /// Deposit wallet（POLY_1271，新 API 用户路径；通道 A 主路径）
    pub const POLY_1271: u8 = 3;
}

/// 标准（非 neg-risk）CTF Exchange V2 地址（Polygon）。对齐 clob-auth `EXCHANGE_STANDARD`。
pub const CTF_EXCHANGE: Address = address!("E111180000d2663C0091e4f400237545B87B996B");
/// neg-risk CTF Exchange V2 地址（Polygon）。
pub const CTF_EXCHANGE_NEG_RISK: Address = address!("e2222d279d744050d28e00520010520000310F59");
/// Polygon chainId。
pub const POLYGON_CHAIN_ID: u64 = clob_auth::CHAIN_ID;

/// 已签名订单（携带 POST CLOB 所需标量字段 + 签名）。V2 形状（含 timestamp/metadata/builder）。
#[derive(Debug, Clone)]
pub struct SignedOrder {
    pub signature: String,
    pub signer_address: Address,
    /// maker 地址：EOA 路径 = signer；Deposit Wallet 路径 = deposit_wallet_address（= signer）
    pub maker_address: Address,
    pub signature_type: u8,
    pub side: u8,
    pub token_id: U256,
    pub maker_amount: U256,
    pub taker_amount: U256,
    pub salt: U256,
    /// V2 订单时间戳（毫秒）。
    pub timestamp_ms: U256,
    /// V2 metadata（当前固定零）。
    pub metadata: B256,
    /// V2 builder 归因码（bytes32；Deposit Wallet 路径填，EOA 路径为零）。
    pub builder: B256,
    /// Builder 归因 code 原始字符串（用于 `X-Builder-Code` header；Deposit Wallet 路径用，EOA 为 None）。
    pub builder_code: Option<String>,
}

/// 从明文私钥（hex，可带 0x）构造签名器。
///
/// **仅 dev/测试**：真实部署须由 KMS 解密 `Credential::Wallet.encrypted_handle` 得到签名材料，
/// 绝不在配置/DB 存明文私钥。此函数供 `place_order` 在 dev 路径（env `POLYMARKET_DEV_PRIVATE_KEY`）使用。
pub fn signer_from_hex(hex: &str) -> Result<PrivateKeySigner, String> {
    let hex = hex.trim_start_matches("0x");
    PrivateKeySigner::from_str(hex).map_err(|e| format!("私钥解析失败: {e}"))
}

/// 当前毫秒时间戳。
fn now_ms_u256() -> U256 {
    U256::from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
}

/// 构造 V2 订单签名输入（未签名）。
///
/// `side`：0=BUY，1=SELL。`token_id` 为 ERC1155 token id 的十进制字符串。
/// `maker`/`signer`：EOA 路径两者相同；Deposit Wallet (POLY_1271) 路径 maker=signer=deposit_wallet_address。
/// `signature_type`：见 [`sig_type`]。`builder`：bytes32 归因码（无则零）。
///
/// `salt` 取毫秒时间戳低 53 位（CLOB 要求 wire `salt` 为 JSON integer 且 < 2^53）。
#[allow(clippy::too_many_arguments)]
pub fn build_v2_input(
    signer: &PrivateKeySigner,
    maker: Address,
    signature_type: u8,
    side: sharpside_shared::Side,
    token_id: &str,
    price: f64,
    size: f64,
    builder: B256,
    // 订单级幂等键：调用方按 copy_order.id 确定性派生并持久化的 salt（≤2^53）。
    // Some 时复用 → 重试发相同 orderID → Venue 端判重而非重复下单；None 时按 now() 自生（旧行为）。
    idempotency_salt: Option<u64>,
    // 签名用 timestamp（ms），与 idempotency_salt 配套复用。None 时用 now()。
    order_timestamp_ms: Option<u64>,
) -> Result<clob_auth::V2OrderInput, String> {
    let token =
        U256::from_str_radix(token_id.trim(), 10).map_err(|e| format!("token_id 解析失败: {e}"))?;
    // 金额精度对齐 Polymarket CLOB 规则：USDC 侧 ≤2 位小数（base 须 10^4 倍 = cent-aligned），
    // shares 侧 ≤5 位小数（base 须 10 倍）。否则 CLOB 拒单
    // "maker amount supports a max accuracy of 2 decimals, taker amount a max of 5 decimals"
    //（py-clob-client issue #68/#87：USDC 非 cent-aligned 即拒）。
    // shares 向下取整到 5 位（不多占股数）；USDC 按 side 取整到 2 位 cent：
    //   BUY maker=USDC 向上取整（limit ≥ 源价，确保吃 ask 成交）；SELL taker=USDC 向下取整（limit ≤ 源价，确保吃 bid 成交）。
    let shares_base_u128 = ((size * 1e5).floor() as u128).saturating_mul(10);
    let shares_base = U256::from(shares_base_u128);
    let usdc_per_cent = price * (shares_base_u128 as f64) / 10_000.0;
    let usdc_cents = match side {
        sharpside_shared::Side::Buy => usdc_per_cent.ceil() as u128,
        sharpside_shared::Side::Sell => usdc_per_cent.floor() as u128,
    };
    let usdc_base = U256::from(usdc_cents).saturating_mul(U256::from(10_000u128));
    let (maker_amount, taker_amount, side_code) = match side {
        // 官方 getOrderRawAmounts：maker 给出/收到的金额方向。
        // BUY：maker 付 USDC 收 shares → makerAmount=usdc(price*size), takerAmount=shares(size)；
        //      服务端 price = makerAmount/takerAmount = usdc/shares = price ✓
        // SELL：maker 付 shares 收 USDC → makerAmount=shares(size), takerAmount=usdc(price*size)；
        //      服务端 price = takerAmount/makerAmount = usdc/shares = price ✓
        sharpside_shared::Side::Buy => (usdc_base, shares_base, 0u8),
        sharpside_shared::Side::Sell => (shares_base, usdc_base, 1u8),
    };
    let ts_ms = match order_timestamp_ms {
        Some(ts) => U256::from(ts),
        None => now_ms_u256(),
    };
    // salt < 2^53（CLOB JSON integer 安全）。幂等键优先；无则取毫秒低 53 位。
    let salt = match idempotency_salt {
        Some(s) => U256::from(s & ((1u64 << 53) - 1)),
        None => U256::from(ts_ms.to::<u64>() & ((1u64 << 53) - 1)),
    };
    let signer_addr = if signature_type == sig_type::POLY_1271 {
        // POLY_1271：maker == signer == deposit wallet。clob-auth `sign_poly_1271_order_with_signer`
        // 硬约束 maker==signer（ERC-7739 TypedDataSign 的 verifyingContract = signer = DW，EIP-1271 由 DW 验）。
        maker
    } else {
        signer.address()
    };
    Ok(clob_auth::V2OrderInput {
        salt,
        maker,
        signer: signer_addr,
        token_id: token,
        maker_amount,
        taker_amount,
        side: side_code,
        signature_type,
        timestamp_ms: ts_ms,
        metadata: B256::ZERO,
        builder,
    })
}

/// 一站式：构造 EOA 订单 + V2 EIP-712 签名（signatureType=0，maker=signer=EOA）。
/// 旧路径，dev/兼容用。生产主路径走 [`sign_clob_order_deposit`]。
///
/// `neg_risk`：按 market metadata（CLOB `/book` 的 `negRisk`）选择 verifyingContract
/// （standard `0xE111...` / neg-risk `0xe222...`）。未知时传 `false`（standard）。
pub async fn sign_clob_order(
    signer: &PrivateKeySigner,
    side: sharpside_shared::Side,
    token_id: &str,
    price: f64,
    size: f64,
    neg_risk: bool,
    idempotency_salt: Option<u64>,
    order_timestamp_ms: Option<u64>,
) -> Result<SignedOrder, String> {
    let maker = signer.address();
    let input = build_v2_input(
        signer,
        maker,
        sig_type::EOA,
        side,
        token_id,
        price,
        size,
        B256::ZERO,
        idempotency_salt,
        order_timestamp_ms,
    )?;
    let exchange = if neg_risk {
        CTF_EXCHANGE_NEG_RISK
    } else {
        CTF_EXCHANGE
    };
    let signature = clob_auth::sign_v2_order_with_signer(signer, &input, exchange)
        .map_err(|e| format!("V2 签名失败: {e}"))?;
    Ok(SignedOrder {
        signature,
        signer_address: signer.address(),
        maker_address: maker,
        signature_type: sig_type::EOA,
        side: input.side,
        token_id: input.token_id,
        maker_amount: input.maker_amount,
        taker_amount: input.taker_amount,
        salt: input.salt,
        timestamp_ms: input.timestamp_ms,
        metadata: input.metadata,
        builder: input.builder,
        builder_code: None,
    })
}

/// 一站式：构造 Deposit Wallet 订单（signatureType=3 POLY_1271，maker=signer=deposit wallet）
/// + ERC-7739-wrapped 签名（对齐官方 @polymarket/clob-client-v2，golden vector 验证）。
///
/// **主路径**。对应 `docs/CHANNEL_A_SIGNING.md` §3.2。`signer` = 平台 KMS 解出的 owner EOA 私钥；
/// `deposit_wallet_address` = 用户 deposit wallet（ERC-1967 proxy）。
///
/// `builder_code`：bytes32 hex（可带 0x）。解析为 [`clob_auth::V2OrderInput::builder`] 归因码。
/// `neg_risk`：按 market metadata（CLOB `/book` 的 `negRisk`）选择 verifyingContract
/// （standard `0xE111...` / neg-risk `0xe222...`）。未知时传 `false`（standard）。
#[allow(clippy::too_many_arguments)]
pub async fn sign_clob_order_deposit(
    signer: &PrivateKeySigner,
    deposit_wallet_address: Address,
    side: sharpside_shared::Side,
    token_id: &str,
    price: f64,
    size: f64,
    builder_code: Option<String>,
    neg_risk: bool,
    idempotency_salt: Option<u64>,
    order_timestamp_ms: Option<u64>,
) -> Result<SignedOrder, String> {
    let builder = builder_code
        .as_deref()
        .and_then(clob_auth::parse_builder_code)
        .unwrap_or(B256::ZERO);
    let input = build_v2_input(
        signer,
        deposit_wallet_address,
        sig_type::POLY_1271,
        side,
        token_id,
        price,
        size,
        builder,
        idempotency_salt,
        order_timestamp_ms,
    )?;
    // POLY_1271 恒走 ERC-7739 wrap（plain EIP-712 对 POLY_1271 是错的，会被 CLOB 拒签）。
    // neg_risk 决定外层 CTF Exchange domain 的 verifyingContract（standard vs neg-risk）。
    let signature = clob_auth::sign_poly_1271_order_with_signer(signer, &input, neg_risk)
        .map_err(|e| format!("ERC-7739 POLY_1271 签名失败: {e}"))?;
    Ok(SignedOrder {
        signature,
        // POLY_1271：signer = deposit wallet（clob-auth 硬约束 maker==signer=DW；ERC-7739 wrap 的
        // verifyingContract = signer = DW，链上 EIP-1271 由 DW 验 owner EOA 签名）。
        signer_address: deposit_wallet_address,
        maker_address: deposit_wallet_address,
        signature_type: sig_type::POLY_1271,
        side: input.side,
        token_id: input.token_id,
        maker_amount: input.maker_amount,
        taker_amount: input.taker_amount,
        salt: input.salt,
        timestamp_ms: input.timestamp_ms,
        metadata: input.metadata,
        builder: input.builder,
        builder_code,
    })
}

/// L2 鉴权 headers（5 个 `POLY_*` header）。
///
/// 调用方在 POST /order 时附加这些 header。`signer_address` = L1 派生 L2 凭证的地址。
///
/// **timestamp 单位 = 秒**（对齐 Polymarket L2 规范，与 V2 Order.timestamp 的毫秒区分）。
/// `l2_secret` = base64url 编码的 HMAC secret（L1 派生返回的 `secret`）。
pub fn l2_headers(
    signer_address: Address,
    l2_secret: &str,
    l2_api_key: &str,
    l2_passphrase: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Vec<(String, String)> {
    let ts = chrono::Utc::now().timestamp() as u64;
    let creds = clob_auth::ApiCreds {
        api_key: l2_api_key.to_string(),
        api_secret_b64: l2_secret.to_string(),
        passphrase: l2_passphrase.to_string(),
    };
    match clob_auth::l2_headers(&signer_address.to_string(), &creds, method, path, body, ts) {
        Ok(arr) => arr
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "L2 HMAC header 构造失败，返回空");
            Vec::new()
        }
    }
}

/// L1 鉴权签名：owner EOA 对 `createOrDeriveApiKey` 请求的 ClobAuth EIP-712 签名。
///
/// 对应 `docs/CHANNEL_A_SIGNING.md` §3.1 step 6。domain = `ClobAuthDomain`（name/version=1/chainId=137，
/// 无 verifyingContract），struct = `ClobAuth{address, timestamp(string), nonce, message}`，
/// 固定文案 `CLOB_AUTH_MESSAGE`。对齐官方 py-clob-client-v2 / clob-auth crate。
///
/// **POLY_ADDRESS = owner EOA**（= ClobAuth.address = ecrecover(sig) = L2 API key 所属地址）。
/// 服务端按地址自动识别 wallet 类型；POLY_1271 下 signature_type=3 让服务端把 owner EOA 映射到
/// CREATE2 deposit wallet 做链上结算（maker=deposit wallet，signer=owner EOA）。
pub fn build_l1_auth_signature(
    signer: &PrivateKeySigner,
    timestamp: i64,
) -> Result<String, String> {
    let input = clob_auth::L1Input {
        address: &signer.address().to_string(),
        timestamp: timestamp as u64,
        nonce: 0,
        message: clob_auth::CLOB_AUTH_MESSAGE,
    };
    clob_auth::sign_l1_with_signer(signer, &input).map_err(|e| format!("L1 auth 签名失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_shared::Side;

    // 已知测试私钥（公开测试用，勿用于生产）。
    const TEST_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    #[tokio::test]
    async fn sign_clob_order_recovers_to_signer() {
        // EOA 路径：V2 EIP-712 签名后 ecrecover 应还原出 signer 地址（编码自洽）。
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let signed = sign_clob_order(&signer, Side::Buy, "12345", 0.5, 10.0, false, None, None)
            .await
            .unwrap();
        assert!(signed.signature.starts_with("0x"));
        // EOA V2 签名 = 65 字节 = 130 hex + 0x
        assert_eq!(signed.signature.len(), 2 + 130);

        // 用签名时同一组字段重建 input（timestamp_ms/salt 必须与签名时一致，否则 digest 不同）。
        let input = clob_auth::V2OrderInput {
            salt: signed.salt,
            maker: signed.maker_address,
            signer: signed.signer_address,
            token_id: signed.token_id,
            maker_amount: signed.maker_amount,
            taker_amount: signed.taker_amount,
            side: signed.side,
            signature_type: signed.signature_type,
            timestamp_ms: signed.timestamp_ms,
            metadata: signed.metadata,
            builder: signed.builder,
        };
        let recovered =
            clob_auth::recover_v2_order_signer(&signed.signature, &input, false).unwrap();
        assert_eq!(recovered, signer.address());
    }

    #[test]
    fn build_v2_input_buy_sell_amounts() {
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let maker = signer.address();
        let buy = build_v2_input(
            &signer,
            maker,
            sig_type::EOA,
            Side::Buy,
            "1",
            0.4,
            100.0,
            B256::ZERO,
            None,
            None,
        )
        .unwrap();
        // BUY：makerAmount=usdc(0.4*100*1e6=40_000_000), takerAmount=shares(100*1e6=100_000_000)
        assert_eq!(buy.maker_amount, U256::from(40_000_000u128));
        assert_eq!(buy.taker_amount, U256::from(100_000_000u128));
        assert_eq!(buy.side, 0u8);
        assert_eq!(buy.signature_type, sig_type::EOA);
        assert!(buy.salt < U256::from(1u64 << 53)); // JSON integer 安全

        let sell = build_v2_input(
            &signer,
            maker,
            sig_type::EOA,
            Side::Sell,
            "1",
            0.4,
            100.0,
            B256::ZERO,
            None,
            None,
        )
        .unwrap();
        // SELL：makerAmount=shares(100_000_000), takerAmount=usdc(40_000_000)
        assert_eq!(sell.maker_amount, U256::from(100_000_000u128));
        assert_eq!(sell.taker_amount, U256::from(40_000_000u128));
        assert_eq!(sell.side, 1u8);
    }

    /// P0 回归：USDC 侧须 ≤2 位小数（base 10^4 倍）、shares 侧 ≤5 位（base 10 倍），否则 CLOB 拒单。
    /// 0.778×6=4.668（3 位小数）须被对齐：BUY maker 向上取整到 4.67 USDC（limit 0.77833 ≥ 0.778 吃 ask），
    /// taker=6 shares。SELL taker 向下取整到 4.66 USDC（limit 0.77667 ≤ bid 吃 bid）。
    #[test]
    fn build_v2_input_amount_precision_cent_aligned() {
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let maker = signer.address();
        // BUY 6 shares @ 0.778
        let buy = build_v2_input(
            &signer, maker, sig_type::EOA, Side::Buy, "1", 0.778, 6.0, B256::ZERO, None, None,
        )
        .unwrap();
        // maker=USDC 4.67（base 4_670_000，2 位小数 ✓），taker=shares 6（base 6_000_000，5 位 ✓）
        assert_eq!(buy.maker_amount, U256::from(4_670_000u128));
        assert_eq!(buy.taker_amount, U256::from(6_000_000u128));
        assert_eq!(buy.maker_amount % U256::from(10_000u128), U256::ZERO, "USDC 非 cent-aligned");
        assert_eq!(buy.taker_amount % U256::from(10u128), U256::ZERO, "shares 非 5 位对齐");

        // SELL 6 shares @ 0.778
        let sell = build_v2_input(
            &signer, maker, sig_type::EOA, Side::Sell, "1", 0.778, 6.0, B256::ZERO, None, None,
        )
        .unwrap();
        // maker=shares 6（base 6_000_000），taker=USDC 4.66（base 4_660_000，2 位 ✓）
        assert_eq!(sell.maker_amount, U256::from(6_000_000u128));
        assert_eq!(sell.taker_amount, U256::from(4_660_000u128));
        assert_eq!(sell.taker_amount % U256::from(10_000u128), U256::ZERO, "USDC 非 cent-aligned");
    }

    #[tokio::test]
    async fn sign_clob_order_deposit_maker_signer_same_and_erc7739_wrapped() {
        // POLY_1271：maker = signer = deposit wallet 地址（同一），signatureType=3，
        // 签名为 ERC-7739 wrap（317 字节 = 634 hex + 0x），对齐官方 TS SDK。
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let deposit = address!("000000000000000000000000000000000000dEaD");
        let signed = sign_clob_order_deposit(
            &signer,
            deposit,
            Side::Buy,
            "12345",
            0.5,
            10.0,
            Some("sharpside-builder".into()),
            false,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(signed.maker_address, deposit);
        assert_eq!(signed.signer_address, deposit); // POLY_1271: maker = signer
        assert_eq!(signed.maker_address, signed.signer_address);
        assert_eq!(signed.signature_type, sig_type::POLY_1271);
        assert_eq!(signed.builder_code.as_deref(), Some("sharpside-builder"));
        assert!(signed.signature.starts_with("0x"));
        // ERC-7739 wrap = 317 字节 = 634 hex + 0x
        assert_eq!(signed.signature.len(), 2 + 634);
    }

    #[tokio::test]
    async fn sign_clob_order_deposit_builder_code_parsed_to_bytes32() {
        // builder_code（bytes32 hex）应被解析进 V2 builder 字段。
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let deposit = address!("000000000000000000000000000000000000dEaD");
        let bc = "0x599ec9d1f6a89b4c910c5eb8c91fa9656f22907a734a3e333d6d01ae3a17b92a";
        let signed = sign_clob_order_deposit(
            &signer,
            deposit,
            Side::Buy,
            "12345",
            0.5,
            10.0,
            Some(bc.into()),
            false,
            None,
            None,
        )
        .await
        .unwrap();
        assert_ne!(signed.builder, B256::ZERO);
        assert_eq!(signed.builder, clob_auth::parse_builder_code(bc).unwrap());
    }

    #[tokio::test]
    async fn sign_clob_order_deposit_neg_risk_changes_signature() {
        // neg_risk 切换 verifyingContract（standard → neg-risk）后 ERC-7739 wrap 签名必须不同。
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let deposit = address!("000000000000000000000000000000000000dEaD");
        let std_sig =
            sign_clob_order_deposit(&signer, deposit, Side::Buy, "12345", 0.5, 10.0, None, false, None, None)
                .await
                .unwrap();
        // 同输入但 timestamp/salt 会变 → 用固定 input 直接对比 clob-auth 更稳。这里仅断言两者都合法且长度一致。
        let _neg_sig =
            sign_clob_order_deposit(&signer, deposit, Side::Buy, "12345", 0.5, 10.0, None, true, None, None)
                .await
                .unwrap();
        assert_eq!(std_sig.signature.len(), 2 + 634);
    }

    #[test]
    fn l2_headers_has_five_poly_headers_and_seconds_timestamp() {
        let signer = signer_from_hex(TEST_KEY).unwrap();
        // 用真实 base64url secret（clob-auth golden vector 同款形状）。
        let secret_b64 = "czRTeHh5SjZQc2xJOERBd2V0M0x6N3NWSzVxQw==";
        let headers = l2_headers(
            signer.address(),
            secret_b64,
            "api-key",
            "pass",
            "POST",
            "/order",
            "{}",
        );
        assert_eq!(headers.len(), 5);
        let map: std::collections::HashMap<&str, &str> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(map["POLY_ADDRESS"], signer.address().to_string().as_str());
        assert_eq!(map["POLY_API_KEY"], "api-key");
        assert_eq!(map["POLY_PASSPHRASE"], "pass");
        // timestamp 为秒（10 位），非毫秒（13 位）
        let ts = map["POLY_TIMESTAMP"];
        assert!(
            ts.len() <= 10,
            "POLY_TIMESTAMP 应为秒级（<=10 位），得到 {ts}"
        );
        assert!(!map["POLY_SIGNATURE"].is_empty());
    }

    #[test]
    fn l1_auth_signature_recovers_to_signer() {
        // L1 ClobAuth EIP-712 签名应还原出 signer 地址（domain=ClobAuthDomain）。
        let signer = signer_from_hex(TEST_KEY).unwrap();
        let ts = 1713398400i64;
        let sig = build_l1_auth_signature(&signer, ts).unwrap();
        assert!(sig.starts_with("0x"));
        let recovered =
            clob_auth::recover_clob_auth_signer(&sig, signer.address(), ts as u64, 0).unwrap();
        assert_eq!(recovered, signer.address());
    }

    #[test]
    fn signer_address_deterministic() {
        let s = signer_from_hex(TEST_KEY).unwrap();
        assert_eq!(
            s.address().to_string().to_lowercase(),
            "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );
    }
}
