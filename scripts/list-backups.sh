#!/usr/bin/env bash
# List existing backups with size and age.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
shopt -s nullglob
files=("$BACKUP_DIR"/*.dump)
if (( ${#files[@]} == 0 )); then
  echo "no backups in $BACKUP_DIR"
  exit 0
fi
printf "%-45s %10s  %s\n" "FILE" "SIZE" "MODIFIED (UTC)"
for f in "${files[@]}"; do
  printf "%-45s %10s  %s\n" "$(basename "$f")" "$(du -h "$f" | cut -f1)" "$(date -u -r "$f" +%Y-%m-%dT%H:%M:%SZ)"
done
