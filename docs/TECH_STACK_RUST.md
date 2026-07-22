# 技术栈建议 · 全 Rust 版

> 原则：**全 Rust 主栈 + 按需 WASM 前端 + 可选 Python 分析**。
> 一句话结论：**后端/TG/daemon 用 Rust 一等公民，前端用 Leptos (SSR+WASM)，图表用 ECharts 桥接；分析增强可选 Python。**

## 1. 为什么全 Rust

| 收益 | 说明 |
|---|---|
| 单二进制部署 | 每服务一个静态二进制，docker 镜像可 < 50MB，启动毫秒级 |
| 内存安全 + 无 GC | 长跑 daemon/采集器无停顿，低内存占用 |
| 高吞吐低核心 | 同样硬件扛更多并发轮询，infra 成本低 |
| daemon 一等公民 | polycopier 本身就是 Rust，官方 `rs-clob-client` 直接用 |
| 类型安全更强 | serde + sum type，比 TS schema 更稳 |
| 无 node_modules | 无 npm 供应链风险 |

## 2. 代价（先说清楚）

| 代价 | 对策 |
|---|---|
| 编译慢 | sccache + cargo nextest + 增量编译；CI 缓存 target |
| 招聘池小 | 核心服务少而精，运营面靠 docker/CI 抽象 |
| 前端图表生态弱 | Leptos SSR 壳 + ECharts WASM 桥接（见 §5） |
| 快速数据探索不如 pandas | 主路径用 polars（Rust 原生 DataFrame）；重分析走可选 Python 服务 |
| NestJS 式 DI 没有 | 用 axum + tower + 模块化 crate 组织，靠 trait 抽象 |

## 3. Monorepo / Workspace

| 项 | 选型 |
|---|---|
| 包管理 | **Cargo workspace**（原生） |
| 构建缓存 | **sccache** + CI 缓存 `target/` |
| 测试 | **cargo nextest**（并行快） |
| lint | **clippy + rustfmt** + CI 强制 |
| 版本 | Rust 1.80+ / edition 2021 |

workspace 成员：

```
sharpside/
├── crates/
│   ├── shared/        # serde 类型、CopyOrder/TradeEvent schema
│   ├── venues/
│   │   ├── core/      # Venue trait + VenueInfo + VenueRegistry + 通用类型
│   │   └── polymarket/ # Data/Gamma/CLOB SDK 封装 + 限流 + 分页（adapter）
│   ├── db/            # sqlx schema + 迁移
│   └── perf/          # 仓位重建 + 指标计算（可被多服务复用）
├── services/
│   ├── gateway/       # axum
│   ├── venue-hub/     # axum + 采集 worker（多平台采集+映射+身份+绩效+热钥+影子）
│   ├── follow/        # axum + 信号派生 worker
│   ├── copier/        # axum + 执行 worker
│   └── account/       # axum
├── apps/
│   ├── web/           # Leptos (SSR + WASM)
│   ├── tg-bot/        # teloxide
│   └── daemon/        # ratatui TUI + 本地执行
└── infra/
```

## 4. 后端 services（axum 栈）

| 项 | 选型 | 理由 |
|---|---|---|
| Web 框架 | **axum 0.7** | tokio 原生、tower 中间件、类型安全 extractor |
| 异步运行时 | **tokio 1** | 事实标准 |
| DB 驱动 | **sqlx 0.8**（PostgreSQL） | 编译期校验 SQL、async、无 ORM 黑盒 |
| 迁移 | **sqlx-cli** | 版本化 SQL 迁移 |
| 缓存/队列 | **redis-rs + bb8** 连接池 + **apalis**（BullMQ 风格 job queue） | apalis 与 Redis 协议兼容 |
| HTTP 客户端 | **reqwest 0.12** + keep-alive | 高并发轮询 |
| 限流 | **governor**（tower middleware） | Data API 总体 ~1000 req/10s，`/trades` ~200、`/positions` ~150（按端点差异化配额，见 `DATA_SOURCES.md` §5） |
| 鉴权 | **jsonwebtoken** + daemon_api_key 自研 | JWT + API key 双模式 |
| 校验 | **garde**（或 validator） | serde 派生校验 |
| 日志 | **tracing + tracing-subscriber** | 结构化、tokio 集成 |
| 配置 | **figment**（TOML + env） | 启动期校验 |
| 序列化 | **serde + serde_json** | 端到端 schema 共享 `crates/shared` |
| 错误 | **thiserror** + **anyhow** | 库/服务分层 |
| 测试 | **cargo nextest + testcontainers-rs** | PG/Redis 集成测试 |
| 指标 | **prometheus + axum-prometheus** | OTel 也可 |

