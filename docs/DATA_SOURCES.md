# Polymarket 官方 API · 可获取数据清单

> **文档状态**：Polymarket 专属数据源清单。多平台扩展时按 Venue 增补同类文档（`DATA_SOURCES_KALSHI.md` 等），本文不另加 `platform` 维度。
>
> 三套 API：**Gamma**（市场发现）、**Data**（用户/交易/排行榜）、**CLOB**（订单簿/价格/交易）。
> Gamma 与 Data **完全公开、免鉴权**；CLOB 公开读免鉴权，下单需钱包签名。
> 所有"用户类"端点传的是 **proxy wallet 地址**（链上 Polymarket 地址），不是签名 key。

## 1. 三套 API 一览

| API | Base URL | 鉴权 | 用途 |
|---|---|---|---|
| Gamma | `https://gamma-api.polymarket.com` | 公开 | 市场、事件、标签、搜索、公开 profile |
| Data | `https://data-api.polymarket.com` | 公开 | 持仓、成交、活动、持有者、OI、排行榜、组合价值 |
| CLOB | `https://clob.polymarket.com` | 公开读 / 钱包签名写 | 订单簿、价格、中间价、价差、价格历史、下单/撤单 |
| WebSocket | `wss://ws-subscriptions-clob.polymarket.com` | 公开行情 / 鉴权用户频道 | 实时订单簿、价格、用户订单状态 |

## 2. Gamma API · 市场发现

| 端点 | 说明 |
|---|---|
| `GET /events` | 事件列表，支持过滤与分页 |
| `GET /events/{id}` | 单个事件 |
| `GET /markets` | 市场列表 |
| `GET /markets/{id}` | 单个市场（含 `condition_id`、`enableOrderBook` 等） |
| `GET /public-search` | 跨事件/市场/profile 搜索 |
| `GET /tags` | 标签/分类排名 |
| `GET /series` | 系列（成组事件，如 BTC 15min UP/DOWN） |
| `GET /sports` / `GET /teams` | 体育元数据 |

**对 sharpside 的用途**：把 `condition_id` / `token_id` / 市场标题/标签/结算时间等元数据落库，给绩效归因、市场过滤、DW/type-3 打标用。

## 3. Data API · 用户/交易/排行榜（核心数据源）

| 端点 | 关键参数 | 说明 |
|---|---|---|
| `GET /positions?user={addr}` | user, market, status | 当前持仓（含 PnL） |
| `GET /closed-positions?user={addr}` | user, market | 已平仓历史 |
| `GET /activity?user={addr}` | user | 链上活动（LP、赎回等） |
| `GET /value?user={addr}` | user | 组合总价值 |
| `GET /trades` | user, market, side, limit, offset | 成交历史（最新优先） |
| `GET /oi` | market | 市场未平仓量 |
| `GET /holders` | market | 市场头部持有者 |
| `GET /leaderboard`（`/v1/leaderboard`） | category, timePeriod, orderBy | 排行榜 |

### 3.1 排行榜返回字段（官方 `TraderLeaderboardEntry`）

| 字段 | 类型 | 说明 |
|---|---|---|
| `rank` | string | 排名 |
| `proxyWallet` | address | 钱包地址（自然键） |
| `userName` | string | 用户名 |
| `vol` | number | 成交量（USD） |
| `pnl` | number | 盈亏（USD） |
| `profileImage` | string | 头像 URL |
| `xUsername` | string | X(Twitter) 用户名 |
| `verifiedBadge` | boolean | 是否认证 |

**查询参数**：
- `category`：`OVERALL` / `POLITICS` / `CRYPTO` / `SPORTS` …
- `timePeriod`：`DAY` / `WEEK` / `MONTH` / `ALL`
- `orderBy`：`PNL` / `VOL`

### 3.2 第三方增强排行榜字段（PolyEdge / polynode 同源）

> 官方端点只给 PnL/Volume；ROI、胜率、回撤等需自行基于 `/trades`+`/positions` 计算，或参考第三方增强端点。

