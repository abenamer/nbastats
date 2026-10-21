#!/usr/bin/env bash
# Restore a backup created by scripts/backup.sh, then verify the restored database.
# Usage: scripts/restore.sh <backup-file> [--yes]
# The application container is stopped during the restore and started again afterwards.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
file="${1:-}"
confirm="${2:-}"
if [[ -z "$file" ]]; then
  echo "usage: scripts/restore.sh <backup-file> [--yes]" >&2
  exit 1
fi
if [[ ! -f "$file" && -f "$BACKUP_DIR/$file" ]]; then
  file="$BACKUP_DIR/$file"
fi
if [[ ! -f "$file" ]]; then
  echo "error: backup file not found: $file" >&2
  exit 1
fi
require_db_running
echo "This replaces ALL data in database '$POSTGRES_DB' with the contents of:"
echo "  $file"
if [[ "$confirm" != "--yes" ]]; then
  read -r -p "Continue? [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" ]] || { echo "aborted"; exit 1; }
fi
echo "stopping $APP_SERVICE"
compose stop "$APP_SERVICE" >/dev/null
echo "recreating schema"
db_exec psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1 -q \
  -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
echo "restoring"
db_exec pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --no-owner --no-privileges --exit-on-error < "$file"
echo "verifying"
"$SCRIPT_DIR/verify-database.sh"
echo "starting $APP_SERVICE"
compose start "$APP_SERVICE" >/dev/null
echo "restore complete"
