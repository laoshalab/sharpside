#!/usr/bin/env bash
# Sharpside 端到端集成验证（离线 dry_run 闭环）。
#
# 前置：docker 可用、postgres:16-alpine 镜像可达（首次需联网拉取）。
# 用法：bash infra/e2e.sh
#
# 覆盖：
#   1. docker compose 起 PG
#   2. 构建全部二进制（offline）
#   3. 起 account / follow / copier(dry_run) / admin
#   4. 通道 A：注册→跟随(tg)→信号→copier 合成成交→校验 copy_execution
#   5. 通道 B：颁发 daemon_api_key→跟随(daemon)→信号→daemon 拉取回传→校验 copy_execution
#   6. admin：tag-rule / audit-threshold / visibility 端点冒烟
#   7. 钱包登录：随机 EOA→SIWE 签名→/auth/wallet→/me→/me/wallets→nonce 重放防护
#   8. 汇总 PASS/FAIL，保留 PG 与日志便于排查
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside'
ACCT=http://127.0.0.1:8084
FOLLOW=http://127.0.0.1:8082
COPIER=http://127.0.0.1:8083
ADMIN=http://127.0.0.1:8086/api
ADMIN_TOKEN=dev-admin-token

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

echo "=== 2. 构建二进制（offline）==="
CARGO_TARGET_DIR="$ROOT/target" cargo build --bins --offline 2>&1 | tail -1
[ -x "$ROOT/target/debug/sharpside-account" ] && ok "二进制就绪" || { bad "构建失败"; exit 1; }

echo "=== 3. 启动服务 ==="
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info ACCOUNT_LISTEN_ADDR=127.0.0.1:8084 \
  "$ROOT/target/debug/sharpside-account" >/tmp/e2e_account.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info FOLLOW_LISTEN_ADDR=127.0.0.1:8082 \
  INTERNAL_SIGNAL_SECRET=e2e-internal-secret \
  "$ROOT/target/debug/sharpside-follow" >/tmp/e2e_follow.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info COPIER_LISTEN_ADDR=127.0.0.1:8083 \
  COPIER_DRY_RUN=true WORKER_EXEC_SECS=2 \
  "$ROOT/target/debug/sharpside-copier" >/tmp/e2e_copier.log 2>&1 & PIDS+=($!)
DATABASE_URL="$DB_URL" RUST_LOG=warn,sharpside=info ADMIN_LISTEN_ADDR=127.0.0.1:8086 \
  ADMIN_TOKEN="$ADMIN_TOKEN" \
  "$ROOT/target/debug/sharpside-admin" >/tmp/e2e_admin.log 2>&1 & PIDS+=($!)

wait_ready() { for i in $(seq 1 30); do curl -fs "$1/readyz" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
for ep in "$ACCT" "$FOLLOW" "$COPIER" "$ADMIN"; do
  wait_ready "$ep" && ok "$ep ready" || bad "$ep 未就绪"
done

echo "=== 4. 通道 A 闭环 ==="
# 邮箱注册已移除：用 TG 登录建用户 + JWT（account 默认 TG_BOT_SECRET=dev-tg-bot-secret）。
TG_ADDR="0xe2e-tg-$(date +%s)"
DAEMON_ADDR="0xe2e-dm-$(date +%s)"
TG_ID=$(( (RANDOM << 15) + RANDOM + $(date +%s) % 100000 ))
REG=$(curl -fs -X POST "$ACCT/auth/tg" -H "X-TG-Bot-Secret: dev-tg-bot-secret" -H 'Content-Type: application/json' \
  -d "{\"tg_id\":$TG_ID}")
JWT=$(echo "$REG" | jq_get '["token"]')
USER_ID=$(echo "$REG" | jq_get '["user"]["id"]')
[ -n "$JWT" ] && ok "TG 登录建用户 $USER_ID" || { bad "TG 登录失败"; echo "$REG" >&2; exit 1; }

curl -fs -X POST "$FOLLOW/follows" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"follow_platform\":\"polymarket\",\"follow_address\":\"$TG_ADDR\",\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":50}},\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"same_venue_only\":false}}" >/dev/null && ok "创建跟随(tg)"

