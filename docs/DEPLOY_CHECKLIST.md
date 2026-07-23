# 生产部署清单（安全修复 3.4）

上线 / 灰测前逐项勾选。对应阶段 1–3 的运维门禁。

功能灰测（Channel A 受控账户）见 [`CHANNEL_A_CANARY.md`](./CHANNEL_A_CANARY.md)。

**范围定案：** 本清单服务 **Channel A 受控灰测**。公开 Channel B 与营销/公域上线 **本期不上线**（不作为本清单验收目标）。

## 1. 环境与密钥

- [ ] `APP_ENV=production`（所有服务）
- [ ] `JWT_SECRET` ≥32 字符、非默认值
- [ ] `TG_BOT_SECRET` ≥32 字符；`TG_BOT_TOKEN` 已配置
- [ ] `FOLLOW_SIGNAL_SECRET` / `INTERNAL_SIGNAL_SECRET` 一致且 ≥32
- [ ] `VENUE_HUB_ADMIN_TOKEN` ≥32、非 `dev-admin-token`
- [ ] `SHARPSIDE_KMS_MASTER_KEY_PATH` 已挂载（**生产默认 LocalKms 站内签**；禁止 `SHARPSIDE_KMS_DEV_PLAINTEXT`；不接云 KMS）
- [ ] LocalKms master key：0600、仅服务可读；**已离线备份**并做过至少一次恢复演练（丢失=全体用户密钥不可恢复）
- [ ] account 与 copier 挂载**同一** master key（多实例同钥）
- [ ] `COOKIE_SECURE=1`（HTTPS 终止后）
- [ ] `JWT_TTL_SECONDS=1800`（或更短）
- [ ] Postgres / MinIO 口令非 `sharpside_dev`，经 env 注入

## 2. Admin SSO（阶段 3.3）

- [ ] `OIDC_ISSUER` / `OIDC_CLIENT_ID` / `OIDC_CLIENT_SECRET` / `OIDC_REDIRECT_URI`
- [ ] `OIDC_ALLOWED_EMAILS` 白名单（逗号分隔）
- [ ] `ADMIN_SESSION_SECRET` ≥32
- [ ] 浏览器走「使用 SSO 登录」；共享 `ADMIN_TOKEN` 不可用于生产
- [ ] Admin 仅 `127.0.0.1` 或 VPN 可达（见 `docker-compose.prod.yml`）

## 3. 网络与暴露面

- [ ] 使用 `docker-compose.yml` + `docker-compose.prod.yml`
- [ ] 公网仅暴露 web（:80/:443）；gateway/后端不映射宿主机
- [ ] postgres / redis / minio **无**宿主机端口
- [ ] TLS 终止（Caddy/nginx/云 LB）；HSTS 可选
- [ ] web 无静态 bind-mount

## 4. 资源与健康

- [ ] 各服务 `deploy.resources` 限额已生效（swarm/compose v2）
- [ ] `/healthz` / `/readyz` 探针通过（venue-hub `readyz` 含 worker 心跳与快照新鲜度）
- [ ] Prometheus 抓取 `venue-hub` / `copier` 的 `/metrics`（outbox、deadletter、copy_order、clob_429）
- [ ] `ALERT_WEBHOOK_URL` 已指向值班通道（deadletter 告警）
- [ ] `LOG_FORMAT=json` 或 `APP_ENV=production`（结构化日志）

## 5. 数据与迁移

- [ ] 迁移已跑（venue-hub 为 migrator，或独立 migrate job）
- [ ] `copy_status` CHECK 含 `submitted`（迁移 0042；否则实盘 place_order 成功后写 submitted 触发 CHECK 违例 → 账实分裂）
- [ ] `raw_trades` 复合索引 `idx_raw_trades_trader_token` 存在（迁移 0043；diff 对账覆盖查询用）
- [ ] `jwt_denylist` / `copy_execution` UNIQUE / credential revoke 列存在
- [ ] PG 备份已排期：`infra/scripts/pg_backup.sh`（cron 示例见脚本头；`BACKUP_DIR` / `RETENTION_DAYS`）
- [ ] 至少一次恢复演练（gunzip | psql）

## 6. 功能门禁（Channel A 灰测）

环境开关（细则与用例见 [`CHANNEL_A_CANARY.md`](./CHANNEL_A_CANARY.md)）：

- [ ] `POLYMARKET_CLOB_POST=1` 仅在受控账户开启
- [ ] `COPIER_DRY_RUN=0` 仅在验收后开启（启动门禁：须 `APP_ENV=production` + LocalKms）
- [ ] `COPIER_ORDER_TYPE=FAK`（默认；GTC 易挂死单，跟单场景不建议）
- [ ] `COPIER_AGGRESSIVE_PRICING=true`（吃对手盘立即成交；关闭则按信号价 IOC，成交率低）
- [ ] `WORKER_FOLLOW_SCAN_SECS=10`（默认；缩短源钱包→跟单信号滞后，受 /positions 速率约束勿过低）
- [ ] `WORKER_TRADE_WATCH_SECS=3`（默认；逐笔成交信号主源，hot 降级为对账补漏）
- [ ] 订阅页无「测试开通」入口（`APP_ENV=production` → `/config.js`）
- [ ] 委托可 revoke；revoke 后 copier 停派发
- [ ] [`CHANNEL_A_CANARY.md`](./CHANNEL_A_CANARY.md) 功能项已勾选

## 7. 启动命令示例

```bash
cd infra
# 确认 .env 无默认口令，且含上表必填项
docker compose -f docker-compose.yml -f docker-compose.prod.yml --env-file ../.env up -d --build
docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
curl -fsS http://127.0.0.1/config.js   # 应含 production: true
```

## 8. 回滚

- [ ] 保留上一版镜像 tag
- [ ] DB 迁移向前兼容；不可逆迁移有备份快照
- [ ] 紧急：将 `COPIER_DRY_RUN=1` 或缩容 copier 停真钱下单
