# VenueHub · 存储内容总览（多平台原生版）

> VenueHub 是 sharpside 的"多平台交易者数据中心"，负责多 Venue 采集、市场映射、跨 Venue 身份、绩效计算、热钥浮仓监控、影子校验。
> 所有数据落在 PostgreSQL 的 `trader_hub` schema，按"原始层 / 实体层 / 映射层 / 身份层 / 计算层 / 监控层 / 运营层 / 影子层"八层组织。

## 1. 八层存储总览

```
原始层   raw_trades, raw_positions, raw_closed_positions, raw_prices, raw_markets
   ↓
实体层   traders, identities
   ↓
映射层   market_mappings
   ↓
身份层   identity_performance（物化视图）
   ↓
计算层   position_timeline, trader_performance, trader_equity_curve, trader_tag
   ↓
监控层   hot_wallets, trader_positions_snapshot
   ↓
运营层   tag_rules, category_mapping, fetch_state, audit_thresholds, user_venue_credentials
   ↓
影子层   trader_performance_third_party, metric_audit
```

每层职责独立，原始层可重算上层，互不污染。所有业务表带 `platform` 列，跨平台身份与映射在专用层处理。

## 2. 原始层（raw · 保留各 Venue API 原貌）

只存各 Venue 官方 API 返回的原始字段，便于重算与回溯。所有 raw 表带 `platform` 列。

### raw_trades
来源：各 signal_source Venue 的 trades 端点
| 列 | 说明 |
|---|---|
| platform | Polymarket / Manifold / Zeitgeist / Azuro |
| address | 交易者在该 Venue 的标识（proxy wallet / user id） |
| token_id, condition_id | Venue 内合约标识 |
| side | BUY/SELL |
| price, size | |
| ts | |
| tx_hash | 去重键 unique（链上 Venue）；玩钱 Venue 用 (platform, trade_id) |
| fetched_at | |
| 索引 | (platform, address, ts), (platform, condition_id, ts) |

### raw_positions
来源：各 signal_source Venue 的 positions 端点（热钥高频快照）
| 列 | 说明 |
|---|---|
| platform, address, token_id, condition_id | |
| size, avg_price, current_price, pnl | |
| captured_at | 快照时间 |
| 索引 | (platform, address, captured_at) |

### raw_closed_positions
来源：各 Venue 的 closed-positions 端点（若有）
| 列 | 说明 |
|---|---|
| platform, address, token_id, condition_id | |
| realized_pnl, opened_at, closed_at, outcome | |
| fetched_at | |

### raw_prices
来源：各 Venue 的价格历史端点（Polymarket CLOB /prices-history，Kalshi bars，Manifold market-probs）
| 列 | 说明 |
|---|---|
| platform, token_id | |
| ts, price, interval | |
| 索引 | (platform, token_id, ts, interval) unique |

### raw_markets
来源：各 Venue 的 markets 端点（Polymarket Gamma，Kalshi markets，Manifold markets）
| 列 | 说明 |
|---|---|
| platform, venue_market_id | 复合唯一 |
| title, slug, tags, category | |
| end_date | 结算时间 |
| outcome_yes, outcome_no | 结算后填 |
| fetched_at | |
| 索引 | (platform, venue_market_id) unique, (end_date) |

## 3. 实体层

### traders
某 Venue 上的交易者主表。**复合主键 `(platform, address)`**。
| 列 | 说明 |
|---|---|
| platform | text PK 一部分 |
| address | text PK 一部分（proxy wallet / user id，小写） |
| identity_id | uuid 可空，指向 identities 表 |
| alias | 站内显示名 |
| source | `leaderboard`/`imported`/`manual` |
| is_hot | bool 是否进热钥浮仓监控 |
| visibility | `visible`/`hidden`/`featured` |
| profile_image, x_username, verified_badge, user_name | 来自 Venue API |
| first_seen, updated_at | |
| 索引 | (is_hot) WHERE is_hot, (visibility), (identity_id) |

