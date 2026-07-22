# Sharpside Bot / 做市过滤规则

> 公开可审阅文档。与 `crates/botfilter/src/lib.rs` 源码、`services/venue-hub/src/workers/perf.rs` 聚合口径三者保持一致。
> 版本对应一期（6 条规则）。二期路线（MakerTakerSelfPair / KnownEntity）见末尾。

## 1. 目标

把做市商、scalper、wash trade 钱包从 smart money 候选中剔除。官方 `/profit` 排行把这类钱包也算「赚了」，但它们的「赚」来自价差/刷量/对冲，不是可跟的 skill。Sharpside 的承诺：**每条规则可解释、有反例、有 evidence**，相对 Hashdive/Polydata 黑盒。

输出 `BotFlags { is_bot, confidence, hit_rules: Vec<RuleHit> }`，被三处消费：

- **scoring / conviction**：`bot_filter_penalty` 乘性因子（confidence=1 → conviction≈0）—— *待接入*（当前一期仅落标签 + 过滤 + 门控）。
- **discovery / 排行榜**：`GET /traders?include_bots=false` 读 `trader_tag.tags` 含 `'bot'` 过滤。
- **follow / 跟单门控**：`POST /follows` 跟随 `bot` 标签的交易者 → 400 拒绝。

## 2. 输入契约

纯函数 `detect_with(stats: &AggregatedStats, cfg: &BotFilterConfig) -> BotFlags`。无 DB、无网络、deterministic。`AggregatedStats` 由 venue-hub perf worker 预聚合（见 §4）。

```rust
pub struct AggregatedStats {
    pub n_trades: u64,           // 总成交笔数
    pub n_buys: u64,              // BUY 笔数
    pub n_sells: u64,             // SELL 笔数
    pub symmetric_ratio: f64,     // 1 - |buys-sells|/(buys+sells)
    pub self_trade_count: u64,    // 同 (tx_hash, token_id) 买卖共存的组数
    pub round_trips: u64,         // 已平仓且 holding_seconds ≤ 3600 的配对数
    pub median_hold_secs: i64,    // 全部配对 hold 中位秒数；无配对 → -1（未知）
    pub unique_conditions: u64,  // distinct condition_id（非空）
    pub large_trade_count: u64,   // 单笔 notional = size*price ≥ sc_large_notional
    pub n_resolved: u64,          // 已结算 condition 数（wins + losses）
    pub n_resolved_wins: u64,     // 已结算且盈利的 condition 数
}
```

## 3. 6 条规则

### 3.1 HighFreqSymmetric — 高频且买卖近对称

做市 / scalper 特征：成交量大且 buy/sell 近 1:1。

**触发条件**：`n_trades ≥ hf_min_trades` **且** `symmetric_ratio ≥ hf_min_symmetric`。

```
symmetric_ratio = 1 - |n_buys - n_sells| / (n_buys + n_sells)
```

**confidence**：`(symmetric_ratio - hf_min_symmetric) / (1 - hf_min_symmetric)`，clamp [0,1]，最低 0.5。

**默认阈值**：`hf_min_trades = 500`，`hf_min_symmetric = 0.85`。

**evidence**：`{n_trades, n_buys, n_sells, symmetric_ratio, thresholds}`。

**反例**：正常长线钱包 30 笔、buys/sells = 25/5 → symmetric_ratio ≈ 0.33，远低于 0.85，不命中（见单测 `normal_wallet_not_flagged`）。

### 3.2 WashTrade — 同 tx+token 对冲腿

借鉴 polyterm `wash_trade_detector`。同一笔交易（tx+token）里同时出现买卖腿，是对冲/wash 的强信号。

**触发条件**：`self_trade_count ≥ wash_min_count`。

**confidence**：`self_trade_count / wash_full_count`，clamp [0,1]，最低 0.5。

**默认阈值**：`wash_min_count = 1`，`wash_full_count = 5`（5 条 self-trade → confidence 1.0）。

**evidence**：`{self_trade_count, thresholds}`。

**反例**：纯单向建仓的钱包 self_trade_count = 0，不命中。

### 3.3 RoundTripScalper — 大量短窗口 round-trip + 极短持仓

scalper 特征：频繁开平、持仓极短。

**触发条件**：`round_trips ≥ rt_min_round_trips` **且**（持仓时长未知 `median_hold_secs < 0` **或** `median_hold_secs ≤ rt_max_hold_secs`）。

**confidence**：

- 时长已知且 ≤ 上限：`((round_trips - rt_min_round_trips) / rt_min_round_trips)` clamp [0,1]，最低 0.5。
- 时长未知（-1）：弱命中，`min(rt_conf, 0.3)`，**单条不触发 is_bot**（避免误伤）。

