# F0 落地检查清单

> 对应 `docs/FRONTEND_RUST.md` §6（13 用户页）+ §7（6 运营后台页）+ §11（后端契约补点）。
> 用途：F0 阶段（离线可落地）交付前的实现就绪度核对与验收。
> 状态图例：✅ 就绪 · 🟡 已实现/有降级 · 🔴 缺失/阻塞

---

## 1. 用户端页面（`apps/web`，13 页）

| # | 路由 | 页面 | 设计章节 | 状态 | 备注 |
|---|---|---|---|---|---|
| 1 | `#/` | 首页/Venue 总览 | §6.7 | ✅ | Venue 表 + 热门交易者预览（点击跳详情） |
| 2 | `#/login` | 登录注册 | §6.8 | ✅ | tab 切换注册/登录 + 管辖域 + 条款 |
| 3 | `#/leaderboard` | 排行榜 | §6.2 | ✅ | 筛选(平台/周期/排序/搜索/热钥/验证) + 分页 + URL 同步 |
| 4 | `#/traders/:p/:a` | 交易者详情 | §6.1 | ✅ | 头部+KPI+权益曲线SVG+持仓+成交+跟随模态 |
| 5 | `#/follows` | 我的跟随 | §6.9 | ✅ | 跟随卡 + 筛选/排序 + Pro+ 槽位条 + 编辑/复制ID/删除 |
| 6 | `#/follows/new` | 创建跟随 | §6.10 | ✅ | 单Venue/跨Venue radio + 身份下拉(manual_verified) + sizing 三模式 + 高级风控折叠 |
| 7 | `#/copy-history` | 成交历史 | §6.11 | ✅ | 服务端筛选(since/follow/venue/status) + 全量表 + 分页 + CSV 导出 |
| 8 | `#/dashboard` | 仪表盘 | §6.6 | ✅ | BFF 概览(jurisdiction/venues/portfolio_kpi) + 跟随 + 近期成交/指令 |
| 9 | `#/portfolio` | 投资组合 | §6.3 | ✅ | KPI + 权益曲线 + 分跟随/Venue + 延迟直方图 + 持仓 + CSV |
| 10 | `#/settings/subscription` | 订阅 | §6.12 | ✅ | 档位对比卡 + 升级/取消 + 支付占位（测试开通） |
| 11 | `#/settings/credentials` | Venue 凭证 | §6.5 | ✅ | Polymarket 卡 + 预配状态机 + Kalshi/Manifold 占位 + daemon key 入口 |
| 12 | `#/settings/daemon-key` | daemon API key | §6.13 | ✅ | 状态 + 颁发/轮换 + 一次性明文弹窗 + 安装步骤 |
| 13 | `#/settings/delegation` | 委托管理 | §6.4 | ✅ | 托管横幅 + 资产/交易权双卡 + 8步预配 + 凭证详情 + 撤销(锁) |

**路由注册核对**（`apps/web/static/main.js`）：以上 13 路由全部注册，鉴权页标 `'auth'`，登录页标 `'guest'`。

---

## 2. 运营后台页面（`apps/admin`，6 页）

| # | 路由 | 页面 | 设计章节 | 状态 | 备注 |
|---|---|---|---|---|---|
| 1 | `#/login` | admin 登录 | §7.1 | ✅ | admin token（Bearer），验证后跳 `/mappings` |
| 2 | `#/mappings` | 市场映射审核 | §7.2 | ✅ | 候选卡 + direction_flip + notes + min_notional + 验证/撤销 |
| 3 | `#/identities` | 身份审核 | §7.3 | ✅ | 候选卡 + alias/confidence/启发式 + 确认/删除 |
| 4 | `#/hot-wallets` | 热钥管理 | §7.4 | ✅ | Venue 筛选 + 表格 + 添加/编辑模态 + 删除 |
| 5 | `#/tag-rules` | 标签阈值 | §7.5 | ✅ | 表格 + JSON 编辑器（JSON.parse 校验）+ enabled |
| 6 | `#/visibility` | 可见性管控 | §7.6 | ✅ | 搜索 + Venue 筛选 + 行内三态切换 + 应用 |
| 7 | `#/audit-thresholds` | 影子阈值 | §7.7 | ✅ | 表格 + 编辑模态（warn/alert × pct/abs） |

