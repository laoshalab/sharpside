# 第三方增强数据 · 能否入库与怎么用

> 结论先行：**技术上完全可以入库；但商用展示给终端用户需要 Enterprise 授权**。
> 主路径仍走 Polymarket 官方 API + 自算；第三方数据只做**交叉校验 / 冷启动 / 可选增强档**。

## 1. 两家服务的实际条款（2026-07 查证）

### PolyEdge（ToS 生效 2026-04-15）

| 档位 | 价格 | 授权范围 |
|---|---|---|
| Individual | $99/mo | **仅个人使用** |
| Pro | $399/mo | **仅个人使用** |
| Enterprise | 定制 | **完整商用授权**（自定义 API、专属服务器、无限 QPS） |

明确禁止：抓取其数据库、共享 API key、逆向同步引擎。
数据"as is"，不保证准确性与及时性。

### PolyNode

| 档位 | 价格 | V3 REST 历史数据 |
|---|---|---|
| Free | $0 | **不开放**（请求返回 402） |
| Starter | $50/mo | 1000 req/min |
| Growth | $200/mo | 2000 req/min |
| Enterprise | $750/mo | 4000 req/min+ |

托管服务，**无自托管选项**。数据集：12 亿 fills、2.28 亿 positions、270 万 wallets。

## 2. 三种用法与授权要求

| 用法 | 描述 | 所需授权 |
|---|---|---|
| **A. 交叉校验（影子模式）** | 拉第三方指标，与自算结果 diff，只写日志不展示 | PolyEdge Individual/Pro 即可（属个人使用） |
| **B. 冷启动 / 算法验证** | 开发期用第三方数据验证自算公式正确性，上线后关闭 | 同上 |
| **C. 增强展示** | 把第三方 roi/win_rate 入库并展示给终端用户 | **PolyEdge Enterprise / PolyNode Enterprise** |

sharpside 是面向用户的商业产品，**用法 C 必须买 Enterprise**，否则违反"personal use only"与"commercial redistribution"条款。

## 3. 推荐策略：主路径自算，第三方做 A+B，C 作为可选付费档

```
主路径（默认）：
  Polymarket 官方 API → 自算指标 → 物化 → 展示
  （零第三方授权成本，符合"不自建链上数据"原则）

增强路径（可选，需 Enterprise 授权）：
  第三方 API → 入库 third_party 表 → 作为"验证徽章"展示
  （明示数据来源，TTL 刷新，不替代自算）
```

## 4. 入库设计（若走用法 C，需 Enterprise）

新增 schema `trader_hub` 下的表，与自算表**物理隔离**，避免混淆来源：

### trader_performance_third_party

| 列 | 类型 | 说明 |
|---|---|---|
| address | text PK | |
| source | text PK | `polyedge` / `polynode` |
| period | text PK | `1H`/`1D`/`7D`/`30D`/`ALL` |
| roi | numeric | |
| win_rate | numeric | |
| unrealized_pnl | numeric | |
| realized_pnl | numeric | |
| wins | int | |
| losses | int | |
| markets_count | int | |
| total_volume | numeric | |
| fetched_at | timestamptz | 拉取时间 |
| 索引 | (source, period, fetched_at) | |

**关键约束**：
- `fetched_at` TTL：超过 N 小时（如 6h）标记 stale，前端降级到自算值。
- 展示时必须带来源徽章（如 "Verified by PolyEdge"），满足归属要求。
- 不与 `trader_performance`（自算）合并；前端取数时优先自算，第三方作"对照值"展示。

## 5. 交叉校验模式（用法 A，零额外授权成本）

```
定时任务：
  1. 拉第三方 top N 交易者指标
  2. 查本地自算 trader_performance 同期同地址
  3. 计算 diff = |self - third_party| / third_party
  4. diff > 阈值（如 10%）→ 写入 metric_audit 表，告警
```

### metric_audit

| 列 | 说明 |
|---|---|
| address, period, metric_name | |
| self_value, third_party_value, diff_pct | |
| source | |
| audited_at | |

价值：上线前验证自算公式正确性；上线后持续监控数据漂移；发现自算 bug 的早期预警。

## 6. 成本对照

| 方案 | 月成本 | 覆盖 |
|---|---|---|
| 纯自算（官方 API） | $0 | 全量，但深度历史受 3500 条限制 |
| 自算 + PolyEdge Individual（校验） | $99 | 仅内部校验，不可展示 |
| 自算 + PolyNode Starter（校验+深度历史） | $50 | 内部校验 + 突破 3500 条限制（仍不可商用展示） |
| 自算 + PolyEdge Enterprise（展示） | 定制（通常 $1k+） | 可展示第三方指标 |
| 自算 + PolyNode Enterprise（展示+深度历史） | $750 | 可展示 + 深度历史 |

## 7. 推荐落地节奏

| 阶段 | 第三方用法 | 授权档位 |
|---|---|---|
| MVP | 不用 | $0 |
| v1 上线前 | 用法 A/B 校验自算 | PolyEdge Individual $99 或 PolyNode Starter $50（内部用） |
| v1 上线 | 主路径自算展示 | $0 |
| v2（可选） | 用法 C 增强展示 | PolyEdge/PolyNode Enterprise（按转化率决定是否值得） |

## 8. 与"不自建链上数据"原则的关系

- 用第三方增强数据**不违反**该原则——仍是消费外部数据，不自建链上索引。
- 但引入了**第三方依赖与授权成本**，需在架构上隔离（独立表 + TTL + 来源徽章），避免主路径被第三方可用性/定价绑架。
- 主路径始终是"官方 API + 自算"，第三方是"锦上添花"而非"雪中送炭"。

## 9. 决策建议

1. **MVP 不引入第三方**，专注跑通"官方 API + 自算 + 展示"闭环。
2. **上线前买一个月 PolyNode Starter（$50）或 PolyEdge Individual（$99）做交叉校验**，验证自算公式，然后可降级停用。
3. **若运营后期发现"自算指标不够权威"影响转化**，再评估买 Enterprise 做增强展示；用 A/B 测试验证付费值不值。
4. **永远不要把第三方数据当主路径**——一旦涨价/停服/改条款，整站指标会塌。
