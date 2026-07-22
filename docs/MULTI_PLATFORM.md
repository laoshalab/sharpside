# Venue 接入可行性参考

> 本文档是各预测市场平台（Venue）接入 sharpside 的可行性参考与约束清单。
> 多平台原生已是架构本身（见 `ARCHITECTURE.md` / `VENUE_DESIGN.md`），本文回答"接入某 Venue 能拿到什么、能做什么、有什么约束"。
> 结论先行：**交易者数据获取仅 Polymarket / Manifold / Zeitgeist / Azuro 可行，Kalshi/PredictIt 不可行；跨平台跟单技术上可行但复杂度高，按 Phase 2→3→4 渐进接入。**

## 1. 各平台交易者数据可获取性

| 平台 | 类型 | 交易者数据 | API | 适合做信号源 |
|---|---|---|---|---|
| **Polymarket** | 链上 Polygon / USDC | ✅ 完整（proxy wallet、positions、trades、leaderboard） | Data/Gamma/CLOB 公开免鉴权 | ✅ 主信号源 |
| **Kalshi** | CFTC 监管 / USD 法币 | ❌ **官方 API 不暴露个体交易者**（监管隐私） | REST v2 + WS，市场数据公开，交易需 KYC | ❌ 不可作信号源 |
| **Manifold** | 玩钱 mana | ✅ 完整（positions、bets、user metrics） | REST `api.manifold.markets/v0` 读免鉴权 | ⚠️ 仅作辅助（玩钱信号质量弱） |
| **PredictIt** | CFTC no-action / USD | ❌ 无交易者 API，仅市场行情 JSON | 只读 `predictit.org/api/marketdata/all/` | ❌ 不可用 |
| **Zeitgeist** | 链上 Polkadot | ✅ 链上 + subgraph | SDK + subgraph | ⚠️ 量级小 |
| **Azuro** | 链上多链 / 体育 | ✅ 链上 + SDK | SDK | ⚠️ 仅体育 |
| **Augur v2** | 链上 Ethereum | ✅ subgraph | The Graph | ❌ 量级极小 |

**关键结论**：**Kalshi 因监管隐私不暴露个体交易者**，无法做"找 Kalshi 优秀交易者"的跟单。Codex.io 等聚合器也明确："Kalshi does not provide trader data. These endpoints only work with Polymarket data."

## 2. 跨平台跟单的两种含义

| 含义 | 描述 | 可行性 |
|---|---|---|
| **A. 多平台信号源** | 从 Polymarket/Manifold/Zeitgeist 多处找优秀交易者，但执行仍在原平台 | ✅ 可行，增量小 |
| **B. 跨平台执行** | 跟随 Polymarket 交易者，但在 Kalshi 执行（或反向） | ⚠️ 技术可行但复杂 |

## 3. 跨平台执行（B）的难点

| 难点 | 说明 |
|---|---|
| **市场映射** | 同一事件在不同平台是不同合约（Polymarket condition_id ↔ Kalshi ticker），需语义匹配 + 人工校对，易错 |
| **单位差异** | Polymarket 价格 0–1 USDC；Kalshi 1–99 cents；需换算 |
| **结算差异** | Polymarket CTF outcome 0/1；Kalshi 合约结算规则不同 |
| **认证差异** | Polymarket 钱包签名；Kalshi KYC 账户 + API key + RSA 签名 |
| **监管差异** | Kalshi 仅美国 KYC 用户；Polymarket 美国受限类目；用户身份决定能跑哪条通道 |
| **费率差异** | Kalshi 概率加权 taker（峰值 1.75%）；Polymarket 按类目 0.75%–1.80% |
| **持仓限制** | PredictIt $850–$3500/合约；Kalshi 无上限但需 KYC |
| **流动性差异** | 同事件两边深度不同，跟单滑点差异大 |
| **协议演进** | 每平台 API 独立变更，维护成本 N 倍 |

## 4. 合理性评估

### 4.1 支持多平台的理由

| 理由 | 强度 |
|---|---|
| 扩大优秀交易者池（更多信号） | 中（但 Kalshi 无数据，扩容有限） |
| 跨平台套利机会 | 中（属另一产品形态，非跟单） |
| 对冲单平台监管风险 | 强（Polymarket 监管不确定性高） |
| 多市场覆盖（体育=Azuro、政治=Kalshi/PredictIt） | 中 |

### 4.2 反对多平台的理由

| 理由 | 强度 |
|---|---|
| Kalshi 无交易者数据，主信号源扩容落空 | 强 |
| 跨平台市场映射错误率高，跟单可能跟错合约 | 强 |
| 每平台独立 auth/SDK/费率/监管，运维 N 倍 | 强 |
| 与"不自建链上数据"原则冲突（Zeitgeist/Azuro 需链上索引） | 中 |
| MVP 阶段分散精力，单平台未跑通就上多平台风险大 | 强 |

## 5. 推荐策略：分四阶段

```
Phase 1 (MVP)        Polymarket only —— 跑通单平台闭环
Phase 2 (信号扩容)    + Manifold 作辅助信号源（玩钱，仅发现/参考，不执行）
Phase 3 (跨平台执行)  + Kalshi 作执行 venue（信号仍来自 Polymarket，给 US KYC 用户多一个合规执行地）
Phase 4 (链上扩容)    + Zeitgeist/Azuro 作信号源+执行（量级起来后）
```

