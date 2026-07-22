# 关键流程时序（多平台原生版）

> 涵盖多 Venue 采集、市场映射、跨 Venue 身份、跟随建立、信号派生、双通道 × Venue 执行、影子校验。

## 1. 多 Venue 数据回填

```mermaid
sequenceDiagram
  participant CR as 爬虫/导入触发
  participant VH as VenueHub
  participant V as 各 Venue API
  participant DB as PostgreSQL
  CR->>VH: 排行榜定时 / POST /traders/import {platform, address}
  VH->>V: 调 Venue::leaderboard / positions / trades
  V-->>VH: 各 Venue 原始数据
  VH->>DB: upsert traders(platform, address, ...) / raw_trades / raw_positions / raw_markets
  VH->>VH: 绩效计算 worker（per (platform, address, period)）
  VH->>DB: upsert trader_performance / trader_equity_curve / trader_tag
  VH->>VH: 刷新 identity_performance 物化视图
  VH-->>CR: 完成
```

**要点**：每个 Venue 走自己的 adapter（实现 `Venue` trait），主路径不区分平台；Kalshi 不实现 signal_source 方法，不会被调到。

## 2. 市场映射（启发式 + 人工校对）

```mermaid
sequenceDiagram
  participant W as 映射 worker
  participant VH as VenueHub
  participant V1 as Venue A
  participant V2 as Venue B
  participant DB as PostgreSQL
  participant ADM as admin
  W->>V1: Venue::markets
  W->>V2: Venue::markets
  W->>W: 相似度匹配（title×0.5 + tag×0.3 + end_date×0.2）
  W->>DB: insert market_mappings(confidence, manual_verified=false, resolution_verified=false, status='active')
  Note over ADM: 候选进审核队列
  ADM->>DB: 确认 → update manual_verified=true, resolution_verified=true, direction_flip, resolution_notes, min_notional, verified_by, verified_at
```

**要点**：跨 Venue 跟单只读 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射；未确认的候选不影响主路径。

## 3. 跨 Venue 身份链接

```mermaid
sequenceDiagram
  participant W as 身份 worker
  participant DB as PostgreSQL
  participant ADM as admin
  W->>DB: 找 identity_id IS NULL 的 traders
  W->>W: 启发式评分（同 x_username +0.5 / 同 alias +0.3 / 持仓相似 +...）
  W->>DB: 候选链接进审核队列
  ADM->>DB: 确认 → 创建 identities 行 + update 两边 traders.identity_id
  Note over DB: identity_performance 物化视图每日刷新聚合
```

## 4. 建立跟随（单 Venue 或跨 Venue 身份）

```mermaid
sequenceDiagram
  participant U as 用户(web/TG)
  participant GW as Gateway
  participant ACC as Account
  participant VH as VenueHub
  participant FLW as Follow
  U->>GW: POST /follows {platform+address 或 identity_id, execute_venue, sizing}
  GW->>ACC: 校验 jurisdiction 允许 execute_venue
  GW->>VH: 校验 trader/identity 存在
  GW->>FLW: 创建 follow_relation
  FLW-->>GW: relation
  GW-->>U: 已跟随
```

**要点**：跟随对象可以是单 Venue 的 trader，也可以是跨 Venue 的 identity；`execute_venue` 是用户偏好的执行 Venue，受 jurisdiction 约束。

## 5. 信号派生 → 跟单指令（跨 Venue）

> 实现口径：venue-hub hot worker 检出仓位 diff 后**同步 HTTP `POST {FOLLOW_URL}/internal/signals`**（携带 `X-Internal-Secret`）→ follow 派生 → 入 **Postgres `account.copy_order` 表队列**（非 Redis）。

```mermaid
sequenceDiagram
  participant VH as VenueHub(hot worker)
  participant FLW as Follow
  participant DB as PostgreSQL(account.copy_order)
  VH->>VH: 检测某 (platform, address) 浮仓变化（快照 diff）
  VH->>FLW: POST /internal/signals {platform, trader_id, token_id, market_id, side, price, size, identity_id}
  FLW->>FLW: 匹配 active follow_relation（含 execute_venue）
  alt 跟随的是 identity
    FLW->>FLW: 命中 identity 下任一 trader 的变化都触发（须 manual_verified）
  end
  FLW->>FLW: derive_copy_orders（sizing / same_venue_only / max_notional_per_order）
  FLW->>DB: INSERT copy_order(status=pending|skipped, ...)
```

## 6. 通道 A 执行（TG Deposit Wallet 委托代签，跨 Venue）

> 详见 `docs/CHANNEL_A_SIGNING.md` §3.2。FrenFlow 式：资产在 Polymarket Deposit Wallet（POLY_1271），平台持委托交易 owner EOA（KMS）代签。
> 队列实现：copier worker 轮询 **Postgres `account.copy_order WHERE channel='tg' AND status='pending'`**（表队列，非 Redis）。