| 字段 | 说明 |
|---|---|
| `net_realized_pnl` | 已实现净 PnL |
| `gross_profit` / `gross_loss` | 盈/亏仓位累计 |
| `unrealized_pnl` | 浮动 PnL |
| `total_pnl` | realized + unrealized |
| `wins` / `losses` | 胜/负仓位数 |
| `position_count` / `open_positions` | 总仓位 / 当前持仓数 |
| `total_volume` | 总成交 |
| `roi` | 投入资金回报率 |
| `win_rate` | 胜率 |
| `markets_count` | 参与市场数 |
| `profile_created_at` / `last_trade_at` | 注册/最后交易时间 |

## 4. CLOB API · 订单簿与价格

| 端点 | 说明 |
|---|---|
| `GET /price` | 单 token 价格 |
| `GET /prices` | 多 token 价格 |
| `GET /book` | 单 token 订单簿 |
| `POST /books` | 多 token 订单簿 |
| `GET /prices-history` | 历史价格 |
| `GET /midpoint` | 中间价 |
| `GET /spread` | 价差 |
| `POST /order`（鉴权） | 下单 |
| `POST /orders/cancel`（鉴权） | 撤单 |

**对 sharpside 的用途**：
- 公开读：跟单下单前的滑点/深度评估、价格历史回填绩效。
- 鉴权写：通道 A（平台代用户 session wallet 签名）与通道 B（daemon 本地私钥签名）的执行入口。

## 5. 速率限制与分页约束（关键）

| 项 | 限制 |
|---|---|
| Data API 总体 | ~1000 req / 10s（超限 throttle 不 reject） |
| `/trades` | ~200 req/10s，**单次分页有效上限约 3500 条** |
| `/positions` | ~150 req/10s |

**重要约束**：
- `/trades` 单钱包历史超过 ~3500 条时，Data API 无法翻页拿到全部；**深度历史回填需走 Goldsky subgraph 或 Polygon RPC**。
- 这与"不自建链上数据"原则存在张力 → 见下文策略。

## 6. 与"不自建链上数据"原则的对齐策略

| 数据需求 | 来源 | 是否链上自建 |
|---|---|---|
| 优秀交易者发现 | Data API `/leaderboard` | 否 |
| 交易者身份/profile | Data API `/leaderboard` + Gamma `/public-search` | 否 |
| 当前持仓（浮仓快照） | Data API `/positions` | 否 |
| 近期成交（绩效样本） | Data API `/trades`（≤3500 条） | 否 |
| 已平仓历史 | Data API `/closed-positions` | 否 |
| 市场元数据 | Gamma `/events` `/markets` | 否 |
| 价格/深度 | CLOB `/book` `/prices-history` | 否 |
| **超 3500 条的深度历史** | Goldsky subgraph / Polygon RPC | **是（仅限此场景，可选）** |

**建议**：
- MVP 完全用 Data API，覆盖 95% 跟单场景（绝大多数交易者成交 < 3500 条）。
- 仅当某热钥成交深度超限且运营认定必须做长周期绩效时，**按需**对个别地址走 subgraph 补全，作为"可选增强"，不作为主路径。主路径仍保持"不自建链上数据"。

## 7. 映射到 sharpside 模块

| sharpside 模块 | 主要消费的 API |
|---|---|
| VenueHub · 排行榜爬取 | Data `/leaderboard`、Gamma `/public-search` |
| VenueHub · 钱包导入回填 | Data `/positions` `/closed-positions` `/trades` `/value` `/activity` |
| VenueHub · 浮仓快照 | Data `/positions`（仅 is_hot 钱包，10–60s 自适应） |
| VenueHub · 绩效计算 | Data `/trades`+`/positions` + CLOB `/prices-history`（算 ROI/回撤） |
| Follow · 信号派生 | 订阅浮仓快照 diff（内部事件，不直接调外部 API） |
| Copier · 通道 A 执行 | CLOB 鉴权 `/order`（用户 session wallet 签名） |
| Copier · 通道 B 执行 | daemon 本地调 CLOB 鉴权 `/order`（用户私钥签名） |
| Copier · 滑点/深度风控 | CLOB `/book` `/spread` |
