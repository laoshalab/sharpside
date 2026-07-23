# Venue 抽象 · 实现细节

> 把 `ARCHITECTURE.md` §7-9 的 Venue trait / 市场映射 / 跨 Venue 身份落到可实现级别。
> 目标：新增一个平台 = 新增一个 `crates/venues/<name>` crate 实现 `Venue` trait + 注册，主路径零改动。

## 1. crate 划分

```
crates/venues/
├── core/           # Venue trait + VenueInfo + VenueRegistry + 通用类型
├── polymarket/     # Polymarket adapter（signal + execution）
├── kalshi/         # Kalshi adapter（execution only）
├── manifold/       # Manifold adapter（signal only）
└── zeitgeist/      # 预留
```

`crates/venues/core` 是唯一被 services 直接依赖的 crate；各 adapter 只被 `core` 的 `VenueRegistry` 在启动时按配置注册。

## 2. 通用类型（crates/venues/core/src/types.rs）

```rust
use serde::{Deserialize, Serialize};

/// 平台标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Polymarket,
    Kalshi,
    Manifold,
    Zeitgeist,
    Azuro,
}

/// Venue 能力位
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct VenueCapabilities: u8 {
        const SIGNAL_SOURCE   = 0b01;
        const EXECUTION_VENUE = 0b10;
    }
}

/// 认证模型
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthModel {
    /// 钱包签名（Polymarket/Zeitgeist/Azuro）
    Wallet,
    /// KYC 账户 + API key + RSA 签名（Kalshi）
    KycApiKey,
    /// API key（Manifold 玩钱）
    ApiKey,
    /// 无需鉴权（只读）
    None,
}

/// 计价单位
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

/// 地理限制
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geo {
    Global,
    UsOnly,
    GlobalWithUsRestrictions,
}

/// Venue 元信息（静态，启动时声明）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueInfo {
    pub platform: Platform,
    pub display_name: String,
    pub capabilities: VenueCapabilities,
    pub auth_model: AuthModel,
    pub unit: Unit,
    pub geo: Geo,
}

/// 通用交易者
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trader {
    pub platform: Platform,
    pub venue_trader_id: String,   // Polymarket proxy wallet / Kalshi user id / Manifold user id
    pub alias: Option<String>,
    pub profile_image: Option<String>,
    pub x_username: Option<String>,
    pub verified: bool,
}

/// 通用市场
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub platform: Platform,
    pub venue_market_id: String,   // Polymarket condition_id / Kalshi ticker / Manifold marketId
    pub title: String,
    pub slug: Option<String>,
    pub tags: Vec<String>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
    pub outcome_yes: Option<f64>,
    pub outcome_no: Option<f64>,
}

/// 通用持仓
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

/// 通用成交
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub platform: Platform,
    pub trader_id: String,
    pub market_id: String,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side { Buy, Sell }

/// 凭证（per-Venue，加密存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    Wallet { encrypted_handle: String },        // Polymarket/Zeitgeist
    KycApiKey { encrypted_api_key: String, encrypted_api_secret: String }, // Kalshi
    ApiKey { encrypted_key: String },            // Manifold
}
```

## 3. Venue trait（crates/venues/core/src/lib.rs）

