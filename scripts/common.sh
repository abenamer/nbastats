#!/usr/bin/env bash
# Shared helpers for the operations scripts. Source this file; do not run it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

if [[ -f .env ]]; then
  # shellcheck disable=SC1091
  set -a; source .env; set +a
fi

POSTGRES_USER="${POSTGRES_USER:-nba}"
POSTGRES_DB="${POSTGRES_DB:-nba}"
BACKUP_DIR="${BACKUP_DIR:-$PROJECT_DIR/backups}"
DB_SERVICE="${DB_SERVICE:-nba-postgres}"
APP_SERVICE="${APP_SERVICE:-nba-app}"

compose() { docker compose "$@"; }

db_exec() { compose exec -T "$DB_SERVICE" "$@"; }

require_db_running() {
  if ! compose ps --status running --services 2>/dev/null | grep -qx "$DB_SERVICE"; then
    echo "error: service '$DB_SERVICE' is not running (start it with: docker compose up -d $DB_SERVICE)" >&2
    exit 1
  fi
}

mkdir -p "$BACKUP_DIR"
