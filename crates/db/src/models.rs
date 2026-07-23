//! 类型化行结构（`FromRow`），对应 `trader_hub` schema 关键表。
//!
//! 字段口径与 `docs/TRADERS_TABLE.md` / `docs/VENUEHUB_STORAGE.md` / `docs/VENUE_DESIGN.md` §6.1 一致。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// `trader_hub.traders` 行。对应 `docs/TRADERS_TABLE.md` §1。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Trader {
    pub platform: String,
    pub address: String,
    pub identity_id: Option<Uuid>,
    pub alias: Option<String>,
    pub source: String,
    pub is_hot: bool,
    pub visibility: String,
    pub profile_image: Option<String>,
    pub x_username: Option<String>,
    pub verified_badge: Option<bool>,
    pub user_name: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 上次回填 raw_trades 的时间；NULL = 从未回填。回填 worker 维护。
    pub trades_backfilled_at: Option<DateTime<Utc>>,
}

/// `trader_hub.identities` 行。对应 `docs/VENUE_DESIGN.md` §7.1。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Identity {
    pub id: Uuid,
    pub alias: Option<String>,
    pub confidence: Decimal,
    pub manual_verified: bool,
    pub verified_by: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// `trader_hub.market_mappings` 行。对应 `docs/ARCHITECTURE.md` §8.1 / `docs/VENUE_DESIGN.md` §6.1。
///
/// 跨 Venue 跟单只读 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MarketMapping {
    pub from_platform: String,
    pub from_market_id: String,
    pub to_platform: String,
    pub to_market_id: String,
    pub confidence: Decimal,
    pub manual_verified: bool,
    pub verified_by: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub direction_flip: bool,
    pub resolution_notes: Option<String>,
    pub resolution_verified: bool,
    pub min_notional: Option<Decimal>,
    pub status: String,
    pub retired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// `trader_hub.trader_performance` 行。对应 `docs/VENUEHUB_STORAGE.md` §6。
///
/// per `(platform, address, period)` 物化，覆盖写 `1d`/`1w`/`1m`/`1y`/`ytd`/`all` 六行。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TraderPerformance {
    pub platform: String,
    pub address: String,
    pub period: String,
    /// 'OVERALL' = 全部成交；其余 = 站内分类（perf worker 按 raw_markets.category 切片）。
    pub category: String,
    pub roi: Decimal,
    pub sharpe: Decimal,
    pub sortino: Decimal,
    pub win_rate: Decimal,
    pub max_drawdown: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub gross_profit: Decimal,
    pub gross_loss: Decimal,
    pub profit_factor: Decimal,
    pub wins: i32,
    pub losses: i32,
    pub position_count: i32,
    pub open_positions: i32,
    pub total_volume: Decimal,
    pub cost_basis: Decimal,
    pub computed_at: DateTime<Utc>,
    /// 官方排行榜该周期盈亏（USD）。NULL = 未抓到/不在榜，前端回落到自算 `realized_pnl`。
    /// 来源见 `official_source`（如 'polymarket_leaderboard'）。对应 docs/SHADOW_MODE.md 对齐口径。
    pub official_pnl: Option<Decimal>,
    /// 官方排行榜该周期成交量（USD）。NULL = 未抓到。
    pub official_vol: Option<Decimal>,
    /// 官方盈亏数据来源（如 'polymarket_leaderboard'），便于审计。
    pub official_source: Option<String>,
    /// 官方盈亏抓取时间，用于新鲜度判断与刷新调度。
    pub official_pnl_at: Option<DateTime<Utc>>,
}

/// `trader_hub.raw_trades` 行。对应 `docs/VENUEHUB_STORAGE.md` §2。
///
/// 各 signal_source Venue 的 trades 端点原貌；链上 Venue 用 `tx_hash` 去重，玩钱/KYC 用 `trade_id`。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RawTrade {
    pub platform: String,
    pub address: String,
    pub token_id: String,
    pub condition_id: Option<String>,
    pub side: String,
    pub price: Decimal,
    pub size: Decimal,
    pub ts: DateTime<Utc>,
    pub tx_hash: Option<String>,
    pub trade_id: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

