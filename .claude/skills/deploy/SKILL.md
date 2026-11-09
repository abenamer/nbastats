---
name: deploy
description: Ship the committed work the maintainer's way. Push main, wait for the GitHub Actions run to pass, then pull and rebuild the local Docker Compose application and check its health. CI first, then the local rebuild, never the other way round.
disable-model-invocation: true
---

1. Be on `main` with every change committed. `git status --short` may show only untracked files you know about. If not, stop and ask.
2. `git push -u origin main`.
3. Find the run of the head commit and watch it to the end:

       sha=$(git rev-parse HEAD)
       gh run list --branch main --limit 5 --json databaseId,headSha --jq ".[] | select(.headSha==\"$sha\") | .databaseId"
       gh run watch <id> --exit-status --interval 15

   The run can take half a minute to appear; look again rather than guessing. If it fails, fix the cause, commit, and start again from step 2. Never rebuild on a red run.
4. `git pull origin main`.
5. If the commits touched only `docs/`, `README.md` or `CHANGELOG.md`, stop here and say so: the application is unchanged, and a rebuild would only restart it. Otherwise:

       docker compose up -d --build

   Never `docker compose down -v`. Do not run the backup step of the upgrade runbook unless a migration changes data; say when you skip it.
6. Wait for `curl -s http://localhost:3000/api/health` to report `"ok":true`, and check that `started_at` moved and `migrations_applied` is the count you expect. `docker compose ps` shows both containers healthy.
7. If a page changed, check it on the running application in a headless browser, then report the run id, the start time, what you verified and anything that did not go as expected.