## 5. 前端 web / admin（Leptos + ECharts 桥接）

| 项 | 选型 | 理由 |
|---|---|---|
| 框架 | **Leptos 0.7**（SSR + hydration） | SolidJS 式响应式，server functions 像 RPC，路由内置 |
| SSR | Leptos axum 集成 | 排行榜 SEO 友好 |
| 样式 | **Tailwind 4**（via trunk/wasm-pack） | 与 Leptos 兼容 |
| 图表 | **echarts-rs**（wasm-bindgen 桥接 ECharts） | 权益曲线/回撤阴影用 ECharts，比纯 Rust 写 SVG 省力 |
| 表格 | 自建虚拟滚动组件（leptos 信号驱动） | TanStack Table 无 Rust 对等品，需自建 |
| 表单 | **leptos-form** 或自建 + garde 校验 | 跟随配置表单 |
| i18n | **leptos_i18n** | 中英双语 |
| 构建 | **trunk**（WASM 构建） | Leptos 官方推荐 |
| 状态 | Leptos 信号（signal/store） | 内置响应式 |

**诚实说明**：Rust 前端图表/表格生态远不如 React。ECharts 桥接是务实折中；复杂交互表格需自建虚拟滚动。若后期运营觉得前端迭代太慢，可保留 Rust 后端、前端切回 Next.js（协议走 OpenAPI/zod-json）。

## 6. TG bot（通道 A · Deposit Wallet 委托代签）

> 详见 `docs/CHANNEL_A_SIGNING.md`。FrenFlow 式签名模型，全 Rust 实现（不用 Privy）。主路径走 Polymarket 官方推荐的新 API 用户路径（POLY_1271）。

| 项 | 选型 |
|---|---|
| 框架 | **teloxide 0.15**（异步、tokio 原生） |
| 会话存储 | Redis（加密授权句柄） |
| 资产仓 | **Polymarket Deposit Wallet**（ERC-1967 proxy，`deriveDepositWalletAddress(owner_eoa)` CREATE2 确定性推导，gasless 部署） |
| 委托签名 | **alloy** EIP-712 + **ERC-7739-wrapped POLY_1271**（signatureType=3，maker=signer=deposit wallet，owner EOA 签） |
| CLOB 鉴权 | **L2 HMAC-SHA256**（`POLY_*` headers，hmac + sha2 crate） |
| Builder 归因 | `X-Builder-Code` header + builderCode 入订单 |
| 免 gas | **Polymarket Builder Relayer**（WALLET-CREATE / WALLET batch，reqwest 直调 REST，无需用户签名） |
| KMS | 抽象 trait（dev=明文 env / prod=AWS KMS / 未来 HSM） |
| CLOB | **polymarket/rs-clob-client** 或 **polymarket_client_sdk_v2**（支持 `SignatureType::Poly1271`） |
| 部署 | 单二进制 + systemd（与 polycopier 同款） |

## 7. daemon（通道 B，Rust 一等公民）

| 项 | 选型 | 理由 |
|---|---|---|
| 异步 | tokio 1 | |
| HTTP | reqwest keep-alive 长轮询 | 拉 `/me/copy-orders?since=` |
| 签名 | **alloy** + **polymarket/rs-clob-client** | 本地私钥签名 |
| TUI | **ratatui 0.28** + crossterm | 对标 polycopier |
| 本地账本 | **rusqlite**（SQLite） | copy_ledger |
| 风控 | 本地实现，共享 `crates/shared` 规则 | 平台风控在指令侧，daemon 再校验 |
| 配置 | figment + .env | |
| 部署 | 单二进制 + systemd unit（参考 polycopier） | 用户一键安装 |

## 8. 共享 crates

