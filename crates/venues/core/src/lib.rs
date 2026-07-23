//! Venue 抽象核心：`Venue` trait + `VenueError` + `VenueRegistry`。
//!
//! 对应 `docs/VENUE_DESIGN.md` §3-§4 与 `docs/ARCHITECTURE.md` §7。
//!
//! 新增平台 = 新增 `crates/venues/<name>` 实现 `Venue` trait + 注册到 `VenueRegistry`，
//! 主路径零改动。trait 稳住的是 API 形状，不降映射/合规/费率/流动性差异——
//! 这些仍需 per-Venue 适配（见 `docs/MULTI_PLATFORM.md` §3）。

#![forbid(unsafe_code)]

pub mod types;

// 从 `sharpside-shared` re-export 基础枚举，给 adapter 作者单一 import 入口。
pub use sharpside_shared::{Platform, Side};
pub use types::{
    AuthModel, Balance, Credential, Fill, Geo, LeaderboardQuery, Market, MarketQuery, MergeResult,
    Order, OrderBook, OrderBookLevel, OrderState, OrderStatus, OrderType, Pagination, Position,
    RedeemResult, SplitResult, Trade, Trader, Unit, VenueCapabilities, VenueInfo, WithdrawResult,
};

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Venue 错误。对应 `docs/VENUE_DESIGN.md` §3 与 §10 错误兜底。
#[derive(Debug, thiserror::Error)]
pub enum VenueError {
    /// 该 Venue 不支持此能力（如 Kalshi 调 `leaderboard`、Manifold 调 `place_order`）。
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("rate limited")]
    RateLimited,
    #[error("auth: {0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// Venue 抽象。对应 `docs/VENUE_DESIGN.md` §3 与 `docs/ARCHITECTURE.md` §7.2。
///
/// signal_source 方法（`leaderboard` / `positions` / `trades` / `markets`）默认返回
/// `VenueError::Unsupported`，便于 execution-only Venue（Kalshi）不实现。
/// execution_venue 方法（`place_order` 等）同理，便于 signal-only Venue（Manifold）不实现。
#[async_trait]
pub trait Venue: Send + Sync {
    /// 静态元信息。
    fn info(&self) -> &VenueInfo;

    // ── signal_source 能力（默认 Unsupported，execution-only Venue 不实现）──

    async fn leaderboard(&self, _q: LeaderboardQuery) -> Result<Vec<Trader>, VenueError> {
        Err(VenueError::Unsupported("leaderboard"))
    }

    async fn positions(&self, _trader_id: &str) -> Result<Vec<Position>, VenueError> {
        Err(VenueError::Unsupported("positions"))
    }

    async fn trades(&self, _trader_id: &str, _q: Pagination) -> Result<Vec<Trade>, VenueError> {
        Err(VenueError::Unsupported("trades"))
    }

    async fn markets(&self, _q: MarketQuery) -> Result<Vec<Market>, VenueError> {
        Err(VenueError::Unsupported("markets"))
    }

    /// 当前持仓总 USD 估值（Data API `/value`，快照非时间序列）。
    /// 用于非榜地址的官方盈亏 delta 兜底：worker 周期快照积累历史后按周期算差。
    /// 默认 Unsupported，仅提供该端点的 Venue（Polymarket）实现。
    async fn portfolio_value(&self, _trader_id: &str) -> Result<f64, VenueError> {
        Err(VenueError::Unsupported("portfolio_value"))
    }

    // ── execution_venue 能力（默认 Unsupported，signal-only Venue 不实现）──

    async fn place_order(&self, _cred: &Credential, _order: Order) -> Result<Fill, VenueError> {
        Err(VenueError::Unsupported("place_order"))
    }

    async fn cancel_order(&self, _cred: &Credential, _id: &str) -> Result<(), VenueError> {
        Err(VenueError::Unsupported("cancel_order"))
    }

    /// 订单状态查询（部分成交/撤单/拒绝）。
    async fn order_status(&self, _cred: &Credential, _id: &str) -> Result<OrderStatus, VenueError> {
        Err(VenueError::Unsupported("order_status"))
    }

    /// 订单成交状态对账（含已成交股数/均价）。reconcile worker 调此回写真实成交，
    /// 替代"提交即记全成"的账面假设。默认 Unsupported，已实现成交对账的 Venue override。
    async fn order_state(&self, _cred: &Credential, _id: &str) -> Result<OrderState, VenueError> {
        Err(VenueError::Unsupported("order_state"))
    }

    /// 余额/仓位对账（跟单前后核对，防漏单/重复成交）。
    async fn balance(&self, _cred: &Credential) -> Result<Balance, VenueError> {
        Err(VenueError::Unsupported("balance"))
    }