**基础设施**：`AdminAuth` extractor（Bearer token）+ hash 路由守卫 + `/api` 同源 + `ServeDir` SPA fallback + `noindex,nofollow`。

---

## 3. 后端契约就绪度（§11 补点对照）

| 端点 | 服务 | 用途页 | 状态 | 前端降级 |
|---|---|---|---|---|
| 扩 `GET /traders`（join 绩效+标签，sort/period/q/hot_only/verified_only） | venue-hub | 排行榜/首页 | ✅ | — |
| `GET /traders/{p}/{a}/equity-curve` | venue-hub | 详情 | ✅ | — |
| `GET /traders/{p}/{a}/positions` | venue-hub | 详情 | ✅ | — |
| `GET /traders/{p}/{a}/trades` | venue-hub | 详情 | ✅ | — |
| `GET /copier/me/portfolio?period=` | copier | 组合 | ✅ | — |
| `GET /copier/me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=` | copier | 成交历史/导出 | ✅ | 服务端过滤已就绪；前端 `#/copy-history` 透传筛选参数 |
| `GET /me/delegation`（安全视图） | account | 委托/凭证 | ✅ | — |
| provision 状态持久化 | account | 委托/凭证 | ✅ | `provision_live`/`provision_steps`/`kms_key_id` 写入 credential blob；旧 blob 回退推断 |
| BFF `/me/dashboard` 补全 | gateway | 仪表盘 | ✅ | jurisdiction→available_venues；并发拉 portfolio_kpi；jurisdiction 字段 |
| `UserVenueCredential` 加 `kind` 字段 | db | 凭证 | ✅ | 列级 `kind`（迁移 0017）；列表 API 返回；预配/upsert 从 blob 同步 |
| `GET /venue-hub/identities`（身份列表） | venue-hub | 创建跟随 | ✅ | `manual_verified=true` 列表；前端下拉已接线 |
| `POST /copier/manual-order` | copier | 手动下单 | — | Phase 2，F0 不含 |
| `POST /account/me/deposit-wallet/revoke` | account | 委托撤销 | — | Phase 2，前端灰显+锁标 |
| `GET /account/me/security-log` | account | 安全日志 | — | Phase 2，F0 不含 |

---

## 4. 横切关注点

| 项 | 状态 | 说明 |
|---|---|---|
| 路由 | ✅ | hash 路由（`#/path?query`），守卫 auth/guest，401 全局事件跳登录 |
| 鉴权 | ✅ | web: JWT 存 localStorage + `Authorization: Bearer`；admin: admin token |
| 设计系统 | ✅ | `tokens.css` 暗色优先 + `prefers-color-scheme` light 覆盖 |
| 通用组件 | ✅ | `ui.js`（el/statCard/dataTable/skeleton/emptyState/fmt*）+ `follow-form` + `one-time-secret` |
| 降级策略 | ✅ | 上游不可达返空结构，前端对 null 区块隐藏不阻塞整页 |
| SEO | ✅ | 公开页(首页/排行榜/详情)可 SSR 直出；登录/鉴权页 `noindex` |
| 安全关键交互 | ✅ | daemon key 一次性明文弹窗强制勾选 [我已保存] 才能关，不允许点背景关 |
| 诚实口径 | ✅ | 托管等级横幅标注"委托交易（未到完全非托管）"；Phase 项灰显+锁标 |

---

## 5. 验收步骤（手动）

### 5.1 启动
```bash
# 后端（按 ARCHITECTURE.md 启动 gateway/venue-hub/follow/copier/account/admin）
# 前端静态由各服务 ServeDir 提供：
#   web  → gateway 或独立静态服务 apps/web/static
#   admin → apps/admin (nest /api + ServeDir SPA fallback)
```