SIG=$(curl -fs -X POST "$FOLLOW/internal/signals" -H 'Content-Type: application/json' \
  -H 'X-Internal-Secret: e2e-internal-secret' \
  -d "{\"platform\":\"polymarket\",\"trader_id\":\"$TG_ADDR\",\"token_id\":\"tok-yes\",\"market_id\":\"cond-123\",\"side\":\"buy\",\"price\":0.5,\"size\":100,\"ts\":\"2026-07-21T09:45:00Z\"}")
echo "$SIG" | grep -qE '"enqueued":[1-9]' && ok "信号派生 enqueued≥1" || bad "信号派生失败 ($SIG)"

for i in $(seq 1 15); do
  ST=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
    "SELECT status FROM account.copy_order WHERE channel='tg' AND user_id='$USER_ID' ORDER BY enqueued_at DESC LIMIT 1;")
  [ "$ST" = "filled" ] && break
  sleep 1
done
[ "$ST" = "filled" ] && ok "通道A copy_order=filled" || bad "通道A 未成交($ST)"
EXA=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
  "SELECT count(*) FROM account.copy_execution ce JOIN account.copy_order co ON co.id=ce.copy_order_id WHERE co.channel='tg' AND co.user_id='$USER_ID';")
[ "$EXA" -ge 1 ] && ok "通道A copy_execution 写入($EXA)" || bad "通道A 无 execution"

echo "=== 5. 通道 B 闭环 ==="
DAEMON_KEY=$(curl -fs -X POST "$ACCT/me/daemon-api-key" -H "Authorization: Bearer $JWT" | jq_get '["daemon_api_key"]')
[ -n "$DAEMON_KEY" ] && ok "颁发 daemon_api_key" || bad "daemon_api_key 失败"

curl -fs -X POST "$FOLLOW/follows" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"follow_platform\":\"polymarket\",\"follow_address\":\"$DAEMON_ADDR\",\"execute_venue\":\"polymarket\",\"channel\":\"daemon\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":50}},\"execute_venue\":\"polymarket\",\"channel\":\"daemon\",\"same_venue_only\":false}}" >/dev/null && ok "创建跟随(daemon)"

SIGB=$(curl -fs -X POST "$FOLLOW/internal/signals" -H 'Content-Type: application/json' \
  -H 'X-Internal-Secret: e2e-internal-secret' \
  -d "{\"platform\":\"polymarket\",\"trader_id\":\"$DAEMON_ADDR\",\"token_id\":\"tok-yes\",\"market_id\":\"cond-456\",\"side\":\"buy\",\"price\":0.4,\"size\":80,\"ts\":\"2026-07-21T09:46:00Z\"}")
echo "$SIGB" | grep -qE '"enqueued":[1-9]' && ok "信号派生 enqueued≥1" || bad "信号派生失败 ($SIGB)"

RUST_LOG=info COPIER_URL="$COPIER" DAEMON_USER_ID="$USER_ID" DAEMON_API_KEY="$DAEMON_KEY" \
  DAEMON_POLL_SECS=1 DAEMON_DRY_RUN=true \
  timeout 8 "$ROOT/target/debug/sharpside-daemon" >/tmp/e2e_daemon.log 2>&1
grep -q "dry-run 合成成交回传" /tmp/e2e_daemon.log && ok "daemon 回传成交" || bad "daemon 未回传"

STB=""
for i in $(seq 1 10); do
  STB=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
    "SELECT status FROM account.copy_order WHERE channel='daemon' AND user_id='$USER_ID' ORDER BY enqueued_at DESC LIMIT 1;")
  [ "$STB" = "filled" ] && break
  sleep 0.5
