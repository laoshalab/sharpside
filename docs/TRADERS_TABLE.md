# traders 表 · 字段详解（多平台原生版）

> `traders` 是 VenueHub 实体层的交易者主表，是 sharpside 一切"人 × 平台"维度的根表。
> 一行 = 某 Venue 上的一个交易者。**复合主键 `(platform, address)`**。

## 1. 表定义

```sql
CREATE TABLE trader_hub.traders (
    platform          text        NOT NULL,
    address           text        NOT NULL,
    identity_id       uuid        REFERENCES trader_hub.identities(id),
    alias             text,
    source            text        NOT NULL,
    is_hot            boolean     NOT NULL DEFAULT false,
    visibility        text        NOT NULL DEFAULT 'visible',
    profile_image     text,
    x_username        text,
    verified_badge    boolean,
    user_name         text,
    first_seen        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address)
);

CREATE INDEX idx_traders_is_hot      ON trader_hub.traders (platform, is_hot) WHERE is_hot;
CREATE INDEX idx_traders_visibility  ON trader_hub.traders (platform, visibility);
CREATE INDEX idx_traders_identity    ON trader_hub.traders (identity_id) WHERE identity_id IS NOT NULL;
```

`address` 一律小写存储。`(platform, address)` 作为跨表自然键（`position_timeline` / `trader_performance` / `trader_positions_snapshot` / `follow_relation` 等均引用它，不强外键，便于服务独立演进）。

## 2. 字段逐项

### platform · text NOT NULL（PK 一部分）
- **含义**：该交易者所属 Venue
- **取值**：`polymarket` / `kalshi` / `manifold` / `zeitgeist` / `azuro`
- **来源**：入库逻辑判定（从哪个 Venue 爬到/导入）
- **作用**：与 address 共同定位唯一交易者；决定走哪个 Venue adapter 抓数据/执行

### address · text NOT NULL（PK 一部分）
- **含义**：该交易者在 Venue 内的标识
  - Polymarket / Zeitgeist / Azuro：proxy wallet 地址（0x + 40 hex，小写）
  - Kalshi：user id（Kalshi 不暴露个体交易者，此字段通常为空——Kalshi 不作信号源）
  - Manifold：user id
- **来源**：Venue leaderboard 或用户/运营导入
- **处理**：链上地址 `to_lowercase`；玩钱/KYC 平台原值
- **作用**：与 platform 共同作为自然键

### identity_id · uuid（可空）
- **含义**：跨 Venue 身份聚合指针，指向 `identities.id`
- **来源**：身份 worker 启发式候选 + admin 人工确认后写入
- **作用**：把同一人在多平台的 trader 行链接到同一 identity，供 `identity_performance` 聚合
- **空值策略**：未链接身份时为 NULL，仅按单平台展示

### alias · text
- **含义**：站内显示名（可空）
- **来源**：优先取 `user_name`；运营可在 admin 改；改名轨迹写 `trader_alias`
- **作用**：排行榜、详情页、TG 推送展示
- **空值回退**：为空时前端显示 `address.slice(0,6)…slice(-4)`

### source · text NOT NULL
- **含义**：该交易者进入 sharpside 的来源
- **取值**：
  - `leaderboard`：爬某 Venue 排行榜时自动入库
  - `imported`：用户/运营输入地址主动导入
  - `manual`：运营在 admin 手动添加（如合作 KOL）
- **作用**：运营统计来源占比、决定回填深度

### is_hot · boolean NOT NULL DEFAULT false
- **含义**：是否进入热钥浮仓监控
- **来源**：admin 一键切换 / 自动规则（如某 Venue 排行榜 top 100 自动标 hot）
- **作用**：决定 `trader_positions_snapshot` 是否高频抓取（10–60s 自适应）
- **索引**：部分索引 `(platform, is_hot) WHERE is_hot`

### visibility · text NOT NULL DEFAULT 'visible'
- **含义**：运营对该交易者的展示管控
- **取值**：`visible` / `hidden` / `featured`
- **作用**：运营管控（作弊/合规/合作位售卖）

### profile_image · text
- **含义**：头像 URL
- **来源**：Venue leaderboard / profile API

### x_username · text
- **含义**：X(Twitter) 用户名
- **来源**：Venue leaderboard / profile API
- **作用**：详情页社交链接、运营联系合作；**身份启发式链接的关键信号**

### verified_badge · boolean
- **含义**：Venue 官方认证标记
- **来源**：Venue leaderboard
- **作用**：详情页徽章、排行榜筛选

### user_name · text
- **含义**：Venue 官方用户名（原值，与 `alias` 区别：`user_name` 不可改，`alias` 站内可改）
- **来源**：Venue leaderboard / profile API
- **作用**：保留官方原值做审计；`alias` 为空时回退到它；**身份启发式链接的关键信号**

