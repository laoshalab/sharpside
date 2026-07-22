# 前端设计 · 用户端与运营后台

> 对应 `docs/ARCHITECTURE.md` §6.5 BFF 与 `docs/TECH_STACK_RUST.md` §5 前端栈。
> 本文是用户端 `apps/web` 与运营后台 `apps/admin` 的可交付前端设计文档，覆盖信息架构、13 个 F0 用户页 + 6 个运营后台页详设、组件库、后端契约补点与分阶段落地路线。
>
> **状态**：F0（离线可落地）已实施——13 用户页 + 6 运营后台页全部落地，逐页验收见 `docs/F0_LANDING_CHECKLIST.md`。后端 §11 补点大部分就绪，剩余缺口均有前端降级兜底。F1（Leptos 迁移）待网络环境。

## 1. 定位与原则

Sharpside 前端分两个 app：

| app | 面向 | 栈目标 | 当前 |
|---|---|---|---|
| `apps/web` | 终端用户（跟单者） | Leptos 0.7 SSR+WASM + Tailwind 4 + echarts-rs | vanilla JS 单文件 |
| `apps/admin` | 运营 | Leptos SSR（复用 web 栈） | 仅 axum HTTP API，无前端 |

设计原则：

- **离线可走 → 目标栈平滑迁移**：F0 用结构化 vanilla JS（ES Modules，零构建），按 Leptos 心智模型（页面=路由、store=signal、组件=props）组织，使 F1 迁移是"翻译"而非"重做"。
- **契约先行**：所有页面按现有后端 API 设计，缺口单列 §11，前端对未就绪端点降级占位，不阻塞 F0。
- **诚实口径**：托管等级、Phase 占位、P0 缺口在 UI 上明确标注，不含糊其辞（对标 FrenFlow "non-custodial" 卖点的差异化）。
- **SEO 友好**：排行榜/详情 SSR 直出，需登录页 noindex。

## 2. 现状盘点

| 维度 | 现状 | 缺口 |
|---|---|---|
| 用户端 | 单文件 `index.html`（128 行 vanilla JS）+ axum 反代 `/api/*` | 无路由/组件化/状态管理/设计系统，无详情/组合/凭证/委托页 |
| 运营后台 | 仅 axum API（映射/身份/热钥/标签/可见性/影子阈值） | 完全无界面 |
| 技术栈目标 | Leptos 0.7 SSR+WASM + Tailwind 4 + echarts-rs | 待网络环境（trunk/wasm-pack/Leptos crate 拉取受阻） |
| 后端契约 | gateway BFF `/me/dashboard`、venue-hub/follow/copier/account/admin 全套端点 | 部分端点缺 join/聚合/用户态视图（见 §11） |
| 离线约束 | crates.io 可达，npm/CDM/Polymarket/Telegram 不可达 | 任何依赖 CDN/npm 的方案当前不可行 |

## 3. 技术栈决策

| 阶段 | 栈 | 理由 |
|---|---|---|
| **F0 · 现在（离线）** | 结构化 Vanilla JS（ES Modules）+ 内联 CSS token + 自绘 SVG 图表 | 零构建、零依赖、离线可跑；为 F1 铺骨架 |
| **F1 · 有网络后** | Leptos 0.7 SSR+WASM + Tailwind 4 + echarts-rs | SSR 利 SEO；server functions 替手写 fetch；复用 F0 页面结构与契约 |
| **F2 · 兜底** | 若 Leptos 迭代过慢 → Next.js + OpenAPI/zod | 后端零改动（`TECH_STACK_RUST.md` 已预留此出口） |

## 4. 信息架构

```
Sharpside Web
├── 公开层
│   ├── 首页/Venue 总览        #/
│   └── 登录（钱包）            #/login
├── 发现层 Discover
│   ├── 排行榜                  #/traders     (SSR, SEO)
│   ├── 交易者详情              #/traders/{p}/{a}   (SSR)
│   ├── 跨平台身份详情          #/identities/{id}   (Phase 2)
│   └── 市场浏览                #/markets     (Phase 2)
├── 跟单层 Copy
│   ├── 我的跟随                #/follows
│   ├── 创建跟随                #/follows/new
│   └── 成交历史                #/copy-history
├── 交易层 Trade (Phase 2)
│   └── 手动下单                #/trade/{market_id}
├── 投资组合层 Portfolio
│   ├── 仪表盘                  #/dashboard   (BFF)
│   ├── 钱包（充值/提现）        #/wallet
│   └── 投资组合分析             #/portfolio
└── 账户层 Account
    ├── 订阅                    #/settings/subscription
    ├── Venue 凭证               #/settings/credentials
    ├── daemon API key          #/settings/daemon-key
    ├── 委托管理                 #/settings/delegation
    └── 安全日志                 #/settings/security-log   (Phase 2)
```

导航顶层：**发现 / 跟单 / 组合 / 账户**（F0；「交易」Phase 2），桌面顶栏下拉，移动端底栏。

## 5. 页面清单总览

| 页面 | 路由 | 阶段 | 主要 API | 详设 |
|---|---|---|---|---|
| 首页/Venue 总览 | `#/` | F0 | `GET /venue-hub/venues` | §6.7 |
| 登录 | `#/login` | F0 | `POST /account/auth/wallet`（SIWE） | §6.8 |
| 排行榜 | `#/traders` | F0 | `GET /venue-hub/traders` (扩 join) | §6.2 |
| 交易者详情 | `#/traders/{p}/{a}` | F0 | `GET /traders/{p}/{a}` `performance` + 补 3 端点 | §6.1 |
| 身份详情 | `#/identities/{id}` | Phase 2 | `GET /identities/{id}` | — |
| 市场浏览 | `#/markets` | Phase 2 | `GET /markets` | — |
| 我的跟随 | `#/follows` | F0 | `GET /follow/me/follows` `PATCH` `DELETE` | §6.9 |
| 创建跟随 | `#/follows/new` | F0 | `POST /follow/follows` | §6.10 |
| 成交历史 | `#/copy-history` | F0 | `GET /copier/me/copy-executions` (补) | §6.11 |
| 手动下单 | `#/trade/{market_id}` | Phase 2 | `POST /copier/manual-order` (补) | — |
| 仪表盘 | `#/dashboard` | F0 | `GET /me/dashboard` (BFF 补全) | §6.6 |
| 钱包（充值/提现） | `#/wallet` | F0 | `GET /copier/me/wallet` `POST /copier/me/wallet/withdraw` `GET /copier/me/wallet/withdrawals` | §6.5b |
| 投资组合 | `#/portfolio` | F0 | `GET /copier/me/portfolio` (补) | §6.3 |
| 订阅 | `#/settings/subscription` | F0 | `POST /account/me/subscription` | §6.12 |
| Venue 凭证 | `#/settings/credentials` | F0 | `GET /me/venue-credentials` `GET /me/delegation` (补) | §6.5 |
| daemon key | `#/settings/daemon-key` | F0 | `POST /me/daemon-api-key` | §6.13 |
| 委托管理 | `#/settings/delegation` | F0(查)/Phase 2(撤) | `GET /me/delegation` (补) | §6.4 |
| 安全日志 | `#/settings/security-log` | Phase 2 | `GET /me/security-log` (补) | — |

**F0 落地 13 页，全部有详设（§6.1-6.13）**。Phase 2 的 4 页（身份详情/市场浏览/手动下单/安全日志）待后端就绪后补详设。

## 6. 页面详设

### 6.1 交易者详情 `#/traders/{p}/{a}`

**定位**：全站转化点（用户在此决定跟随），数据最密的页。

**数据**：

