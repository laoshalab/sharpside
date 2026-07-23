# 自托管 daemon 开发方案（通道 B）

> 平台零钥 · 用户本地运行 · 单二进制 · 长期无人值守。对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §7 / `docs/TECH_STACK_RUST.md` §7。
>
> 本文档是 `apps/daemon/` 从 MVP 闭环演进到生产可用的权威路线图，里程碑独立可发布。

## 1. 定位与边界

```
平台侧（copier）         通道 B（daemon，用户本地）         目标 Venue
  派发指令  ──── HTTP ────→  daemon 拉取
                            ├─ 本地风控
                            ├─ 本地私钥签名  ──→  CLOB / 链上
                            └─ 回传成交  ──── HTTP ────→  copier 归集
```

- **零钥底线**：私钥永不离开 daemon 进程，不写日志、不进账本、不回传 copier。
- **Pull 模型**：daemon 主动轮询，copier 不主动推——对用户防火墙/NAT 友好。
- **可降级**：网络/凭证/CLOB 任一不可达 → skip + 回传原因，不丢指令。
- **可重放**：本地账本 + 幂等回传，崩溃重启不双签不漏签。
- **单二进制**：`sharpside-daemon` + 一份 `.env` + 一条 systemd unit，5 分钟装好。

## 2. 现状盘点

`apps/daemon/` 当前 4 文件 / 813 行，已落地 MVP 闭环。

| 维度 | 已落地 | 缺口 |
|---|---|---|
| 配置 | `std::env` 直读（`config.rs`） | 无 `.env`/TOML 叠层、无校验、无私钥安全加载 |
| 轮询 | 固定间隔 `GET /me/copy-orders?since=`（`main.rs:177`） | 无长轮询/SSE、无指数退避、无断路、无 401 自动停 |
| 风控 | 单笔 `local_max_notional`（`main.rs:239`） | 无日上限/冷却/黑白名单/价格区间/per-venue 限额 |
| 执行 | Polymarket EIP-712 dry-sign + `POLYMARKET_CLOB_POST` 真提交（`sign.rs`） | 仅 polymarket 硬编码、无 venue 分发抽象 |
| 持久化 | 无 | 缺 rusqlite 本地账本——崩溃丢指令、无审计/重放 |
| 鲁棒性 | 单线程 `poll_loop` | 无幂等回传、无去重、无 in-flight 状态恢复 |
| 协议 | 无版本协商 | `crates/shared` 演进时 daemon 静默错配 |
| 部署 | 手动 `cargo run` | 无 systemd unit / 安装脚本 / 单二进制分发 |
| TUI | 三段式只读（`ui.rs`） | 无暂停/过滤/手动确认/键轮换提示 |
| 观测 | tracing stdout | 无日志文件轮转、无 metrics、无健康端点 |

### 2.1 daemon 接收的数据结构

copier `GET /me/copy-orders` 返回 `Json<Vec<CopyOrderRow>>`（`crates/db/src/models.rs:241`，对应 `account.copy_order` 全字段）：

| 字段 | 类型 | 含义 | daemon 当前 |
|---|---|---|---|
| `id` | UUID | copy_order 主键，回传 `/result` 定位用 | ✅ 接 |
| `follow_relation_id` | UUID | 来源跟随关系，风控 cooldown 按 follow 聚合 | ❌ 丢弃 |
| `user_id` | UUID | 所属用户（应等于 `DAEMON_USER_ID`，校验防串号） | ❌ 丢弃 |
| `source_venue` | String | 信号来源 Venue | ❌ 丢弃 |
| `execute_venue` | String | 执行 Venue（按此分发签名器） | ✅ 接 |
| `source_market_id` | String | 信号源市场 id | ✅ 接 |
| `source_token_id` | String | 信号源 token id | ✅ 接 |
| `execute_market_id` | Option | 映射后执行市场 id | ✅ 接 |
| `execute_token_id` | Option | 映射后执行 token id（签名用） | ✅ 接 |
| `side` | String | `buy` / `sell` | ✅ 接 |
| `price` | Decimal | 限价（字符串序列化） | ✅ 接（降级为 f64） |
| `size` | Decimal | 数量（字符串序列化） | ✅ 接（降级为 f64） |
| `channel` | String | `tg` / `daemon`，daemon 拉 `channel=daemon` | ❌ 丢弃 |
| `signal_at` | DateTime | 信号产生时间（延迟度量用） | ❌ 丢弃 |
| `enqueued_at` | DateTime | 入队时间（`since` 游标字段） | ❌ 丢弃 |
| `status` | String | `pending` / `filled` / `skipped` / `failed` | ❌ 丢弃 |
| `skip_reason` | Option | 平台已 skip 的原因 | ❌ 丢弃 |

daemon 当前解析子集见 `apps/daemon/src/main.rs:33`（`CopyOrderDto`，`#[serde(default)]` 满地，对协议演进无防御）。M1 ledger 要用 `enqueued_at` 做 since 游标恢复，M3 风控 cooldown 要用 `follow_relation_id`，M4 延迟监控要用 `signal_at`，M4 协议握手要靠严格反序列化发现版本错配——均需补回。

### 2.2 两层风控现状对照