### first_seen · timestamptz NOT NULL DEFAULT now()
- **含义**：sharpside 首次发现该 (platform, address) 的时间
- **作用**：运营看"新晋交易者"、冷启动监控

### updated_at · timestamptz NOT NULL DEFAULT now()
- **含义**：本行最近更新时间
- **作用**：增量同步判断、运营审计

## 3. 字段来源映射

| 字段 | 来源 |
|---|---|
| platform | 入库逻辑判定（从哪个 Venue 来） |
| address | Venue leaderboard.proxyWallet / 用户导入 |
| identity_id | 身份 worker + admin 确认 |
| user_name, profile_image, x_username, verified_badge | Venue leaderboard / profile API |
| alias | user_name 初始化，admin 可改 |
| source | 入库逻辑判定 |
| is_hot, visibility | admin 操作或自动规则 |
| first_seen, updated_at | 系统自动 |

## 4. 不存在 `traders` 里的数据（避免误设计）

| 不存的字段 | 落在哪 |
|---|---|
| pnl/volume/roi/胜率 | `trader_performance`（per (platform, address, period)） |
| 跨平台聚合绩效 | `identity_performance` 物化视图 |
| 当前持仓 | `trader_positions_snapshot`（带 platform） |
| 历史成交 | `raw_trades`（带 platform） |
| 权益曲线 | `trader_equity_curve`（带 platform） |
| 标签 | `trader_tag`（per (platform, address)） |
| 抓取游标 | `fetch_state`（per (platform, source, address)） |
| 跨平台身份 | `identities` 表（traders.identity_id 指向它） |

**原则**：`traders` 只存"相对稳定的身份与运营管控字段"，时变数据全部下沉到计算层/监控层，避免主表频繁更新。

## 5. 写入路径

```
某 Venue 排行榜爬虫 ──→ upsert (platform, address, source=leaderboard, user_name, profile_image, ...)
用户/运营导入(指定 platform) ──→ insert (platform, address, source=imported) → 触发该 Venue 完整回填
admin 手动添加 ──→ insert (platform, address, source=manual, alias, is_hot, visibility)
身份 worker + admin 确认 ──→ update identity_id + updated_at
admin 改 alias/visibility/is_hot ──→ update 对应列 + updated_at
profile 刷新任务 ──→ update user_name/profile_image/x_username/verified_badge
```

upsert 冲突策略：`ON CONFLICT (platform, address) DO UPDATE SET ... WHERE excluded.字段 IS DISTINCT FROM ...`，避免无意义写。

## 6. 读取路径

| 调用方 | 用法 |
|---|---|
| 排行榜 API（per Venue） | `WHERE platform=? AND visibility='visible'` JOIN `trader_performance` 排序 |
| 跨平台身份排行榜 | JOIN `identities` + `identity_performance` |
| 详情页 | `WHERE platform=? AND address=?` + JOIN performance/equity_curve/tag |
| 跟随页 | 校验 (platform, address) 存在 + visible |
| TG bot | 查 alias 用于推送文案 |
| admin 后台 | 全字段筛选/编辑 |
| Follow 信号派生 | `WHERE platform=? AND is_hot` JOIN snapshot |
| 身份 worker | 找 `identity_id IS NULL` 的候选做启发式链接 |

## 7. 示例行

```
platform        polymarket
address         0x56687bf447db6ffa42ffe2204a05edaa20f55839
identity_id     7f3c...-...-... (指向 identities 行)
alias           mintblade
source          leaderboard
is_hot          true
visibility      featured
profile_image   https://polymarket.com/.../avatar.png
x_username      mintblade_pm
verified_badge  true
user_name       mintblade
first_seen      2026-07-01 08:00:00+00
updated_at      2026-07-20 13:00:00+00
```

## 8. 多平台场景示例（同一人跨 Polymarket + Manifold）

```
traders 行 1:
  platform=polymarket, address=0x566...839, identity_id=7f3c...
  alias=mintblade, x_username=mintblade_pm

traders 行 2:
  platform=manifold, address=user_12345, identity_id=7f3c...
  alias=mintblade, x_username=mintblade_pm

identities 行:
  id=7f3c..., alias=mintblade, manual_verified=true
```

两行 `traders` 共享同一 `identity_id`，`identity_performance` 视图聚合两边绩效，前端展示"跨平台交易者 mintblade"。

## 9. 演进预留

- 加 `risk_tier` 字段（按风险分档展示）→ ALTER ADD COLUMN，向后兼容
- 加 `language` 字段（分区域推荐）→ 同上
- 新增 Venue 接入 → `platform` 取值新增，无需改表结构
- **不存任何 PII / 私钥 / 联系方式**（sharpside 不接触用户私密信息）