| 区块 | API | 缺口 |
|---|---|---|
| 头部 | `GET /traders/{p}/{a}` → `Trader` | — |
| 绩效+标签 | `GET /traders/{p}/{a}/performance` → `PerformanceOut` | — |
| 权益曲线 | ❌ | 补 `GET /traders/{p}/{a}/equity-curve?period=`（读 `equity_curve` 表） |
| 当前持仓 | ❌ | 补 `GET /traders/{p}/{a}/positions`（读 `position_timeline`） |
| 近期成交 | ❌ | 补 `GET /traders/{p}/{a}/trades?limit=`（复用 `list_raw_trades_for_trader`） |

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  ← 返回排行榜                                                 │
├──────────────────────────────────────────────────────────────┤
│  [头像] 0x7b51...1a8a ✓已验证 🔥热钥  alias @x  Polymarket     │
│         首次出现 2025-03-01  归属身份 #abc(✓人工)  [跟随此交易者▸]│
├──────────────────────────────────────────────────────────────┤
│  周期 [ 1天 | 1周 | 1个月 | 1年 | 年初至今 | 全部 ]                │
├──────────────────────────────────────────────────────────────┤
│  [ROI][Sharpe][胜率][最大回撤][利润因子][持仓数]  6 KPI 卡    │
├──────────────────────────────────────────────────────────────┤
│  权益曲线                              ☑ 显示回撤阴影         │
│  (SVG 折线 + 回撤阴影 + hover 十字线 tooltip)                 │
├──────────────────────────────────────────────────────────────┤
│  标签 [DW:diamond][DW:win][type-3]  标签说明 ⓘ                │
├──────────────────────────────────────────────────────────────┤
│  当前持仓 (3)                       近期成交 (20)             │
│  市场 方向 规模 P&L 开仓于          时间 市场 方向 价 量 tx   │
└──────────────────────────────────────────────────────────────┘
```

**关键交互**：

- 周期 tab 切换：仅刷新 KPI + 曲线 + 持仓（本地从 `performance[]` 取，曲线重发）；URL `?period=1m` 可分享。
- 跟随按钮 → 弹 `follow-form` 模态（非跳转），预填 follow_platform/follow_address；提交 `POST /follow/follows`，401 跳登录回跳，400（identity 未 manual_verified）顶部红字 + 禁用提交。
- KPI 字段映射 `TraderPerformance`：roi/sharpe/win_rate/max_drawdown/profit_factor/open_positions，正绿负红。
- 绩效 None（新导入未算）显示 "—"，不参与排序置顶。

**状态矩阵**：头部 404 → "交易者不存在或已隐藏" 整页；KPI None → "计算中"；曲线/持仓/成交 empty → 友好空态；error → [重试]。

**加载**：首屏并行 `get_trader` + `get_performance`（SSR 出头部+KPI+标签）；hydrate 后并行 `equity-curve` + `positions` + `trades`（不阻塞 LCP）。

**SEO**：`<title>{alias} · {platform} · Sharpside`；og:description = "{ROI 1m} ROI · {win_rate} 胜率 · {tags}"。

**组件**：`trader-header` `stat-card` `equity-chart` `tag-chip` `follow-form` `data-table` `period-tabs` `empty-state` `skeleton`。

### 6.2 排行榜 `#/traders`

**定位**：流量入口，决定用户进不进详情页。

**数据**：当前 `GET /traders` 仅返 `Trader` 行（无绩效/标签），现有 `index.html` 的 `roi_1m`/`tags` 实为 undefined。**硬后端缺口**：扩 `GET /traders` 加 `sort/period/q/hot_only/verified_only`，join `trader_performance`+`trader_tag`，返 `LeaderboardRow{ #[serde(flatten)] trader, roi, sharpe, win_rate, max_drawdown, realized_pnl, total_volume, open_positions, tags }`（对齐 `ARCHITECTURE.md` §6.1 原设计）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  排行榜  ·  共 1,284 名                                        │
├──────────────────────────────────────────────────────────────┤
│  [🔍 地址/alias/@x]  平台[全部▾] 周期[1个月▾] 排序[ROI▾] 每页[50▾]│
│  ☐ 仅热钥  ☐ 仅已验证                                         │
├──────────────────────────────────────────────────────────────┤
│  #  交易者              ROI   胜率 Sharpe 回撤 标签    操作    │
│  1  [img]WhaleA 0x7b51  +42%  64%  1.83 -12% [DW:diamond][跟随▸]│
│  2  ...                                                        │
├──────────────────────────────────────────────────────────────┤
│  ← 1 2 3 ... 26 →                  显示 1-50 / 1,284          │
└──────────────────────────────────────────────────────────────┘
```

**筛选/排序**：`q`(地址/alias/@x 模糊) / `platform` / `period`(1d/1w/1m/1y/ytd/all，默认 1m) / `sort`(roi/sharpe/win_rate/max_drawdown/realized_pnl/total_volume，默认 roi DESC，回撤 ASC) / `hot_only` / `verified_only` / `limit`(20/50/100，默认 50)。全进 URL query 可分享。

**行规格**：#序号 / 交易者(头像+alias/地址缩略+平台 badge+🔥+✓) / ROI(+42.3% 正绿负红) / 胜率 / Sharpe / 回撤(越小越红) / 标签(最多 2 chip+溢出 +N) / [跟随▸]。点击行 → 详情页；跟随按钮 → `follow-form` 模态预填。绩效 None 显 "—" 灰。

**加载**：SSR 首屏默认 query 渲染前 50 行 + 计数（SEO）；客户端筛选/排序/翻页仅替换表格 body + `history.pushState` 同步 URL；搜索 300ms 防抖。

**SEO**：`<title>排行榜 · {platform} · {period} · Sharpside`；SSR 直出前 50 行 HTML，每行链详情页形成内链。

**组件**：`filter-bar` `data-table` `trader-cell` `tag-chip` `follow-form` `pagination` `skeleton` `empty-state`。

### 6.3 投资组合 `#/portfolio`

**定位**：FrenFlow 对比新增，留存核心。

**数据**：`crates/perf` 是为交易者建的，无用户级组合逻辑；`copy_execution` 无 `realized_pnl` 字段。**硬后端缺口**：新增 `GET /copier/me/portfolio?period=`，FIFO 仓位重建 + P&L + 权益曲线 + per_follow/per_venue 聚合 + 延迟统计，返 `Portfolio{ period, kpi, equity_curve, per_follow, per_venue, latency, recent_executions }`（复用 `crates/perf` 仓位重建模式，数据源换 `copy_execution`，按 user_id 过滤）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  投资组合              周期[1天|1周|1个月|1年|年初至今|全部]  [导出CSV]│
├──────────────────────────────────────────────────────────────┤
│  [总P&L][总ROI][持仓市值][胜率][成交数][未实现]  6 KPI 卡     │
├──────────────────────────────────────────────────────────────┤
│  权益曲线                  ☑ 回撤阴影  ☐ 叠加未实现            │
├──────────────────────────────────────────────────────────────┤
│  分跟随 P&L                  分 Venue P&L                     │
│  WhaleA +$520 43%             polymarket +$1.1k                │
│  ProB   +$310 26%             kalshi     +$120                 │
├──────────────────────────────────────────────────────────────┤
│  延迟分布（信号→成交）  中位1.2s P95 3.1s  Block0命中 0%(未启用)│
│  (直方图 5 桶 <1s/1-2s/2-3s/3-5s/>5s)                          │
├──────────────────────────────────────────────────────────────┤
│  近期成交 (20)                            [全部 →]            │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 延迟分布是**对标 FrenFlow 速度卖点的预留展示位**：数据全在（`copy_order.signal_at` + `copy_execution.executed_at`），mempool 旁路未上前诚实显示"基于官方 API，中位 1.2s"，Block0 命中率灰色"未启用"；旁路上线后自动展示命中率，无需改前端。
- 单次 `GET /copier/me/portfolio?period=1m` 拿全聚合，避免瀑布；周期切换重发。
- 导出 CSV：前端用 `recent_executions` 拼装；"全部"导出走 `GET /copier/me/copy-executions?limit=10000`（补端点）。
- 需登录，noindex（留存非获客）。

**组件**：`stat-card` `equity-chart` `period-tabs` `pnl-breakdown` `latency-histogram`(新) `data-table` `skeleton` `empty-state`。

### 6.4 委托管理 `#/settings/delegation`