    /// 某 outcome token 的链上持仓量（SELL 前校验，防无仓位下单被拒）。
    ///
    /// `token_id` 为 ERC-1155 positionId 的十进制字符串（Polymarket 口径）；
    /// 返回 deposit wallet 的该 token balanceOf（人类单位，1:1 collateral）。
    /// 默认 Unsupported，仅链上可读的 Venue（Polymarket）实现。
    async fn outcome_token_balance(
        &self,
        _cred: &Credential,
        _token_id: &str,
    ) -> Result<f64, VenueError> {
        Err(VenueError::Unsupported("outcome_token_balance"))
    }

    /// 链上余额兜底（无需 CLOB 凭证）：直接 RPC 读 collateral ERC-20 `balanceOf(deposit_wallet)`。
    ///
    /// 用于离线预配（`provision_live=false`）等 CLOB 不可用场景，展示 Deposit Wallet 的
    /// collateral 持有量。口径为链上原始余额，非 CLOB `/balance-allowance` 的可用资金。
    /// 默认 Unsupported，仅支持链上读取的 Venue（Polymarket）实现。
    async fn balance_onchain(&self, _deposit_wallet_address: &str) -> Result<f64, VenueError> {
        Err(VenueError::Unsupported("balance_onchain"))
    }

    /// 提现：从 deposit wallet 转出 collateral（如 pUSD）到外部地址。
    /// `to` 为目标地址 hex（0x…），`amount` 为人类单位（如 7.0 pUSD）。
    /// 默认 Unsupported，仅支持资产转出的 Venue（Polymarket Deposit Wallet）实现。
    async fn withdraw(
        &self,
        _cred: &Credential,
        _to: &str,
        _amount: f64,
    ) -> Result<WithdrawResult, VenueError> {
        Err(VenueError::Unsupported("withdraw"))
    }

    /// 赎回：把已结算市场的赢仓位 CTF token 换回 collateral（如 pUSD），转入 deposit wallet。
    /// 对应 `docs/CHANNEL_A_SIGNING.md` §4.2。owner 签 WALLET batch 调 CTF.redeemPositions。
    ///
    /// - `condition_id`：市场 conditionId（bytes32 hex，CTF redeemPositions 入参）。
    /// - `amount`：赢方 token 数量（人类单位，CTF token 1:1 collateral；仅审计用，链上按余额全赎）。
    ///
    /// 默认 Unsupported，仅支持赎回的 Venue（Polymarket Deposit Wallet）实现。
    async fn redeem(
        &self,
        _cred: &Credential,
        _condition_id: &str,
        _amount: f64,
    ) -> Result<RedeemResult, VenueError> {
        Err(VenueError::Unsupported("redeem"))
    }

    /// 拆分：把 `amount` collateral（如 pUSD）锁入 CTF，铸造各 outcome token
    /// （二元市场：1 pUSD → 1 YES + 1 NO）。
    ///
    /// - `condition_id`：市场 conditionId（bytes32 hex，CTF splitPositions 入参）。
    /// - `amount`：拆分的 collateral 数量（人类单位，6 decimals）。
    ///
    /// 默认 Unsupported，仅支持拆分的 Venue（Polymarket Deposit Wallet）实现。
    async fn split(
        &self,
        _cred: &Credential,
        _condition_id: &str,
        _amount: f64,
    ) -> Result<SplitResult, VenueError> {
        Err(VenueError::Unsupported("split"))
    }

    /// 合并：烧掉 `amount` 的各 outcome token，返还 collateral
    /// （二元市场：1 YES + 1 NO → 1 pUSD）。
    ///
    /// - `condition_id`：市场 conditionId（bytes32 hex，CTF mergePositions 入参）。
    /// - `amount`：合并的每组 outcome token 数量（人类单位，6 decimals）。
    ///
    /// 默认 Unsupported，仅支持合并的 Venue（Polymarket Deposit Wallet）实现。
    async fn merge(
        &self,
        _cred: &Credential,
        _condition_id: &str,
        _amount: f64,
    ) -> Result<MergeResult, VenueError> {
        Err(VenueError::Unsupported("merge"))
    }

    /// 盘口深度（滑点保护与最小 notional 校验用）。
    async fn book(&self, _market_id: &str, _token_id: &str) -> Result<OrderBook, VenueError> {
        Err(VenueError::Unsupported("book"))
    }

    /// 市场可交易性（是否开放、是否已结算、是否暂停）。
    async fn market_tradable(&self, _market_id: &str) -> Result<bool, VenueError> {
        Err(VenueError::Unsupported("market_tradable"))
    }

    /// 市场最小下单股数（服务端强制；0 = 未知/不限）。
    /// 用于风控层下单前校验，避免撞服务端 400。默认返回 0（不校验）。
    async fn market_min_size(&self, _market_id: &str) -> Result<f64, VenueError> {
        Ok(0.0)
    }
}

/// Venue 注册表。对应 `docs/VENUE_DESIGN.md` §4。
///
/// 启动时由 `services/venue-hub` 按配置注入，Venue 启停 = 配置开关。
/// 运营在 admin 一键启用/停用某 Venue 的能力，不影响其他 Venue。
pub struct VenueRegistry {
    venues: HashMap<Platform, Arc<dyn Venue>>,
}