风控分两层：**平台侧 copier**（指令派发前）+ **daemon 本地**（用户最后一道防线）。

**平台侧 copier**（`services/copier/src/risk.rs`，454 行 / 16 单测，已落地）：

- 装配链（`effective_limits`）：全局默认 → 档位缩放（free ×1 / pro_plus ×3/2/2）→ 用户 `risk_overrides` → Venue `ExecParams`。
- 校验项（`check_risk`）：`min_notional` / `min_size` / `daily_max_notional` / `max_open_positions` / `rapid_flip_max_count` / `consecutive_loss_limit` / `max_slippage_bps`（`check_slippage`，需 `Venue::book()`，dry_run 跳过）。
- per-follow 风控（`check_follow_risk`）：单条跟随关系的 `daily_max_notional` / `max_open_positions`，独立于全局。

**daemon 本地**（`apps/daemon/src/main.rs:239`，仅 1 项）：

- 只有 `DAEMON_LOCAL_MAX_NOTIONAL`（默认 1000.0）单笔 notional 上限，无日累计 / 持仓 / 冷却 / 黑白名单 / 价格区间。

| | 平台侧 copier | daemon 本地 |
|---|---|---|
| 校验项数 | 7 项 + per-follow 2 项 | 1 项 |
| 数据来源 | PG 实时聚合（日累计/持仓/连续失败） | 单笔 notional 算术 |
| 配置层级 | 三级覆盖 + Venue 参数 | 单个 env |
| 测试覆盖 | 16 单测 | 0 单测 |

**设计意图**：平台侧风控在指令派发前已生效——copier 只把通过风控的 `copy_order` 推给 daemon，daemon 拿到的已是"平台允许的"。daemon 本地风控是用户最后一道防线，防"平台配置错误 / 账号被盗用 / 协议被篡改"等极端情况，故默认只需一个保守的 notional 硬上限。

**M3 落地后**：把 `crates/shared::risk` 抽出来让 daemon 复用同一组判定函数，daemon 本地补上日累计 / cooldown / 黑白名单 / 价格区间 / per-venue 限额，但**不是重做平台侧风控**，而是"平台侧已放过 + daemon 再校验一次"的双保险。两层规则同源（共用 `crates/shared::risk`），避免漂移。

## 3. 设计原则

1. **零钥底线不破**：私钥永不离开 daemon 进程；不写日志、不进账本、不回传 copier。
2. **平台侧只派发+归集**：daemon 永远是 pull 模型，copier 不主动推（防火墙/NAT 友好）。
3. **可降级**：网络/凭证/CLOB 任一不可达 → skip + 回传原因，不丢指令。
4. **可重放**：本地账本 + 幂等回传，崩溃重启不双签不漏签。
5. **单二进制**：`sharpside-daemon` 一个文件 + 一份 `.env` + 一条 systemd unit，用户 5 分钟装好。

## 4. 里程碑路线图

| 里程碑 | 主题 | 估时 | 可发布 |
|---|---|---|---|
| M1 | 鲁棒性 + 本地账本 | 2-3 天 | ✅ 生产准入门槛 |
| M2 | 配置体系 + 密钥安全 | 2 天 | ✅ |
| M3 | 本地风控 + 多 Venue 执行抽象 | 3 天 | ✅ 与 Phase 3 Kalshi 复用 |
| M4 | 协议版本 + 部署 + 观测 | 2-3 天 | ✅ |
| M5 | TUI 进阶 + 长轮询 + 体验 | 3-4 天 | ✅ |

M1/M2 是生产准入硬门槛，优先做完再谈体验；M3 可提前到 Phase 3 之前任何时点；M4/M5 可与 Phase 2 信号扩容并行，互不阻塞。

## 5. M1 · 鲁棒性 + 本地账本

**目标**：从"能跑通"到"敢长期跑"。

### 5.1 新增 `apps/daemon/src/ledger.rs` — rusqlite 本地账本

```sql
-- 首次启动自建（迁移内嵌，无外部 schema 文件）
CREATE TABLE copy_order_local (
  id            TEXT PRIMARY KEY,          -- copy_order UUID
  received_at   INTEGER NOT NULL,          -- unix ms
  payload_json  TEXT NOT NULL,             -- 原始 CopyOrderDto
  status        TEXT NOT NULL,             -- pending|in_flight|filled|skipped|failed|dead
  in_flight     INTEGER NOT NULL DEFAULT 0,
  attempt       INTEGER NOT NULL DEFAULT 0,
  last_error    TEXT,
  reported_at    INTEGER
);
CREATE TABLE copy_execution_local (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  order_id        TEXT NOT NULL REFERENCES copy_order_local(id),
  filled_size     REAL, filled_price REAL, fee REAL,
  tx_hash         TEXT, venue_order_id TEXT,
  executed_at     INTEGER NOT NULL
);
CREATE TABLE run_state ( key TEXT PRIMARY KEY, value TEXT );
-- run_state keys: since_cursor, protocol_version, last_poll_at, last_success_at
```

### 5.2 改造 `main.rs::poll_once`

