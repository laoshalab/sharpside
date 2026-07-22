#!/usr/bin/env bash
# 真钱 B：验证 /me/portfolio 的 wallet.cash_balance 对 live CLOB 真实返回（只读余额，零资金风险）。
#
# 与 e2e_real_trade.sh 的区别：不起 follow、不下单、不撤单——只注入 funded 凭证（provision_live=true）
# 后 HTTP GET copier /me/portfolio，断言 wallet.cash_balance 为正数。
#
# 前置：docker daemon 已起（PG）、代理已起（POLYMARKET_HTTP_PROXY）、.env.local 有 funded 凭证。
# 用法：bash infra/e2e_real_portfolio.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside'
COPIER=http://127.0.0.1:8083
PROXY="${POLYMARKET_HTTP_PROXY:-http://127.0.0.1:7890}"

if [ -f "$ROOT/.env.local" ]; then set -a; . "$ROOT/.env.local"; set +a; fi

PIDS=()
cleanup() {
  echo "--- 清理服务进程 ---"
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
}
trap cleanup EXIT

echo "=== 1. docker compose 起 PG ==="
docker compose -f infra/docker-compose.yml up -d postgres >/dev/null 2>&1
for i in $(seq 1 30); do
  docker exec sharpside-pg pg_isready -U sharpside -d sharpside >/dev/null 2>&1 && break
  sleep 1
done
docker exec sharpside-pg pg_isready -U sharpside -d sharpside >/dev/null 2>&1 && echo "  ✅ PG ready" || { echo "  ❌ PG 未就绪"; exit 1; }

echo "=== 2. 代理探测 ==="
curl -fs --max-time 6 --proxy "$PROXY" https://clob.polymarket.com/time >/dev/null 2>&1 && echo "  ✅ 代理可达 Polymarket" || { echo "  ❌ 代理 $PROXY 不可达"; exit 1; }

echo "=== 3. 构建二进制（offline）==="
CARGO_TARGET_DIR="$ROOT/target" cargo build --bins --offline 2>&1 | tail -1
[ -x "$ROOT/target/debug/sharpside-copier" ] && echo "  ✅ 二进制就绪" || { echo "  ❌ 构建失败"; exit 1; }

echo "=== 4. 启动 copier（DevKms 明文 + 代理 + JWT_SECRET 默认）==="
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info COPIER_LISTEN_ADDR=127.0.0.1:8083 \
  COPIER_DRY_RUN=true WORKER_EXEC_SECS=30 \
  SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
  POLYMARKET_HTTP_PROXY="$PROXY" \
  "$ROOT/target/debug/sharpside-copier" >/tmp/realportfolio_copier.log 2>&1 & PIDS+=($!)

wait_ready() { for _ in $(seq 1 30); do curl -fs "$1/readyz" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
wait_ready "$COPIER" && echo "  ✅ copier ready" || { echo "  ❌ copier 未就绪"; exit 1; }

echo "=== 5. 跑 portfolio 余额测试（注入凭证→/me/portfolio→断言 cash_balance>0）==="
DATABASE_URL="$DB_URL" \
SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
POLYMARKET_HTTP_PROXY="$PROXY" \
COPIER_URL="$COPIER" \
  cargo test --offline -p sharpside-copier real_portfolio_balance_e2e -- --ignored --nocapture 2>&1 | \
  tee /tmp/realportfolio_test.log | rg "B\.step|REAL_PORTFOLIO_BALANCE|panicked|assertion|error\[|cash_balance"

TEST_RC=${PIPESTATUS[0]}
echo "=========================================="
if [ "$TEST_RC" -eq 0 ]; then
  echo " ✅ portfolio 真实余额验证通过（/me/portfolio.wallet.cash_balance 实时返回 pUSD）"
  echo "    详见 /tmp/realportfolio_test.log；copier 日志 /tmp/realportfolio_copier.log"
else
  echo " ❌ portfolio 余额验证失败（rc=$TEST_RC）—— 查 /tmp/realportfolio_test.log + /tmp/realportfolio_copier.log"
  echo "    --- copier 日志尾部 ---"
  tail -30 /tmp/realportfolio_copier.log
fi
echo "=========================================="
exit $TEST_RC