done
[ "$STB" = "filled" ] && ok "通道B copy_order=filled" || bad "通道B 未成交($STB)"
EXB=$(docker exec sharpside-pg psql -U sharpside -d sharpside -tAc \
  "SELECT count(*) FROM account.copy_execution ce JOIN account.copy_order co ON co.id=ce.copy_order_id WHERE co.channel='daemon' AND co.user_id='$USER_ID';")
[ "$EXB" -ge 1 ] && ok "通道B copy_execution 写入($EXB)" || bad "通道B 无 execution"

echo "=== 5.5 Watchlist 闭环 ==="
WL_ADDR="0xe2e-wl-$(date +%s)"
# 创建收藏（trader）
WL=$(curl -fs -X POST "$FOLLOW/watchlists" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"watch_platform\":\"polymarket\",\"watch_address\":\"$WL_ADDR\"}")
WL_ID=$(echo "$WL" | jq_get '["id"]')
[ -n "$WL_ID" ] && ok "创建 watchlist($WL_ID)" || { bad "创建 watchlist 失败"; echo "$WL" >&2; }
# 重复收藏 → 409
DC=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$FOLLOW/watchlists" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"watch_platform\":\"polymarket\",\"watch_address\":\"$WL_ADDR\"}")
[ "$DC" = "409" ] && ok "重复收藏 → 409" || bad "重复收藏应 409，实际 $DC"
# 列出 → 含本条
WLL=$(curl -fs "$FOLLOW/me/watchlists" -H "Authorization: Bearer $JWT")
echo "$WLL" | grep -q "$WL_ADDR" && ok "GET /me/watchlists 含本条" || bad "watchlist 列表缺失"
# 单条 GET
WLG=$(curl -fs "$FOLLOW/me/watchlists/$WL_ID" -H "Authorization: Bearer $JWT")
echo "$WLG" | grep -q "$WL_ID" && ok "GET /me/watchlists/:id" || bad "watchlist 单条缺失"
# 升级为 Follow（trader 无 trader_tag → botfilter 放行）
UP=$(curl -fs -X POST "$FOLLOW/watchlists/$WL_ID/upgrade" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":50}},\"execute_venue\":\"polymarket\",\"channel\":\"tg\",\"same_venue_only\":false}}")
echo "$UP" | grep -q '"watchlist_id"' && echo "$UP" | grep -q '"follow_platform"' && ok "升级为 Follow 成功" || { bad "升级失败"; echo "$UP" >&2; }
# 升级后 watchlist 应已被消费（GET → 404）
WLAFTER=$(curl -s -o /dev/null -w '%{http_code}' "$FOLLOW/me/watchlists/$WL_ID" -H "Authorization: Bearer $JWT")
[ "$WLAFTER" = "404" ] && ok "升级后 watchlist 已删除(404)" || bad "升级后 watchlist 应 404，实际 $WLAFTER"

echo "=== 5.6 管辖域校验（jurisdiction）==="
# e2e 用户默认 jurisdiction=other，Kalshi 仅 US 可用 → create_follow 应被拒（400）。
# 早拒绝：避免创建出"每个信号都被 copier 跳过"的静默失效跟随。
JF=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$FOLLOW/follows" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"follow_platform\":\"polymarket\",\"follow_address\":\"0xjuris-test\",\"execute_venue\":\"kalshi\",\"channel\":\"tg\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":50}},\"execute_venue\":\"kalshi\",\"channel\":\"tg\",\"same_venue_only\":false}}")
[ "$JF" = "400" ] && ok "create_follow kalshi(other) → 400 拒绝" || bad "create_follow kalshi 应 400，实际 $JF"
# watchlist 升级到 kalshi 也应被拒，且 watchlist 保留（不被消费）
WLJ_ADDR="0xe2e-wlj-$(date +%s)"
WLJ=$(curl -fs -X POST "$FOLLOW/watchlists" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"watch_platform\":\"polymarket\",\"watch_address\":\"$WLJ_ADDR\"}")
WLJ_ID=$(echo "$WLJ" | jq_get '["id"]')
UJ=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$FOLLOW/watchlists/$WLJ_ID/upgrade" -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d "{\"execute_venue\":\"kalshi\",\"channel\":\"tg\",\"config\":{\"sizing\":{\"mode\":\"fixed\",\"value\":{\"amount\":50}},\"execute_venue\":\"kalshi\",\"channel\":\"tg\",\"same_venue_only\":false}}")
[ "$UJ" = "400" ] && ok "upgrade kalshi(other) → 400 拒绝" || bad "upgrade kalshi 应 400，实际 $UJ"
# watchlist 应仍存在（门控失败不消费）
WLJ_AFTER=$(curl -s -o /dev/null -w '%{http_code}' "$FOLLOW/me/watchlists/$WLJ_ID" -H "Authorization: Bearer $JWT")
[ "$WLJ_AFTER" = "200" ] && ok "升级被拒后 watchlist 保留(200)" || bad "升级被拒后 watchlist 应 200，实际 $WLJ_AFTER"