- 拉到指令先 `INSERT OR IGNORE` 进账本（按 `id` 去重），再逐条处理；崩溃重启从 `status='pending' AND in_flight=0` 续跑。
- 回传成功才 `UPDATE status='filled'`；失败计 `attempt++`，超 3 次标 `dead`，回传 `failed`。
- `since` 游标从 `run_state` 读，不再用内存变量——重启不回退、不漏。
- `report` 幂等：copier 侧已按 `copy_order_id` 唯一约束去重，daemon 侧补"已 reported 则跳过"。

### 5.3 新增 `apps/daemon/src/retry.rs`

指数退避（1s/2s/4s/8s/16s 上限 60s）+ 抖动；连续 5 次失败进冷却（停轮询 5min），TUI 红标。

### 5.4 验收

- `cargo test -p sharpside-daemon` 含 ledger 重放单测（`kill -9` 后重启不双签）。
- `infra/e2e.sh` 通道 B 用例在中途 `kill` daemon 后重启仍能 `filled`。

### 5.5 实现契约（M1 立即开发前的 How 层级）

#### 5.5.1 依赖（`Cargo.toml` 改动）

`Cargo.toml` `[workspace.dependencies]` 加：

```toml
rusqlite = { version = "0.32", features = ["bundled"] }  # bundled：musl 静态链接自带 sqlite
```

`apps/daemon/Cargo.toml` `[dependencies]` 加：

```toml
rusqlite.workspace = true
dirs = "5"   # 跨平台用户目录解析（~/.local/share, ~/Library, %APPDATA%）
```

`figment` / `zeroize` 已在 workspace deps（M2 用），M1 不引。

#### 5.5.2 配置键（M1 不动 `config.rs`，仅加 env）

`apps/daemon/src/config.rs` `Config` 加 2 字段：

```rust
pub ledger_path: String,           // DAEMON_LEDGER_PATH，默认 dirs::data_dir()/sharpside/daemon.db
pub max_attempts: u32,             // DAEMON_MAX_ATTEMPTS，默认 3
```

`from_env()` 加对应解析；`is_configured()` 不变。M2 重写 config.rs 时把这两项平滑迁入 figment。

#### 5.5.3 `Ledger` API（`apps/daemon/src/ledger.rs`）

```rust
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub struct Ledger {
    db: Mutex<Connection>,
}

pub struct LocalOrder {
    pub id: String,
    pub payload_json: String,
    pub status: OrderStatus,
    pub attempt: u32,
    pub last_error: Option<String>,
}

pub enum OrderStatus { Pending, InFlight, Filled, Skipped, Failed, Dead }

impl Ledger {
    /// 打开/创建 DB，跑内嵌迁移（CREATE TABLE IF NOT EXISTS，PRAGMA user_version=1）。
    /// 失败返回 Err，daemon 启动时直接 abort（不做内存降级，账本是 M1 的前提）。
    pub fn open(path: &str) -> anyhow::Result<Self>;

    /// 拉到指令先调；已存在则 IGNORE，返回 false 表示重复（去重）。
    pub fn insert_order(&self, id: &str, payload_json: &str) -> anyhow::Result<bool>;

    /// 启动恢复：返回 status=Pending AND in_flight=0 的指令（崩溃续跑）。
    pub fn list_pending(&self) -> anyhow::Result<Vec<LocalOrder>>;

    /// 处理前标记 in_flight=1，防崩溃后双签。
    pub fn mark_in_flight(&self, id: &str) -> anyhow::Result<()>;

    /// 回传成功后调；status=Filled|Skipped|Failed，写 copy_execution_local（仅 Filled）。
    pub fn finalize(
        &self,
        id: &str,
        status: OrderStatus,
        exec: Option<&LocalExec>,
        skip_reason: Option<&str>,
    ) -> anyhow::Result<()>;

    /// 失败但未达 max_attempts：attempt++，in_flight=0，last_error=reason。
    /// 达 max_attempts：status=Dead，回传 failed（调用方负责回传）。
    pub fn record_failure(&self, id: &str, reason: &str, max_attempts: u32) -> anyhow::Result<()>;

    /// since 游标持久化（替代 main.rs:178 的内存 since 变量）。
    pub fn get_since_cursor(&self) -> anyhow::Result<Option<DateTime<Utc>>>;
    pub fn set_since_cursor(&self, ts: DateTime<Utc>) -> anyhow::Result<()>;
}

pub struct LocalExec {
    pub filled_size: f64, pub filled_price: f64, pub fee: f64,
    pub tx_hash: String, pub venue_order_id: String,
}
```

并发模型：`Arc<Mutex<Connection>>`（rusqlite `Connection` 非 `Sync`；单写者 Mutex 够用，poll_loop 是唯一写者，TUI 只读 `UiState` 不读 ledger）。不用 r2d2 池（单连接够，避免引入依赖）。

#### 5.5.4 `poll_once` 新控制流