```rust
use async_trait::async_trait;

#[async_trait]
pub trait Venue: Send + Sync {
    /// 静态元信息
    fn info(&self) -> &VenueInfo;

    // ── signal_source 能力（默认实现返回 Unsupported，便于 execution-only Venue 不实现）──

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

    // ── execution_venue 能力 ──

    async fn place_order(&self, _cred: &Credential, _order: Order) -> Result<Fill, VenueError> {
        Err(VenueError::Unsupported("place_order"))
    }
    async fn cancel_order(&self, _cred: &Credential, _id: &str) -> Result<(), VenueError> {
        Err(VenueError::Unsupported("cancel_order"))
    }
    /// 订单状态查询（部分成交/撤单/拒绝）
    async fn order_status(&self, _cred: &Credential, _id: &str) -> Result<OrderStatus, VenueError> {
        Err(VenueError::Unsupported("order_status"))
    }
    /// 余额/仓位对账（跟单前后核对，防漏单/重复成交）
    async fn balance(&self, _cred: &Credential) -> Result<Balance, VenueError> {
        Err(VenueError::Unsupported("balance"))
    }
    /// 盘口深度（滑点保护与最小 notional 校验用）
    async fn book(&self, _market_id: &str, _token_id: &str) -> Result<OrderBook, VenueError> {
        Err(VenueError::Unsupported("book"))
    }
    /// 市场可交易性（是否开放、是否已结算、是否暂停）
    async fn market_tradable(&self, _market_id: &str) -> Result<bool, VenueError> {
        Err(VenueError::Unsupported("market_tradable"))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VenueError {
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

pub struct LeaderboardQuery {
    pub category: Option<String>,
    pub time_period: String,    // 1d/1w/1m/1y/ytd/all（对应前端 1天/1周/1个月/1年/年初至今/全部）
    pub order_by: String,       // pnl/vol/roi/win_rate
    pub limit: u32,
    pub offset: u32,
}

pub struct Pagination { pub limit: u32, pub offset: u32 }

pub struct MarketQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub limit: u32,
}

pub struct Order {
    pub market_id: String,
    pub token_id: String,
    pub side: Side,
    pub price: f64,      // 目标 Venue 单位
    pub size: f64,
}

pub struct Fill {
    pub order_id: String,
    pub filled_size: f64,
    pub filled_price: f64,
    pub tx_hash: Option<String>,
    pub fee: f64,
}
```

## 4. VenueRegistry（运行时按配置注册）

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct VenueRegistry {
    venues: HashMap<Platform, Arc<dyn Venue>>,
}

impl VenueRegistry {
    pub fn new() -> Self { Self { venues: HashMap::new() } }

    pub fn register(&mut self, v: Arc<dyn Venue>) {
        let p = v.info().platform;
        self.venues.insert(p, v);
    }

    pub fn get(&self, p: Platform) -> Option<&Arc<dyn Venue>> {
        self.venues.get(&p)
    }

    /// 列出所有具备某能力的 Venue
    pub fn with_capability(&self, cap: VenueCapabilities) -> Vec<Platform> {
        self.venues.values()
            .filter(|v| v.info().capabilities.contains(cap))
            .map(|v| v.info().platform)
            .collect()
    }
}
```

启动时由 `services/venue-hub` 按配置注入：

```rust
let mut registry = VenueRegistry::new();
if cfg.polymarket_enabled {
    registry.register(Arc::new(polymarket::PolymarketVenue::new(...)?));
}
if cfg.kalshi_enabled {
    registry.register(Arc::new(kalshi::KalshiVenue::new(...)?));
}
// ...
```

Venue 启停 = 配置开关，运营在 admin 一键切换，不影响其他 Venue。

## 5. Polymarket adapter 示例（crates/venues/polymarket）

```rust
pub struct PolymarketVenue {
    info: VenueInfo,
    client: sharpside_polymarket::PolymarketClient, // 复用现有封装
}

impl PolymarketVenue {
    pub fn new() -> Result<Self, VenueError> {
        let client = sharpside_polymarket::PolymarketClient::new(
            DATA_API_DEFAULT, GAMMA_API_DEFAULT, CLOB_API_DEFAULT,
        )?;
        let info = VenueInfo {
            platform: Platform::Polymarket,
            display_name: "Polymarket".into(),
            capabilities: VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE,
            auth_model: AuthModel::Wallet,
            unit: Unit::UsdcCtf,
            geo: Geo::GlobalWithUsRestrictions,
        };
        Ok(Self { info, client })
    }
}

#[async_trait]
impl Venue for PolymarketVenue {
    fn info(&self) -> &VenueInfo { &self.info }