/// `trader_hub.raw_markets` 行。对应 `docs/VENUEHUB_STORAGE.md` §2。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RawMarket {
    pub platform: String,
    pub venue_market_id: String,
    pub title: String,
    pub slug: Option<String>,
    pub tags: Vec<Option<String>>,
    pub category: Option<String>,
    pub end_date: Option<DateTime<Utc>>,
    pub outcome_yes: Option<Decimal>,
    pub outcome_no: Option<Decimal>,
    /// 市场是否已结算（Gamma `/markets` 的 `closed`）。赎回 worker 据此扫新结算市场。
    pub closed: bool,
    /// 结算时间（首次观测到 closed=true 时填充；worker 游标用）。
    pub resolved_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
}

/// `trader_hub.hot_wallets` 行。对应 `docs/VENUEHUB_STORAGE.md` §7。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HotWallet {
    pub platform: String,
    pub address: String,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
    pub priority: i32,
    pub scan_interval_secs: i32,
    pub enabled: bool,
}

/// hot worker 的监控目标（热钥 ∪ 活跃跟随目标）。`identity_id` 来自 trader_hub.traders，
/// 用于让 identity 跟随的信号携带 identity_id 以命中跨 Venue 身份跟随关系。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SignalTarget {
    pub address: String,
    pub identity_id: Option<Uuid>,
    /// 该目标的扫描间隔（秒）：热钥取 `hot_wallets.scan_interval_secs`，跟随类取全局 `follow_scan_secs`。
    pub interval_secs: i32,
    /// 最近一次扫描时间：派生自 `trader_positions_snapshot` 的 `max(captured_at)`；NULL 表示从未扫描（bootstrap）。
    pub last_scanned_at: Option<DateTime<Utc>>,
}

/// `trader_hub.trader_positions_snapshot` 行。对应 `docs/VENUEHUB_STORAGE.md` §7。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub platform: String,
    pub address: String,
    pub token_id: String,
    pub condition_id: Option<String>,
    pub size: Decimal,
    pub avg_price: Decimal,
    pub current_price: Decimal,
    pub pnl: Decimal,
    pub captured_at: DateTime<Utc>,
}

// ── account schema（用户 / 跟随 / 跟单 / 凭证）──

/// `account.users` 行。对应 `docs/ARCHITECTURE.md` §6.4。
///
/// 身份方式：TG（`tg_id`）或 钱包（`account.user_wallets` 表，1:N）。邮箱认证已移除。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub tg_id: Option<i64>,
    pub jurisdiction: String,
    pub subscription_tier: String,
    pub subscription_until: Option<DateTime<Utc>>,
    pub risk_overrides: serde_json::Value,
    #[serde(skip_serializing)]
    pub daemon_api_key_hash: Option<String>,
    pub daemon_api_key_rotated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `account.user_wallets` 行。对应钱包登录（模型 A · 身份钱包）。
///
/// 一个用户可绑多个钱包（恢复因子）；`address` 全局唯一（一个地址只绑一个用户）。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserWallet {
    pub user_id: Uuid,
    /// 小写 0x hex
    pub address: String,
    pub label: Option<String>,
    pub is_primary: bool,
    pub linked_at: DateTime<Utc>,
}

/// `account.follow_relation` 行。对应 `docs/ARCHITECTURE.md` §6.2。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FollowRelation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub follow_platform: Option<String>,
    pub follow_address: Option<String>,
    pub follow_identity_id: Option<Uuid>,
    pub execute_venue: String,
    pub channel: String,
    pub config: serde_json::Value,
    pub same_venue_only: bool,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// `account.watchlist` 行。对应 Watchlist 功能规划（纯收藏，不进执行路径）。