```text
poll_once:
  1. GET /me/copy-orders?since=<ledger.get_since_cursor() or now-24h>
  2. for o in orders:
       a. ledger.insert_order(o.id, payload_json)  → 若返回 false（重复），跳过
       b. ledger.mark_in_flight(o.id)
       c. 风控检查（沿用现有 local_max_notional）
          → 拒: ledger.finalize(id, Skipped, None, Some(reason))
                report(skipped)
                continue
       d. dry_run? :
            yes: ledger.finalize(id, Filled, Some(synthetic_exec), None)
                 report(filled)
            no : match execute_local(o):
                   Ok(fill) => ledger.finalize(id, Filled, Some(&fill), None); report(filled)
                   Err(reason) => ledger.record_failure(id, reason, max_attempts)
                                  if attempt_reached_max: ledger.finalize(id, Dead, None, Some(reason)); report(failed)
                                  else: continue  (不回传，下次重试)
  3. ledger.set_since_cursor(Utc::now())
```

关键变化：
- `since` 从内存变量改为 ledger 持久化（重启不回退）。
- 每条指令处理前 `mark_in_flight`，崩溃重启 `list_pending` 跳过 in_flight=1 的（防双签）。
- `report` 失败不再 `?` 冒泡中断整批——单条 catch，记 `last_error`，继续下一条。
- `execute_local` 失败未达 max_attempts 不回传（下次轮询重试），达 max 才回传 failed。

#### 5.5.5 `retry.rs` API

```rust
pub struct Backoff {
    attempt: u32,
    max: u32,           // 上限 60s
    cooldown_until: Option<Instant>,
}

impl Backoff {
    pub fn new() -> Self;
    /// 连续失败计数 +1，返回下次轮询前应 sleep 的时长（含抖动 ±20%）。
    /// 连续 5 次失败返回 cooldown（5min），期间 is_in_cooldown()=true。
    pub fn on_failure(&mut self) -> Duration;
    /// 成功时重置 attempt=0，清 cooldown。
    pub fn on_success(&mut self);
    pub fn is_in_cooldown(&self) -> bool;
    pub fn next_delay(&self) -> Duration;  // 1,2,4,8,16,32,60,60...
}
```

接入点：`poll_loop`（`main.rs:177`）的 `sleep` 处——

```rust
loop {
    if config.is_configured() && !backoff.is_in_cooldown() {
        match poll_once(...).await {
            Ok(_) => backoff.on_success(),
            Err(e) => { backoff.on_failure(); sleep(backoff.next_delay()).await; continue; }
        }
    }
    sleep(Duration::from_secs(config.poll_interval_secs)).await;
}
```

冷却状态存 `Backoff` 自身（不进 ledger，进程级状态，重启清零可接受）。

#### 5.5.6 迁移机制

启动 `Ledger::open` 跑：

```sql
PRAGMA user_version = 1;
CREATE TABLE IF NOT EXISTS copy_order_local (...);
CREATE TABLE IF NOT EXISTS copy_execution_local (...);
CREATE TABLE IF NOT EXISTS run_state (...);
CREATE INDEX IF NOT EXISTS idx_order_status ON copy_order_local(status, in_flight);
```

未来 M3 加 `risk_overrides` 列时 `PRAGMA user_version = 2` + `ALTER TABLE`。不用迁移框架（refinery/sqlx-migrate 过重，单表 daemon 够用）。

#### 5.5.7 错误处理策略

| 失败点 | 策略 |
|---|---|
| `Ledger::open` 失败 | daemon 启动 abort（账本是 M1 前提，不降级） |
| `insert_order` 失败 | `poll_once` 整批 abort，`poll_loop` 记错 + backoff，下次重试 |
| `mark_in_flight` 失败 | 单条 skip + log，继续下一条 |
| `finalize` 失败 | log warn，不阻塞（账本已记 payload，可手动补） |
| `report` 失败 | 单条 catch，记 `last_error`，不回传（下次轮询 copier 仍会再派，ledger 去重兜底） |
| `execute_local` 失败 | `record_failure`，未达 max 不回传，达 max 回传 failed |

#### 5.5.8 测试 harness

**单测**（`apps/daemon/src/ledger.rs` `#[cfg(test)]`）：

- 用 `tempfile::NamedTempFile` 建临时 DB（不污染用户目录）。
- `apps/daemon/Cargo.toml` `[dev-dependencies]` 加 `tempfile = "3"`。
- 重放场景：
  1. `Ledger::open` → `insert_order("a", ...)` → `mark_in_flight("a")` → drop Ledger（模拟崩溃）
  2. `Ledger::open` 同路径 → `list_pending()` 应返回空（in_flight=1 的不续跑，防双签）
  3. `insert_order("a", ...)` 应返回 false（去重）
- `Backoff` 单测：`on_failure` 5 次后 `is_in_cooldown()`；`on_success` 重置。

**e2e**（`infra/e2e.sh` 通道 B 段，约 line 109-124 改动）：

```bash
# 原：timeout 8 daemon 跑完
# 改：分两段
RUST_LOG=info ... timeout 3 daemon &  DAEMON_PID=$!
sleep 3
kill -9 $DAEMON_PID 2>/dev/null
# 重启，应从 ledger 续跑，不双签，最终 filled
RUST_LOG=info ... timeout 8 daemon >/tmp/e2e_daemon.log 2>&1
grep -q "dry-run 合成成交回传" /tmp/e2e_daemon.log && ok "daemon 重启后续跑成交" || bad "daemon 重启失败"
```

