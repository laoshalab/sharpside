# Channel A 灰测检查单

受控账户、小额、可回滚。部署/密钥/网络门禁见 [`DEPLOY_CHECKLIST.md`](./DEPLOY_CHECKLIST.md)；本页覆盖**功能灰测**与建议执行顺序。

对应生产化修复阶段 1–4（资金安全、凭证、会话运维、可观测）。

**产品范围（定案）：** 本期只做受控 Channel A 灰测。**不上线**公开 Channel B（公域 daemon 投放），**不做**营销/公域上线。通道 B 代码与 e2e 可保留自测，不作为发布目标。

---

## 0. 启动前（环境）

- [ ] 专用灰测账户 + 小额余额；`COPIER_DRY_RUN=1` 先跑通，再开 `=0`
- [ ] `POLYMARKET_CLOB_POST` 默认关；真钱路径单独开
- [ ] 迁移 `0035`–`0040` 已应用（含 jwt_denylist / copy_execution UNIQUE / credential revoke / archives / billing）；`infra/scripts/pg_backup.sh` 已跑过一次
- [ ] Prometheus 能抓到 venue-hub / copier `/metrics`
- [ ] [`DEPLOY_CHECKLIST.md`](./DEPLOY_CHECKLIST.md) 第 1–5、7 节已勾选

---

## 1. 身份与会话（阶段 1 + 3）

- [ ] 登录后 JWT / `Set-Cookie` 正常；刷新仍登录（cookie-first）
- [ ] `POST /auth/logout` 后旧 token 不可用（denylist）
- [ ] 绑钱包：无 SIWE → 失败；签名地址不一致 → 失败；合法签名 → 成功
- [ ] 订阅页**无**「测试开通」；`/config.js` 含 `production: true`

---

## 2. 委托与凭证（阶段 2）

- [ ] 预配后委托页可查；新凭证走 `encrypted_l2_passphrase` 路径可用
- [ ] `POST /me/deposit-wallet/revoke` 成功；之后 pending 单应 skip / 拒派发
- [ ] revoke 后 UI 锁定、不可再下单

---

## 3. 跟单配置（阶段 4.4）

- [ ] 跟随地址大小写混用仍命中同一 trader
- [ ] Fixed：`amount ≤ 0` 被拒
- [ ] Proportional：`ratio` 不在 `(0,1]` 被拒

---

## 4. 执行路径（阶段 1 核心）

先 dry，再真钱。

### A. Dry-sign（`COPIER_DRY_RUN=0`，`POLYMARKET_CLOB_POST≠1`）

- [ ] 订单最终 `skipped`（原因含 dry-sign）
- [ ] **无** `copy_execution` 行；**非** `filled` / `submitted`

### B. 真钱小额（`POLYMARKET_CLOB_POST=1`，极小 size）

- [ ] 状态机：`pending → dispatched → submitted → filled`（或超时 cancel）
- [ ] 同一 `copy_order` 重复上报 `/result` 仍只有一条 execution（UNIQUE / CAS）
- [ ] reclaim：人为卡住 `dispatched` 后可幂等恢复，不双花
- [ ] 触发 429 时 metrics `sharpside_clob_429_total` 增加

---

## 5. 风控 fail-closed（阶段 1.5）

- [ ] 超日名义 / 超仓位数 → skip，不静默当 0
- [ ] DB 查询失败时不 unwrap 成放行（日志可见，订单 skip / fail）

---

## 6. 可观测与韧性（阶段 4）

- [ ] venue-hub `readyz`：停 hot / ingest 超 `WORKER_STALE_SECS` → 503
- [ ] 制造 deadletter → `ALERT_WEBHOOK_URL` 收到告警 + metrics 死信计数增
- [ ] 日志为 JSON（`APP_ENV=production` 或 `LOG_FORMAT=json`）

---

## 7. 回滚演练

- [ ] `COPIER_DRY_RUN=1` 立即停真钱
- [ ] 缩容 / 停 copier；未完成单状态可解释
- [ ] 保留上一镜像 tag；必要时从备份恢复（演练一次即可）

---

## 建议执行顺序

1. [`DEPLOY_CHECKLIST.md`](./DEPLOY_CHECKLIST.md) 勾完  
2. 会话 / SIWE / logout  
3. dry-sign 路径  
4. revoke  
5. 真钱 1 笔  
6. metrics / 告警确认  
7. 回滚开关验证  

通过后可扩大 **Channel A** 灰测样本。**不上线**公开 Channel B，**不做**营销/公域投放（产品定案，非「稍后开阶段 5」）。