echo "=== 6. admin 冒烟 ==="
curl -fs -X PUT -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  "$ADMIN/tag-rules/win_rate" -d '{"params":{"metric":"win_rate","op":"gte","threshold":0.6,"tag":"DW:win"},"enabled":true,"updated_by":"e2e"}' >/dev/null && ok "PUT tag-rule"
curl -fs -X PUT -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  "$ADMIN/audit-thresholds/roi" -d '{"warn_abs":0.02,"warn_pct":5,"alert_abs":0.05,"alert_pct":15}' >/dev/null && ok "PUT audit-threshold"
TR=$(curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/tag-rules")
echo "$TR" | grep -q win_rate && ok "GET tag-rules" || bad "GET tag-rules 失败"
AT=$(curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/audit-thresholds")
echo "$AT" | grep -q roi && ok "GET audit-thresholds" || bad "GET audit-thresholds 失败"
curl -fs -X PUT -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  "$ADMIN/category-mappings" -d '{"platform":"polymarket","official_category":"E2E_TEST","site_category":"E2E","display_name":"e2e"}' >/dev/null && ok "PUT category-mapping"
CM=$(curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/category-mappings?platform=polymarket")
echo "$CM" | grep -q E2E_TEST && ok "GET category-mappings" || bad "GET category-mappings 失败"
curl -fs -X DELETE -H "Authorization: Bearer $ADMIN_TOKEN" \
  "$ADMIN/category-mappings/polymarket/E2E_TEST" >/dev/null && ok "DELETE category-mapping"
SH=$(curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/shadow-health/summary?hours=24")
echo "$SH" | grep -q ok_rate && ok "GET shadow-health/summary" || bad "GET shadow-health/summary 失败"
curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/shadow-health/heatmap?hours=24" >/dev/null && ok "GET shadow-health/heatmap"
curl -fs -H "Authorization: Bearer $ADMIN_TOKEN" "$ADMIN/shadow-health/top-diffs?hours=24&limit=5" >/dev/null && ok "GET shadow-health/top-diffs"

echo "=== 7. 钱包登录闭环（SIWE / EIP-4361）==="
# 集成测试（tests/wallet_login.rs，#[ignore]）：随机 EOA → nonce → 签名 → /auth/wallet → /me → /me/wallets → 重放防护。
# 依赖已运行的服务（account:8084）+ 已迁移 0014 的 PG。
WL_OUT=$(cargo test -p sharpside-account --test wallet_login -- --ignored --nocapture 2>&1)
echo "$WL_OUT" | grep -q 'wallet_login_e2e' && echo "$WL_OUT" | grep -qE 'test result: ok' \
  && ok "钱包登录闭环（SIWE 验签 + nonce 重放防护）" \
  || { bad "钱包登录闭环失败"; echo "$WL_OUT" | grep -E '✅|❌|panicked|Error|test result' >&2; }

echo "=========================================="
echo " 端到端验证结果：PASS=$pass  FAIL=$fail"
[ "$fail" -eq 0 ] && echo " ✅ 全部通过" || echo " ❌ 存在失败，见 /tmp/e2e_*.log"
echo "=========================================="
exit $fail