#### 5.5.9 文件改动清单（M1 范围）

```
Cargo.toml                              # +1 行：rusqlite workspace dep
apps/daemon/Cargo.toml                  # +2 行：rusqlite, dirs deps；+1 dev-dep：tempfile
apps/daemon/src/config.rs               # +2 字段：ledger_path, max_attempts
apps/daemon/src/ledger.rs               # 新文件（~250 行）
apps/daemon/src/retry.rs                # 新文件（~80 行）
apps/daemon/src/main.rs                 # 改 poll_loop + poll_once（约 -30/+60 行）
infra/e2e.sh                            # 通道 B 段改 kill/重启（约 -2/+8 行）
```

#### 5.5.10 开发顺序（M1 内部）

1. `Cargo.toml` + `apps/daemon/Cargo.toml` 加依赖
2. `ledger.rs`：`open` + schema + `insert_order` + 单测
3. `ledger.rs`：`mark_in_flight` + `list_pending` + `finalize` + 重放单测
4. `ledger.rs`：`get/set_since_cursor` + `record_failure`
5. `retry.rs`：`Backoff` + 单测
6. `config.rs`：加 `ledger_path` / `max_attempts` 字段
7. `main.rs`：`poll_loop` 接 `Backoff`
8. `main.rs`：`poll_once` 接 ledger（按 §5.5.4 控制流）
9. `infra/e2e.sh`：kill/重启断言
10. `cargo test --all` + `cargo clippy -D warnings` + e2e 全绿

每步可独立编译/测试，失败不阻塞前序步骤。

## 6. M2 · 配置体系 + 密钥安全

**目标**：用户不再手改环境变量，私钥不入 shell history。

### 6.1 重写 `apps/daemon/src/config.rs`

- `figment`（TOML + Env + File overlay）：`~/.config/sharpside/daemon.toml` ← `.env` ← 环境变量，优先级递增。
- 启动校验 + 友好报错（缺哪个、哪个非法、哪个被忽略）。
- `Config::redacted_display()` 供 TUI/日志显示，永不输出私钥/api_key 全文。

### 6.2 新增 `apps/daemon/src/secrets.rs`

私钥三种来源（优先级降序）：

1. `SHARPSIDE_KEYRING=1` 走 OS keyring（linux: secret-service，macOS: keychain，windows: credential-manager）。
2. 加密文件 `~/.config/sharpside/keys.enc`（argon2 派生 key，开箱时输 passphrase 解密）。
3. env `POLYMARKET_PRIVATE_KEY`（仅开发/CI）。

约束：

- 私钥以 `Zeroizing<[u8; 32]>` 持有，drop 时清零；禁止 `Display`/`Debug` 派生。
- 启动时只解一次进 `Arc<Zeroizing<...>>`，签名函数借用，永不复制出。
- `mlock` 私钥页（`memsec` crate），防被 swap 出磁盘。

### 6.3 验收

- 单测覆盖三源回退。
- TUI 头部显示"密钥来源: keyring / file / env"而非密钥本身。
- `strings` 二进制无残留测试私钥。

## 7. M3 · 本地风控 + 多 Venue 执行抽象

**目标**：daemon 不只是签名器，是用户本地的"风控 + 执行"网关。

### 7.1 新增 `apps/daemon/src/risk.rs`

共享 `crates/shared::risk` 规则的本地实现：

```rust
pub struct LocalRisk {
    max_notional_per_order: f64,
    daily_notional_cap: f64,         // 滚动 24h
    min_price: f64, max_price: f64,  // 防 0/1 边界单
    cooldown_secs: u64,              // 同 follow_id 连续下单间隔
    venue_quotas: HashMap<String, f64>,        // per-venue 日上限
    market_allowlist: Option<HashSet<String>>,
}
pub fn check(&self, o: &CopyOrderDto, ledger: &Ledger) -> Result<(), SkipReason>
```

日上限/cooldown 从 ledger 聚合（24h 窗口 filled notional 求和），不另起状态。

### 7.2 重构 `sign.rs` → `apps/daemon/src/exec/` 模块

- `exec/mod.rs`：`trait LocalExecutor { async fn execute(&self, o: &Order, price, size) -> Result<LocalFill, SkipReason>; }`
- `exec/polymarket.rs`：现 `sign.rs` 逻辑迁入，实现 trait。
- `exec/kalshi.rs`（Phase 3 占位）：RSA 签名 stub，`unimplemented` 回 skip。
- `main.rs::execute_local` 改为按 `execute_venue` 查 `HashMap<&str, Box<dyn LocalExecutor>>`，新 Venue 加一个文件 + 注册一行。

### 7.3 验收

- 单测覆盖风控各分支（超限/冷却/黑名单/价格越界）。
- `exec/mod.rs` 有 mock executor 单测验证 trait 边界。
- e2e 通道 B 注入一条超 notional 指令 → 回传 `skipped` 且 ledger 记原因。

## 8. M4 · 协议版本 + 部署 + 观测

**目标**：用户 5 分钟装好，长期跑能自诊断。

### 8.1 协议握手