**定位**：非托管话术的 UX 落地，对标 FrenFlow "non-custodial" 卖点的诚实口径。

**数据**：`GET /me/venue-credentials` 返 `UserVenueCredential` 但 `encrypted_blob` 被 `#[serde(skip_serializing)]`——前端只拿 platform/proxy_address，看不到 owner/builder/l2/状态。**硬后端缺口**：新增 `GET /me/delegation` 返安全视图 `DelegationView{ platform, custody_tier, custody_label, deposit_wallet_address, owner_address, builder_code, l2_api_key, provision_live, provision_steps, kms_key_id, created_at, can_revoke }`（解析 blob 非密字段，密钥留服务端）；provision 状态持久化（`store_credential` 写 steps 入 blob 或新表）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  委托管理                                                      │
├──────────────────────────────────────────────────────────────┤
│  托管等级 ⚠ 委托交易（未到完全非托管）  [什么是托管等级？ⓘ]      │
├──────────────────────────────────────────────────────────────┤
│  ┌─资产权─────────────────┐ ┌─交易权─────────────────┐         │
│  │ Deposit Wallet(ERC-1967)│ │ 平台 KMS 代签         │         │
│  │ 0xa7a8...3711           │ │ owner EOA 0x7b51...   │         │
│  │ 资产存放处              │ │ 平台持其私钥(KMS加密) │         │
│  │ ✓ 你可导出 owner 自行rotate│ │ ✗ 平台可签WALLET转资产│         │
│  │                         │ │ ✓ 平台可代你下单      │         │
│  └─────────────────────────┘ └─────────────────────────┘         │
├──────────────────────────────────────────────────────────────┤
│  预配状态  ①✅②✅③✅④⏳/✅/❌⑤⏳/✅/❌⑥❌[重试]⑦⏳/✅/❌⑧✅  │
│  step⑥ batch approve 是 P0 缺口，高亮红色可重试                │
├──────────────────────────────────────────────────────────────┤
│  凭证详情  平台/Deposit Wallet/Owner/L2 API Key/Builder/模式/创建│
├──────────────────────────────────────────────────────────────┤
│  撤销委托  [撤销委托（Phase 2）] 灰显+锁标                       │
│  升级非托管(Phase 2)  当前vs目标对比  [预约迁移通知]            │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- **托管等级横幅是本页灵魂**：当前 ⚠"委托交易（未到完全非托管）"，Phase 2 后 ✓"非托管交易"。tooltip 展开 `CHANNEL_A_SIGNING.md` §4.3 口径。不含糊其辞是差异化。
- 资产权/交易权双卡用 ✓/✗ 让权限边界一眼可见；Phase 2 升级后交易权卡 ✗ 变 ✓。
- 预配 stepper 8 步状态机（复用 `provision-stepper`）；step ⑥ batch approve P0 缺口高亮 + [重试]；step ② 显示 kms_key_id（dev 标"开发明文"，生产标 aws key id）；step ④ tx 链区块浏览器。
- 凭证详情只读，**绝不显示 encrypted_owner_key / encrypted_l2_secret**（前端无解密能力也不该有）。
- 撤销按钮 `can_revoke=false` 灰显 + 锁标 + 诚实文案"Phase 2 上线后可自助撤销，当前紧急撤销联系 support"。
- 预配进行中每 5s 轮询 stepper（Relayer 部署需等上链）。

**组件**：`custody-banner`(新) `permission-card`(新) `provision-stepper` `delegation-card` `copy-field`(新) `locked-button`(新)。

### 6.5 Venue 凭证 `#/settings/credentials`

**定位**：多 Venue 凭证中心（与委托管理区别：委托聚焦 Polymarket 托管/信任，凭证是所有 Venue 凭证总入口）。

**数据**：复用 `GET /me/venue-credentials` + `GET /me/delegation`（§6.4 补）+ `POST /me/deposit-wallet/provision` + `POST /me/daemon-api-key`。补：`UserVenueCredential` 加 `kind` 字段（列或从 blob 解析）供前端区分凭证类型。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  Venue 凭证                                                   │
├──────────────────────────────────────────────────────────────┤
│  ┌─Polymarket─────────────────────────────────────────────┐  │
│  │ 通道A·Deposit Wallet委托代签    ⚠委托交易               │  │
│  │ 状态：✅已预配(在线) / ⚠已预配(离线,跳过4步)            │  │
│  │ Deposit Wallet 0xa7a8...  Owner 0x7b51...              │  │
│  │ [查看委托详情→] [重新预配] [撤销(Phase 2)]              │  │
│  └────────────────────────────────────────────────────────┘  │
│  ┌─预配状态机（展开）────────────────────────────────────┐  │
│  │ ①✅②✅③✅④⏳/✅/❌⑤⏳/✅/❌⑥❌[重试]⑦⏳/✅/❌⑧✅       │  │
│  └────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────┤
│  ┌─Kalshi──────────────────────────────────────────────┐  │
│  │ 通道A/B·KYC+API key            🔒Phase 3  [配置(Phase3)]│  │
│  └────────────────────────────────────────────────────────┘  │
│  ┌─Manifold────────────────────────────────────────────┐  │
│  │ 仅信号源·API key              🔒Phase 2  [配置(Phase2)]│  │
│  └────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────┤
│  通道B·daemon API key（跨Venue）  ✅已颁发  [轮换key]         │
│  明文仅显示一次，请妥善保存。                                  │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- Polymarket 卡复用委托管理页的 `GET /me/delegation` 与 `provision-stepper`；[查看委托详情] 跳 `#/settings/delegation`；[重新预配] 二次确认（"将生成新 owner EOA，旧 deposit wallet 需手动迁移资产"）。
- Kalshi/Manifold 卡 Phase 占位，按钮灰显 + 锁标 + 阶段标，让用户预见路线图。
- daemon key 轮换 → 明文一次性弹窗，**强制勾选 [我已保存] 才能关闭**（安全关键交互，防未保存就关）。

**组件**：`venue-credential-card`(新) `provision-stepper` `locked-button` `copy-field` `one-time-secret-modal`(新) `confirm-dialog`(新)。

### 6.5b 钱包（充值/提现） `#/wallet`

**定位**：FrenFlow 对比新增，跟单前必备——用户需看到余额、充值地址、并能提现到自己的外部钱包。

**数据**：
- `GET /copier/me/wallet` → `WalletView{ venue, owner_address, deposit_wallet_address, cash_balance, provision_live, balance_note }`（复用 §6.3 portfolio 的 `build_wallet_view`：实时 CLOB `/balance-allowance`，5s 超时，离线/失败降级 `cash_balance=null` + `balance_note`）。
- `POST /copier/me/wallet/withdraw` body `{ to, amount }` → `WithdrawResponse{ id, status, to, amount, tx_hash, relayer_tx_id, note }`。
- `GET /copier/me/wallet/withdrawals?limit=&offset=` → `Vec<Withdrawal>`（审计历史）。

**提现链路**（对应 `docs/CHANNEL_A_SIGNING.md` §4.1）：owner EOA（平台 KMS 代签）对 `DepositWallet.Batch` 签 EIP-712 → 单笔 `pUSD.transfer(to, amount)` calldata → relayer `WALLET` batch gasless 提交 → 轮询至确认。落库 `account.withdrawals` 审计（pending/mined/failed）。