///
/// 与 `FollowRelation` 同构的"二选一目标"，但无 execute_venue/channel/config——
/// 仅用于观察，不派生信号、不下单。一键升级为 Follow 时删除本行。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Watchlist {
    pub id: Uuid,
    pub user_id: Uuid,
    pub watch_platform: Option<String>,
    pub watch_address: Option<String>,
    pub watch_identity_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// `account.copy_order` 行。对应 `docs/ARCHITECTURE.md` §6.2-6.3 / `docs/FLOWS.md` §5。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CopyOrderRow {
    pub id: Uuid,
    pub follow_relation_id: Uuid,
    pub user_id: Uuid,
    pub source_venue: String,
    pub execute_venue: String,
    pub source_market_id: String,
    pub source_token_id: String,
    pub execute_market_id: Option<String>,
    pub execute_token_id: Option<String>,
    pub side: String,
    pub price: Decimal,
    pub size: Decimal,
    pub channel: String,
    pub signal_at: DateTime<Utc>,
    pub enqueued_at: DateTime<Utc>,
    /// 进入 dispatched 的时间（claim 时写入；reclaim worker 超时判据）。NULL = 未被 claim。
    pub dispatched_at: Option<DateTime<Utc>>,
    /// Venue 返回的订单 ID（place_order 成功后写入；reconcile worker 据此查成交）。
    pub venue_order_id: Option<String>,
    /// 进入 submitted 的时间（place_order 成功后写入；reconcile worker 超时撤单判据）。
    pub submitted_at: Option<DateTime<Utc>>,
    /// 订单级幂等键：按 copy_order.id 确定性派生的 Polymarket CLOB salt（≤2^53）。
    /// claim 时写入，place_order / reclaim 重试复用 → 相同 orderID → 幂等。NULL = 旧行未设。
    pub idempotency_salt: Option<i64>,
    /// 签名用 timestamp（ms），claim 时写入，重试复用 → 逐字节相同已签订单。NULL = 旧行未设。
    pub order_timestamp_ms: Option<i64>,
    /// 单位换算后的目标 Venue 价格 / 股数（claim 时写入），让 reclaim 重试自洽无需重跑映射。
    pub exec_price: Option<f64>,
    pub exec_size: Option<f64>,
    pub status: String,
    pub skip_reason: Option<String>,
    /// 信号去重键（migration 0031）。配合 venue-hub signal_outbox 重发幂等：
    /// 同一 signal_id 对同一 follow_relation 仅一条 copy_order。历史行 / 非 signal 派生为 NULL。
    pub signal_id: Option<String>,
}

/// `account.copy_execution` 行。对应 `docs/FLOWS.md` §7。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CopyExecution {
    pub id: Uuid,
    pub copy_order_id: Uuid,
    pub user_id: Uuid,
    pub venue: String,
    pub market_id: String,
    pub token_id: String,
    pub venue_order_id: Option<String>,
    pub side: String,
    pub filled_size: Decimal,
    pub filled_price: Decimal,
    pub fee: Decimal,
    pub tx_hash: Option<String>,
    pub executed_at: DateTime<Utc>,
}

/// `account.user_venue_credentials` 行。对应 `docs/ARCHITECTURE.md` §6.4。
///
/// `kind` 列级公开字段（如 `deposit_wallet_delegated`），与 blob 内 `kind` 对齐；
/// `encrypted_blob` 不序列化给前端。对应 `docs/FRONTEND_DESIGN.md` §6.5 / §11。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserVenueCredential {
    pub user_id: Uuid,
    pub platform: String,
    /// 凭证类型（公开）：deposit_wallet_delegated / wallet / kyc_api_key / api_key 等。
    pub kind: String,
    #[serde(skip_serializing)]
    pub encrypted_blob: serde_json::Value,
    pub proxy_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 排行榜行：`traders` LEFT JOIN `trader_performance`(指定 period) LEFT JOIN `trader_tag`。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.2 与 `docs/ARCHITECTURE.md` §6.1。
