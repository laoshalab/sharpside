# 绩效数据到网站 · 端到端落地

> **文档状态**：单平台版（Polymarket only）。多平台扩展时需加 `platform` 维度（trader_performance / identity_performance 按 `(platform, address)` 与 `(identity_id, period)` 物化），与 `VENUE_DESIGN.md` §7.3 对齐。
>
> 把 Polymarket 原始 API 数据变成网站上可看、可排序、可筛选、可跟随的交易者绩效页面。
> 主链路：**采集 → 仓位重建 → 指标计算 → 物化存储 → API 暴露 → 前端展示**。

## 1. 端到端管道

```
Polymarket API
  │  Data /trades, /positions, /closed-positions, /value
  │  CLOB /prices-history
  │  Gamma /markets (结算时间/outcome)
  ▼
[1] 采集层 (ingest)        ── raw_trades / raw_positions / raw_prices
  ▼
[2] 仓位重建 (reconstruct) ── position_timeline (按 trader×token_id)
  ▼
[3] 指标计算 (compute)     ── trader_performance + equity_curve
  ▼
[4] 物化存储 (materialize) ── PostgreSQL 物化视图/表
  ▼
[5] API 暴露 (VenueHub)     ── /traders, /traders/{addr}, /traders/ranking
  ▼
[6] 前端展示 (web)         ── 排行榜 / 详情 / 跟随
```

每一步都是**离线/异步批处理**，与在线读路径解耦：网站只读物化结果，不实时算。

## 2. 第 1 步 · 采集层

落到 `trader_hub` schema 的 raw 表（保留原始字段，便于重算）：

| 表 | 来源 | 抓取策略 |
|---|---|---|
| `raw_trades` | Data `/trades?user=` | 导入时全量（≤3500 条上限内），热钥增量（5–10s 轮询，按 ts 增量） |
| `raw_positions` | Data `/positions?user=` | 热钥 10–60s 自适应快照 |
| `raw_closed_positions` | Data `/closed-positions?user=` | 每小时拉增量 |
| `raw_prices` | CLOB `/prices-history` | 按 token_id 拉日线/小时线，缓存 |
| `raw_markets` | Gamma `/markets` | 全量日刷 + 增量 |

去重键：`raw_trades (tx_hash)`、`raw_positions (address, token_id, captured_at)`。

## 3. 第 2 步 · 仓位重建

对每个 `(trader_address, token_id)`，按时间顺序回放 `raw_trades`，重建仓位时间线：

```
position_timeline:
  - 每条 trade 累加到 running_size，更新 avg_cost（加权平均）
  - SELL 时计算 realized_pnl = (sell_price - avg_cost) * sell_size
  - 结算（市场到期）时按 outcome(0/1) 计算 realized_pnl
  - 任意时刻 open_size = running_size
```

输出表 `position_timeline`：

| 列 | 说明 |
|---|---|
| address, token_id, condition_id | 自然键 |
| opened_at, closed_at | 仓位起止 |
| total_bought_size, total_sold_size | |
| avg_cost | 加权平均成本 |
| realized_pnl | 已实现 PnL |
| final_open_size | 当前剩余持仓 |
| is_closed | 是否已平仓/结算 |
| holding_seconds | 持有时长（DW 标签用） |

## 4. 第 3 步 · 指标计算（公式）

### 4.1 基础量

```
unrealized_pnl = final_open_size * (current_price - avg_cost)      # 浮动 PnL
total_pnl      = sum(realized_pnl) + unrealized_pnl
cost_basis     = sum(buy_size * buy_price)                         # 投入本金
roi            = total_pnl / cost_basis
```

### 4.2 胜率与盈亏结构

```
wins          = count(positions where realized_pnl > 0)
losses        = count(positions where realized_pnl < 0)
win_rate      = wins / (wins + losses)
gross_profit  = sum(realized_pnl where realized_pnl > 0)
gross_loss    = sum(realized_pnl where realized_pnl < 0)            # 负数
profit_factor = gross_profit / abs(gross_loss)
```

### 4.3 权益曲线与回撤（按日 mark-to-market）

```
daily_equity[t] = cumulative_realized_until[t]
                 + sum(open_size_i * mark_price_i[t])              # 当日浮仓估值
daily_pnl[t]    = daily_equity[t] - daily_equity[t-1]

max_drawdown = max over t of (peak_before[t] - equity[t]) / peak_before[t]
sharpe        = mean(daily_pnl) / std(daily_pnl) * sqrt(365)        # 年化
sortino       = mean(daily_pnl) / std(daily_pnl where <0) * sqrt(365)
```

`mark_price` 取 CLOB `/prices-history` 当日收盘；结算后 outcome=1 的 YES token 记 1，NO 记 0。

### 4.4 运营标签

```
DW (Diamond/Win):
  - Diamond 子标: median(holding_seconds) > 24h  (持有型)
  - Win    子标: win_rate > 60% 且 roi > 0       (高胜率型)

type-3 (交易手法):
  - limit_sniper : 限价单占比 > 70% 且 fill 时长 < 2 block
  - market_follow: 市价单占比 > 70%
  - rebalance    : 同 token_id 单日反向交易次数 > 阈值
```

标签规则放在 `trader_hub.tag_rules` 表，运营后台可调阈值，无需改代码。

## 5. 第 4 步 · 物化存储

计算结果写入两张物化表（按周期重算）：

### `trader_performance`（已见 `VENUEHUB_STORAGE.md` §6，按 period 维度）

每次重算覆盖写入 `1d / 1w / 1m / 1y / ytd / all` 六行（对应前端周期 tab `1天/1周/1个月/1年/年初至今/全部`）。

