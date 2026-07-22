# Runbook · 通道 A TG bot（teloxide）

> 对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §6。
> `apps/tg-bot` 用 teloxide 0.15 长轮询实现通道 A 用户面。平台代签 session wallet，
> 法律/产品定位为「平台代签」非「非托管」。

## 1. 前置

- PostgreSQL 已起：`docker compose -f infra/docker-compose.yml up -d postgres`
- account / follow / venue-hub 已起（bot 调用它们的 HTTP API）
- 在 `@BotFather` 创建 bot，拿到 `TG_BOT_TOKEN`
- account 与 tg-bot 的 `TG_BOT_SECRET` 须一致（bot 用它代用户换 JWT）

## 2. 启动

```bash
TG_BOT_TOKEN=<token> \
TG_BOT_SECRET=dev-tg-bot-secret \
ACCOUNT_URL=http://127.0.0.1:8084 \
FOLLOW_URL=http://127.0.0.1:8082 \
VENUE_HUB_URL=http://127.0.0.1:8081 \
./target/debug/sharpside-tg-bot
```

`TG_BOT_TOKEN` 留空则启动时优雅退出（便于无网络环境不崩）。

## 3. 命令

| 命令 | 说明 |
|---|---|
| `/start` | 绑定账户（account `POST /auth/tg` 按 tg_id upsert 用户 + 换 JWT，缓存） |
| `/help` | 列出命令 |
| `/follow <platform> <address> [amount]` | 建跟随（channel=tg，execute_venue=polymarket，固定金额；amount 省略用默认/`/setamount`） |
| `/follows` | 列出我的跟随 |
| `/unfollow <id>` | 取消跟随 |
| `/traders` | 列出热门交易者（venue-hub `/traders`） |
| `/perf <platform> <address>` | 查绩效（venue-hub `/traders/{p}/{a}/performance`，全周期 + 标签） |
| `/setamount <amount>` | 设置默认下单金额（USDC） |

## 4. 通道 A 全流程（bot 触发）

```
TG /start → account /auth/tg (X-TG-Bot-Secret) → JWT（缓存）
TG /follow → follow /follows (Bearer JWT, channel=tg) → follow_relation
venue-hub 检出仓位变化 → follow /internal/signals → copy_order(pending, tg)
copier tg worker → 风控 + Venue::place_order（或 dry_run 合成）→ copy_execution
（后续：copier 推送成交通知给 bot → bot 转发用户，待落地）
```

## 5. 后端契约（新增端点）

- account `POST /auth/tg`：body `{tg_id}`，头 `X-TG-Bot-Secret`，返回 `{token, user}`。
- venue-hub `GET /traders/{platform}/{address}/performance`：返回 `{performance:[...], tags:[...]}`。

## 6. 离线/受限网络验证

`api.telegram.org` 不可达时无法活跑长轮询，但可用 curl 等价验证 bot 的后端契约：

```bash
# /start 等价
curl -X POST $ACCT/auth/tg -H 'X-TG-Bot-Secret: dev-tg-bot-secret' \
  -H 'Content-Type: application/json' -d '{"tg_id":99999}'
# /follow 等价（用上一步 token）
curl -X POST $FOLLOW/follows -H "Authorization: Bearer <jwt>" ...
# /perf 等价
curl $VH/traders/polymarket/<addr>/performance
```

详见 README「通道 A TG bot 真实现」小节的验证结果。
