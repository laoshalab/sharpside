#!/usr/bin/env bash
# 批量导入交易者地址 → venue-hub 回填 raw_trades。
#
# 用法：
#   bash infra/import_addresses.sh -f addresses.txt
#   bash infra/import_addresses.sh -f addresses.txt -p polymarket -u http://127.0.0.1:8081
#   bash infra/import_addresses.sh -f addresses.txt --single   # 逐条调 /traders/import
#
# 地址文件格式（每行一条）：
#   0xabc...                       # 仅地址
#   0xabc...,可选别名               # 地址,alias
#   0xabc...,可选别名,@x_user       # 地址,alias,x_username
#   # 开头为注释，空行跳过
#
# 默认走 POST /traders/import/batch（单请求逐条回填，最多 100 条/批）；
# 超过 100 条自动分批。--single 改为循环调 POST /traders/import。
#
# 限流：batch 端点内部地址间 sleep 200ms；--single 模式脚本侧 sleep 300ms。
set -uo pipefail

PLATFORM="polymarket"
URL="http://127.0.0.1:8081"
FILE=""
MODE="batch"   # batch | single
BATCH_MAX=100
# venue-hub 写端点需 admin token：从环境变量 VENUE_HUB_ADMIN_TOKEN 读取（与 venue-hub 服务一致）。
ADMIN_TOKEN="${VENUE_HUB_ADMIN_TOKEN:-}"

usage() {
  sed -n '2,/^set /p' "$0" | sed 's/^# \{0,1\}//'
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    -f) FILE="$2"; shift 2 ;;
    -p) PLATFORM="$2"; shift 2 ;;
    -u) URL="$2"; shift 2 ;;
    --single) MODE="single"; shift ;;
    --batch) MODE="batch"; shift ;;
    -h|--help) usage ;;
    *) echo "未知参数: $1" >&2; usage ;;
  esac
done

if [ -z "$FILE" ]; then echo "错误：请用 -f 指定地址文件" >&2; usage; fi
if [ ! -f "$FILE" ]; then echo "错误：文件不存在: $FILE" >&2; exit 1; fi
command -v curl >/dev/null || { echo "错误：需要 curl" >&2; exit 1; }
command -v python3 >/dev/null || { echo "错误：需要 python3" >&2; exit 1; }
if [ -z "$ADMIN_TOKEN" ]; then
  echo "错误：请设环境变量 VENUE_HUB_ADMIN_TOKEN（venue-hub 写端点需 admin 鉴权）" >&2
  exit 1
fi
AUTH_HDR="Authorization: Bearer $ADMIN_TOKEN"

# 解析地址文件 → JSON 数组（统一注入 platform）。输出到 stdout。
parse_items() {
  python3 - "$FILE" "$PLATFORM" <<'PY'
import json, sys
items = []
with open(sys.argv[1]) as f:
    for line in f:
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = [p.strip() for p in line.split(",")]
        item = {"platform": sys.argv[2], "address": parts[0]}
        if len(parts) > 1 and parts[1]:
            item["alias"] = parts[1]
        if len(parts) > 2 and parts[2]:
            item["x_username"] = parts[2]
        items.append(item)
print(json.dumps(items))
PY
}

ITEMS=$(parse_items)
TOTAL=$(python3 -c 'import json,sys;print(len(json.load(sys.stdin)))' <<<"$ITEMS")
if [ "$TOTAL" -eq 0 ]; then echo "地址文件为空（无有效条目）"; exit 0; fi
echo "=== 导入 $TOTAL 个地址（platform=$PLATFORM, mode=$MODE, url=$URL）==="

ok=0; fail=0; total_trades=0

if [ "$MODE" = "single" ]; then
  # 逐条调 POST /traders/import。
  python3 -c 'import json,sys
for x in json.load(sys.stdin): print(json.dumps(x))' <<<"$ITEMS" | while IFS= read -r line; do
    addr=$(python3 -c 'import json,sys;print(json.load(sys.stdin)["address"])' <<<"$line")
    printf '  导入 %s ... ' "$addr"
    resp=$(curl -sS -X POST "$URL/traders/import" -H 'Content-Type: application/json' -H "$AUTH_HDR" -d "$line" 2>&1)
    n=$(python3 -c 'import json,sys
try: print(json.load(sys.stdin)["trades_backfilled"])
except: print("")' <<<"$resp" 2>/dev/null)
    if [ -n "$n" ]; then echo "OK ($n 笔成交)"; else echo "FAIL: $resp"; fi
    sleep 0.3
  done
  echo "=== 完成（逐条模式，详见上方逐行输出）==="
  exit 0
fi

# batch 模式：每 BATCH_MAX 条一批。
NB=$(( (TOTAL + BATCH_MAX - 1) / BATCH_MAX ))
echo "分 $NB 批，每批最多 $BATCH_MAX 条"

python3 - "$BATCH_MAX" "$ITEMS" <<'PY' > /tmp/.sharpside_import_batches.txt
import json, sys
items = json.loads(sys.argv[2])
batch_max = int(sys.argv[1])
for i in range(0, len(items), batch_max):
    print(json.dumps(items[i:i+batch_max]))
PY

bi=0
while IFS= read -r batch_items; do
  bi=$((bi+1))
  printf '=== 第 %d/%d 批 ===\n' "$bi" "$NB"
  body=$(python3 -c 'import json,sys;print(json.dumps({"items":json.loads(sys.argv[1])}))' "$batch_items")
  resp=$(curl -sS -X POST "$URL/traders/import/batch" -H 'Content-Type: application/json' -H "$AUTH_HDR" -d "$body" 2>&1)
  rc=$?
  if [ $rc -ne 0 ]; then
    echo "  ❌ 批次请求失败: $resp" >&2
    fail=$((fail + 1))
    continue
  fi
  python3 - <<'PY' <<<"$resp"
import json, sys
try:
    r = json.load(sys.stdin)
except Exception as e:
    print(f"  ❌ 响应解析失败: {e}"); sys.exit(0)
if isinstance(r, dict) and "error" in r:
    print(f"  ❌ 批次错误: {r['error']}"); sys.exit(0)
for it in r.get("items", []):
    mark = "✅" if it["ok"] else "❌"
    extra = f" ({it['trades_backfilled']} 笔)" if it["ok"] else f": {it.get('error','')}"
    print(f"  {mark} {it['address']}{extra}")
print(f"  小计: 成功 {r.get('succeeded',0)} / 失败 {r.get('failed',0)} / 回填 {r.get('total_trades_backfilled',0)} 笔")
PY
  s=$(python3 -c 'import json,sys
try: print(json.load(sys.stdin)["succeeded"])
except: print(0)' <<<"$resp" 2>/dev/null)
  f=$(python3 -c 'import json,sys
try: print(json.load(sys.stdin)["failed"])
except: print(0)' <<<"$resp" 2>/dev/null)
  t=$(python3 -c 'import json,sys
try: print(json.load(sys.stdin)["total_trades_backfilled"])
except: print(0)' <<<"$resp" 2>/dev/null)
  ok=$((ok + s)); fail=$((fail + f)); total_trades=$((total_trades + t))
done < /tmp/.sharpside_import_batches.txt

rm -f /tmp/.sharpside_import_batches.txt
echo "=== 全部完成：成功 $ok / 失败 $fail / 回填 $total_trades 笔 ==="