**风控**（copier 路由层）：
- 目标地址须为用户已绑定钱包（`account.user_wallets`）之一——防资产被转到非本人地址。
- 金额 ∈ `[WITHDRAW_MIN_AMOUNT, WITHDRAW_MAX_AMOUNT]`（env，默认 1 / 10000 pUSD）。
- 实时余额 ≥ 金额（CLOB `/balance-allowance`，5s 超时）。
- 当日累计（pending+mined）+ 金额 ≤ `WITHDRAW_DAILY_MAX`（默认 10000 pUSD）。
- 前端二次确认弹窗（金额 + 目标地址预览，"平台代签链上转账，不可撤销"）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  钱包                                                         │
├──────────────────────────────────────────────────────────────┤
│  充值                                                         │
│  [可用余额（pUSD）  $7.00  实时（CLOB /balance-allowance）]     │
│  Deposit Wallet 地址                                          │
│  0xa7a8…3711  [复制充值地址]                                  │
│  从外部钱包向此地址发送 pUSD（Polygon 链）即可充值。           │
│  pUSD 合约地址（Polygon）                                     │
│  0xC011…2DFB  [复制合约地址]                                  │
│  Polygon 主网（Chain ID 137）。添加自定义代币时用此地址。       │
│  [🔄 刷新余额]                                                 │
├──────────────────────────────────────────────────────────────┤
│  提现                                                         │
│  提现 pUSD 到你已绑定的钱包地址。目标地址须在 设置→钱包 绑定。 │
│  提现到 [选择绑定钱包…▾]   金额（pUSD）[          ]           │
│  可用余额：$7.00                                              │
│  [提现]  ⚠ 离线预配无法提现（需在线预配后充值 pUSD）          │
├──────────────────────────────────────────────────────────────┤
│  提现历史                                                     │
│  时间        目标地址      金额     状态     交易哈希   备注  │
│  …           0x7b51…1a8a  $5.00   ✅mined  0xe7d3…   —       │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：
- 充值是手动链上转账（平台无法代发起），故"充值按钮"= 地址展示 + 复制 + 实时余额 + 刷新；无"一键充值"。
- 提现目标限绑定钱包——防资产被转到非本人地址（与 §6.4 委托"平台可签 WALLET 转资产"的高敏能力对应）。
- 离线预配（`provision_live=false`）禁用提现并标注原因；余额不可查时表单仍可填但提交由后端兜底拒。
- 仪表盘（§6.6）顶部加钱包余额快捷卡 + [充值/提现 →] 跳转。

**组件**：`copy-field` `kpi` `data-table` `confirm-dialog`(浏览器 confirm) `toast`。

### 6.6 仪表盘 `#/dashboard`

**定位**：登录后首屏，BFF 单次拼装，"概览不深挖"。

**数据**：BFF `GET /me/dashboard` 已存在但三字段需补全：`follows` 路径 `/me/{user_id}/follows` 暴露 user_id 应改 `/me/follows`；`available_venues` 硬编码应按 jurisdiction 推导；`leaderboard` 用扩 join 后 `/traders`；加 `portfolio_kpi`（复用 §6.3 组合聚合取 1m kpi 子集）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  早上好，me@x.com · Pro+订阅中(至2026-12)   [Pro+权益]         │
├──────────────────────────────────────────────────────────────┤
│  [总P&L][总ROI][持仓市值][活跃跟随][今日成交][未实现] 6 KPI    │
│  [查看投资组合 →]                                              │
├──────────────────────────────────────────────────────────────┤
│  我的跟随（7）横滚卡                        [全部 →]            │
│  [WhaleA +$520 ✅active] [ProB +$310 ✅] [BigC -$80 ⏸paused] ...│
├──────────────────────────────────────────────────────────────┤
│  热门交易者（5）                            [排行榜 →]          │
│  [img]WhaleA +42% 64% [DW:diamond] [跟随▸]                     │
│  ...                                                          │
├──────────────────────────────────────────────────────────────┤
│  快捷操作  [导入地址][配置凭证][轮换daemon key][升级Pro+]      │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- **单次** `GET /me/dashboard` 拼装全页（BFF 价值，避免客户端瀑布）；登录后默认落地；30s 内不重复请求。
- BFF 已是降级模式（上游不可达返 null），前端对 null 区块隐藏不阻塞整页。
- 组合 KPI 6 卡复用 `stat-card`；我的跟随横滚卡（active/paused/error 状态标）；热门交易者精简行复用 `trader-cell`；快捷操作网格跳各专门页。
- 需登录，noindex。

**组件**：`stat-card` `follow-card-mini`(新) `trader-cell` `tag-chip` `quick-action-grid`(新) `subscription-badge`(新) `skeleton` `empty-state`。

## 7. 运营后台详设（apps/admin）

运营后台 `apps/admin` 当前仅有 axum HTTP API（`apps/admin/src/routes.rs`），无前端。本节定义 6 个审核/管控页，复用 web 的 `tokens.css` 与通用组件，按 `ARCHITECTURE.md` §14 的 **Venue × 业务面** 二维菜单组织。

### 7.1 信息架构

```
Sharpside Admin
├── 登录                      #/login        (admin token)
├── 市场映射审核              #/mappings
├── 身份审核                  #/identities
├── 热钥管理                  #/hot-wallets
├── 标签阈值                  #/tag-rules
├── 可见性管控                #/visibility
└── 影子阈值                  #/audit-thresholds
```

侧栏按 **Venue × 业务面** 矩阵：行=Venue（Polymarket / Kalshi(Phase3) / Manifold(Phase2)），列=业务面（映射/身份/热钥/标签/可见性/影子）。Phase 1b 只有 Polymarket，单行多列；灰格表示该 Venue 不具备该业务面。

鉴权：所有 admin 端点需 `AdminAuth`（`apps/admin/src/state.rs`），前端在请求头注入 admin token；401 → 跳 `#/login`。noindex。

### 7.2 市场映射审核 `#/mappings`

**定位**：候选跨 Venue 映射队列，运营确认/拒绝，须标 direction_flip / resolution_notes / min_notional。

**数据**：`GET /mappings/pending` → `Vec<MarketMapping>`；`POST /mappings/verify`；`POST /mappings/retire`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  市场映射审核（待审 23）                                        │
├──────────────────────────────────────────────────────────────┤
│  ┌─候选映射──────────────────────────────────────────────┐    │
│  │ from: polymarket/0xabc "ETH>2k by Dec"                │    │
│  │ to:   kalshi/XYZ12     "Ether Above 2000"              │    │
│  │ confidence: 0.87   direction_flip: ☐                  │    │
│  │ resolution_notes: [同事件，结算规则一致            ]   │    │
│  │ min_notional: [100                                 ]  │    │
│  │ [✓ 确认验证]  [✗ 撤销(retire)]  [跳过]                 │    │
│  └──────────────────────────────────────────────────────┘    │
│  ...                                                          │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 每张候选卡：from/to 市场标题对比 + confidence 评分 + direction_flip 复选（YES↔NO 翻转）+ resolution_notes 文本框 + min_notional 数字。
- [确认验证] → `POST /mappings/verify`（带 verified_by=admin）；[撤销] → `POST /mappings/retire`（标 status=retired）。
- 仅 `manual_verified=true AND resolution_verified=true AND status='active'` 的映射用于跨 Venue 跟单——UI 须提示运营"确认即生效进跟单路径"。
- direction_flip 是安全关键字段（跟反方向会亏光），UI 高亮 + tooltip "Polymarket YES 可能对应 Kalshi No 合约"。
- 空状态："无待审映射"。

**组件**：`mapping-card`(新) `confirm-dialog` `form-field` `toast`。

### 7.3 身份审核 `#/identities`

**定位**：跨 Venue 身份候选队列，运营确认/删除。

**数据**：`GET /identities/pending` → `Vec<Identity>`；`POST /identities/{id}/verify`；`DELETE /identities/{id}`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  身份审核（待审 8）                                             │
├──────────────────────────────────────────────────────────────┤
│  ┌─候选身份 #abc-123────────────────────────────────────┐    │
│  │ alias: "WhaleA"   confidence: 0.78                    │    │
│  │ 关联 traders:                                          │    │
│  │  · polymarket/0x7b51...1a8a  @x_handle  verified ✓    │    │
│  │  · kalshi/user_88             (待 Phase 3 接入)        │    │
│  │ 启发式依据: 同 X 用户名 + 持仓高度相似                  │    │
│  │ [✓ 确认人工校对]  [✗ 删除候选]                          │    │
│  └──────────────────────────────────────────────────────┘    │
│  ...                                                          │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 每张候选卡：alias + confidence + 关联 traders 列表（平台/地址/@x/verified）+ 启发式依据。
- [确认] → `POST /identities/{id}/verify`（置 manual_verified=true，verified_by=admin）；[删除] → `DELETE /identities/{id}`（`confirm-dialog`）。
- **跟随门禁硬规则**：用户只能跟随 `manual_verified=true` 的 Identity——UI 须提示运营"确认后该身份可被用户跟随"。
- 空状态："无待审身份"。

