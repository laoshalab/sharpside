# Sharpside

> Polymarket 跟单平台 · 多平台原生（Venue 一等公民）· 双通道（TG Deposit Wallet 委托代签 + 自托管 daemon 零钥）· 主路径不自建链上索引

**多平台预测市场跟单平台**，核心做四件事：接得广 / 找得到 / 跟得上 / 运营得动。详见 `docs/ARCHITECTURE.md`。

## 状态

✅ **Phase 1a · MVP 闭环已落地**（Polymarket only，通道 B daemon 闭环，最小 web）
🚧 **Phase 1b · 双通道补齐进行中**（admin 后台 + 影子模式 + 完整风控已落地；通道 A TG bot 真实现待 teloxide）

### Phase 1a 落地进度

| Step | 模块 | 状态 |
|---|---|---|
| 0 | Cargo workspace 骨架 + CI + docker-compose | ✅ |
| 1 | `crates/shared`（serde 类型） | ✅ |
| 2 | `crates/venues/core`（Venue trait + Registry） | ✅ |
| 3 | `crates/db`（schema + 迁移 + queries） | ✅ |
| 5 | `crates/perf`（仓位重建 + 绩效） | ✅ |
| 6 | `crates/mapping`（市场映射启发式） | ✅ |
| 7 | `crates/identity`（跨平台身份链接） | ✅ |
| 8 | `services/gateway`（BFF + 鉴权 + 限流） | ✅ |
| 9 | `crates/venues/polymarket`（Polymarket adapter） | ✅ |
| 10 | `services/venue-hub`（编排 ingest/mapping/identity/perf/hot worker） | ✅ |
| 11 | `services/account` + `services/follow` | ✅ |
| 12 | `services/copier` + 通道 B daemon + 最小 web | ✅ |

### Phase 1b 落地进度

| 子项 | 模块 | 状态 |
|---|---|---|
| B1 | `apps/admin`（映射/身份审核、热钥 CRUD、标签阈值、可见性、影子阈值） | ✅ |
| B2 | 完整风控（连续亏损熔断 + 档位/用户/Venue 三级覆盖 + 滑点保护） | ✅ |
| B3 | 影子模式 worker（第三方 diff → metric_audit → 告警） | ✅ |
| B4 | 通道 A TG bot（teloxide）真实现 | ✅ |
| B5 | 真依赖替换 | 🚧 argon2 ✅ / ratatui ✅ / alloy ✅ · apalis/leptos 待网络环境 |

> **Phase 1a · MVP 闭环已落地**（Polymarket only，通道 B daemon 闭环，最小 web）。
> 离线/无凭证环境默认 `COPIER_DRY_RUN=true` / `DAEMON_DRY_RUN=true`，合成成交跑通闭环。

### 端到端集成验证（dry_run 闭环）

`infra/e2e.sh` 一键验证全链路（docker compose 起 PG → 构建 → 起 account/follow/copier/admin → 双通道闭环 → admin 冒烟）：

| 验证项 | 链路 | 结果 |
|---|---|---|
| 通道 A（TG） | 注册→跟随(tg)→`/internal/signals`→copier worker dry_run 合成成交→`copy_execution` | ✅ |
| 通道 B（daemon） | 颁发 daemon_api_key→跟随(daemon)→信号→daemon 拉取+回传→`copy_execution` | ✅ |
| admin 冒烟 | tag-rule / audit-threshold PUT+GET、trader visibility 端点 | ✅ |

运行：`bash infra/e2e.sh`（21 项断言全绿）。验证中修复 3 个真实 bug：daemon `since` 时间戳 URL 编码（`+00:00` 被解码为空格）、daemon `Decimal→f64` 反序列化（copier 返回字符串）、web 前端 `SizingMode` payload 格式。

### 接入真实 Polymarket 数据联调

