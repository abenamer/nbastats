#!/usr/bin/env bash
# Verify that the database is usable: migrations present, key tables readable, counts shown.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_db_running
sql="SELECT
  (SELECT count(*) FROM _sqlx_migrations WHERE success) AS migrations,
  (SELECT count(*) FROM games) AS games,
  (SELECT count(*) FROM seasons) AS seasons,
  (SELECT count(*) FROM championships) AS championships,
  (SELECT count(*) FROM simulation_jobs) AS jobs,
  (SELECT count(*) FROM data_imports) AS imports;"
out="$(db_exec psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1 -At -F ' ' -c "$sql")"
read -r migrations games seasons championships jobs imports <<< "$out"
echo "migrations=$migrations games=$games seasons=$seasons championships=$championships jobs=$jobs imports=$imports"
if (( migrations < 1 )); then
  echo "error: no migrations recorded; the schema is missing" >&2
  exit 1
fi
if (( games < 1 )); then
  echo "warning: the games table is empty (import a dataset from the Data page)" >&2
fi
echo "database OK"
