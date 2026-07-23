//! Venue 通用类型：`VenueInfo` / `VenueCapabilities` / `AuthModel` / `Unit` / `Geo`
//! / `Trader` / `Market` / `Position` / `Trade` / `Order` / `Fill` / `Credential`
//! / `OrderBook` / `OrderStatus` / `Balance` / 查询参数。
//!
//! 对应 `docs/VENUE_DESIGN.md` §2。新增平台只需实现 `Venue` trait + 提供这些类型映射。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sharpside_shared::Platform;

// Venue 能力位。对应 `docs/ARCHITECTURE.md` §7.1。
//
// - `SIGNAL_SOURCE` — 可拿交易者数据（leaderboard / positions / trades / markets）
// - `EXECUTION_VENUE` — 可下单执行（place_order / cancel / balance / book）
//
// 两者可兼有（Polymarket），也可只有其一（Kalshi execution-only / Manifold signal-only）。
bitflags::bitflags! {
    /// Venue 能力位。见模块顶部注释。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct VenueCapabilities: u8 {
        const SIGNAL_SOURCE   = 0b01;
        const EXECUTION_VENUE = 0b10;
    }
}

/// 认证模型。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthModel {
    /// 钱包签名（Polymarket / Zeitgeist / Azuro）
    Wallet,
    /// KYC 账户 + API key + RSA 签名（Kalshi）
    KycApiKey,
    /// API key（Manifold 玩钱）
    ApiKey,
    /// 无需鉴权（只读）
    None,
}

/// 计价单位。对应 `docs/VENUE_DESIGN.md` §2 与 `docs/MULTI_PLATFORM.md` §3 单位差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Polymarket CTF，价格 0.0–1.0 USDC
    UsdcCtf,
    /// Kalshi 合约，价格 1–99 cents
    UsdCents,
    /// Manifold mana
    Mana,
    /// 链上原生
    Native,
}

/// 地理限制。对应 `docs/ARCHITECTURE.md` §7.1 与 `docs/FLOWS.md` §8 管辖域路由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geo {
    /// 全球可用
    Global,
    /// 仅美国
    UsOnly,
    /// 全球但美国有限制（Polymarket：美国限类目）
    GlobalWithUsRestrictions,
}

/// Venue 静态元信息，启动时声明。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueInfo {
    pub platform: Platform,
    pub display_name: String,
    pub capabilities: VenueCapabilities,
    pub auth_model: AuthModel,
    pub unit: Unit,
    pub geo: Geo,
}

/// 通用交易者。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trader {
    pub platform: Platform,
    /// Polymarket proxy wallet / Kalshi user id / Manifold user id
    pub venue_trader_id: String,
    pub alias: Option<String>,
    pub profile_image: Option<String>,
    /// X(Twitter) 用户名，身份启发式链接的关键信号
    pub x_username: Option<String>,
    pub verified: bool,
    /// 排行榜来源的临时绩效种子（pnl/vol），供 ingest 写临时 `trader_performance` 行。
    /// 非 leaderboard 来源或 Venue 不提供时为 `None`。对应 `docs/FLOWS.md` §1 临时展示层。
    #[serde(default)]
    pub seed_pnl: Option<f64>,
    #[serde(default)]
    pub seed_vol: Option<f64>,
}

/// 通用市场。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub platform: Platform,
    /// Polymarket condition_id / Kalshi ticker / Manifold marketId
    pub venue_market_id: String,
    pub title: String,
    pub slug: Option<String>,
    pub tags: Vec<String>,
    /// 站内归一化分类（由 venue 官方 category 或 tags 派生）。None = 未分类。
    /// 排行榜按分类切片时用（perf worker JOIN raw_markets.category 重算绩效）。
    pub category: Option<String>,
    pub end_date: Option<DateTime<Utc>>,
    pub outcome_yes: Option<f64>,
    pub outcome_no: Option<f64>,
    /// 市场是否已结算（Polymarket Gamma `closed`）。赎回 worker 据此扫新结算市场。
    #[serde(default)]
    pub closed: Option<bool>,
}

/// 通用持仓。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub platform: Platform,
    pub trader_id: String,
    pub market_id: String,
    pub token_id: String,
    pub size: f64,
    pub avg_price: f64,
    pub current_price: f64,
    pub pnl: f64,
}

/// 通用成交。对应 `docs/VENUE_DESIGN.md` §2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub platform: Platform,
    pub trader_id: String,
    pub market_id: String,
    pub token_id: String,
    pub side: sharpside_shared::Side,
    pub price: f64,
    pub size: f64,
    pub ts: DateTime<Utc>,
    pub tx_hash: Option<String>,
}