**组件**：`identity-card`(新) `confirm-dialog` `tag-chip` `toast`。

### 7.4 热钥管理 `#/hot-wallets`

**定位**：per Venue 热钥清单 CRUD + 优先级 + 抓取频率。

**数据**：`GET /hot-wallets?platform=` → `Vec<HotWallet>`；`POST /hot-wallets`；`DELETE /hot-wallets/{platform}/{address}`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  热钥管理                       Venue [polymarket ▾]  [+ 添加] │
├──────────────────────────────────────────────────────────────┤
│  地址              优先级  抓取频率  启用  操作                │
│  0x7b51...1a8a     100     30s      ✅   [编辑] [删除]         │
│  0x9c...           50      60s      ✅   [编辑] [删除]         │
│  ...                                                          │
└──────────────────────────────────────────────────────────────┘
┌─添加/编辑模态──────────────────────────────────────────────┐
│  平台 [polymarket]  地址 [0x...      ]                      │
│  优先级 [100]  抓取频率(秒) [30]  启用 [✓]                   │
│  added_by: admin   [保存]                                    │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- Venue 筛选下拉（来自 `/venues`），切换重拉该 Venue 热钥。
- 表格列：地址 / 优先级（数字，小=高优先）/ 抓取频率 / 启用开关 / 操作。
- [添加]/[编辑] → 模态（platform/address/priority/scan_interval_secs/enabled/added_by）；[删除] → `confirm-dialog`。
- 启用开关可即时 PATCH（若后端补 toggle 端点；当前需 DELETE+POST，UI 可简化为开关+确认）。
- 空状态："该 Venue 无热钥"。

**组件**：`filter-bar` `data-table` `hot-wallet-modal`(新) `confirm-dialog` `toast`。

### 7.5 标签阈值 `#/tag-rules`

**定位**：标签规则编辑，params 为 JSON，需带 schema 校验提示。

**数据**：`GET /tag-rules` → `Vec<TagRule>`；`PUT /tag-rules/{rule_id}`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  标签阈值规则                                                  │
├──────────────────────────────────────────────────────────────┤
│  rule_id        enabled  updated_by  更新于    操作            │
│  DW:diamond     ✅       admin      07-20     [编辑]          │
│  DW:win         ✅       admin      07-20     [编辑]          │
│  type-3         ☐        admin      07-19     [编辑]          │
└──────────────────────────────────────────────────────────────┘
┌─编辑模态────────────────────────────────────────────────────┐
│  rule_id: DW:diamond (只读)                                  │
│  params (JSON):                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ { "min_hold_days": 30, "min_roi": 0.5 }              │    │
│  └──────────────────────────────────────────────────────┘    │
│  enabled [✓]  updated_by: admin   [保存]                     │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 表格列：rule_id / enabled 开关 / updated_by / 更新于 / [编辑]。
- 编辑模态：rule_id 只读 + params JSON 编辑器（textarea + JSON 语法高亮占位，F0 纯 textarea）+ enabled + updated_by。
- 保存前前端做 JSON.parse 校验，非法 → 红字提示，不发请求。
- `PUT /tag-rules/{rule_id}`（params + enabled + updated_by）。
- 空状态："无规则"。

**组件**：`data-table` `json-editor`(新，F0 textarea) `form-field` `toast`。

### 7.6 可见性管控 `#/visibility`

**定位**：交易者可见性三态切换（visible/hidden/blocked）。

**数据**：`PATCH /traders/{platform}/{address}/visibility`（body: visibility）；交易者搜索复用 `GET /traders`（扩 join 后含全部，admin 可见 blocked）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  可见性管控                   [🔍 搜索地址/alias]  Venue[全部▾] │
├──────────────────────────────────────────────────────────────┤
│  交易者              平台   当前可见性  切换至               │
│  0x7b51...1a8a WhaleA poly  visible      [visible▾][应用]    │
│  0x9c...  ProB      poly    hidden       [hidden▾] [应用]    │
│  0x...   BadActor  poly    blocked      [blocked▾][应用]    │
│  ...                                                          │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 搜索 + Venue 筛选（复用 `filter-bar`），admin 视角可见全部（含 blocked/hidden）。
- 每行：交易者 + 平台 + 当前可见性 + 下拉切换（visible/hidden/blocked）+ [应用]。
- [应用] → `PATCH /traders/{p}/{a}/visibility`（`confirm-dialog`，"blocked 用户将无法被搜索/跟随"）。
- 后端校验 visibility ∈ visible/hidden/blocked（已实现）。
- 空状态："无匹配交易者"。

**组件**：`filter-bar` `data-table` `trader-cell` `confirm-dialog` `toast`。

### 7.7 影子阈值 `#/audit-thresholds`

**定位**：影子模式交叉校验告警阈值（warn/alert，pct+abs）per metric。

**数据**：`GET /audit-thresholds` → `Vec<AuditThreshold>`；`PUT /audit-thresholds/{metric}`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  影子阈值（交叉校验告警）                                       │
├──────────────────────────────────────────────────────────────┤
│  metric       warn_pct  warn_abs  alert_pct  alert_abs  操作 │
│  roi_1m       5%        $100     10%        $500     [编辑]   │
│  sharpe_1m    0.3       —        0.5        —        [编辑]   │
│  ...                                                          │
└──────────────────────────────────────────────────────────────┘
┌─编辑模态────────────────────────────────────────────────────┐
│  metric: roi_1m (只读)                                       │
│  warn_pct [5]  warn_abs [100]  alert_pct [10]  alert_abs [500]│
│  [保存]                                                       │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 表格列：metric / warn_pct / warn_abs / alert_pct / alert_abs / [编辑]。
- 编辑模态：metric 只读 + 4 个数字输入（warn_pct/warn_abs/alert_pct/alert_abs）。
- `PUT /audit-thresholds/{metric}`。
- tooltip："影子模式与第三方数据交叉校验，超 warn 阈值记录，超 alert 阈值告警；不影响主路径展示"。
- 空状态："无阈值配置"。

**组件**：`data-table` `form-field` `toast`。

## 8. 通用组件库

F0 vanilla JS 组件按 Leptos 组件心智模型组织（props 进、event 出），F1 迁移时机械翻译。