### identities
跨 Venue 的同一人聚合。
| 列 | 说明 |
|---|---|
| id | uuid PK |
| alias | 跨平台身份别名 |
| confidence | 启发式聚合置信度 0–1 |
| manual_verified | bool |
| verified_by, verified_at | |
| created_at | |
| 索引 | (manual_verified) |

## 4. 映射层

### market_mappings
跨 Venue 同事件合约等价关系。字段与 `ARCHITECTURE.md` §8.1 / `VENUE_DESIGN.md` §6.1 保持一致。
| 列 | 说明 |
|---|---|
| from_platform, from_market_id | 复合 PK 一部分 |
| to_platform, to_market_id | 复合 PK 一部分 |
| confidence | 启发式匹配置信度 0–1 |
| manual_verified | bool，跨 Venue 跟单必须为 true |
| verified_by, verified_at | |
| direction_flip | bool，YES↔NO 翻转（跟反方向会亏光） |
| resolution_notes | 人工标注的结算差异/对齐说明 |
| resolution_verified | bool，跨 Venue 跟单必须为 true |
| min_notional | 该映射建议的最小成交额，低于此跳过 |
| status | `active`/`retired`/`rejected` |
| retired_at | 映射失效时间 |
| created_at | |
| 索引 | (from_platform, from_market_id), (to_platform, manual_verified, resolution_verified, status) |

跨 Venue 跟单只读 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射。

**执行参数（ExecParams）不入库，运行时拼装**：Copier 下单前按 `VenueInfo` + 映射行组装 `ExecParams`（与 `VENUE_DESIGN.md` §6.4 对齐），不另设存储表：

| 参数 | 来源 | 说明 |
|---|---|---|
| `taker_fee_bps` | `VenueInfo`（per-Venue 默认，如 Kalshi 峰值 175bps、Polymarket 75–180bps 按类目） | 从成交回报扣减，下单前仅校验 |
| `min_notional` | `market_mappings.min_notional`，缺省回退到 Venue 默认 | 低于此 notional 跳过 |
| `max_slippage_bps` | `VenueInfo` + 档位/用户覆盖（三级风控，见 `FLOWS.md` §10） | 下单前 `Venue::book()` 比对中间价，超限拒单 |

费率/滑点参数随 Venue 与档位差异化，不固化在映射行里；`market_mappings` 只承载「该映射建议的最小成交额」这一映射级门槛。

## 5. 身份层

### identity_performance（物化视图）
聚合某 identity 下所有 trader 的绩效。
| 列 | 说明 |
|---|---|
| identity_id, period | 复合 PK |
| realized_pnl, cost_basis, roi | 聚合重算 |
| win_rate, sharpe, max_drawdown | 聚合重算 |
| trader_count | 该 identity 关联的 trader 数 |
| computed_at | |
| 刷新 | 每日定时 REFRESH MATERIALIZED VIEW |

## 6. 计算层

### position_timeline
由 raw_trades 重建的仓位时间线（每 (platform, address, token_id) 一行）。
| 列 | 说明 |
|---|---|
| platform, address, token_id, condition_id | 复合键 |
| opened_at, closed_at | |
| total_bought_size, total_sold_size, avg_cost | |
| realized_pnl, final_open_size, is_closed | |
| holding_seconds | median 用于 DW:diamond |
| computed_at | |
| 索引 | (platform, address, is_closed), (platform, address, opened_at) |

### trader_performance
按周期物化的绩效（覆盖写 `1d`/`1w`/`1m`/`1y`/`ytd`/`all` 六行，per (platform, address)）。
| 列 | 说明 |
|---|---|
| platform, address | 复合 PK 一部分 |
| period | 复合 PK 一部分 `1d`/`1w`/`1m`/`1y`/`ytd`/`all` |
| roi, sharpe, sortino, win_rate, max_drawdown | |
| realized_pnl, unrealized_pnl | |
| gross_profit, gross_loss, profit_factor | |
| wins, losses, position_count, open_positions | |
| total_volume, cost_basis | |
| computed_at | |
| 索引 | (period), (platform, period) |

