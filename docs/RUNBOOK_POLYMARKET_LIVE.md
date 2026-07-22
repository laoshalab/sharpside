# Runbook · 接入真实 Polymarket 数据联调

> 对应 `docs/ARCHITECTURE.md` §6.1 / `docs/DATA_SOURCES.md`。
> venue-hub 通过 `crates/venues/polymarket` 的 `PolymarketClient` 调 Polymarket 公开 API（读免鉴权）：
> Data API（leaderboard / positions / trades）、Gamma API（markets）、CLOB API（book）。

## 1. 前置

- PostgreSQL 已起：`docker compose -f infra/docker-compose.yml up -d postgres`
- 宿主机可直连 `data-api.polymarket.com` / `gamma-api.polymarket.com` / `clob.polymarket.com`（443）。
  - 受限网络下若直连超时，可走自托管代理/镜像，或用本地 mock 跑通代码路径（见 §4）。
- 已构建二进制：`cargo build --bins --offline`

## 2. 真实 API 联调

```bash
# 1) 起 PG
docker compose -f infra/docker-compose.yml up -d postgres

# 2) 起 venue-hub（默认即指向真实 Polymarket API；不设 POLYMARKET_*_URL 即用默认线上地址）
DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
RUST_LOG=info \
VENUE_HUB_LISTEN_ADDR=127.0.0.1:8081 \
WORKER_INGEST_SECS=30 WORKER_PERF_SECS=60 \
./target/debug/sharpside-venue-hub
```

ingest worker 每 `WORKER_INGEST_SECS` 秒：
- 拉 `/leaderboard`（PNL top 100，all）→ upsert `trader_hub.traders`（source=leaderboard）
- 拉 `/markets`（active）→ upsert `trader_hub.raw_markets`

验证：

```bash
curl -s 'http://127.0.0.1:8081/traders?platform=polymarket&limit=5' | python3 -m json.tool
docker exec sharpside-pg psql -U sharpside -d sharpside -c \
  "SELECT address, source, user_name, x_username FROM trader_hub.traders LIMIT 5;"
docker exec sharpside-pg psql -U sharpside -d sharpside -c \
  "SELECT venue_market_id, title FROM trader_hub.raw_markets LIMIT 5;"
```

## 3. 导入真实地址 + 绩效物化

```bash
# 导入某地址：upsert trader(source=imported) + 回填 raw_trades
curl -s -X POST http://127.0.0.1:8081/traders/import \
  -H 'Content-Type: application/json' \
  -d '{"platform":"polymarket","address":"<0x...>","alias":"<可选>","x_username":"<可选>"}'

# 等 perf worker（WORKER_PERF_SECS）一轮后：
docker exec sharpside-pg psql -U sharpside -d sharpside -c \
  "SELECT period, roi, win_rate, realized_pnl, position_count FROM trader_hub.trader_performance \
   WHERE address='<0x...>' ORDER BY period;"
docker exec sharpside-pg psql -U sharpside -d sharpside -c \
  "SELECT tags FROM trader_hub.trader_tag WHERE address='<0x...>';"
```

perf worker 读 `raw_trades` → `sharpside_perf::reconstruct_position_timeline` →
`compute_equity_curve` / `compute_performance` / `compute_tags` →
覆盖写 `position_timeline` / `trader_equity_curve` / `trader_performance` / `trader_tag`。

> 限流：Polymarket 公开 API 无公开配额承诺，`WORKER_INGEST_SECS` 不宜过小（生产建议 ≥300s）。
> adapter 内部不在客户端限流，由 worker 间隔控制。

## 4. 受限网络 / 离线联调（本地 Mock）

无法直连 Polymarket 时，用本地 mock 跑通**完全相同的代码路径**（仅数据源换成本地 fixture）：

```bash
# 1) 起 mock（真实 DTO 形状的 fixture：5 traders / 3 markets / trades / positions / book）
python3 infra/mock/polymarket_mock.py 9200 &

# 2) 起 venue-hub 指向 mock
DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
RUST_LOG=info \
VENUE_HUB_LISTEN_ADDR=127.0.0.1:8081 \
POLYMARKET_DATA_API_URL=http://127.0.0.1:9200 \
POLYMARKET_GAMMA_API_URL=http://127.0.0.1:9200 \
POLYMARKET_CLOB_API_URL=http://127.0.0.1:9200 \
WORKER_INGEST_SECS=3 WORKER_PERF_SECS=6 \
./target/debug/sharpside-venue-hub
```

三个 URL 覆盖均由 `services/venue-hub/src/config.rs` 读取、`registry.rs` 注入
`PolymarketClient::with_urls(...)`。**有网络环境后留空这三个变量，同一二进制即直连真实 API，零代码改动。**

## 5. 联调中发现并修复的 bug

- **`upsert_trader_performance` NaN/inf 静默丢行**：无亏损时 `profit_factor=inf`、无方差时
  `sharpe=NaN`，`Decimal::try_from` 报错被 `let _ =` 吞掉，整行 performance 不落库。
  已在 `crates/db/src/queries/perf.rs` 的 `to_dec` 把 NaN/±inf 归零（position_timeline /
  equity_curve 同处理）。