| 组件 | 用途 | 首次定义页 | 复用到 |
|---|---|---|---|
| `stat-card` | KPI 数值卡（标题+主值+副值+趋势色） | 详情 | 组合、仪表盘 |
| `equity-chart` | 权益曲线 + 回撤阴影（F0 SVG，F1 echarts） | 详情 | 组合 |
| `tag-chip` | 标签（带说明 tooltip） | 详情 | 排行榜、身份详情 |
| `follow-form` | 跟随配置模态（sizing/channel/execute_venue/风控） | 详情 | 排行榜、创建跟随页 |
| `data-table` | 通用表格（分页+排序+骨架） | 详情 | 全站 |
| `period-tabs` | 周期切换（1d/1w/1m/1y/ytd/all） | 详情 | 组合 |
| `trader-header` | 交易者头部信息 | 详情 | 排行榜行、跟随卡 |
| `trader-cell` | 行内交易者信息 | 排行榜 | 仪表盘、跟随列表 |
| `filter-bar` | 搜索+筛选+排序 | 排行榜 | 市场浏览、成交历史 |
| `pagination` | 分页 | 排行榜 | 全站列表 |
| `pnl-breakdown` | 分跟随/分 Venue P&L 列表 | 组合 | 仪表盘 |
| `latency-histogram` | 延迟直方图（Block0 命中率预留） | 组合 | 仪表盘 |
| `custody-banner` | 托管等级横幅 | 委托 | 凭证页、仪表盘顶部 |
| `permission-card` | 资产权/交易权双卡 | 委托 | — |
| `provision-stepper` | 8 步预配状态机 | 委托 | 凭证页 |
| `delegation-card` | 凭证详情只读卡 | 委托 | 凭证页 |
| `copy-field` | 复制字段行 | 委托 | 全站 |
| `locked-button` | 灰显+锁标按钮（Phase/Pro+ 门禁） | 委托 | 全站 |
| `venue-credential-card` | Venue 凭证卡 | 凭证 | — |
| `one-time-secret-modal` | 明文一次性弹窗（强制确认已保存） | 凭证 | daemon key、未来 API key |
| `confirm-dialog` | 二次确认 | 凭证 | 全站 |
| `follow-card-mini` | 跟随横滚卡 | 仪表盘 | 跟随列表 |
| `quick-action-grid` | 快捷操作网格 | 仪表盘 | — |
| `subscription-badge` | 订阅状态标 | 仪表盘 | 设置页 |
| `skeleton` / `empty-state` | 骨架/空态 | 详情 | 全站 |
| `form-field` | 表单字段（label+input+error） | 登录 | 全站表单 |
| `tab-switch` | tab 切换 | 登录 | 全站 |
| `follow-card` | 跟随完整卡（含操作行） | 我的跟随 | — |
| `tier-comparison-card` | 档位对比卡 | 订阅 | — |
| `mapping-card` | 映射审核候选卡 | 映射审核 | — |
| `identity-card` | 身份审核候选卡 | 身份审核 | — |
| `hot-wallet-modal` | 热钥添加/编辑模态 | 热钥管理 | — |
| `json-editor` | JSON 编辑器（F0 textarea） | 标签阈值 | — |

## 9. 状态管理与鉴权

- **路由**：F0 hash 路由（`#/traders/{p}/{a}?period=1m`），F1 换 Leptos path 路由。守卫：`#/follows` `#/dashboard` `#/portfolio` `#/copy-history` `#/settings/*` 需登录，否则 `#/login?redirect=<原路径>`。
- **auth store**：JWT 存 localStorage，全局 signal `user` + `requireAuth()`；401 全局事件 → 清 token + 跳登录。TG 通道用户由 bot 代签 JWT（`POST /account/auth/tg`），web 不直接处理。
- **toast 总线**：错误/成功提示，全局事件驱动。
- **降级**：所有上游不可达返空结构而非整页失败（BFF 已是此模式，前端对 null 区块隐藏）。

## 10. 设计系统

暗色优先（量化用户偏好），尊重 `prefers-color-scheme`。

```css
:root {
  --c-bg:#0b0f14; --c-surface:#121821; --c-border:#1f2a37;
  --c-text:#e6edf3; --c-muted:#8b98a8;
  --c-up:#22c55e; --c-down:#ef4444; --c-accent:#6366f1;
  --r-sm:6px; --r-md:10px; --r-lg:16px;
  --sp-1:4px; --sp-2:8px; --sp-3:12px; --sp-4:16px; --sp-6:24px;
  --fs-12:12px; --fs-14:14px; --fs-16:16px; --fs-20:20px; --fs-28:28px;
}
@media (prefers-color-scheme: light){ :root{ --c-bg:#fff; --c-surface:#f7f9fc; ... } }
```

## 11. 后端契约补点

按优先级与归属页整理。前端对未就绪端点降级占位，不阻塞 F0。逐项验收见 `docs/F0_LANDING_CHECKLIST.md` §3。

| 端点 | 服务 | 用途页 | 阶段 | 状态 |
|---|---|---|---|---|
| 扩 `GET /traders`（join trader_performance+trader_tag，加 sort/period/q/hot_only/verified_only） | venue-hub | 排行榜 | F0 | ✅ |
| `GET /traders/{p}/{a}/equity-curve?period=` | venue-hub | 详情 | F0 | ✅ |
| `GET /traders/{p}/{a}/positions` | venue-hub | 详情 | F0 | ✅ |
| `GET /traders/{p}/{a}/trades?limit=&offset=` | venue-hub | 详情 | F0 | ✅ |
| `GET /copier/me/portfolio?period=`（FIFO 重建+聚合+延迟） | copier | 组合 | F0 | ✅ |
| `GET /copier/me/wallet`（地址+实时 pUSD 余额，复用 build_wallet_view） | copier | 钱包/充值 | F0 | ✅ |
| `POST /copier/me/wallet/withdraw`（owner 签 WALLET batch transfer，relayer gasless，落库审计） | copier | 钱包/提现 | F0 | ✅ |
| `GET /copier/me/wallet/withdrawals?limit=&offset=` | copier | 钱包/提现历史 | F0 | ✅ |
| `account.withdrawals` 表 + 查询（提现审计日志） | db | 钱包/提现 | F0 | ✅ |
| `GET /copier/me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=` | copier | 成交历史/组合导出 | F0 | ✅ |
| `GET /venue-hub/identities`（manual_verified 列表） | venue-hub | 创建跟随下拉 | F0 | ✅ |
| `GET /me/delegation`（安全视图，密钥留服务端） | account | 委托/凭证 | F0 | ✅ |
| provision 状态持久化（steps 入 blob 或新表） | account | 委托/凭证 | F0 | ✅ |
| BFF `/me/dashboard` 补全（available_venues 按 jurisdiction、加 portfolio_kpi） | gateway | 仪表盘 | F0 | ✅ |
| `UserVenueCredential` 加 `kind` 字段 | db | 凭证 | F0 | ✅ |
| `POST /copier/manual-order` | copier | 手动下单 | Phase 2 | ⏳ |
| `POST /account/me/deposit-wallet/revoke` | account | 委托撤销 | Phase 2 | ⏳ |
| `GET /account/me/security-log` | account | 安全日志 | Phase 2 | ⏳ |

## 12. 分阶段落地路线

| 阶段 | 范围 | 页面数 | 估时 | 依赖 | 状态 |
|---|---|---|---|---|---|
| **F0.1** 骨架 + 7 核心页 | 首页/登录/排行榜/详情/跟随 CRUD/仪表盘/设置基础 | 7 | 3-4 天（含后端） | 离线 | ✅ 已实施 |
| **F0.2** 组合 + 凭证 + 委托 | 投资组合/凭证预配/委托管理 | +3 | 4-5 天（含后端） | F0.1 | ✅ 已实施 |
| **F0.3** 运营后台 | 映射/身份/热钥/标签/可见性/影子阈值 | 6 | 2 天 | F0.1（复用组件） | ✅ 已实施 |
| **F1** Leptos 迁移 | 逐页翻译，契约不变，图表换 echarts | 同 F0 | 视页数 | 网络环境 | ⏳ 待网络 |
| **Phase 2** 交易+非托管 | 市场浏览/手动下单/委托撤销迁移/安全日志 | +4 | 视后端就绪 | 后端 P2 | ⏳ 待后端 |

**F0 合计**：13 用户页 + 6 admin 页，全部离线可做，不被网络阻塞。✅ **已全部实施**，验收清单见 `docs/F0_LANDING_CHECKLIST.md`。

## 13. 风险与对策

| 风险 | 对策 |
|---|---|
| F0 vanilla JS 功能膨胀难维护 | 严格模块边界 + 每页单文件 + 组件 props 化，单文件 < 200 行；F1 机械翻译 |
| F0→F1 迁移成本被低估 | F0 数据形状/路由/API 按 Leptos server function 心智模型设计；迁移=换实现不换契约 |
| 离线期无法验证 echarts/Tailwind | F0 用 SVG + 内联 token 零依赖；F1 才引入构建链 |
| admin 与 web 重复造轮子 | 共享 `tokens.css` + 通用组件，admin 相对路径引用或软链 |
| 后端 §11 缺口阻塞 | 前端降级占位先行，后端补齐后无缝生效 |
| 托管话术被质疑 | 委托管理页诚实标注等级与升级路径，差异化而非回避 |

## 14. 附录：与 FrenFlow 对标

FrenFlow 是 Sharpside 通道 A 签名模型的对标基准（`CHANNEL_A_SIGNING.md`），也是产品层竞品。对标结论：

