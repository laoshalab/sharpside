#!/usr/bin/env bash
# 本地前端：挂载宿主机 static 到 Docker web，或直接起二进制。
# 用法：
#   bash scripts/dev-web.sh docker   # 推荐：重建/重启 web 容器并挂载 apps/web/static
#   bash scripts/dev-web.sh host     # 宿主机跑二进制（cwd=仓库根），端口默认 8070
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MODE="${1:-docker}"
PORT="${WEB_PORT:-8070}"

case "$MODE" in
  docker)
    echo "==> 停旧 web 容器（若存在）"
    docker rm -f sharpside-web 2>/dev/null || true
    echo "==> 起 web：宿主机 static → /app/apps/web/static，映射 ${PORT}:8080"
    # 不依赖完整 compose 健康检查；单独跑 web，gateway 用宿主端口。
    docker run -d --name sharpside-web \
      -p "${PORT}:8080" \
      -e WEB_LISTEN_ADDR=0.0.0.0:8080 \
      -e GATEWAY_URL="${GATEWAY_URL:-http://host.docker.internal:8085}" \
      --add-host=host.docker.internal:host-gateway \
      -v "$ROOT/apps/web/static:/app/apps/web/static:ro" \
      sharpside-web:latest
    echo "==> 就绪 http://127.0.0.1:${PORT}/  （改 static/ 后直接刷新即可）"
    echo "    验证：curl -s http://127.0.0.1:${PORT}/pages/leaderboard.js | grep -c midTruncate"
    ;;
  host)
    echo "==> 编译 sharpside-web（debug）"
    cargo build -p sharpside-web
    echo "==> 从仓库根启动，监听 ${PORT}"
    exec env \
      WEB_LISTEN_ADDR="127.0.0.1:${PORT}" \
      GATEWAY_URL="${GATEWAY_URL:-http://127.0.0.1:8085}" \
      RUST_LOG="${RUST_LOG:-info,sharpside=debug}" \
      ./target/debug/sharpside-web
    ;;
  *)
    echo "用法: $0 {docker|host}" >&2
    exit 1
    ;;
esac