venue-hub 通过 `PolymarketClient` 调 Polymarket 公开 API（读免鉴权）跑全链路：ingest leaderboard → `traders`、markets → `raw_markets`、import 地址 → 回填 `raw_trades`、perf worker → `position_timeline` / `equity_curve` / `trader_performance` / `trader_tag`，并可接到跟单闭环（跟随真实 trader → 信号 → copier dry_run 成交）。

| 验证项 | 结果 |
|---|---|
| ingest leaderboard（5 traders）+ markets（3）入库 | ✅ |
| import 地址回填 raw_trades（5 笔） | ✅ |
| perf 物化 trader_performance（7d/30d/all）+ trader_tag（DW:diamond/DW:win） | ✅ |
| 真实 trader 地址接入跟单闭环 → copier dry_run 成交 | ✅ |

> 本机到 `*.polymarket.com` 直连超时（受限网络），用本地 mock（`infra/mock/polymarket_mock.py`，真实 DTO 形状 fixture）跑通**完全相同的代码路径**；新增 `POLYMARKET_{DATA,GAMMA,CLOB}_API_URL` 覆盖注入 `PolymarketClient::with_urls`，有网络环境后留空即直连真实 API，零代码改动。详见 `docs/RUNBOOK_POLYMARKET_LIVE.md`。
>
> 联调中发现并修复 bug：`upsert_trader_performance` 遇 NaN/inf 指标（无亏损时 `profit_factor=inf`、无方差时 `sharpe=NaN`）静默丢行——`Decimal::try_from` 报错被吞；已在 `crates/db/src/queries/perf.rs` 的 `to_dec` 归零处理。

### 通道 A TG bot 真实现（teloxide 0.15）

`apps/tg-bot` 用 teloxide 长轮询实现通道 A 用户面：`/start` 绑定账户、`/follow` 建跟随（channel=tg）、`/follows` `/unfollow` 管理、`/traders` `/perf` 查看交易者与绩效、`/setamount` 设默认金额。bot 代 TG 用户换 JWT（account 新增 `POST /auth/tg`，受 `X-TG-Bot-Secret` 共享密钥保护），缓存 JWT 并在 401 时自动重换。

| 验证项 | 结果 |
|---|---|
| `POST /auth/tg` 错误密钥→401 / 正确密钥→token+user(tg_id) | ✅ |
| TG 用户（tg_id）驱动通道 A 全流程：/auth/tg→/follows→信号→copier dry_run 成交(111.11@0.45) | ✅ |
| `GET /traders/{p}/{a}/performance` 端点（venue-hub 新增，bot /perf 用） | ✅ |
| bot 无 `TG_BOT_TOKEN` 时优雅退出 | ✅ |

> 本机 `api.telegram.org` 不可达（同 polymarket 被阻断），无法活跑 teloxide 长轮询；bot 的 teloxide 接线由编译验证，后端契约由上述 curl 等价流程端到端验证。有网络环境后设 `TG_BOT_TOKEN` 即可活跑。
> 联调中发现并修复：bot `/perf` 解析绩效时 `Decimal` 字段（roi/realized_pnl）序列化为字符串，`as_f64()` 返回 None→显示 0；已加 `num()` helper 兼容 number/string。

### B5 真依赖替换（argon2 / ratatui / alloy）

| 子项 | 替换对象 | 结果 |
|---|---|---|
| argon2 | account/copier 手写 PBKDF2 密码哈希 → argon2 Argon2id（PHC 字符串） | ✅ |
| ratatui | daemon 最小 CLI → ratatui TUI（头部状态/指令表/日志面板） | ✅ |
| alloy | Polymarket EIP-712 链上签名（降版 2.1.1，MSRV 1.91） | ✅ |
| apalis | HTTP `/internal/signals` → 消息队列 | ⏳ 待网络环境 |
| leptos | vanilla JS `index.html` → Leptos SSR | ⏳ 待网络环境 |