/// 凭证（per-Venue，加密存储）。对应 `docs/VENUE_DESIGN.md` §2 与
/// `docs/VENUEHUB_STORAGE.md` §8 `user_venue_credentials` 表。
/// `docs/CHANNEL_A_SIGNING.md` §2 凭证模型。
///
/// **绝不存明文私钥**，KMS 主钥加密后入库。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    /// 旧：session wallet 句柄（Polymarket / Zeitgeist）。保留兼容，不推荐新用户。
    Wallet { encrypted_handle: String },
    /// 主路径 · FrenFlow 式 · Deposit Wallet (POLY_1271) + 委托交易 owner EOA + L2 HMAC + Builder 归因。
    /// 详见 `docs/CHANNEL_A_SIGNING.md`。Polymarket 通道 A 新默认（官方推荐新 API 用户路径）。
    DepositWalletDelegated {
        /// Deposit wallet 地址（ERC-1967 proxy）= CLOB order maker / signer / funder
        deposit_wallet_address: String,
        /// Owner EOA 地址（拥有 deposit wallet，签 POLY_1271 订单 + WALLET batch）
        owner_address: String,
        /// KMS 加密的 owner EOA 私钥（hex，可带 0x）
        encrypted_owner_key: String,
        /// L2 CLOB API key（HMAC 鉴权用，明文存 jsonb）
        l2_api_key: String,
        /// KMS 加密的 L2 secret
        encrypted_l2_secret: String,
        /// L2 passphrase
        l2_passphrase: String,
        /// Polymarket Builder Code（归因 + 免 gas + fee）
        builder_code: String,
    },
    /// KYC 账户 + API key + secret（Kalshi）
    KycApiKey {
        encrypted_api_key: String,
        encrypted_api_secret: String,
    },
    /// API key（Manifold 玩钱）
    ApiKey { encrypted_key: String },
}

/// 订单类型（时间在先策略）。对应 Polymarket CLOB `orderType` wire 字段。
///
/// - `Gtc`：Good-Til-Cancelled，挂单直到成交或撤单（默认，限价）。
/// - `Gtd`：Good-Til-Date，挂到 `Order.expiration` 指定时间后自动过期（限价）。
/// - `Fok`：Fill-Or-Kill，立即全部成交否则整单取消（市价，all-or-nothing）。
/// - `Fak`：Fill-And-Kill，立即成交能成交的部分，剩余取消（市价，允许部分成交）。
///
/// Kalshi 等其他 Venue 若语义可映射可复用；wire 字符串映射由各 adapter 负责。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    #[default]
    Gtc,
    Gtd,
    Fok,
    Fak,
}

/// 下单请求。对应 `docs/VENUE_DESIGN.md` §3。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub market_id: String,
    pub token_id: String,
    pub side: sharpside_shared::Side,
    /// 目标 Venue 单位下的价格
    pub price: f64,
    pub size: f64,
    /// 订单级幂等键：Venue 侧订单 nonce/salt（如 Polymarket CLOB salt，≤2^53）。
    /// 由 copier 按 copy_order.id 确定性派生并持久化，place_order / 重试复用 → 相同 orderID → 幂等。
    /// None 时 Venue 自行生成（非跟单路径 / 测试）。
    pub idempotency_salt: Option<u64>,
    /// 签名用 timestamp（ms），与 idempotency_salt 配套复用以发逐字节相同已签订单。None 时用 now()。
    pub order_timestamp_ms: Option<u64>,
    /// 订单类型（时间在先策略）。默认 Gtc（挂单直到成交或撤单）。
    /// Fok/Fak 为市价语义（立即对盘口成交或取消）；Gtd 需配 `expiration`。
    /// 注意：orderType/expiration 是 Polymarket CLOB wire-only 字段，不进 EIP-712 签名 struct。
    #[serde(default)]
    pub order_type: OrderType,
    /// GTD 过期时间（unix 秒）。仅 `OrderType::Gtd` 时有意义；None → wire "0"（即 GTC 语义）。
    #[serde(default)]
    pub expiration: Option<i64>,
    /// Post-only：仅做 maker，订单价格若会立即吃盘口（cross）则被服务端拒绝而非成交。
    /// 仅对限价类型（Gtc/Gtd）有意义；与 Fok/Fak 互斥（place_order 会拒）。
    /// wire-only 字段（V2 `postOnly`），不进签名。默认 false。
    #[serde(default)]
    pub post_only: bool,
}

/// 成交回报。对应 `docs/VENUE_DESIGN.md` §3。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: String,
    pub filled_size: f64,
    pub filled_price: f64,
    pub tx_hash: Option<String>,
    pub fee: f64,
}

