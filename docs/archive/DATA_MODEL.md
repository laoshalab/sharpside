# 数据模型（已归档 · ARCHIVED）

> **⚠️ 本文档已归档，不再作为 schema 权威。**
> 多平台原生升级已完成，schema 权威改为以下两份文档：
> - **`VENUEHUB_STORAGE.md`** —— VenueHub 存储总览（八层 + 全表字段）
> - **`TRADERS_TABLE.md`** —— `traders` 表字段详解
>
> 本文档保留仅作历史参考：缺 `platform` 复合主键、`identities` / `market_mappings` / `hot_wallets` / `user_venue_credentials` 等多平台表、`jurisdiction` 字段。**实现时切勿以本文为准**，任何与权威文档冲突处以权威文档为准。
>
> PostgreSQL 单实例，按服务域分 schema：`trader_hub`、`follow`、`copier`、`account`。跨域只读引用通过 `address` 等自然键关联，不做强外键，便于服务独立演进。

## schema: trader_hub

### traders
| 列 | 类型 | 说明 |
|---|---|---|
| address | text PK | Polymarket 钱包地址（小写） |
| alias | text | 别名（可空，运营可改） |
| source | text | `leaderboard` / `imported` / `manual` |
| is_hot | bool | 是否热钥（进入浮仓监控） |
| tags | text[] | `DW`、`type-3` 等标签 |
| first_seen | timestamptz | |
| updated_at | timestamptz | |

### trader_positions_snapshot
热钥浮仓快照（仅 is_hot=true 的钱包高频更新）。
| 列 | 类型 | 说明 |
|---|---|---|
| id | bigserial PK | |
| address | text | 关联 traders.address |
| token_id | text | CTF token id |
| condition_id | text | 市场 condition |
| size | numeric | 持仓量 |
| avg_price | numeric | |
| captured_at | timestamptz | 抓取时间 |
| 索引 | (address, captured_at) | |

### trader_trades
回填的历史成交（导入钱包或排行榜钱包）。
| 列 | 类型 | 说明 |
|---|---|---|
| id | bigserial PK | |
| address | text | |
| token_id | text | |
| side | text | BUY/SELL |
| price | numeric | |
| size | numeric | |
| ts | timestamptz | 链上时间 |
| tx_hash | text | 去重用 |
| 索引 | (address, ts) unique(tx_hash) | |

### trader_performance
绩效快照（按周期重算）。
| 列 | 类型 | 说明 |
|---|---|---|
| address | text PK | |
| period | text PK | `1d`/`1w`/`1m`/`1y`/`ytd`/`all` |
| roi | numeric | |
| sharpe | numeric | |
| win_rate | numeric | |
| max_drawdown | numeric | |
| pnl_usd | numeric | |
| volume_usd | numeric | |
| computed_at | timestamptz | |

## schema: follow

### follow_relation
| 列 | 类型 | 说明 |
|---|---|---|
| id | uuid PK | |
| user_id | uuid | 关联 account.users |
| trader_address | text | 关联 traders.address（自然键） |
| sizing_mode | text | `proportional`/`fixed`/`mirror` |
| sizing_value | numeric | 比例/固定金额/镜像系数 |
| max_per_trade | numeric | 单笔上限 |
| max_daily | numeric | 日上限 |
| filters | jsonb | dust 过滤、市场过滤等 |
| status | text | `active`/`paused`/`stopped` |
| created_at | timestamptz | |
| 索引 | (user_id), (trader_address, status) | |

## schema: copier

### copy_order
Follow 派生的待执行指令。
| 列 | 类型 | 说明 |
|---|---|---|
| id | uuid PK | |
| follow_id | uuid | 关联 follow_relation |
| user_id | uuid | |
| trader_address | text | |
| token_id | text | |
| side | text | |
| intended_size | numeric | |
| intended_price | numeric | |
| channel | text | `tg`/`daemon` |
| status | text | `pending`/`executed`/`failed`/`skipped` |
| risk_reason | text | 风控跳过原因 |
| created_at | timestamptz | |

### copy_execution
执行结果（通道 A 平台代执行 / 通道 B daemon 上报）。
| 列 | 类型 | 说明 |
|---|---|---|
| id | bigserial PK | |
| copy_order_id | uuid | |
| filled_size | numeric | |
| filled_price | numeric | |
| tx_hash | text | |
| fee_usd | numeric | |
| executed_at | timestamptz | |
| executor | text | `platform`/`daemon` |

## schema: account

### users
| 列 | 类型 | 说明 |
|---|---|---|
| id | uuid PK | |
| telegram_chat_id | text | 可空，TG 用户 |
| email | text | 可空，web 用户 |
| tier | text | `free`/`pro_plus` |
| created_at | timestamptz | |

### subscriptions
| 列 | 类型 | 说明 |
|---|---|---|
| id | uuid PK | |
| user_id | uuid | |
| plan | text | `pro_plus` |
| status | text | |
| current_period_end | timestamptz | |

### tg_session_wallets
通道 A 平台代签授权句柄（**不存明文私钥**；定位为「平台代签」而非完全非托管，详见 `ARCHITECTURE.md` §6.3）。
| 列 | 类型 | 说明 |
|---|---|---|
| user_id | uuid PK | |
| auth_handle | text | 加密的授权句柄（KMS 主钥） |
| proxy_address | text | 关联的 Polymarket proxy |
| updated_at | timestamptz | |

### daemon_api_keys
通道 B 凭证（**仅 API key，不含私钥**）。
| 列 | 类型 | 说明 |
|---|---|---|
| id | uuid PK | |
| user_id | uuid | |
| key_hash | text | 仅存 hash |
| last_used_at | timestamptz | |
| revoked_at | timestamptz | |