### Phase 1 · MVP（当前）
- 仅 Polymarket，已设计完成
- 验证"官方 API + 自算 + 双通道跟单"闭环

### Phase 2 · 信号源扩容（+Manifold）
- 接入 Manifold REST API（免鉴权读）
- 把 Manifold 交易者作为"辅助信号源"：发现高频/高胜率玩家，**仅作参考标签**，不在 Manifold 执行
- 价值：扩大交易者池、做"跨平台一致性"研究（同一人在多平台表现）
- 工程量：新增 `crates/venues/manifold`，traders 表加 `platform` 列

### Phase 3 · 跨平台执行（+Kalshi）
- 信号仍来自 Polymarket 交易者
- 新增"市场映射"服务：Polymarket condition_id ↔ Kalshi ticker（语义匹配 + 人工校对表）
- 新增 Kalshi 执行 adapter（KYC 账户 + RSA 签名）
- 用户画像带 `jurisdiction`（US/非US），US 用户可走 Kalshi 执行通道
- 价值：US 合规用户多一个执行地；对冲 Polymarket 单平台风险
- 工程量：大，约 2–3 人月（市场映射 + Kalshi adapter + 监管路由）

### Phase 4 · 链上扩容（+Zeitgeist/Azuro）
- 仅当某平台量级起来且用户有需求
- 接入其 SDK + subgraph
- 与 Polymarket 同等作信号源 + 执行 venue
- 工程量：每平台约 1 人月

## 6. 架构影响（为多平台预留）

### 6.1 抽象 `Platform` 为一等公民

`traders` 表主键改为 `(platform, address)` 复合键：

```sql
ALTER TABLE trader_hub.traders
  ADD COLUMN platform text NOT NULL DEFAULT 'polymarket';
ALTER TABLE trader_hub.traders
  DROP CONSTRAINT traders_pkey;
ALTER TABLE trader_hub.traders
  ADD PRIMARY KEY (platform, address);
CREATE INDEX idx_traders_platform_address ON trader_hub.traders (platform, address);
```

### 6.2 通用 venue trait

`crates/polymarket` → `crates/venues/{polymarket,manifold,kalshi,zeitgeist,azuro}`，统一 trait：

```rust
#[async_trait]
pub trait Venue: Send + Sync {
    fn platform(&self) -> Platform;
    async fn leaderboard(&self, ...) -> Result<Vec<Trader>>;
    async fn positions(&self, addr: &str) -> Result<Vec<Position>>;
    async fn trades(&self, addr: &str, ...) -> Result<Vec<Trade>>;
    // 执行类 venue 才实现
    async fn place_order(&self, order: Order) -> Result<Fill>;
}
```

### 6.3 市场映射服务（Phase 3）

新增市场映射能力（并入 `services/venue-hub`，不另起服务，与 `ARCHITECTURE.md` §6.1 一致）：
- `market_mappings` 表：`(from_platform, from_market_id, to_platform, to_market_id, confidence, manual_verified, resolution_verified, direction_flip, resolution_notes, min_notional, status)`，字段口径与 `ARCHITECTURE.md` §8.1 / `VENUE_DESIGN.md` §6.1 一致
- 自动匹配：标题/标签/结算日期相似度
- 人工校对：admin 后台确认/拒绝
- 跟单时按 mapping 翻译 token_id

### 6.4 Copier 多 executor

`copier` 内按 `Channel × Platform` 矩阵实现 executor：
- TG × Polymarket（已有）
- Daemon × Polymarket（已有）
- Daemon × Kalshi（Phase 3 新增）
- Daemon × Zeitgeist（Phase 4）

### 6.5 用户管辖域

`account.users` 加 `jurisdiction` 字段，copier 按用户管辖域过滤可用执行 venue。

## 7. 决策建议

1. **MVP 严格单平台（Polymarket）**，不分散精力。
2. **Phase 2 优先级中**：Manifold 接入成本低（免鉴权 REST），可作"信号多样性"卖点，但不要在 Manifold 执行。
3. **Phase 3 优先级低**：仅当 US 用户占比高且 Polymarket 监管风险加剧时推进；Kalshi 无交易者数据意味着不能"找 Kalshi 优秀交易者跟"，只能"用 Polymarket 信号在 Kalshi 执行"，价值打折。
4. **Phase 4 按需**：Zeitgeist/Azuro 量级起来再说。
5. **永远不接入 PredictIt**：无 API、有持仓上限、无交易者数据，无价值。
6. **架构现在就预留 `platform` 维度**（traders 复合主键 + venue trait），避免后期重构。

## 8. 与原架构原则的兼容性

| 原则 | 多平台影响 |
|---|---|
| 不自建链上数据 | Zeitgeist/Azuro 需链上索引，**违反**；Phase 4 需重新评估 |
| 低耦合不过度拆 | venue trait + per-platform crate，保持域内聚合 |
| 适合运营 | 多平台增加运营面（每平台一个菜单），需控制 |
| 双通道跟单 | 跨平台执行增加通道矩阵，但模型不变 |

## 9. 一句话结论

**Kalshi 不能作信号源（无交易者数据），只能作执行 venue；Manifold 可作辅助信号源；Zeitgeist/Azuro 量级起来再考虑。架构以 Venue 为一等公民，按 Phase 2→3→4 渐进接入新 Venue，每阶段独立可上线，新增 Venue 只需实现 `Venue` trait。**