///
/// 绩效字段为 `Option`：新导入未算或无该周期行时为 `None`，前端显 "—"。
///
/// 注意：`#[serde(flatten)] trader: Trader` 与 sqlx `FromRow` derive 不兼容
/// （flatten 让 sqlx 把 Trader 当单列解码），故手动实现 `FromRow`，serde flatten 仅用于 JSON 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardRow {
    #[serde(flatten)]
    pub trader: Trader,
    pub roi: Option<Decimal>,
    pub sharpe: Option<Decimal>,
    pub win_rate: Option<Decimal>,
    pub max_drawdown: Option<Decimal>,
    pub realized_pnl: Option<Decimal>,
    pub total_volume: Option<Decimal>,
    pub open_positions: Option<i32>,
    pub tags: Vec<String>,
    /// botfilter 合成置信度 ∈ [0,1]；无 `tag_attrs.bot` 时为 `None`。
    pub bot_confidence: Option<f64>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LeaderboardRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let trader = Trader {
            platform: row.try_get("platform")?,
            address: row.try_get("address")?,
            identity_id: row.try_get("identity_id")?,
            alias: row.try_get("alias")?,
            source: row.try_get("source")?,
            is_hot: row.try_get("is_hot")?,
            visibility: row.try_get("visibility")?,
            profile_image: row.try_get("profile_image")?,
            x_username: row.try_get("x_username")?,
            verified_badge: row.try_get("verified_badge")?,
            user_name: row.try_get("user_name")?,
            first_seen: row.try_get("first_seen")?,
            updated_at: row.try_get("updated_at")?,
            trades_backfilled_at: row.try_get("trades_backfilled_at")?,
        };
        Ok(Self {
            trader,
            roi: row.try_get("roi")?,
            sharpe: row.try_get("sharpe")?,
            win_rate: row.try_get("win_rate")?,
            max_drawdown: row.try_get("max_drawdown")?,
            realized_pnl: row.try_get("realized_pnl")?,
            total_volume: row.try_get("total_volume")?,
            open_positions: row.try_get("open_positions")?,
            tags: row.try_get("tags")?,
            bot_confidence: row.try_get("bot_confidence")?,
        })
    }
}

/// `trader_equity_curve` 单点。对应 `docs/FRONTEND_DESIGN.md` §6.1 权益曲线。
///
/// `ts` 为小时级时间戳；`daily_pnl` 为相对前一点的权益增量。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EquityCurvePoint {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub equity: Decimal,
    pub daily_pnl: Decimal,
    pub drawdown_pct: Decimal,
}

/// `trader_equity_curve` 批量查询行（含 platform/address，供排行榜 sparkline 批量端点分组用）。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EquityCurveBatchRow {
    pub platform: String,
    pub address: String,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub equity: Decimal,
    pub daily_pnl: Decimal,
    pub drawdown_pct: Decimal,
}

/// `position_timeline` 行（当前持仓视图）。对应 `docs/FRONTEND_DESIGN.md` §6.1 当前持仓表。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PositionRow {
    pub token_id: String,
    pub condition_id: Option<String>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub total_bought_size: Decimal,
    pub total_sold_size: Decimal,
    pub avg_cost: Decimal,
    pub realized_pnl: Decimal,
    pub final_open_size: Decimal,
    pub is_closed: bool,
}

/// `account.withdrawals` 行。对应 `docs/CHANNEL_A_SIGNING.md` §4.1 提现审计。
///
/// 提现是高敏操作（平台代签 WALLET batch 转出 deposit wallet 资产），全量审计。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Withdrawal {
    pub id: Uuid,
    pub user_id: Uuid,
    pub venue: String,
    pub asset: String,
    /// 人类单位金额（如 7.0 pUSD）。
    pub amount: Decimal,
    /// 提现目标地址（小写 0x hex，须为用户绑定钱包之一）。
    pub to_address: String,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
    /// pending / mined / failed。
    pub status: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// `account.redemptions` 行。赎回审计日志（自动 worker + 手动端点共用）。
/// 对应 `docs/CHANNEL_A_SIGNING.md` §4.2 与 migration 0025。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Redemption {
    pub id: Uuid,
    pub user_id: Uuid,
    pub venue: String,
    /// 市场 condition_id（CTF redeemPositions 入参）。
    pub condition_id: String,
    /// 赢方 outcome：YES / NO。
    pub outcome: String,
    /// 赢方 token 的 ERC-1155 id（链上 balanceOf 校验 + 审计）。
    pub token_id: String,
    /// 赎回的 token 数量（人类单位，CTF token 1:1 collateral）。
    pub amount: Decimal,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
    /// pending / mined / failed。
    pub status: String,
    /// auto = worker 触发；manual = 用户点【赎回】按钮触发。
    pub source: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}