    async fn leaderboard(&self, q: LeaderboardQuery) -> Result<Vec<Trader>, VenueError> {
        let entries = self.client.leaderboard(
            q.category.as_deref().unwrap_or("OVERALL"),
            &q.time_period, &q.order_by, q.limit, q.offset,
        ).await?;
        Ok(entries.into_iter().map(|e| Trader {
            platform: Platform::Polymarket,
            venue_trader_id: e.proxy_wallet,
            alias: e.user_name,
            profile_image: e.profile_image,
            x_username: e.x_username,
            verified: e.verified_badge.unwrap_or(false),
        }).collect())
    }
    // positions / trades / markets / place_order 类似，调 self.client.* 并映射类型
}
```

Kalshi adapter 同结构，但 `capabilities = EXECUTION_VENUE` only，`leaderboard/positions/trades` 用 trait 默认实现返回 `Unsupported`。Manifold 反之，`capabilities = SIGNAL_SOURCE` only，`place_order` 返回 `Unsupported`。

## 6. 市场映射（crates/mapping）

### 6.1 schema

```sql
CREATE TABLE trader_hub.market_mappings (
    from_platform    text NOT NULL,
    from_market_id   text NOT NULL,
    to_platform      text NOT NULL,
    to_market_id     text NOT NULL,
    confidence       numeric NOT NULL,
    manual_verified  boolean NOT NULL DEFAULT false,
    verified_by      text,
    verified_at      timestamptz,
    direction_flip   boolean NOT NULL DEFAULT false,        -- YES↔NO 翻转
    resolution_notes text,
    resolution_verified boolean NOT NULL DEFAULT false,
    min_notional     numeric,
    status           text NOT NULL DEFAULT 'active',        -- active / retired / rejected
    retired_at       timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (from_platform, from_market_id, to_platform, to_market_id)
);
CREATE INDEX idx_mappings_from ON trader_hub.market_mappings (from_platform, from_market_id);
CREATE INDEX idx_mappings_verified ON trader_hub.market_mappings (to_platform, manual_verified, resolution_verified, status);
```

### 6.2 启发式匹配（定时任务）

```rust
/// 对每对 (signal_source, execution_venue) 拉两边 markets，算相似度，产候选映射。
pub fn candidate_mappings(a: &[Market], b: &[Market]) -> Vec<CandidateMapping> {
    let mut out = vec![];
    for ma in a {
        for mb in b {
            let score = similarity(ma, mb);
            if score >= 0.7 { out.push(CandidateMapping { from: ma, to: mb, confidence: score }); }
        }
    }
    out
}

fn similarity(a: &Market, b: &Market) -> f64 {
    let title_sim = token_jaccard(&a.title, &b.title);          // 0–1
    let tag_sim = tag_overlap(&a.tags, &b.tags);                // 0–1
    let time_sim = end_date_closeness(a.end_date, b.end_date);   // 0–1
    0.5 * title_sim + 0.3 * tag_sim + 0.2 * time_sim
}
```

候选入表 `manual_verified=false`，进 admin 审核队列。

### 6.3 跟单时翻译（Copier 调用）

```rust
pub async fn resolve_mapping(
    db: &PgPool,
    source: Platform, source_market: &str,
    execute: Platform,
) -> Result<Mapping, MappingError> {
    let row = sqlx::query_as::<_, Mapping>(
        "SELECT to_market_id, direction_flip, min_notional
         FROM trader_hub.market_mappings
         WHERE from_platform=$1 AND from_market_id=$2 AND to_platform=$3
           AND manual_verified=true AND resolution_verified=true AND status='active'
         ORDER BY confidence DESC LIMIT 1"
    )
    .bind(source).bind(source_market).bind(execute)
    .fetch_optional(db).await?;
    row.ok_or(MappingError::NoVerifiedMapping { source, source_market, execute })
}

// Copier 调用方：
// let m = resolve_mapping(...).await?;
// let side = if m.direction_flip { order.side.flip() } else { order.side };
// if let Some(min) = m.min_notional { if notional < min { return Skip; } }
```

### 6.4 单位换算与执行参数（crates/mapping/src/unit.rs）

```rust
pub fn convert_price(from: Unit, to: Unit, price: f64) -> f64 {
    match (from, to) {
        (Unit::UsdcCtf, Unit::UsdCents) => price * 100.0,        // 0.5 USDC → 50 cents
        (Unit::UsdCents, Unit::UsdcCtf) => price / 100.0,
        (Unit::UsdcCtf, Unit::UsdcCtf) => price,
        (Unit::UsdCents, Unit::UsdCents) => price,
        _ => price, // 链上/玩钱场景按需扩展
    }
}