### trader_equity_curve
每日 mark-to-market 权益曲线。
| 列 | 说明 |
|---|---|
| platform, address, date | 复合键 |
| equity, daily_pnl, drawdown_pct | |
| 索引 | (platform, address, date) |

### trader_tag
DW / type-3 标签（per (platform, address)）。
| 列 | 说明 |
|---|---|
| platform, address | 复合 PK |
| tags | text[]（`DW:diamond`, `DW:win`, `type-3:limit_sniper`…） |
| tag_attrs | jsonb 打标依据 |
| tagged_at | |

## 7. 监控层（热钥浮仓）

### hot_wallets
热钥清单与抓取配置（per Venue）。
| 列 | 说明 |
|---|---|
| platform, address | 复合 PK |
| added_by, added_at | |
| priority | 抓取优先级 |
| scan_interval_secs | 自适应基准（10–60s） |
| enabled | bool |
| 索引 | (enabled, priority) |

### trader_positions_snapshot
热钥当前浮仓最新快照（带 platform）。
| 列 | 说明 |
|---|---|
| platform, address, token_id, condition_id | |
| size, avg_price, current_price, pnl | |
| captured_at | |
| 索引 | (platform, address, captured_at) |
| 归档 | 90 天后转对象存储 |

## 8. 运营层

### tag_rules
标签阈值，运营后台可调，零代码改动。
| 列 | 说明 |
|---|---|
| rule_id | text PK |
| params | jsonb |
| enabled | bool |
| updated_by, updated_at | |

### category_mapping
某 Venue 的官方 category → 站内分类映射（per platform）。
| 列 | 说明 |
|---|---|
| platform, official_category | 复合 PK |
| site_category, display_name | |

### fetch_state
抓取游标与限流状态，per (platform, source, address)。
| 列 | 说明 |
|---|---|
| platform, source, address | 复合键（address 可空） |
| last_ts, last_tx_hash | |
| last_run_at, status, error_msg | |

### audit_thresholds
影子校验阈值（per metric）。
| 列 | 说明 |
|---|---|
| metric_name | text PK |
| warn_pct, warn_abs, alert_pct, alert_abs | |
| updated_at | |

### user_venue_credentials
用户 per-Venue 凭证（加密存储，**绝不存明文私钥**）。由 account 服务写入，copier 读取。
对应 `docs/CHANNEL_A_SIGNING.md` §2.2。

| 列 | 说明 |
|---|---|
| user_id, platform | 复合 PK |
| encrypted_blob | KMS 主钥加密的凭证 blob（jsonb，结构按 platform + kind 不同，见下） |
| proxy_address | Deposit Wallet / 代理钱包地址（DepositWalletDelegated 时 = deposit wallet 地址；便于按地址索引/对账） |
| updated_at | |
| 索引 | (user_id), (platform, proxy_address) |

**encrypted_blob 结构**：

```json
// Polymarket · DepositWalletDelegated（主路径，FrenFlow 式，POLY_1271，新 API 用户推荐）
{ "kind": "deposit_wallet_delegated",
  "deposit_wallet_address": "0x...", "owner_address": "0x...",
  "encrypted_owner_key": "AQICAHh...", "l2_api_key": "poly-uuid",
  "encrypted_l2_secret": "AQICAHh...", "l2_passphrase": "pass",
  "builder_code": "sharpside-builder" }

// Polymarket · Wallet（旧，dev/兼容）
{ "kind": "wallet", "encrypted_handle": "..." }

// Kalshi
{ "kind": "kyc_api_key", "encrypted_api_key": "...", "encrypted_api_secret": "..." }

// Manifold
{ "kind": "api_key", "encrypted_key": "..." }
```

## 9. 影子层（第三方对照，仅监控不展示）