### 5.2 用户端走查
- [ ] `#/` 首页：Venue 表 + 热门交易者加载（或降级文案）
- [ ] `#/leaderboard`：切换平台/周期/排序 → URL 同步 + 表格刷新 + 分页
- [ ] `#/traders/:p/:a`：头部/KPI/曲线/持仓/成交渲染；点[跟随]弹模态
- [ ] `#/login`：注册→登录→跳 `#/dashboard`；401 红字
- [ ] `#/follows`：跟随卡列表 + 筛选/排序 + 槽位条；暂停/编辑/复制ID/删除
- [ ] `#/follows/new`：单Venue/跨Venue 切换；身份下拉（仅 manual_verified）；sizing 三模式；创建→跳列表
- [ ] `#/copy-history`：筛选（时间/跟随/Venue/状态）服务端过滤 + 分页 + 导出 CSV
- [ ] `#/portfolio`：周期切换；KPI/曲线/分项/延迟直方图/持仓；导出 CSV
- [ ] `#/dashboard`：管辖域/可用 Venue；组合 KPI 优先用 BFF `portfolio_kpi`；跟随/成交/指令降级正常
- [ ] `#/settings/subscription`：Free/Pro+ 对比卡；升级弹占位；取消订阅
- [ ] `#/settings/credentials`：Polymarket 卡 + 预配状态机展开；daemon key 入口跳转
- [ ] `#/settings/daemon-key`：状态显示；轮换→明文弹窗（强制勾选才能关）
- [ ] `#/settings/delegation`：托管横幅 + 8步 stepper（读 blob 持久化 steps）+ 撤销(锁)

### 5.3 运营后台走查
- [ ] `#/login`：admin token 登录；错误 token 红字
- [ ] `#/mappings`：候选卡 direction_flip/notes/min_notional；验证/撤销
- [ ] `#/identities`：候选卡确认/删除
- [ ] `#/hot-wallets`：Venue 切换重拉；添加/编辑模态；删除确认
- [ ] `#/tag-rules`：编辑模态 JSON 校验（非法→红字，不发请求）
- [ ] `#/visibility`：搜索/Venue 筛选；行内三态切换 + 应用确认
- [ ] `#/audit-thresholds`：编辑模态 4 数字输入保存

### 5.4 降级验证
- [ ] 关停 copier → `#/portfolio`/`#/copy-history`/仪表盘成交区显示降级文案，不崩页
- [ ] 关停 account → `#/follows` 槽位按 Free(3) 兜底；订阅页显示 Free；仪表盘 jurisdiction 回退 other
- [ ] 重新预配后 `#/settings/delegation` stepper 与 blob 中 `provision_steps` 一致（旧凭证无字段时仍可推断）

---

## 6. 上线前阻塞项 vs 可后补

### 🔴 阻塞（建议上线前处理）
- 无（F0 范围内所有页面已实现，后端缺口均有前端降级兜底，不阻塞离线交付）

### 🟡 建议尽快补（不阻塞 F0，影响体验/数据完整）
- （暂无）F0 §11 补点已全部落地；后续为 Phase 2 / 体验打磨

### ~~已补完~~
- ~~`GET /copier/me/copy-executions` 过滤参数~~ → ✅
- ~~`GET /venue-hub/identities`~~ → ✅
- ~~BFF `/me/dashboard` 补全~~ → ✅
- ~~provision 状态持久化~~ → ✅
- ~~`UserVenueCredential.kind` 列级字段~~ → ✅ 迁移 0017 + 列表 API 暴露

### ⏭ Phase 2（明确延后，设计文档已标注）
- 手动下单 `#/trade/:market_id`
- 委托撤销 `POST /me/deposit-wallet/revoke`
- 安全日志 `#/settings/security-log`
- 身份详情 `#/identities/:id`、市场浏览 `#/markets`
- 非托管升级路径

---

## 7. F1 迁移备忘（Leptos）

F0 按 Leptos 心智模型组织（页面=路由、store=signal、组件=props），迁移为"翻译"：
- 路由：hash → Leptos path 路由，守卫换 `server fn` 鉴权
- `el()` 工厂 → Leptos `view!` 宏
- `lib/*.js` API 封装 → server functions（契约不变）
- SVG 图表 → `echarts-rs`
- `tokens.css` → Tailwind 4 token 映射
- 单文件 < 200 行约束保证可逐页翻译

---

**结论**：F0 设计文档规划的 13 用户页 + 6 运营后台页**全部已实现**，后端 §11 补点大部分就绪，剩余缺口均有前端降级兜底，满足"离线可落地、不被网络阻塞"的 F0 交付标准。
