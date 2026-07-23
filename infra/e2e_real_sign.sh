#!/usr/bin/env bash
# 阶段1 · 半真钱：验证 copier 真钱执行路径的「KMS 解密 → ERC-7739 POLY_1271 签名 → 落库」链路。
#
# 与 infra/e2e.sh 的关键区别：e2e.sh 用 COPIER_DRY_RUN=true，copier 在 place_order 之前短路，
# 根本不调 KMS 解密、不签 POLY_1271。本脚本用 COPIER_DRY_RUN=false（调 place_order）+
# 不设 POLYMARKET_CLOB_POST（place_order dry-sign 不提交），全程不花一分钱，但完整跑过
# 真钱路径的解密+签名+记录链路。
#
# 前置：
#   - docker daemon 已起（用于 PG）
#   - 代理已起（POLYMARKET_HTTP_PROXY，默认 http://127.0.0.1:7890）—— copier 的 book()
#     滑点保护（P0-1）要求盘口拉取成功，须能达 clob.polymarket.com
#   - 二进制已构建：cargo build --bins --offline
#
# 用法：bash infra/e2e_real_sign.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside'
ACCT=http://127.0.0.1:8084
FOLLOW=http://127.0.0.1:8082
COPIER=http://127.0.0.1:8083
PROXY="${POLYMARKET_HTTP_PROXY:-http://127.0.0.1:7890}"
# 真实 builder code（.env.local），让 dry-sign 产出真实归因签名
BUILDER_CODE="${POLYMARKET_BUILDER_CODE:-019f6e85-dce2-7a7a-aa72-cadb8d498bbe}"

PIDS=()
cleanup() {
  echo "--- 清理服务进程 ---"
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
}
trap cleanup EXIT

pass=0; fail=0
ok()   { echo "  ✅ $1"; pass=$((pass+1)); }
bad()  { echo "  ❌ $1"; fail=$((fail+1)); }
jq_get() { python3 -c 'import sys,json;print(json.load(sys.stdin)'"$1"')'; }

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

echo "=== 4. 启动服务（DevKms 明文透传 + copier 非dry_run dry-sign）==="
# account & copier 共用 DevKms（SHARPSIDE_KMS_DEV_PLAINTEXT=1）：account 加密 owner_key/l2_secret，
# copier 解密。copier COPIER_DRY_RUN=false → 调 place_order；不设 POLYMARKET_CLOB_POST → dry-sign 不提交。
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info ACCOUNT_LISTEN_ADDR=127.0.0.1:8084 \
  SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
  "$ROOT/target/debug/sharpside-account" >/tmp/realsign_account.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info FOLLOW_LISTEN_ADDR=127.0.0.1:8082 \
  INTERNAL_SIGNAL_SECRET=e2e-internal-secret \
  "$ROOT/target/debug/sharpside-follow" >/tmp/realsign_follow.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info COPIER_LISTEN_ADDR=127.0.0.1:8083 \
  COPIER_DRY_RUN=false WORKER_EXEC_SECS=2 \
  SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
  SHARPSIDE_ALLOW_DEVKMS_E2E=1 \
  POLYMARKET_HTTP_PROXY="$PROXY" \
  "$ROOT/target/debug/sharpside-copier" >/tmp/realsign_copier.log 2>&1 & PIDS+=($!)

wait_ready() { for i in $(seq 1 30); do curl -fs "$1/readyz" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
for ep in "$ACCT" "$FOLLOW" "$COPIER"; do
  wait_ready "$ep" && ok "$ep ready" || bad "$ep 未就绪"
done

echo "=== 5. TG 登录建用户 + 创建跟随(tg) ==="
# 邮箱注册已移除：用 TG 登录建用户 + JWT。
TG_ADDR="0xrealsign$(date +%s)"
TG_ID=$(( (RANDOM << 15) + RANDOM + $(date +%s) % 100000 ))
REG=$(curl -fs -X POST "$ACCT/auth/tg" -H "X-TG-Bot-Secret: dev-tg-bot-secret" -H 'Content-Type: application/json' \
  -d "{\"tg_id\":$TG_ID}")
JWT=$(echo "$REG" | jq_get '["token"]')
USER_ID=$(echo "$REG" | jq_get '["user"]["id"]')
[ -n "$JWT" ] && ok "TG 登录建用户 $USER_ID" || { bad "TG 登录失败"; echo "$REG" >&2; exit 1; }

curl -fs -X POST "$FOLLOW/follows" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"follow_platform\":\"polymarket\",\"follow_address\":\"$TG_ADDR\",\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":5}},\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"same_venue_only\":false}}" >/dev/null && ok "创建跟随(tg, fixed amount=5)"

