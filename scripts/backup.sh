#!/usr/bin/env bash
# Create a timestamped PostgreSQL backup (pg_dump custom format) in ./backups.
# Usage: scripts/backup.sh [label]
# Backups live outside the PostgreSQL data volume, so you can copy and keep them independently.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_db_running
label="${1:-}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
name="nba_${stamp}${label:+_$label}.dump"
target="$BACKUP_DIR/$name"
echo "backing up database '$POSTGRES_DB' to $target"
db_exec pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --compress=6 --no-owner --no-privileges > "$target"
size="$(du -h "$target" | cut -f1)"
sha="$(sha256sum "$target" | cut -d' ' -f1)"
echo "$sha  $name" >> "$BACKUP_DIR/SHA256SUMS"
echo "done: $target ($size, sha256 $sha)"