| 维度 | FrenFlow | Sharpside | 差距性质 |
|---|---|---|---|
| 签名模型 | 嵌入式钱包+KMS+Builder | POLY_1271+ERC-7739+L2 HMAC+Builder（golden vector 验证） | 骨架对齐，生产化三件套（真KMS/batch approve/非托管）缺 |
| 执行速度 | Block 0 mempool，<1ms 检测，同块执行 | 官方 API 仓位变化，秒级 | 架构级（违反"不自建索引"原则），P3 |
| 多 Venue | Polymarket/Kalshi/Predict.fun/Hyperliquid | Polymarket（Phase 2 Manifold，Phase 3 Kalshi） | 路线图缺 Predict.fun/Hyperliquid |
| 前端完整度 | 排行榜+详情+组合+手动交易+社交+积分 | vanilla JS，F0 规划 13 页 | F0 覆盖核心，手动交易/社交/积分 Phase 2+ |
| 非托管口径 | 主打 non-custodial | 当前"委托交易（未到完全非托管）"，Phase 2 升级 | 诚实口径差异化 |
| 风控 | daily caps+auto-pause+slippage | 三级覆盖+连续亏损熔断+rapid-flip+滑点+min notional | Sharpside 更细 |
| 商业模型 | 1% 交易费，无订阅 | Pro+ 订阅 | 定位差异，非缺口 |

**核心差距**：Block 0 mempool 执行（架构级）+ 通道 A 生产化三件套（P0）+ 前端/组合/手动交易完整度（F0 覆盖一部分）。**Sharpside 优势**：风控更细 + 托管话术诚实 + 全 Rust 单二进制 + serde 端到端类型共享。

## 15. 相关文档

| 文档 | 内容 |
|---|---|
| `ARCHITECTURE.md` | 总体架构 + BFF §6.5 + 路线图 |
| `TECH_STACK_RUST.md` | 前端栈 §5（Leptos + Tailwind + echarts） |
| `CHANNEL_A_SIGNING.md` | 通道 A 签名模型 + 托管口径 §4.3 |
| `FLOWS.md` | 关键流程时序图 |
| `VENUEHUB_STORAGE.md` | 存储表结构（trader_performance/equity_curve/position_timeline/copy_execution） |
| 本文件 | 前端设计（IA + 13 用户页 + 6 admin 页详设 + 组件库 + 后端补点 + 落地路线） |

### 6.7 首页 / Venue 总览 `#/`

**定位**：公开落地页，SEO 友好，展示已接入 Venue 与价值主张，引导注册/登录。

**数据**：`GET /venue-hub/venues` → `Vec<VenueInfo>`（platform/capabilities/auth_model/unit/geo）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  Sharpside · 多平台预测市场跟单                                 │
│  接得广 / 找得到 / 跟得上 / 运营得动                             │
│  [发现交易者 →]  [注册]  [登录]                                  │
├──────────────────────────────────────────────────────────────┤
│  已接入 Venue                                                  │
│  ┌─Polymarket─────┐ ┌─Kalshi(Phase3)┐ ┌─Manifold(Phase2)┐      │
│  │ signal+execute │ │ execute only  │ │ signal only    │      │
│  │ Wallet / USDC   │ │ KYC / USD cents│ │ API key / Mana │      │
│  │ Global         │ │ US only       │ │ Global         │      │
│  └────────────────┘ └───────────────┘ └────────────────┘      │
├──────────────────────────────────────────────────────────────┤
│  双通道跟单                                                    │
│  通道A · TG Deposit Wallet 委托代签   通道B · 自托管 daemon 零钥 │
├──────────────────────────────────────────────────────────────┤
│  热门交易者预览（5）                      [查看完整排行榜 →]    │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 已接入 Venue 用 `venue-badge` 展示能力位（signal_source/execution_venue）；未接入 Venue 灰显 + Phase 标（用 `locked-button` 风格）。
- 双通道用两卡对比：通道 A "登录即用、免 gas、TG 一键跟"，通道 B "Pro+、平台零钥、自持私钥"。
- 热门交易者预览复用 `trader-cell` 精简行（5 行），点击跳详情页（未登录可看，SEO 利好）。
- [发现交易者] → `#/traders`，[登录] → `#/login`（钱包登录；首次即自动建账）。
- SSR 直出全页（公开页，SEO 关键），`<title>Sharpside · 多平台预测市场跟单`。

**组件**：`venue-badge` `trader-cell` `tag-chip`。

### 6.8 登录 `#/login`

**定位**：唯一鉴权入口。仅钱包登录（SIWE / EIP-4361）；邮箱认证已移除（无存量用户）。首次钱包登录即自动建账。

**数据**：
- `GET /account/auth/wallet/nonce?address=` → `{ nonce, domain, chain_id, issued_at }`
- `POST /account/auth/wallet` `{ message, signature }` → `AuthResponse{ token, user }`

**布局**（桌面，居中窄列）：

```
┌──────────────────────────────────────────────────────────────┐
│  ┌─Sharpside 登录────────────────────────────────┐            │
│  │ [🦊 钱包登录]                                   │            │
│  │ 支持 MetaMask / TokenPocket / Rabby / OKX …   │            │
│  │ （EIP-6963 多钱包选择器）                        │            │
│  └────────────────────────────────────────────────┘            │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- EIP-6963 发现已注入钱包；多钱包时弹层选择；无钱包时提示安装。
- `eth_requestAccounts` → 取 nonce → 拼 SIWE → `personal_sign` → `/auth/wallet`。
- 成功 → 存 JWT 到 localStorage + 跳 `?redirect` 或默认 `#/dashboard`。
- 验签失败 / nonce 重放 → 顶部红字展示后端 error。
- TG 登录由 bot 代签 `POST /account/auth/tg`，web 不直接处理。
- 公开页 SSR，`noindex`（登录页不需 SEO）。

**组件**：钱包选择弹层（`modal-backdrop`）`toast`。

### 6.9 我的跟随 `#/follows`

**定位**：跟随关系管理中心，列表 + 暂停/恢复/编辑/删除。

**数据**：`GET /follow/me/follows` → `Vec<FollowRelation>`；`PATCH /follow/follows/{id}` `DELETE /follow/follows/{id}`。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  我的跟随（7）                              [+ 创建跟随]       │
│  筛选 [全部 ▾]  排序 [创建时间 ▾]                              │
├──────────────────────────────────────────────────────────────┤
│  ┌─WhaleA · polymarket──────────────────────────────────┐    │
│  │ 0x7b51...1a8a  通道A(tg)  执行 polymarket  ✅active    │    │
│  │ sizing: fixed $50  日上限: $200  槽位: 1/7            │    │
│  │ 创建于 2026-07-01                                     │    │
│  │ [⏸ 暂停] [✎ 编辑] [复制ID]              [🗑 删除]      │    │
│  └──────────────────────────────────────────────────────┘    │
│  ┌─ProB · polymarket──────────────────────────────────┐      │
│  │ ... ⏸paused ...                                      │      │
│  │ [▶ 恢复] [✎ 编辑] ...                                 │      │
│  └──────────────────────────────────────────────────────┘    │
│  ...                                                          │
├──────────────────────────────────────────────────────────────┤
│  Pro+ 槽位 5/7 已用                              [升级 Pro+]   │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 每条跟随卡：跟随对象（alias/地址缩略 + 平台 badge）+ 通道 + 执行 Venue + 状态标（✅active/⏸paused/❌error）+ sizing 摘要 + 风控摘要 + 创建时间。
- 操作：[暂停/恢复]（PATCH active）、[编辑]（弹 `follow-form` 模态预填现有 config）、[复制ID]、[删除]（`confirm-dialog` 二次确认）。
- 筛选：全部/active/paused/error；排序：创建时间/P&L（若有组合数据）。
- 底部 Pro+ 槽位进度条，free 用户达上限时 [升级 Pro+] 高亮。
- 空状态："尚未跟随任何交易者" + [发现交易者 →]（跳排行榜）。
- 需登录，noindex。