**默认阈值**：`rt_min_round_trips = 50`，`rt_max_hold_secs = 60`。

**evidence**：`{round_trips, median_hold_secs, thresholds}`。

**反例**：round-trip 量足但持仓时长未知 → 弱命中 ≤0.3，不触发 is_bot（见单测 `round_trip_unknown_hold_is_weak`）。

### 3.4 TakerOnlyScalper — 大量 round-trip + 已结算胜率极低

无 edge 的 churner / 噪声 bot：频繁开平却长期亏损，典型刷量或失败套利。

**触发条件**：`round_trips ≥ tos_min_round_trips` **且** `n_resolved ≥ tos_min_resolved` **且** `win_rate ≤ tos_max_win_rate`。

```
win_rate = n_resolved_wins / n_resolved
```

**confidence**：`0.6 * ((tos_max_win_rate - win_rate) / tos_max_win_rate) + 0.4 * ((round_trips - tos_min_round_trips) / tos_min_round_trips)`，clamp [0.5, 1.0]。

**默认阈值**：`tos_min_round_trips = 50`，`tos_min_resolved = 10`，`tos_max_win_rate = 0.3`。

**evidence**：`{round_trips, n_resolved, n_resolved_wins, win_rate, thresholds}`。

**反例**：高频 round-trip 但高胜率（真 skill）→ win_rate 0.75 > 0.3，不命中（见单测 `taker_only_skill_wallet_not_flagged`）。

### 3.5 SizeConcentration — 大额成交集中于极少数 condition

pump / 单市做市 bot：大额成交几乎全部砸在 1–2 个 condition 上。

**触发条件**：`unique_conditions ≤ sc_max_conditions` **且** `large_trade_count ≥ sc_min_large_trades`。

**confidence**：`0.5 * ((large_trade_count - sc_min_large_trades) / sc_min_large_trades) + 0.5 * (1 - (unique_conditions - 1) / sc_max_conditions)`，clamp [0, 0.4]。

> 故意 cap 在 0.4：concentration 单独不足以触发 is_bot（阈值 0.5），需其他规则叠加——避免误伤在少数市场重仓的高 conviction 鲸鱼（见单测 `size_concentration_weak_signal`）。

**默认阈值**：`sc_max_conditions = 2`，`sc_min_large_trades = 20`，`sc_large_notional = 5000.0`（大额 = 单笔 notional ≥ 5000 USDC，worker 侧判定）。

**evidence**：`{unique_conditions, large_trade_count, thresholds}`。

**反例**：diversified conviction 钱包 unique_conditions = 8 → 不命中（见单测 `size_concentration_diversified_not_flagged`）。

### 3.6 HighChurnNoEdge — 成交极高频 + 已结算胜率极低

高频噪声 bot：成交极多但结算长期亏损，区别于「高频且赚钱」的真做市/skill。

**触发条件**：`n_trades ≥ hc_min_trades` **且** `n_resolved ≥ hc_min_resolved` **且** `win_rate ≤ hc_max_win_rate`。

**confidence**：`0.6 * ((hc_max_win_rate - win_rate) / hc_max_win_rate) + 0.4 * ((n_trades - hc_min_trades) / hc_min_trades)`，clamp [0.5, 1.0]。

**默认阈值**：`hc_min_trades = 2000`，`hc_min_resolved = 20`，`hc_max_win_rate = 0.3`。

**evidence**：`{n_trades, n_resolved, n_resolved_wins, win_rate, thresholds}`。

**反例**：高频但高胜率（做市赚钱或真 edge）→ win_rate 0.73 > 0.3，不命中（见单测 `high_churn_skill_not_flagged`）。

## 4. 合成判定

```
confidence = min(1, Σ hit.confidence)
is_bot = confidence ≥ bot_threshold
```

**默认** `bot_threshold = 0.5`。多条弱信号可叠加过阈值（见单测 `combined_weak_signals_flag`：HF 对称 conf 0.5 + wash conf 0.5 → 合成 1.0 → is_bot）。`hit_rules` 保留全部命中记录与各自 evidence。

## 5. worker 聚合口径

`services/venue-hub/src/workers/perf.rs` 的 `aggregate_bot_stats()` 从已加载的数据纯内存聚合（无额外 DB 往返）：

| 字段 | 来源 |
|---|---|
| `n_trades` / `n_buys` / `n_sells` | `raw_trades` 按 `side` 计数 |
| `symmetric_ratio` | `1 - \|buys-sells\|/(buys+sells)` |
| `self_trade_count` | 同 `(tx_hash, token_id)` 同时含 BUY+SELL 的组数 |
| `round_trips` | `position_timeline` 中 `is_closed && holding_seconds ≤ 3600` |
| `median_hold_secs` | 全部配对 hold 中位；无配对 → -1 |
| `unique_conditions` | distinct 非空 `condition_id` |
| `large_trade_count` | `size * price ≥ sc_large_notional` 的笔数 |
| `n_resolved` / `n_resolved_wins` | `perf.wins + losses` / `perf.wins` |

