#!/usr/bin/env bash
# Persistence integration test.
# 1. Starts the stack, waits for health.
# 2. Writes a marker row (a named simulation job) through the API.
# 3. Recreates the containers without deleting volumes (down + up, and up --build if requested).
# 4. Verifies the marker, the imported games and the derived tables survived.
# Usage: scripts/verify-persistence.sh [--build] [--keep]
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
build=""; keep=""
for arg in "$@"; do
  case "$arg" in
    --build) build="--build" ;;
    --keep) keep="1" ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done
port="${APP_HOST_PORT:-3000}"
host="${APP_HOST_BIND:-127.0.0.1}"
base="http://$host:$port"

wait_healthy() {
  local tries=0
  until curl -fsS "$base/healthz" >/dev/null 2>&1; do
    tries=$((tries + 1))
    if (( tries > 120 )); then echo "error: application did not become healthy" >&2; compose logs --tail 50 "$APP_SERVICE" >&2; exit 1; fi
    sleep 2
  done
}
wait_for_games() {
  local tries=0
  until [[ "$(curl -fsS "$base/api/health" | python3 -c 'import sys,json; print(json.load(sys.stdin)["tables"]["games"])')" != "0" ]]; do
    tries=$((tries + 1))
    if (( tries > 120 )); then echo "error: no games imported" >&2; exit 1; fi
    sleep 2
  done
}
games_count() { curl -fsS "$base/api/health" | python3 -c 'import sys,json; print(json.load(sys.stdin)["tables"]["games"])'; }
jobs_named() { curl -fsS "$base/api/jobs?kind=alpha&limit=500" | python3 -c "import sys,json; print(sum(1 for j in json.load(sys.stdin) if j.get('name')=='$1'))"; }
champions() { curl -fsS "$base/api/dashboard?from=2004&to=2024" | python3 -c 'import sys,json; print(json.load(sys.stdin)["seasons_with_champion"])'; }

echo "[1/5] starting stack"
compose up -d $build
wait_healthy; wait_for_games
before_games="$(games_count)"; before_champs="$(champions)"
marker="persistence-check-$(date -u +%Y%m%dT%H%M%SZ)"
echo "[2/5] writing marker '$marker' (games=$before_games, champions=$before_champs)"
curl -fsS -X POST -H 'Content-Type: application/json' "$base/api/jobs/alpha" -d "{\"name\":\"$marker\",\"franchises\":[\"A\",\"B\",\"C\"],\"alphas\":[1,1,1],\"seasons\":5,\"iterations\":1000,\"threshold\":2,\"seed\":1}" >/dev/null
sleep 3
[[ "$(jobs_named "$marker")" == "1" ]] || { echo "error: marker not stored" >&2; exit 1; }

echo "[3/5] recreating containers (down + up) without touching volumes"
compose down
compose up -d
wait_healthy
echo "[4/5] restarting containers (restart) and rebuilding image if requested"
compose restart
wait_healthy
if [[ -n "$build" ]]; then compose up -d --build; wait_healthy; fi

echo "[5/5] verifying"
after_games="$(games_count)"; after_champs="$(champions)"; marker_count="$(jobs_named "$marker")"
echo "games before=$before_games after=$after_games; champions before=$before_champs after=$after_champs; marker rows=$marker_count"
fail=0
[[ "$after_games" == "$before_games" ]] || { echo "FAIL: games count changed" >&2; fail=1; }
[[ "$after_champs" == "$before_champs" ]] || { echo "FAIL: championships changed" >&2; fail=1; }
[[ "$marker_count" == "1" ]] || { echo "FAIL: marker job lost" >&2; fail=1; }
docker volume inspect nba_postgres_data nba_application_data >/dev/null || { echo "FAIL: named volumes missing" >&2; fail=1; }
if (( fail )); then echo "PERSISTENCE TEST FAILED"; exit 1; fi
echo "PERSISTENCE TEST PASSED"
if [[ -z "$keep" ]]; then
  # Remove the marker so repeated runs stay clean (the volumes are untouched).
  id="$(curl -fsS "$base/api/jobs?kind=alpha&limit=500" | python3 -c "import sys,json; print([j['id'] for j in json.load(sys.stdin) if j.get('name')=='$marker'][0])")"
  curl -fsS -X DELETE "$base/api/jobs/$id" >/dev/null || true
fi