- `crates/shared` 加 `PROTOCOL_VERSION: semver::Version`。
- copier `GET /me/copy-orders` 响应头加 `X-Sharpside-Protocol: 1.2.0`。
- daemon 启动首次拉取后比对 `^MAJOR.MINOR`，不兼容则 TUI 红条 + 拒绝执行 + 日志告警；兼容补丁版差异仅 warn。
- daemon `User-Agent: sharpside-daemon/<ver> rust/<ver>` 便于 copier 侧统计。

### 8.2 部署工件

- `apps/daemon/assets/sharpside-daemon.service`：systemd unit（`Restart=on-failure`、`EnvironmentFile=/etc/sharpside/daemon.env`、`ReadWritePaths=/var/lib/sharpside`、`NoNewPrivileges=true`、`PrivateTmp=true`、`ProtectSystem=strict`）。
- `apps/daemon/assets/install.sh`：检测架构 → 下载 release 二进制 → 落 `/usr/local/bin` → 生成 `/etc/sharpside/daemon.env` 模板 → 装 systemd unit → `systemctl enable --now` → 打印"下一步：填 DAEMON_USER_ID / DAEMON_API_KEY / 私钥"。
- `Dockerfile` 加 daemon stage（多平台 buildx：x86_64 + aarch64，musl 静态）。
- GitHub Actions release workflow 产 `sharpside-daemon-<ver>-<arch>.tar.gz` + SHA256。

### 8.3 观测

- `tracing-appender` 滚动日志到 `~/.local/share/sharpside/daemon.log`（按日轮转，留 14 天）。
- `apps/daemon/src/metrics.rs`：进程内 Prometheus 文本指标（`/metrics` on `127.0.0.1:9090`，可关）：
  - `sharpside_daemon_polls_total`
  - `sharpside_daemon_orders{status="filled|skipped|failed"}`
  - `sharpside_daemon_sign_seconds`
  - `sharpside_daemon_since_lag_seconds`
  - `sharpside_daemon_in_flight`
- 健康自检：连续 3 次轮询失败或 since 落后 > 5min → TUI/`/metrics` 标 unhealthy。

### 8.4 验收

- 干净 Ubuntu 22.04 容器跑 `install.sh` → `systemctl status sharpside-daemon` active。
- `curl :9090/metrics` 有数据。
- 手动改 `daemon.env` 错 key → 服务进 degraded 且日志清晰。

## 9. M5 · TUI 进阶 + 长轮询 + 体验

**目标**：从"能用"到"好用"，对标 polycopier 体验。

### 9.1 长轮询（可选开关 `DAEMON_LONG_POLL=1`）

- copier `GET /me/copy-orders?since=&wait=30`（hang 直到有新指令或超时），daemon 端单连接复用；默认仍短轮询保兼容。
- 失败回退：长轮询 501/超时 → 自动降级短轮询。

### 9.1.1 为何不用 WebSocket（选型记录，避免后人重复纠结）

通道 B 是 daemon↔copier 的指令派发链路，候选实时方案：WebSocket / SSE / 长轮询。**选长轮询，不上 WS**，理由：

| 维度 | WebSocket | SSE / 长轮询 |
|---|---|---|
| 方向需求 | 双向，但 daemon 已用 `POST /result` 回传成交，双向优势用不上 | 单向 server→client 即够 |
| copier 状态 | **有状态**：须维护每 daemon 的 WS 连接 + 心跳 + 重连；多副本要 sticky session 或 Redis pubsub 桥接 | **无状态**：仍是普通 axum handler |
| 用户网络 | `Upgrade: websocket` 在企业网络/公共 WiFi 拦截率明显高于纯 HTTP | 纯 HTTP，几乎全通 |
| 协议演进 | WS 帧无 HTTP 语义，版本协商/限流/重试/幂等都要自造 | 复用 HTTP 中间件（`X-Sharpside-Protocol` 头、429 退避） |
| 频率匹配 | copy_order 是低频派发，WS 的低延迟优势被握手开销抵消 | 长轮询在低频场景最优 |

> Polymarket CLOB 自己用 WS（`wss://ws-subscriptions-clob.polymarket.com`）是**行情侧**，不是订单侧；daemon 拉的是 copier 派发的 copy_order，频率远低于行情 tick，不可类比。

**WS 唯一值得上的场景**：未来若 copier 要主动推"撤销该指令 / 暂停该 follow / 风控阈值变更"等带外控制事件给 daemon，且确认用户网络能稳定透传 Upgrade——那时 WS 的双向才有价值，且应作为**控制面**与**数据面（长轮询）分离**，而非替换数据面。

### 9.1.2 SSE 作为延迟不达标时的下一步

若长轮询上线后实测 P99 派发延迟仍超 SLA（如 > 2s），再上 SSE，不一步到位：

- copier 加 `GET /me/copy-orders/stream`（`text/event-stream`），daemon 用 `eventsource-stream` crate 订阅，重连内建。
- 仍是 HTTP，copier 不持有跨请求状态（按连接刷 stream），多副本无需 sticky。
- 鉴权复用现有 `X-User-Id` / `X-Daemon-Api-Key` 头，不引入 query token。
- 与长轮询并存，daemon 启动按 `DAEMON_STREAM=sse|longpoll|poll` 选择，失败降级到下一档。