### `trader_equity_curve`（新增）

| 列 | 类型 | 说明 |
|---|---|---|
| address | text | |
| date | date | |
| equity | numeric | 当日权益 |
| daily_pnl | numeric | |
| drawdown_pct | numeric | 当日回撤 |
| 索引 | (address, date) | |

### `trader_tag`（新增）

| 列 | 类型 | 说明 |
|---|---|---|
| address | text PK | |
| tags | text[] | `DW:diamond`, `DW:win`, `type-3:limit_sniper`… |
| tagged_at | timestamptz | |

## 6. 重算节奏

| 对象 | 频率 | 触发 |
|---|---|---|
| 热钥浮仓快照 | 10–60s 自适应 | 定时 |
| 热钥绩效（1d/1w） | 每小时 | 定时 |
| 全量绩效（1m/1y/ytd/all） | 每日 | 定时 |
| 权益曲线 | 每日 | 定时 |
| 标签 | 每日 + 运营手动 | 定时/admin |
| 导入钱包 | 导入后立即一次 + 之后每日 | 事件 |

用 Redis streams 做任务队列，worker 按地址粒度加锁，避免并发重算同一交易者。

## 7. 第 5 步 · API 暴露（VenueHub）

```
GET /traders?sort=pnl|roi|win_rate|sharpe|max_drawdown&period=1d|1w|1m|1y|ytd|all
            &tag=DW:win&type-3=limit_sniper&min_volume=&page=&size=
  → 列表：address, alias, tags, pnl, roi, win_rate, sharpe, max_drawdown, volume, rank

GET /traders/{address}
  → 详情：profile + performance(六周期) + 当前持仓 + 标签

GET /traders/{address}/equity-curve?period=1m
  → [{date, equity, daily_pnl, drawdown_pct}]

GET /traders/{address}/trades?limit=&offset=
  → 成交明细

GET /traders/{address}/positions
  → 当前持仓 + 浮动 PnL

GET /traders/ranking?category=&timePeriod=&orderBy=
  → 直接代理官方排行榜字段 + 本地增强字段合并
```

API 只读物化表，单查询 < 50ms；排行榜首屏走 Redis 缓存（30s TTL）。

## 8. 第 6 步 · 前端展示（web）

### 8.1 页面结构

| 路由 | 内容 |
|---|---|
| `/traders` | 排行榜表格，列可排序，标签/分类/周期可筛选 |
| `/traders/[address]` | 详情：权益曲线 + 绩效卡 + 持仓表 + 成交表 + 标签 |
| `/traders/[address]/follow` | 跟随配置（sizing/过滤/上限）→ POST /follows |
| `/import` | 输入钱包地址导入 → 触发回填 → 跳转详情 |
| `/me/follows` | 我跟随的交易者及复制收益 |

### 8.2 关键组件

- **TraderTable**：虚拟滚动表格，列 = 排名/别名/标签/PnL/ROI/胜率/Sharpe/回撤/成交量；点击行 → 详情。
- **EquityChart**：基于 `/equity-curve` 的折线 + 回撤阴影区（echarts-rs WASM 桥接，与 `TECH_STACK_RUST.md` §5 对齐）。
- **MetricCards**：PnL / ROI / 胜率 / Sharpe / 最大回撤 / 持仓数，带 1d/1w/1m/1y/ytd/all 切换。
- **PositionTable**：当前持仓 + 浮动 PnL，实时刷新（热钥 10s 轮询）。
- **TagBadges**：DW:diamond / DW:win / type-3:* 彩色徽章。
- **FollowButton**：列表行内 + 详情页头部，点击弹 sizing 配置抽屉。
- **ImportBox**：输入地址 → 调 `/traders/import` → 进度条（回填任务状态轮询）。

### 8.3 性能策略

- 排行榜首屏 SSR（Leptos axum 集成），SEO 友好；翻页/排序客户端走 hydration + 信号驱动缓存。
- 权益曲线降采样（`GET /equity-curve?granularity=hour|day|auto`，默认 `auto`）：
  - `hour`：全历史小时级（原始粒度，长历史 trader 点数多）。
  - `day`：全历史日级（`date_trunc('day')` 取每日末点）。
  - `auto`：近 30 天小时级 + 30 天前日级（UNION ALL），兼顾近期平滑度与长历史规模。
    单曲线点数上限约 `720 + 365×N年`，1.8 年历史 trader 实测 15780 → 1349 点（降 91%）。
  - 前端按 period（1d/1w/1m/1y/ytd/all）在已降采样曲线上二次切片，无需额外请求。
- 热键详情页 WebSocket 推送浮仓变化（可选，MVP 用轮询即可）。

## 9. 运营介入点

| 介入点 | 位置 | 方式 |
|---|---|---|
| 调整标签阈值 | `tag_rules` 表 | admin 后台改阈值，下次重算生效 |
| 手动加/降热钥 | `traders.is_hot` | admin 一键切换，影响浮仓抓取频率 |
| 强制重算某交易者 | 任务队列 | admin 触发，按地址加锁重算 |
| 排行榜分类映射 | `category_mapping` | 官方 category → 站内分类 |
| 隐藏/置顶交易者 | `traders.visibility` | 运营管控展示 |

## 10. 与"不自建链上数据"的边界

- 全程只读 Polymarket API + 物化计算结果，**不索引链上事件**。
- 唯一例外：某热钥 `raw_trades` 超 3500 条且需长周期绩效时，**按需**走 Goldsky subgraph 补全该地址历史，作为可选增强，不进主路径。
- 计算结果全部物化在本地 PG，网站只读本地表 → 读路径快、稳、可运营。