echo "=== 6. 预配 deposit wallet（离线，DevKms 加密）==="
PROV=$(curl -fs -X POST "$ACCT/me/deposit-wallet/provision" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"builder_code\":\"$BUILDER_CODE\"}")
DW=$(echo "$PROV" | jq_get '["deposit_wallet_address"]')
OA=$(echo "$PROV" | jq_get '["owner_address"]')
LIVE=$(echo "$PROV" | jq_get '["live"]')
[ -n "$DW" ] && ok "预配完成 owner=$OA dw=$DW live=$LIVE（离线，DevKms 加密落库）" || { bad "预配失败"; echo "$PROV" >&2; exit 1; }

echo "=== 7. Gamma 取真实活跃 token_id（供 copier book() 滑点校验）==="
MKT=$(curl -fs --max-time 10 --proxy "$PROXY" \
  "https://gamma-api.polymarket.com/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false")
TOKEN_ID=$(echo "$MKT" | python3 -c '
import sys,json
ms=json.load(sys.stdin)
for m in ms:
    ids=m.get("clobTokenIds")
    if isinstance(ids,str) and ids not in ("","[]"):
        arr=json.loads(ids)
        if arr: print(arr[0]); break
')
COND_ID=$(echo "$MKT" | python3 -c "
import sys,json
ms=json.load(sys.stdin)
for m in ms:
    ids=m.get('clobTokenIds')
    if isinstance(ids,str) and ids not in ('','[]'):
        arr=json.loads(ids)
        if arr:
            print(m.get('conditionId','')); break
")
[ -n "$TOKEN_ID" ] && ok "真实 token_id=$TOKEN_ID condition=$COND_ID" || { bad "Gamma 取 token_id 失败（代理/网络）"; exit 1; }

echo "=== 8. 注入信号 → 派生 pending copy_order ==="
SIG=$(curl -fs -X POST "$FOLLOW/internal/signals" -H 'Content-Type: application/json' \
  -H 'X-Internal-Secret: e2e-internal-secret' \
  -d "{\"platform\":\"polymarket\",\"trader_id\":\"$TG_ADDR\",\"token_id\":\"$TOKEN_ID\",\"market_id\":\"$COND_ID\",\"side\":\"buy\",\"price\":0.5,\"size\":100,\"ts\":\"2026-07-22T09:45:00Z\"}")
echo "$SIG" | grep -qE '"enqueued":[1-9]' && ok "信号派生 enqueued≥1" || { bad "信号派生失败 ($SIG)"; exit 1; }

echo "=== 9. 等 copier 处理 → 校验 filled + 317 字节 POLY_1271 签名落库 ==="
ST=""
for i in $(seq 1 20); do
  ST=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
    "SELECT status FROM account.copy_order WHERE channel='tg' AND user_id='$USER_ID' ORDER BY enqueued_at DESC LIMIT 1;")
  [ "$ST" = "filled" ] && break
  sleep 1
done
if [ "$ST" = "filled" ]; then
  ok "copy_order=filled"
else
  bad "copy_order 未 filled（status=$ST）—— 查 /tmp/realsign_copier.log（可能滑点/盘口/解密失败）"
  tail -20 /tmp/realsign_copier.log >&2
  exit 1
fi

# tx_hash 应为 0x + 634 hex = 317 字节 ERC-7739-wrapped POLY_1271 签名（对齐官方 clob-client-v2）
TXH=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
  "SELECT ce.tx_hash FROM account.copy_execution ce JOIN account.copy_order co ON co.id=ce.copy_order_id WHERE co.channel='tg' AND co.user_id='$USER_ID' ORDER BY ce.id DESC LIMIT 1;")
LEN=${#TXH}
if [ "$LEN" -eq 636 ] && [[ "$TXH" == 0x* ]]; then
  ok "copy_execution.tx_hash = 317 字节 POLY_1271 签名（len=$LEN）✓ 解密+签名链路验证通过"
else
  bad "tx_hash 异常（len=$LEN，期望 636=0x+634hex）: ${TXH:0:40}..."
  exit 1
fi

# dispatched 占位锁不应残留（成功路径 pending→dispatched→filled）
STUCK=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
  "SELECT count(*) FROM account.copy_order WHERE channel='tg' AND user_id='$USER_ID' AND status='dispatched';")
[ "$STUCK" = "0" ] && ok "无残留 dispatched（P0-2 占位锁状态机正确）" || bad "残留 $STUCK 条 dispatched"

echo "=========================================="
echo " 阶段1 结果：PASS=$pass  FAIL=$fail"
[ "$fail" -eq 0 ] && echo " ✅ 真钱执行路径（解密→签名→落库）验证通过，未花一分钱" || echo " ❌ 见 /tmp/realsign_*.log"
echo "=========================================="
exit $fail