impl VenueRegistry {
    pub fn new() -> Self {
        Self {
            venues: HashMap::new(),
        }
    }

    /// 注册一个 Venue。若同 platform 已存在则覆盖（支持热替换配置）。
    pub fn register(&mut self, venue: Arc<dyn Venue>) {
        let platform = venue.info().platform;
        self.venues.insert(platform, venue);
    }

    /// 按 platform 取 Venue。
    pub fn get(&self, platform: Platform) -> Option<&Arc<dyn Venue>> {
        self.venues.get(&platform)
    }

    /// 列出所有已注册的 platform。
    pub fn platforms(&self) -> Vec<Platform> {
        self.venues.keys().copied().collect()
    }

    /// 列出所有具备指定能力的 platform。对应 `docs/VENUE_DESIGN.md` §4。
    pub fn with_capability(&self, cap: VenueCapabilities) -> Vec<Platform> {
        self.venues
            .values()
            .filter(|v| v.info().capabilities.contains(cap))
            .map(|v| v.info().platform)
            .collect()
    }
}

impl Default for VenueRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 stub venue：只支持 signal_source 的 leaderboard。
    struct SignalOnlyStub {
        info: VenueInfo,
    }

    #[async_trait]
    impl Venue for SignalOnlyStub {
        fn info(&self) -> &VenueInfo {
            &self.info
        }

        async fn leaderboard(&self, _q: LeaderboardQuery) -> Result<Vec<Trader>, VenueError> {
            Ok(vec![])
        }
    }

    /// 测试用 stub venue：只支持 execution_venue 的 place_order。
    struct ExecutionOnlyStub {
        info: VenueInfo,
    }

    #[async_trait]
    impl Venue for ExecutionOnlyStub {
        fn info(&self) -> &VenueInfo {
            &self.info
        }

        async fn place_order(&self, _cred: &Credential, _order: Order) -> Result<Fill, VenueError> {
            Ok(Fill {
                order_id: "test".into(),
                filled_size: 0.0,
                filled_price: 0.0,
                tx_hash: None,
                fee: 0.0,
                dry: false,
            })
        }
    }

    fn signal_info(platform: Platform) -> VenueInfo {
        VenueInfo {
            platform,
            display_name: "SignalStub".into(),
            capabilities: VenueCapabilities::SIGNAL_SOURCE,
            auth_model: AuthModel::None,
            unit: Unit::UsdcCtf,
            geo: Geo::Global,
        }
    }

    fn execution_info(platform: Platform) -> VenueInfo {
        VenueInfo {
            platform,
            display_name: "ExecStub".into(),
            capabilities: VenueCapabilities::EXECUTION_VENUE,
            auth_model: AuthModel::KycApiKey,
            unit: Unit::UsdCents,
            geo: Geo::UsOnly,
        }
    }

    #[tokio::test]
    async fn trait_defaults_return_unsupported() {
        let exec = ExecutionOnlyStub {
            info: execution_info(Platform::Kalshi),
        };
        // execution-only venue 调 signal 方法 → Unsupported
        let err = exec
            .leaderboard(LeaderboardQuery {
                category: None,
                time_period: "all".into(),
                order_by: "pnl".into(),
                limit: 10,
                offset: 0,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, VenueError::Unsupported("leaderboard")));
        // 新增 split/merge 默认亦 Unsupported（仅 Polymarket Deposit Wallet 实现）。
        assert!(matches!(
            exec.split(&Credential::Wallet { encrypted_handle: "x".into() }, "c", 1.0)
                .await
                .unwrap_err(),
            VenueError::Unsupported("split")
        ));
        assert!(matches!(
            exec.merge(&Credential::Wallet { encrypted_handle: "x".into() }, "c", 1.0)
                .await
                .unwrap_err(),
            VenueError::Unsupported("merge")
        ));
    }

    #[tokio::test]
    async fn registry_with_capability_filters() {
        let mut registry = VenueRegistry::new();
        registry.register(Arc::new(SignalOnlyStub {
            info: signal_info(Platform::Polymarket),
        }));
        registry.register(Arc::new(ExecutionOnlyStub {
            info: execution_info(Platform::Kalshi),
        }));

        let signal_sources = registry.with_capability(VenueCapabilities::SIGNAL_SOURCE);
        assert_eq!(signal_sources, vec![Platform::Polymarket]);

        let execution_venues = registry.with_capability(VenueCapabilities::EXECUTION_VENUE);
        assert_eq!(execution_venues, vec![Platform::Kalshi]);

        let both = registry
            .with_capability(VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE);
        assert!(both.is_empty());
    }

    #[tokio::test]
    async fn registry_get_by_platform() {
        let mut registry = VenueRegistry::new();
        registry.register(Arc::new(SignalOnlyStub {
            info: signal_info(Platform::Polymarket),
        }));

        assert!(registry.get(Platform::Polymarket).is_some());
        assert!(registry.get(Platform::Kalshi).is_none());
        assert_eq!(registry.platforms().len(), 1);
    }
}