### 9.2 TUI 增强（`ui.rs`）

快捷键：

| 键 | 动作 |
|---|---|
| `p` | 暂停轮询（不退出） |
| `r` | 恢复轮询 |
| `f` | 过滤（按 venue/status/follow） |
| `l` | 查看 ledger 全量 |
| `k` | 触发密钥轮换向导（提示去 web `#/settings/daemon-key` 重新颁发后粘贴新 key） |
| `?` | 帮助 |

面板：

- 指令详情面板：选中行展开 payload + 风控检查明细 + 签名耗时 + tx_hash 前 8 位。
- 配置变更热加载：`SIGUSR1` 触发 `Config::reload()`（除私钥外），不停服调参。

### 9.3 文档

- `docs/RUNBOOK_DAEMON.md`：安装、配置、密钥、故障排查、升级、卸载。
- `docs/DAEMON_PROTOCOL.md`：copier↔daemon HTTP 契约（端点、头、字段、版本协商），作为 daemon 第三方实现的规范。

### 9.4 验收

- TUI 单测（`ratatui::backend::TestBackend`）覆盖暂停/过滤/详情。
- `docs/RUNBOOK_DAEMON.md` 按步骤可复现安装。

## 10. 跨阶段共享工作

| 项 | 落点 | 说明 |
|---|---|---|
| `crates/shared::risk` | M3 | 把 daemon 本地风控规则抽到 shared，copier 平台侧与 daemon 共用同一组判定函数，避免漂移 |
| `crates/shared::PROTOCOL_VERSION` | M4 | semver 常量 + 兼容矩阵单测 |
| `infra/e2e.sh` | 每阶段 | 每个里程碑加一组断言（kill 重启、风控 skip、版本不兼容、metrics 端点） |
| `Cargo.toml` | M2/M4 | 加 `rusqlite`/`figment`/`keyring`/`tracing-appender`/`prometheus`/`semver`/`memsec`/`zeroize` 依赖 |

## 11. 风险与对策

| 风险 | 对策 |
|---|---|
| rusqlite 在 musl 静态链接需 `libsqlite3-sys` bundled feature | `Cargo.toml` 锁 `features=["bundled"]`，体积 +2MB 可接受 |
| keyring 跨平台差异大 | 三源回退 + 文件加密兜底，keyring 失败不阻塞启动 |
| 长轮询被代理/防火墙切断 | 默认短轮询，长轮询 opt-in，超时自动降级 |
| 协议版本不兼容时用户茫然 | TUI 红条 + 一键复制升级命令 + 文档锚点 |
| 私钥被 swap 出磁盘 | `mlock` 私钥页（`memsec`），swap 优先级策略兜底 |

## 12. 文件改动一览

```
apps/daemon/
├── Cargo.toml                    # 加依赖（M2/M4）
├── src/
│   ├── main.rs                   # M1 改 poll_once；M3 改 execute_local 分发
│   ├── config.rs                 # M2 重写为 figlet
│   ├── ui.rs                     # M5 增强
│   ├── sign.rs                   # M3 迁入 exec/polymarket.rs，本文件删除
│   ├── ledger.rs                 # M1 新增
│   ├── retry.rs                  # M1 新增
│   ├── secrets.rs                # M2 新增
│   ├── risk.rs                   # M3 新增
│   ├── metrics.rs                # M4 新增
│   └── exec/
│       ├── mod.rs                # M3 LocalExecutor trait
│       ├── polymarket.rs         # M3 迁自 sign.rs
│       └── kalshi.rs              # M3 占位
└── assets/
    ├── sharpside-daemon.service  # M4 systemd unit
    └── install.sh                # M4 安装脚本

crates/shared/
├── src/risk.rs                   # M3 抽公共风控规则
└── src/protocol.rs               # M4 PROTOCOL_VERSION

docs/
├── DAEMON_ROADMAP.md             # 本文档
├── RUNBOOK_DAEMON.md             # M5
└── DAEMON_PROTOCOL.md            # M5
```

## 13. 建议执行顺序

M1 → M2 → M3 → M4 → M5。M1/M2 是生产准入硬门槛，优先做完再谈体验；M3 与 Phase 3 Kalshi 执行复用，可提前到 Phase 3 之前任何时点；M4/M5 可与 Phase 2 信号扩容并行，互不阻塞。

## 14. 是否拆为独立项目（决策记录，避免后人重复纠结）

### 14.1 现状