- **argon2**：`hash_password` 用 `argon2` 0.5 Argon2id + 随机盐（`OsRng`）产出 PHC 字符串；`verify_password` 按前缀分发——`$argon2` 走 argon2，否则按旧 `iterations$salt_hex$hash_hex` 走 PBKDF2 兼容验证，存量 daemon_api_key / 用户密码不失效。account 与 copier 共用同一逻辑（copier 校验 account 颁发的新 argon2 key + 旧 PBKDF2 key）。
- **ratatui**：daemon 主线程跑 ratatui 渲染循环（250ms tick）+ 终端事件（`q`/`Esc` 退出），后台 tokio 任务轮询 copier，状态经 `Arc<Mutex<UiState>>` 共享。TUI 三段式布局：头部状态摘要（轮询次数/指令计数 ✓⚠✗/上次轮询时间）、中部近期指令表（时间/Venue/Market/Side/Price/Size/Status/Skip，状态着色）、底部滚动日志。
- **headless 回退**：非 TTY（e2e.sh / systemd / CI）或 `DAEMON_HEADLESS=1` 时不进 TUI，轮询循环直接跑在主任务，日志走 tracing（保持 `dry-run 合成成交回传` 等关键日志行，e2e grep 兼容）。
- **alloy（降版 2.1.1）**：不用 meta crate（2.2 需 rust 1.94）；最小子 crate 集 `alloy-signer`/`alloy-signer-local`/`alloy-sol-types`/`alloy-primitives`。`crates/venues/polymarket/src/clob.rs` 实现 EIP-712 `PolymarketOrder` 签名 + ecrecover 自洽单测。
  - **通道 A（copier）**：`COPIER_DRY_RUN=false` 时走 `Venue::place_order` → 真 EIP-712 签名；默认 dry-sign（不提交 CLOB，`tx_hash`=签名）；`POLYMARKET_CLOB_POST=1` 真提交。dev 私钥：`POLYMARKET_DEV_PRIVATE_KEY` 或注入 `with_dev_signer` / `POLYMARKET_DEV_PLAINTEXT_HANDLE=1`。
  - **通道 B（daemon）**：`DAEMON_DRY_RUN=false` 时本地签名（平台零钥）；私钥仅存 daemon 进程（`POLYMARKET_PRIVATE_KEY`）；默认 dry-sign，`POLYMARKET_CLOB_POST=1` 真提交。复用 `sharpside-venues-polymarket::clob`。
  - 说明：当前为 EOA `signatureType=0` 基线；真实 Polymarket proxy wallet（`POLY_PROXY`）待 session wallet KMS。

| 验证项 | 结果 |
|---|---|
| argon2 注册→登录→daemon key 颁发→copier 校验（e2e 通道 A/B 闭环 21 项全绿） | ✅ |
| 旧 PBKDF2 哈希仍可校验（兼容单测） | ✅ |
| daemon TUI 编译 + 非 TTY headless 回退（e2e 通道 B daemon 回传成交） | ✅ |
| alloy EIP-712 签名 + ecrecover 自洽（polymarket clob 单测） | ✅ |
| daemon dry-sign 产出 65 字节签名（sign 单测） | ✅ |
| place_order 注入 signer → dry-sign Fill（polymarket 单测） | ✅ |
| workspace `clippy -D warnings` + `cargo test` | ✅ |

> `argon2`/`rand`/`ratatui`/`crossterm`/`alloy-*` 已从 crates.io 拉取并缓存，离线可编译。
> apalis/leptos 为大改，留待网络环境。

> 离线编译说明：`spin 0.9.8` 在 crates.io 被 yanked，sqlx 经 `sqlx-sqlite → flume` 非可选地依赖它。
> 已用 `[patch.crates-io] spin = { path = "vendor/spin" }` 覆盖，有网络环境可删除该 patch。

## 目录结构