pub fn convert_size(from: Unit, to: Unit, size: f64, price: f64) -> f64 {
    // size 通常按 USDC notional 等价换算：notional = size * price
    // 目标 size = notional / target_price
    let notional = size * price;
    let target_price = convert_price(from, to, price);
    if target_price > 0.0 { notional / target_price } else { size }
}
```

**仅做价格/数量换算不够**——Copier 还需按 Venue 差异化套用执行参数（费率、最小 notional、滑点保护），均从 `VenueInfo` + 映射 `min_notional` 取：

```rust
pub struct ExecParams {
    pub taker_fee_bps: f64,        // Kalshi 峰值 175bps；Polymarket 75–180bps 按类目
    pub min_notional: f64,         // 来自 market_mappings.min_notional 或 Venue 默认
    pub max_slippage_bps: f64,     // 下单前 book() 比对中间价，超限拒单
}

pub fn apply_exec_params(order: &mut Order, book: &OrderBook, p: &ExecParams) -> Result<(), ExecError> {
    let mid = (book.bids[0].price + book.asks[0].price) / 2.0;
    let slip = (order.price - mid).abs() / mid;
    if slip * 10000.0 > p.max_slippage_bps { return Err(ExecError::SlippageExceeded); }
    let notional = order.size * order.price;
    if notional < p.min_notional { return Err(ExecError::BelowMinNotional); }
    // 费率从成交回报里扣减，此处仅校验
    Ok(())
}
```

各 Venue 费率/限制参考 `MULTI_PLATFORM.md` §3（费率差异、持仓限制、流动性差异）。

## 7. 跨 Venue 身份（crates/identity）

### 7.1 schema

```sql
CREATE TABLE trader_hub.identities (
    id               uuid PRIMARY KEY,
    alias            text,
    confidence       numeric NOT NULL DEFAULT 0,
    manual_verified  boolean NOT NULL DEFAULT false,
    verified_by      text,
    verified_at      timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- traders 表新增列（复合主键 + identity_id）
-- ALTER TABLE trader_hub.traders DROP CONSTRAINT traders_pkey;
-- ALTER TABLE trader_hub.traders ADD COLUMN platform text NOT NULL DEFAULT 'polymarket';
-- ALTER TABLE trader_hub.traders ADD PRIMARY KEY (platform, address);
-- ALTER TABLE trader_hub.traders ADD COLUMN identity_id uuid REFERENCES trader_hub.identities(id);
```

### 7.2 启发式链接

```rust
pub fn candidate_identities(traders: &[Trader]) -> Vec<CandidateLink> {
    let mut out = vec![];
    for (i, a) in traders.iter().enumerate() {
        for b in traders.iter().skip(i + 1) {
            if a.platform == b.platform { continue; }
            let score = identity_similarity(a, b);
            if score >= 0.6 { out.push(CandidateLink { a, b, confidence: score }); }
        }
    }
    out
}

fn identity_similarity(a: &Trader, b: &Trader) -> f64 {
    let mut score = 0.0;
    if let (Some(xa), Some(xb)) = (&a.x_username, &b.x_username) {
        if xa.eq_ignore_ascii_case(xb) { score += 0.5; }
    }
    if let (Some(na), Some(nb)) = (&a.alias, &b.alias) {
        if na.eq_ignore_ascii_case(nb) { score += 0.3; }
    }
    // 持仓相似度（同事件同方向）由 VenueHub 离线计算后注入，这里省略
    score.min(1.0)
}
```

候选进 admin 审核队列；运营确认后创建 `identities` 行并把两个 trader 的 `identity_id` 指向它。

### 7.3 身份级绩效（物化视图）

```sql
CREATE MATERIALIZED VIEW trader_hub.identity_performance AS
SELECT
    i.id AS identity_id,
    p.period,
    SUM(p.realized_pnl) AS realized_pnl,
    SUM(p.cost_basis) AS cost_basis,
    -- ROI = sum(pnl) / sum(cost_basis)；其余指标按聚合规则重算
    COUNT(*) AS trader_count
FROM trader_hub.identities i
JOIN trader_hub.traders t ON t.identity_id = i.id
JOIN trader_hub.trader_performance p ON p.address = t.address AND p.platform = t.platform
GROUP BY i.id, p.period;
```

定时刷新（每日）。前端展示 identity 时直接读此视图。

## 8. 热钥监控（per Venue）

```sql
CREATE TABLE trader_hub.hot_wallets (
    platform          text NOT NULL,
    address           text NOT NULL,
    added_by          text NOT NULL,
    added_at          timestamptz NOT NULL DEFAULT now(),
    priority          int NOT NULL DEFAULT 0,
    scan_interval_secs int NOT NULL DEFAULT 30,
    enabled           boolean NOT NULL DEFAULT true,
    PRIMARY KEY (platform, address)
);
```

`VenueHub` 的 hot wallet worker 按 `platform` 分组，调对应 `Venue::positions` 抓快照，写入 `trader_positions_snapshot`（带 `platform` 列）。频率按 `scan_interval_secs` 自适应（10–60s）——Phase B 已落地：`list_due_signal_targets` 只返回到期目标（`last_scanned_at + interval_secs <= now()`，`last_scanned_at` 派生自快照 `max(captured_at)`），`hot_secs` 降为调度节拍（默认 5s），跟随类用 `follow_scan_secs`（默认 30s），每 tick `hot_due_cap` 上限防 bootstrap 风暴。出站限流见 §9。

## 9. 限流（per Venue）

每个 adapter 内部持有一个 `governor::RateLimiter`，按 Venue 实际限额配置：

| Venue | 端点 | 限额 |
|---|---|---|
| Polymarket Data | /trades | 200 req/10s |
| Polymarket Data | /positions | 150 req/10s |
| Polymarket Data | 总体 | 1000 req/10s |
| Kalshi | REST | 按 plan（Starter 1000/min） |
| Manifold | REST | 未公开，保守 10 QPS |

`Venue` trait 不暴露限流细节，adapter 内部自管；上层只看到 `VenueError::RateLimited`。

> **Phase A 已落地（Polymarket）**：`PolymarketClient` 持按端点分桶的 `governor::DefaultDirectRateLimiter`（`/positions` 10/s、`/trades` 12/s、`/value` 8/s、`/leaderboard` 5/s，留约 1/3 余量）。超限 `await` 节流（每 10ms 重试令牌桶），上游 429 退避 500ms 重试一次后映射为 `VenueError::RateLimited`。clone 共享同一 `Arc<RateLimits>`，进程内全局生效。

## 10. 错误兜底与降级

| 场景 | 兜底 |
|---|---|
| 某 Venue API 不可用 | `VenueError::Http` → 该 Venue 信号暂停，其他 Venue 不受影响 |
| 限流 | `VenueError::RateLimited` → 退避重试（指数 backoff），不阻塞其他 Venue |
| 市场映射无 verified 项 | `MappingError::NoVerifiedMapping` → 跳过该 copy_order，标记 `skipped`，通知用户 |
| 身份未链接 | 跟随 Identity 但某 Venue 的 trader 未关联 → 仅在已关联 Venue 跟单 |
| 单位换算异常 | 价格 ≤ 0 → 跳过 + 告警 |
| 跨 Venue 执行失败 | 写 `copy_execution.status=failed`，不重试（避免重复成交），通知用户 |

## 11. 配置示例（venue-hub.toml）

```toml
[venues.polymarket]
enabled = true
data_api = "https://data-api.polymarket.com"
gamma_api = "https://gamma-api.polymarket.com"
clob_api = "https://clob.polymarket.com"
trades_rpm = 200
positions_rpm = 150

[venues.kalshi]
enabled = false                # Phase 3 启用
base_url = "https://external-api.kalshi.com/trade-api/v2"

[venues.manifold]
enabled = false                # Phase 2 启用
base_url = "https://api.manifold.markets/v0"

[mapping]
auto_match_threshold = 0.7     # 启发式候选阈值
verify_required = true          # 跨 Venue 跟单必须 manual_verified

[identity]
auto_link_threshold = 0.6
verify_required = true
```

## 12. 落地顺序（与代码骨架对应）

1. `crates/venues/core`：types + Venue trait + VenueRegistry + VenueError
2. `crates/venues/polymarket`：第一个 adapter（signal + execution）
3. `crates/db` 迁移：traders 复合主键 + identities / market_mappings / hot_wallets / user_venue_credentials / copy_order 加 source/execute venue
4. `services/venue-hub`：注入 registry + 采集 worker + 映射 worker + 身份 worker + 绩效 worker
5. `services/copier`：消费 copy_order → 查映射 → 单位换算 → 调 `Venue::place_order`
6. `services/account`：per-Venue 凭证管理 + jurisdiction
7. Phase 2/3/4：新增 `crates/venues/<name>` + 注册，主路径不动

MVP 阶段（Phase 1）只接入 Polymarket，但所有表与 trait 已是多平台结构，后续加 Venue 零重构。