`apps/daemon/` 是 sharpside 单一 cargo workspace 中的**唯一**发给终端用户的 crate；其余 13 个 crate（services/* + apps/web + apps/admin + apps/tg-bot）全跑在平台侧。daemon 依赖 2 个内部 crate（`sharpside-shared`、`sharpside-venues-polymarket`），均为 path 依赖。

### 14.2 支持拆分的理由

| 维度 | 说明 |
|---|---|
| 发布节奏 | daemon 走用户侧升级（周/月），平台服务走 SaaS 滚动发布（日/小时）；同仓强制同步，daemon 用户每次升级拉一堆无关 changelog |
| 克隆体积 | 用户从源码构建需 clone 整 workspace（含 services/web/admin/tg-bot + vendor/ + target/），实际只需 daemon + 2 个 crate |
| 安全审计 | daemon 是"跑在用户机器上持私钥"的二进制，独立仓库让审计边界清晰 |
| Issue 归属 | daemon 用户报 bug 应进 daemon 仓库，不与平台 infra bug 混杂 |
| 开源策略 | 未来若 daemon 开源（让用户审计零钥实现）而平台侧闭源，必须分仓库 |
| CI 效率 | daemon CI 只跑 daemon 测试（当前 `cargo test --all` 跑 14 个 crate），发布产物单一 |

### 14.3 反对拆分的理由

| 维度 | 说明 |
|---|---|
| 共享 crate 同步 | `sharpside-shared`（含 M4 `PROTOCOL_VERSION`、M3 `risk`）和 `sharpside-venues-polymarket` 被 daemon 与 copier 共用；拆仓后必须解决 crates.io 私有 registry / git dep / subtree / vendor 副本——每种都有同步成本 |
| 协议版本漂移 | M4 协议握手要靠 `crates/shared::PROTOCOL_VERSION` 单一来源；拆仓后此常量放哪边？跨仓 PR 协调开销大 |
| 重构摩擦 | 改一个共享类型（如 `CopyOrderRow` 加字段）现在一个 PR 搞定；拆仓后两边各一个 PR + 版本对齐 |
| M3 计划受阻 | M3 明确要抽 `crates/shared::risk` 让 copier 与 daemon 同源判定；拆仓使"同源"变"跨仓同步"，违背初衷 |
| 本地开发 | 开发者需 checkout 两仓 + 配置 path dep 覆盖（`[patch.crates-io]` 指向本地路径），新人上手成本高 |
| 现状已隔离 | `apps/daemon/` 已是独立 crate，与 services 零代码耦合（只通过 shared 类型交互）；逻辑边界已存在，物理边界收益有限 |

### 14.4 拆分方案对照

| 方案 | 同步机制 | 优点 | 代价 | 适合时机 |
|---|---|---|---|---|
| A. 维持 monorepo | path dep | 零同步成本，重构最顺 | 用户克隆体积大、changelog 混杂 | 当前阶段 ✅ |
| B. monorepo + 发布产物提取 | CI 从 monorepo 抽 daemon + 依赖 crate 源码打包 tarball | 保留 monorepo 重构便利，用户拿干净小包 | CI 脚本复杂度 +1 | M4 落地时 ✅ 推荐 |
| C. monorepo + daemon 独立 workspace | 同 repo 双 Cargo.lock，daemon 用 git dep 引 shared | 发布独立，源码同仓 | 双 workspace 配置复杂，IDE 支持差 | 不推荐 |
| D. 拆仓 + 共享 crate 发 crates.io（私有 registry） | 版本号协调 | 真正独立，CI/issue/licence 全分离 | 私有 registry 基建 + 跨仓 PR + 版本对齐 | 协议稳定后 + 团队分叉 |
| E. 拆仓 + git subtree/submodule 共享 crate | subtree sync 命令 | 不需 registry | submodule 体验差，subtree sync 易冲突 | 不推荐 |
| F. 拆仓 + 共享 crate vendor 副本 | 手动同步 | 简单 | 必然漂移，违背 M3 同源初衷 | 不推荐 |

### 14.5 拆分前提条件评估

| 前提 | 当前是否满足 |
|---|---|
| daemon 与平台由不同团队维护、不同发布节奏 | ❌ 单开发者 |
| 协议已稳定，跨仓同步罕见 | ❌ M4 才做协议握手，仍在演进 |
| daemon 需独立开源 / 独立 license | ❌ 全仓 AGPL |
| 共享 crate 已发布且 API 稳定 | ❌ 仍是 path dep，M3 还要抽 `risk` 进去 |
| 用户从源码构建是主要安装方式 | ❌ M4 走二进制 + install.sh |

**5 项前提 0 项满足**——现在拆仓纯负收益。

### 14.6 决策

**M1-M5 期间维持 monorepo（方案 A），M4 落地时升级为方案 B（发布产物提取）。**

M4 具体做法：

1. CI 加 `release-daemon` job：`cargo build -p sharpside-daemon --release --target <arch>-unknown-linux-musl`。
2. 打包脚本把 `apps/daemon/` + `crates/shared/` + `crates/venues/polymarket/` + 裁剪后的 `Cargo.toml` + `Cargo.lock` 打成 `sharpside-daemon-src-<ver>.tar.gz`（供源码构建/审计）。
3. 主产物是预编译二进制 `sharpside-daemon-<ver>-<arch>.tar.gz` + SHA256 + `install.sh`。
4. 用户 99% 走二进制 + install.sh，源码包只是审计/离线构建兜底。

### 14.7 何时重新评估

以下任一满足时重新评估方案 D（拆仓）：

- daemon 与平台分给不同人 / 不同团队。
- `crates/shared` 与 `crates/venues/polymarket` API 稳定 6 个月以上无 breaking change。
- daemon 需独立开源 / 独立 license / 接受第三方贡献。
- 协议版本握手（M4）上线且稳定运行 3 个月以上。