```
sharpside/
├── crates/
│   ├── shared/              # serde 类型：CopyOrder/TradeEvent/Performance/Tag
│   ├── venues/
│   │   ├── core/            # Venue trait + VenueInfo + VenueRegistry
│   │   └── polymarket/      # Polymarket adapter（signal+execution）
│   ├── db/                  # sqlx schema + 迁移
│   ├── perf/                # 仓位重建 + 绩效计算
│   ├── mapping/             # 跨 Venue 市场映射
│   └── identity/            # 跨 Venue 交易者身份链接
├── services/
│   ├── gateway/             # axum · API 网关 + 鉴权 + BFF
│   ├── venue-hub/           # axum · 多平台采集 + 映射 + 身份 + 绩效 + 热钥
│   ├── follow/              # axum · 跟随关系 + 信号派生
│   ├── copier/              # axum · 通道×Venue 执行 + 风控
│   └── account/             # axum · 用户/Pro+/管辖域/凭证
├── apps/
│   ├── web/                 # Leptos · 用户前端
│   ├── tg-bot/              # teloxide · 通道 A
│   ├── daemon/              # ratatui + alloy · 通道 B
│   └── admin/               # Leptos · 运营后台
├── infra/
│   └── docker-compose.yml   # PG16 + Redis7 + MinIO
└── docs/                    # 架构与设计文档（权威）
```

## 快速开始

### 前置

- Rust 1.80+（项目锚定 1.91，见 `rust-toolchain.toml`）
- Docker + Docker Compose

### 起本地基础设施

```bash
docker compose -f infra/docker-compose.yml up -d
cp infra/.env.example .env
```

### 构建

```bash
cargo build --all
cargo nextest run        # 需先 cargo install cargo-nextest
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

### 起服务（Phase 1a 落地后）

```bash
cargo run -p sharpside-gateway    # :8080
cargo run -p sharpside-venue-hub  # :8081
cargo run -p sharpside-follow     # :8082
cargo run -p sharpside-copier     # :8083
cargo run -p sharpside-account    # :8084
```

## 路线图

| 阶段 | 范围 | Venue |
|---|---|---|
| **Phase 1a · 单通道闭环** | gateway + venue-hub + copier/account + Polymarket adapter + 通道 B daemon + 最小 web | Polymarket |
| **Phase 1b · 双通道补齐** | + 通道 A(TG) + admin + 影子模式 + 完整风控 | Polymarket |
| **Phase 2 · 信号扩容** | + Manifold adapter(signal) + 身份启发式 + 跨平台排行榜 | +Manifold |
| **Phase 3 · 跨 Venue 执行** | + Kalshi adapter(execution) + 市场映射人工校对 + 管辖域路由 | +Kalshi |
| **Phase 4 · 链上扩容** | + Zeitgeist/Azuro(按需) | +链上 |

每阶段独立可上线，新增 Venue 只需实现 `Venue` trait + 注册，主路径零改动。

## 文档

权威设计文档在 `docs/`：

| 文档 | 内容 |
|---|---|
| `ARCHITECTURE.md` | 总体架构 + 模块划分 + 路线图 |
| `TECH_STACK_RUST.md` | 全 Rust 技术栈选型 |
| `VENUE_DESIGN.md` | Venue trait + 市场映射 + 跨 Venue 身份实现细节 |
| `VENUEHUB_STORAGE.md` | VenueHub 存储八层总览 |
| `TRADERS_TABLE.md` | traders 表字段详解 |
| `FLOWS.md` | 关键流程时序图 |
| `DATA_SOURCES.md` | Polymarket 官方 API 数据清单 |
| `PERFORMANCE_PIPELINE.md` | 绩效数据端到端管道 |
| `SHADOW_MODE.md` | 交叉校验影子模式 |
| `MULTI_PLATFORM.md` | 各 Venue 可行性与接入约束 |
| `THIRD_PARTY_DATA.md` | 第三方增强数据入库与授权 |

## 许可证

AGPL-3.0-or-later