| crate | 内容 |
|---|---|
| `crates/shared` | serde 类型：CopyOrder、TradeEvent、Performance、Tag、FollowConfig |
| `crates/venues/polymarket` | Data/Gamma/CLOB 客户端 + governor 限流 + 分页 + 3500 条上限处理（Venue trait adapter） |
| `crates/db` | sqlx 查询 + 迁移 |
| `crates/perf` | 仓位重建 + 指标计算（ROI/Sharpe/回撤/DW/type-3），纯函数易测 |

## 9. 可选增强

| 项 | 时机 | 选型 |
|---|---|---|
| Python 分析服务 | 因子分析/ML/大规模回测 | FastAPI + pandas/polars + Celery（共享 Redis）；或 **PyO3** 把 Rust `crates/perf` 暴露给 Python |
| 前端切 React | 运营反馈 Leptos 迭代慢 | 保留 Rust 后端，前端换 Next.js，协议走 OpenAPI |

## 10. 基础设施

| 项 | 选型 |
|---|---|
| 容器 | Docker（distroless/scratch 镜像，< 50MB） |
| 编排 | docker-compose（MVP）→ k8s（后期） |
| 反代 | Caddy（自动 HTTPS） |
| 密钥 | SOPS + age / Vault |
| 监控 | OpenTelemetry（tracing-otlp）→ Grafana / Tempo / Loki |
| 错误 | Sentry（Rust SDK 成熟） |
| CI | GitHub Actions + cargo nextest + sccache |
| 对象存储 | MinIO / S3（rust-s3 crate） |

## 11. 版本锚点（2026-07）

| 组件 | 版本 |
|---|---|
| Rust | 1.80+ / edition 2021 |
| axum | 0.7 |
| tokio | 1 |
| sqlx | 0.8 |
| teloxide | 0.13 |
| Leptos | 0.7 |
| alloy | 0.x |
| ratatui | 0.28 |
| reqwest | 0.12 |
| apalis | 0.6 |
| governor | 0.6 |
| tracing | 0.1 |
| PostgreSQL | 16 |
| Redis | 7 |

## 12. 全 Rust vs 全 TS 对照

| 维度 | 全 Rust | 全 TS |
|---|---|---|
| 部署体积 | 极小（单二进制） | 中（Node + node_modules） |
| 启动速度 | 毫秒 | 百毫秒 |
| 内存占用 | 低 | 中 |
| 吞吐/核 | 高 | 中 |
| 前端生态 | 弱（Leptos+ECharts 桥接） | 强（Next.js+Recharts+TanStack） |
| 前端迭代速度 | 慢 | 快 |
| 量化数据探索 | polars 够用，重分析走 Python | 需引入 Python 服务 |
| daemon 一致性 | 一等公民（与 polycopier 同栈） | 需另起 TS daemon |
| 招聘 | 难 | 易 |
| 编译/迭代 | 慢（sccache 缓解） | 快 |
| schema 共享 | workspace crate 原生 | monorepo + zod |

## 13. 落地节奏

| 阶段 | 范围 |
|---|---|
| MVP | Cargo workspace + 2–3 axum 服务（gateway + venue-hub + copier/account 合并，与 `ARCHITECTURE.md` Phase 1a 对齐）+ Leptos web + teloxide TG + Rust daemon + sqlx + Redis/apalis |
| v1 | Pro+ 订阅 + admin 后台 + OTel/Sentry + daemon TUI 完善 |
| v2 | 按需 Python 分析（PyO3 或独立服务）/ 前端评估是否切 React |

## 14. 风险与对策

| 风险 | 对策 |
|---|---|
| Leptos 前端迭代慢 | 预留 OpenAPI 协议，可切 Next.js 而不动后端 |
| 图表交互复杂 | ECharts 桥接覆盖 90% 场景，复杂图用纯 SVG 自绘 |
| 编译慢 | sccache + nextest + CI 缓存 target |
| Data API 3500 条上限 | 主路径 Data API；超限按需 subgraph |
| 限流 | governor 按 endpoint 配额 |
| daemon 协议演进 | `crates/shared` 语义化版本，daemon 启动检查兼容 |
| alloy 变更频繁 | 封装在 `crates/venues/polymarket`，上层不直接依赖 |