/// 订单状态。对应 `docs/VENUE_DESIGN.md` §3（状态机/部分成交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// 订单当前成交状态（对账用）。reconcile worker 调 `Venue::order_state` 取此结构回写真实成交。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderState {
    pub status: OrderStatus,
    /// 已成交股数（部分成交时 >0 且 < 订单 size）。
    pub filled_size: f64,
    /// 成交均价（Venue 返回；缺失时由调用方回退到下单价）。
    pub filled_price: f64,
    pub fee: f64,
}

/// 余额/仓位对账结果。对应 `docs/VENUE_DESIGN.md` §3。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub cash: f64,
    pub positions: Vec<Position>,
}

/// 提现结果。对应 `docs/VENUE_DESIGN.md` §3 提现能力。
///
/// `amount` 为实际转出的人类单位（如 pUSD 7.0），`to` 为目标地址 hex。
/// `tx_hash` 为链上交易哈希（relayer gasless 提交后返回）；离线/未确认时为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawResult {
    pub to: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    /// relayer transactionID（用于后续对账/轮询）。
    pub relayer_tx_id: Option<String>,
}

/// 赎回结果。对应 `docs/VENUE_DESIGN.md` §3 赎回能力与 `docs/CHANNEL_A_SIGNING.md` §4.2。
///
/// 把已结算市场的赢仓位 CTF token 换回 pUSD（转入 deposit wallet）。
/// `condition_id` 为市场 conditionId（CTF redeemPositions 入参）。
/// `amount` 为赎回的赢方 token 数量（人类单位，CTF token 1:1 collateral）。
/// `tx_hash` 为链上交易哈希（relayer gasless 提交后返回）；未确认时为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemResult {
    pub condition_id: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    /// relayer transactionID（用于后续对账/轮询）。
    pub relayer_tx_id: Option<String>,
}

/// 拆分结果。对应 `docs/CHANNEL_A_SIGNING.md` §4.3。
///
/// 把 `amount` pUSD 锁入 CTF，铸造各 outcome token（二元市场：1 pUSD → 1 YES + 1 NO）。
/// `condition_id` 为市场 conditionId（CTF splitPositions 入参）。
/// `amount` 为拆分的 pUSD 数量（人类单位，6 decimals）。
/// `tx_hash` 为链上交易哈希（relayer gasless 提交后返回）；未确认时为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitResult {
    pub condition_id: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    /// relayer transactionID（用于后续对账/轮询）。
    pub relayer_tx_id: Option<String>,
}

/// 合并结果。对应 `docs/CHANNEL_A_SIGNING.md` §4.3。
///
/// 烧掉 `amount` 的各 outcome token（二元市场：1 YES + 1 NO → 1 pUSD），返还 pUSD。
/// `condition_id` 为市场 conditionId（CTF mergePositions 入参）。
/// `amount` 为合并的每组 outcome token 数量（人类单位，6 decimals）。
/// `tx_hash` 为链上交易哈希（relayer gasless 提交后返回）；未确认时为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub condition_id: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    /// relayer transactionID（用于后续对账/轮询）。
    pub relayer_tx_id: Option<String>,
}

/// 盘口深度。对应 `docs/VENUE_DESIGN.md` §3（滑点/深度评估）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub market_id: String,
    pub token_id: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

// ── 查询参数 ──

/// 排行榜查询。对应 `docs/VENUE_DESIGN.md` §3 与 `docs/DATA_SOURCES.md` §3.1。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardQuery {
    pub category: Option<String>,
    /// `1d` / `1w` / `1m` / `1y` / `ytd` / `all`（各 Venue 端点命名略有差异，adapter 内映射）
    pub time_period: String,
    /// `pnl` / `vol` / `roi` / `win_rate`
    pub order_by: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub limit: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_capabilities_bitops() {
        let both = VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE;
        assert!(both.contains(VenueCapabilities::SIGNAL_SOURCE));
        assert!(both.contains(VenueCapabilities::EXECUTION_VENUE));
        assert!(!VenueCapabilities::SIGNAL_SOURCE.contains(VenueCapabilities::EXECUTION_VENUE));
    }

    #[test]
    fn venue_info_serde_round_trip() {
        let info = VenueInfo {
            platform: Platform::Polymarket,
            display_name: "Polymarket".into(),
            capabilities: VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE,
            auth_model: AuthModel::Wallet,
            unit: Unit::UsdcCtf,
            geo: Geo::GlobalWithUsRestrictions,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: VenueInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.platform, back.platform);
        assert_eq!(info.auth_model, back.auth_model);
        assert_eq!(info.unit, back.unit);
    }

    #[test]
    fn credential_tagged_serde() {
        let c = Credential::Wallet {
            encrypted_handle: "enc_handle_blob".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"kind\":\"wallet\""));
        let back: Credential = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Credential::Wallet { .. }));
    }
}