perf worker 每 tick：对每个可见 trader 重建 `position_timeline` → 算绩效 → 在 `All` 周期分支聚合 `AggregatedStats` → `detect_with()` → 写 `trader_tag.tags`（加 `bot` / `bot:<rule>`）+ `tag_attrs.bot`（`BotFlags` 含 evidence）。下一轮覆盖写，幂等。

## 6. 阈值可调可审计

全部阈值在 [`BotFilterConfig`]（`crates/botfilter/src/lib.rs`），默认值见 `impl Default`。生产环境从 `trader_hub.tag_rules` 表读 `rule_id='botfilter'` 行，`params` jsonb 反序列化为 `BotFilterConfig`（migration 0021 seed 默认值）。

**运营调阈流程**：
1. admin 后台「标签阈值规则」页 → 找到 `rule_id=botfilter` 行 → 编辑 `params` JSON → 保存。
2. perf worker 下一 tick 读取新阈值，重算所有 trader 的 bot 判定。
3. 阈值改动可审计：`tag_rules.updated_by` / `updated_at` 记录修改人与时间。

**回退策略**（`load_bot_filter_config`）：
- 行不存在 → default()
- `enabled=false` → default()（注：仍跑规则，只是默认阈值；要完全关闭需把 `bot_threshold` 调到 >1.0）
- `params` 解析失败 → default() + warn 日志

**可审计性**：
- 每条命中带 evidence，前端交易者详情页「机器人检测」面板可下钻到具体计数/比率/阈值。
- 15 个纯单测覆盖 6 条规则命中/未命中/弱信号叠加/时长未知弱命中/v2 反例/序列化 round-trip（见 `crates/botfilter/src/lib.rs` tests）。

## 7. 消费方

| 消费方 | 行为 | 位置 |
|---|---|---|
| 排行榜 | `include_bots=false`（默认）→ SQL `NOT COALESCE(tags @> ARRAY['bot'], false)` 排除 | `crates/db/src/queries/traders.rs::list_leaderboard` |
| 排行榜总数 | 同上口径，分页「显示 1-50 / N」的 N 也排除 bot | `count_leaderboard` |
| 跟单门控 | 跟随 `bot` 标签的交易者 → 400 BadRequest | `services/follow/src/routes.rs::create_follow` |
| 前端排行榜 | 「隐藏机器人」toggle（默认勾选）+ Bot 列红色徽章 | `apps/web/static/pages/leaderboard.js` |
| 前端详情页 | 「机器人检测」面板：状态 + 置信度 + 命中规则 + evidence 下钻 | `apps/web/static/pages/trader.js::botPanel` |

## 8. 二期路线（需新数据源）

- **MakerTakerSelfPair** — OrderFilled 自配对（同 tx+token 内 wallet 兼 maker/taker）。需 `raw_order_fills` 表（maker/taker 字段），Polymarket CLOB `/trades` 不暴露，需链上事件或 `/order-fills` 端点另起 ingest。比 WashTrade（raw_trades 同 tx 买卖共存）更精准。
- **KnownEntity** — `wallet_labels` 已知实体（exchange / market_maker / mev_bot / custody）直接标 bot。需建 `wallet_labels` 表 + 维护已知实体地址库（手动 seed，低频维护）。
- **niche_match / 完整 14–15 维 Polydata** — 价格冲击对冲、链上聚类、时间优先性、自我成交精检等，需更细粒度输入。

## 9. 已知限制

- **冷启动**：纯冷导入地址在增量成交沉淀前聚合会产出近乎空 `AggregatedStats`（n_trades=0），不命中任何规则 → clean。需等 backfill + perf worker 跑完才有有效判定。
- **per-platform 阈值**：当前 `tag_rules` 是全局的（无 platform 列）。默认阈值是 Polymarket 量级；扩到其他 venue（Kalshi/Manifold 成交稀疏）可能需 per-platform 阈值——后续给 `tag_rules` 加 `platform` 列或用 `rule_id='botfilter:kalshi'` 约定。
- **DW/type-3 标签阈值未接 `tag_rules`**：现有 `TagThresholds`（DW:diamond / DW:win / type-3:*）仍用 `TagThresholds::default()` 硬编码，是 pre-existing gap。可按同一模式接入（`rule_id='tag_thresholds'`）。
- **conviction 乘性惩罚未接入**：一期仅落标签 + 过滤 + 门控；`scoring::bot_filter_penalty`（confidence=1 → conviction≈0）待 scoring crate 落地后接入。