```mermaid
sequenceDiagram
  participant CPY as Copier
  participant DB as PostgreSQL(account.copy_order)
  participant MAP as market_mappings
  participant ACC as Account
  participant KMS as KMS
  participant V as 目标 Venue (CLOB)
  participant TG as TG bot
  CPY->>DB: SELECT pending copy_order(channel=tg) LIMIT batch
  CPY->>CPY: 风控引擎校验（全局×档位×user overrides + per-follow）
  alt source_venue != execute_venue
    CPY->>MAP: 查 manual_verified + resolution_verified 映射
    MAP-->>CPY: execute_market_id
    CPY->>CPY: 单位换算（UsdcCtf ↔ UsdCents）
  end
  CPY->>DB: UPDATE status=dispatched（占单防重）
  CPY->>ACC: 取 user_venue_credentials (kind=deposit_wallet_delegated)
  CPY->>KMS: 解密 encrypted_owner_key + encrypted_l2_secret
  KMS-->>CPY: owner EOA 私钥 + L2 secret
  CPY->>V: sign ERC-7739-wrapped POLY_1271 (signatureType=3, maker=signer=deposit wallet) + L2 HMAC + builderCode → POST /order
  V-->>CPY: 成交
  CPY->>DB: INSERT copy_execution + UPDATE status=filled
  CPY->>TG: 推送成交通知给用户
```

## 7. 通道 B 执行（自托管 daemon · 平台零钥 · 跨 Venue）

```mermaid
sequenceDiagram
  participant DMN as daemon(用户本地)
  participant GW as Gateway
  participant CPY as Copier
  participant MAP as market_mappings
  participant V as 目标 Venue(本地签名)
  DMN->>GW: GET /me/copy-orders?since= (daemon_api_key)
  GW->>CPY: 查 pending copy_order(channel=daemon)
  CPY-->>GW: 指令列表（含 source_venue + execute_venue）
  GW-->>DMN: 返回
  DMN->>DMN: 本地风控
  alt source_venue != execute_venue
    DMN->>GW: GET /market-mappings?from=&to=
    GW->>MAP: 查 manual_verified + resolution_verified
    MAP-->>DMN: execute_market_id
    DMN->>DMN: 单位换算
  end
  DMN->>V: 按 execute_venue 选签名方式（Polymarket 钱包 / Kalshi RSA）本地下单
  V-->>DMN: 成交
  DMN->>GW: POST /me/copy-orders/{id}/result
  GW->>CPY: 写 copy_execution + 更新 status
```

**要点**：平台全程不接触用户私钥/KYC 凭证；daemon 按 execute_venue 选本地签名方式；映射查询走 gateway 只读接口。

## 8. 管辖域路由

```mermaid
flowchart LR
  U[用户 jurisdiction] --> ACC[Account]
  ACC -->|jurisdiction=US| VEN1[可用: Polymarket(限类目) + Kalshi]
  ACC -->|jurisdiction=EU| VEN2[可用: Polymarket + Zeitgeist + Azuro]
  ACC -->|jurisdiction=OTHER| VEN3[可用: Polymarket + Manifold(信号) + Zeitgeist + Azuro]
  VEN1 --> CPY[Copier 过滤 execute_venue]
  VEN2 --> CPY
  VEN3 --> CPY
  CPY -->|execute_venue 不在允许集| SKIP[跳过 + 通知]
  CPY -->|execute_venue 在允许集| EXEC[执行]
```

**要点**：`account.users.jurisdiction` 决定可用 execution_venue 集合；Copier 在派发指令前过滤，不合规直接 skip 并通知用户。

## 9. 影子校验（per Venue）

```mermaid
sequenceDiagram
  participant W as 影子 worker
  participant TP as 第三方 API
  participant DB as PostgreSQL
  participant S as Sentry/告警
  W->>TP: 拉某 platform top N 交易者指标
  TP-->>W: 第三方绩效
  W->>DB: upsert trader_performance_third_party(platform, address, source, period, ...)
  W->>DB: 读 trader_performance 同期同 (platform, address)
  W->>W: 对比 diff，按 audit_thresholds 分类
  W->>DB: insert metric_audit
  alt status=alert
    W->>S: 告警（含 platform/address/指标/双值/diff）
  end
```

**要点**：影子校验 per (platform, address) 进行，与生产展示链路完全解耦；详见 `SHADOW_MODE.md`。

## 10. 风控三级覆盖（per Venue 可差异化）

```
全局默认（copier 配置）
  └─ 档位覆盖（free / pro_plus）
      └─ 用户覆盖（account.users.risk_overrides）
          └─ Venue 覆盖（per-Venue 风控参数，如 Kalshi 持仓上限、Polymarket 滑点容忍）
```

每次取指令时按"Venue > 用户 > 档位 > 全局"合并生效参数。不同 Venue 的费率/流动性/持仓限制差异通过 Venue 覆盖层吸收。