### trader_performance_third_party
| 列 | 说明 |
|---|---|
| platform, address, source, period | 复合 PK |
| roi, win_rate, realized_pnl, unrealized_pnl, wins, losses, markets_count, total_volume | |
| fetched_at | |
| 索引 | (source, period, fetched_at) |

### metric_audit
| 列 | 说明 |
|---|---|
| id | bigserial PK |
| platform, address, source, period, metric_name | |
| self_value, third_party_value, diff_abs, diff_pct | |
| status | `ok`/`warn`/`alert` |
| audited_at | |
| 索引 | (status, audited_at), (platform, address, metric_name) |

## 10. 索引与分区策略

| 表 | 策略 |
|---|---|
| raw_trades | 按 (platform, ts) 月分区 |
| raw_positions | 按 (platform, captured_at) 月分区 |
| trader_equity_curve | 按 (platform, date) 年分区 |
| trader_positions_snapshot | 90 天热 + 归档对象存储 |
| 其余表 | 普通索引即可 |

## 11. 保留与归档

| 表 | 保留期 | 归档 |
|---|---|---|
| raw_trades / raw_prices / trader_equity_curve | 永久 | — |
| raw_positions / trader_positions_snapshot | 90 天热 + 1 年冷 | 转 S3 |
| metric_audit | 1 年 | 转 S3 |
| trader_performance_third_party | 90 天 | 删除 |
| 其余 | 永久 | — |

## 12. 数据流（写入路径）

```
各 Venue 官方 API
   ├─ signal_source Venue ─→ raw_trades / raw_positions / raw_markets
   │                            ↓
   │                          position_timeline ─→ trader_performance
   │                            ↓                 identity_performance（视图）
   │                          market_mappings ← 启发式 + 人工
   │                          identities ← 启发式 + 人工
   │
   └─ execution_venue ─→ Copier 下单（per-Venue 凭证）

运营 admin ─→ tag_rules / category_mapping / hot_wallets / market_mappings / identities / audit_thresholds

第三方 API ─→ trader_performance_third_party ─→ metric_audit（与 trader_performance 对比）
```

## 13. 读写职责边界

| 写入者 | 写入表 |
|---|---|
| 采集 worker | raw_* / fetch_state / traders(upsert) / trader_positions_snapshot |
| 映射 worker | market_mappings（候选） |
| 身份 worker | identities（候选） / traders.identity_id |
| 重建 worker | position_timeline |
| 计算 worker | trader_performance / trader_equity_curve / trader_tag / 刷新 identity_performance |
| 影子 worker | trader_performance_third_party / metric_audit |
| account 服务 | user_venue_credentials |
| admin 后台 | tag_rules / category_mapping / hot_wallets / audit_thresholds / market_mappings(verified) / identities(verified) / traders.visibility |

| 读取者 | 读取表 |
|---|---|
| VenueHub API | trader_performance / identity_performance / trader_equity_curve / trader_tag / traders / identities / market_mappings / trader_positions_snapshot |
| Follow 服务 | trader_positions_snapshot（信号派生） |
| Copier 服务 | market_mappings（映射翻译）/ user_venue_credentials（执行凭证） |
| admin 报表 | 全部（只读） |

## 14. 体量估算

| 表 | 单交易者量级 | 1 万交易者 × 3 Venue |
|---|---|---|
| raw_trades | ≤3500 条（Polymarket）/ 不限（Manifold） | ~1 亿 |
| raw_positions（热钥） | 100/天 | 仅热钥，约 500×3×100 = 15 万/天 |
| position_timeline | ~50 仓位 | 150 万 |
| trader_equity_curve | 365 行/年 | 1095 万 |
| trader_performance | 3 行 | 9 万 |
| market_mappings | — | ~数千（按事件对数） |
| identities | — | ~数千（按跨平台人数） |

单实例 PG（16C/64G）可承载 10 万级交易者 × 3–4 Venue，无需早期分库。
