#!/usr/bin/env bash
# 安全修复 4.3：PostgreSQL 逻辑备份（pg_dump）。
#
# 用法：
#   ./infra/scripts/pg_backup.sh                  # 用 DATABASE_URL 或默认本地
#   BACKUP_DIR=/var/backups/sharpside RETENTION_DAYS=7 ./infra/scripts/pg_backup.sh
#
# cron 示例（每日 03:15）：
#   15 3 * * * cd /opt/sharpside && BACKUP_DIR=/var/backups/sharpside ./infra/scripts/pg_backup.sh >>/var/log/sharpside-pg-backup.log 2>&1
#
# 恢复：
#   gunzip -c sharpside_YYYYMMDD_HHMMSS.sql.gz | psql "$DATABASE_URL"

set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-./infra/data/backups}"
RETENTION_DAYS="${RETENTION_DAYS:-7}"
DATABASE_URL="${DATABASE_URL:-postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside}"
STAMP="$(date -u +%Y%m%d_%H%M%S)"
OUT="${BACKUP_DIR}/sharpside_${STAMP}.sql.gz"

mkdir -p "$BACKUP_DIR"

echo "[pg_backup] dumping to $OUT"
# 优先用 docker 内 pg_dump（compose 服务名 postgres）；否则本机 pg_dump。
if docker compose -f infra/docker-compose.yml ps postgres 2>/dev/null | grep -q Up; then
  docker compose -f infra/docker-compose.yml exec -T postgres \
    pg_dump -U sharpside -d sharpside --no-owner --format=plain \
    | gzip -c > "$OUT"
elif command -v pg_dump >/dev/null 2>&1; then
  pg_dump "$DATABASE_URL" --no-owner --format=plain | gzip -c > "$OUT"
else
  echo "[pg_backup] ERROR: 无 docker postgres 也无本机 pg_dump" >&2
  exit 1
fi

SIZE="$(wc -c < "$OUT" | tr -d ' ')"
echo "[pg_backup] ok size=${SIZE}B file=$OUT"

# 清理过期
find "$BACKUP_DIR" -name 'sharpside_*.sql.gz' -type f -mtime "+${RETENTION_DAYS}" -print -delete \
  || true

echo "[pg_backup] retention=${RETENTION_DAYS}d done"
