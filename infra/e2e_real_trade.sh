#!/usr/bin/env bash
# 真实跟单 e2e：信号 → follow /internal/signals → 派生 copy_order → copier worker 真打 Polymarket /order → 撤单。
#
# 与 e2e_real_sign.sh 的区别：本脚本设 POLYMARKET_CLOB_POST=1（真提交订单到 Polymarket），
# 且走真实 HTTP 信号入口（follow /internal/signals），由运行中的 copier worker 拾取 pending 单下单。
# 用 .env.local 的 funded deposit wallet（已部署 + approved + 充 pUSD），下单后立即撤单（撤回锁定 USDC）。
#
# 前置：
#   - docker daemon 已起（用于 PG）
#   - 代理已起（POLYMARKET_HTTP_PROXY，默认 http://127.0.0.1:7890）
#   - .env.local 有 funded owner PK / DW / builder code
#   - 二进制已构建：cargo build --bins --offline
#
# 用法：bash infra/e2e_real_trade.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside'
ACCT=http://127.0.0.1:8084
FOLLOW=http://127.0.0.1:8082
COPIER=http://127.0.0.1:8083
PROXY="${POLYMARKET_HTTP_PROXY:-http://127.0.0.1:7890}"

# 载入 .env.local（funded owner PK / DW / builder code）
if [ -f "$ROOT/.env.local" ]; then set -a; . "$ROOT/.env.local"; set +a; fi

PIDS=()
cleanup() {
  echo "--- 清理服务进程 ---"
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
}
trap cleanup EXIT

pass=0; fail=0
ok()   { echo "  ✅ $1"; pass=$((pass+1)); }
bad()  { echo "  ❌ $1"; fail=$((fail+1)); }

echo "=== 1. docker compose 起 PG ==="
docker compose -f infra/docker-compose.yml up -d postgres >/dev/null 2>&1
for i in $(seq 1 30); do
  docker exec sharpside-pg pg_isready -U sharpside -d sharpside >/dev/null 2>&1 && break
  sleep 1
done
docker exec sharpside-pg pg_isready -U sharpside -d sharpside >/dev/null 2>&1 && ok "PG ready" || { bad "PG 未就绪"; exit 1; }

echo "=== 2. 代理探测 ==="
curl -fs --max-time 6 --proxy "$PROXY" https://clob.polymarket.com/time >/dev/null 2>&1 && ok "代理可达 Polymarket" || { bad "代理 $PROXY 不可达 Polymarket（先起 clash/代理）"; exit 1; }

echo "=== 3. 构建二进制（offline）==="
CARGO_TARGET_DIR="$ROOT/target" cargo build --bins --offline 2>&1 | tail -1
[ -x "$ROOT/target/debug/sharpside-copier" ] && ok "二进制就绪" || { bad "构建失败"; exit 1; }

echo "=== 4. 启动服务（copier 真打：COPIER_DRY_RUN=false + POLYMARKET_CLOB_POST=1）==="
# copier: DRY_RUN=false（调 place_order）+ CLOB_POST=1（真提交）
#   + WORKER_EXEC_SECS=2（快速轮询 pending 单）
#   余额风控用默认 RISK_MIN_DW_BALANCE=5（不覆盖），验证 funded DW 余额是否 ≥5U
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info ACCOUNT_LISTEN_ADDR=127.0.0.1:8084 \
  SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
  "$ROOT/target/debug/sharpside-account" >/tmp/realtrade_account.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info FOLLOW_LISTEN_ADDR=127.0.0.1:8082 \
  INTERNAL_SIGNAL_SECRET=e2e-internal-secret \
  "$ROOT/target/debug/sharpside-follow" >/tmp/realtrade_follow.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info COPIER_LISTEN_ADDR=127.0.0.1:8083 \
  COPIER_DRY_RUN=false WORKER_EXEC_SECS=2 \
  SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
  SHARPSIDE_ALLOW_DEVKMS_E2E=1 \
  POLYMARKET_CLOB_POST=1 \
  POLYMARKET_HTTP_PROXY="$PROXY" \
  "$ROOT/target/debug/sharpside-copier" >/tmp/realtrade_copier.log 2>&1 & PIDS+=($!)

wait_ready() { for _ in $(seq 1 30); do curl -fs "$1/readyz" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
for ep in "$ACCT" "$FOLLOW" "$COPIER"; do
  wait_ready "$ep" && ok "$ep ready" || bad "$ep 未就绪"
done

echo "=== 5. 跑真实跟单 e2e 测试（信号→派生→copier→真打→撤单）==="
DATABASE_URL="$DB_URL" \
SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
POLYMARKET_CLOB_POST=1 \
POLYMARKET_HTTP_PROXY="$PROXY" \
FOLLOW_URL="$FOLLOW" \
INTERNAL_SIGNAL_SECRET=e2e-internal-secret \
  cargo test --offline -p sharpside-copier real_copy_trade_e2e -- --ignored --nocapture 2>&1 | \
  tee /tmp/realtrade_test.log | rg "step|REAL_COPY_TRADE|panicked|assertion|error\[" 

TEST_RC=${PIPESTATUS[0]}
echo "=========================================="
if [ "$TEST_RC" -eq 0 ]; then
  echo " ✅ 真实跟单完成（信号→follow→copier→Polymarket 真打→撤单）"
  echo "    详见 /tmp/realtrade_test.log；服务日志 /tmp/realtrade_{account,follow,copier}.log"
else
  echo " ❌ 真实跟单失败（rc=$TEST_RC）—— 查 /tmp/realtrade_test.log + /tmp/realtrade_copier.log"
  echo "    --- copier 日志尾部 ---"
  tail -30 /tmp/realtrade_copier.log
fi
echo "=========================================="
exit $TEST_RC