**组件**：`follow-card`（完整版，比 `follow-card-mini` 多操作行）`follow-form` `confirm-dialog` `locked-button` `subscription-badge` `empty-state`。

### 6.10 创建跟随 `#/follows/new`

**定位**：独立创建跟随页（除详情页/排行榜行内模态外的另一入口，支持从 URL 参数预填）。

**数据**：`POST /follow/follows`；预填来源：URL query `?platform=&address=` 或 `?identity_id=`。

**布局**（桌面，居中窄列）：

```
┌──────────────────────────────────────────────────────────────┐
│  创建跟随                                                      │
├──────────────────────────────────────────────────────────────┤
│  跟随对象                                                      │
│  ◉ 单 Venue 交易者   ○ 跨 Venue 身份                            │
│  平台 [polymarket ▾]   地址 [0xabc...      ]  (单 Venue)       │
│  身份 [#uuid ▾]                     (跨 Venue，仅列 manual_verified)│
├──────────────────────────────────────────────────────────────┤
│  执行配置（复用 follow-form 字段）                              │
│  sizing mode / channel / execute_venue / same_venue_only      │
│  高级风控（Pro+ 折叠区）                                        │
├──────────────────────────────────────────────────────────────┤
│  [创建跟随]   [取消]                                            │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 跟随对象单选：单 Venue 交易者（platform+address）/ 跨 Venue 身份（identity 下拉仅列 `manual_verified=true`，否则置灰 + 提示"未人工校对的身份不可跟随"）。
- 执行配置复用 `follow-form` 组件的全部字段（sizing/channel/execute_venue/same_venue_only/风控），但以页面形式而非模态。
- 提交校验：execute_venue 按 `user.jurisdiction` 过滤；channel=daemon 需 Pro+；same_venue_only=false 需 Pro+。
- 成功 → toast "已跟随" + 跳 `#/follows`；400 → 顶部红字显示后端 error。
- 需登录，noindex。

**组件**：`follow-form` `form-field` `tab-switch` `locked-button` `toast`。

### 6.11 成交历史 `#/copy-history`

**定位**：组合页"近期成交→全部"的展开，全量成交明细 + 筛选 + 导出。

**数据**：`GET /copier/me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=`（补端点，§11）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  成交历史                              [导出 CSV]              │
│  ┌─筛选──────────────────────────────────────────────────┐    │
│  │ 时间 [最近1周 ▾]  跟随 [全部 ▾]  Venue [全部 ▾]  状态 [全部▾]│
│  └──────────────────────────────────────────────────────┘    │
├──────────────────────────────────────────────────────────────┤
│  时间    跟随   市场   方向  价    量   P&L   fee  状态  tx   │
│  07-22   WhaleA ETH>2k 买  0.45 100  +$23  $0.1 ✅filled 0xabc│
│  07-22   ProB   BTC>1m 卖  0.62 200  -$40  $0.2 ⏭skipped —   │
│  ...                                                          │
├──────────────────────────────────────────────────────────────┤
│  ← 1 2 3 ... →                  显示 1-100 / 1,284           │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 筛选：时间范围（最近1天/1周/1个月/1年/年初至今/全部/自定义）、跟随（我的跟随列表）、Venue、状态（filled/skipped/failed）。
- 列：时间 / 跟随（alias）/ 市场 / 方向 / 价 / 量 / P&L（已实现显示数值，持仓中"持仓中"）/ fee / 状态（filled✅/skipped⏭+skip_reason tooltip/failed❌）/ tx（链上平台链区块浏览器，玩钱平台隐藏此列）。
- 导出 CSV：当前筛选条件下的全量数据（`limit=10000`），前端拼装下载。
- 分页 100/页，复用 `pagination`。
- 空状态："无成交记录"。
- 需登录，noindex。

**组件**：`filter-bar` `data-table` `pagination` `copy-field` `empty-state` `skeleton`。

### 6.12 订阅 `#/settings/subscription`

**定位**：Pro+ 升级与权益管理。

**数据**：`GET /account/me` → `User`（tier/subscription_until）；`POST /account/me/subscription`（tier+until）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  订阅                                                          │
├──────────────────────────────────────────────────────────────┤
│  当前档位  Free                          [升级 Pro+]           │
│  ┌─Free──────────────────┐ ┌─Pro+──────────────────────┐      │
│  │ 通道A(tg)              │ │ 通道A + 通道B(daemon)       │      │
│  │ 单 Venue 执行           │ │ 跨 Venue 执行              │      │
│  │ 基础风控                │ │ 高级风控(连续亏损熔断等)    │      │
│  │ 3 个跟随槽位            │ │ 无限跟随槽位               │      │
│  └────────────────────────┘ │ $X/月  [升级 Pro+]          │      │
│                              └────────────────────────────┘      │
├──────────────────────────────────────────────────────────────┤
│  Pro+ 用户                                                     │
│  当前：Pro+ 订阅中（至 2026-12-31）    [续费]  [取消订阅]        │
│  权益使用：通道B 已用 / 跨Venue 已用 / 槽位 5/∞                 │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 两档对比卡：Free vs Pro+，权益清单对照（通道/跨 Venue/风控/槽位），Pro+ 卡 [升级] 按钮。
- Pro+ 用户显示当前订阅状态（到期日）+ [续费]/[取消订阅] + 权益使用情况。
- F0 阶段支付未接入，[升级] 弹"支付即将上线"占位（或链外部支付流程，待商务定）。
- free 用户在受限功能处（channel=daemon、same_venue_only=false、槽位上限）触达时引导回此页。
- 需登录，noindex。

**组件**：`subscription-badge` `tier-comparison-card`(新) `confirm-dialog` `toast`。

### 6.13 daemon API key `#/settings/daemon-key`

**定位**：通道 B 凭证轮换，明文一次性显示。

**数据**：`GET /account/me`（查 daemon_api_key_hash 是否非空判状态）；`POST /account/me/daemon-api-key`（轮换，返明文一次）。

**布局**（桌面）：

```
┌──────────────────────────────────────────────────────────────┐
│  daemon API key（通道 B）                                       │
├──────────────────────────────────────────────────────────────┤
│  状态  ✅ 已颁发  /  ❌ 未颁发                                   │
│  daemon API key 用于通道 B（自托管 daemon）拉取跟单指令。       │
│  平台不存私钥，key 仅存 hash，明文仅颁发时显示一次。             │
├──────────────────────────────────────────────────────────────┤
│  [颁发 key]  /  [轮换 key]                                      │
├──────────────────────────────────────────────────────────────┤
│  ┌─明文一次性弹窗（轮换后）──────────────────────────┐          │
│  │ 你的 daemon API key（仅此一次显示）：              │          │
│  │ ┌────────────────────────────────────────────┐    │          │
│  │ │ a3f5b2e1-...-8c4d                           │ [复制]│        │
│  │ └────────────────────────────────────────────┘    │          │
│  │ ☐ 我已妥善保存此 key                              │          │
│  │              [关闭]（勾选后才能点）                 │          │
│  └──────────────────────────────────────────────────┘          │
├──────────────────────────────────────────────────────────────┤
│  daemon 安装                                                    │
│  1. 下载 daemon 二进制（你的平台）  [下载 macOS] [Linux] [Windows]│
│  2. 配置 .env：DAEMON_API_KEY=<上方key>  COPIER_URL=...          │
│  3. 运行 daemon  [查看完整文档 →]                                │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：

- 状态：hash 非空 → "已颁发"；空 → "未颁发"。
- [颁发]/[轮换] → `confirm-dialog`（"轮换后旧 key 立即失效，需更新所有 daemon 配置"）→ `POST /me/daemon-api-key` → 明文一次性弹窗。
- **明文弹窗安全关键**：显示明文 + [复制] + 强制勾选 [我已妥善保存此 key] 才能 [关闭]（防未保存就关）。
- daemon 安装步骤（F0 文档占位，下载链接待构建产物就绪）。
- 需登录，noindex。

**组件**：`one-time-secret-modal` `confirm-dialog` `copy-field` `toast`。
