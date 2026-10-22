# Changelog

## 0.1.0 — 2026-10-21

- Initial application: Engines A, B and C, backtesting, dynasty laboratory, nine pages, PostgreSQL 17 persistence, Docker Compose, backup and restore scripts.
- Aligned with REAS 1.2.0-draft: domain, application and adapters layout; six ports and one `PostgresStore` adapter; `TeamId`, `FranchiseId` and `SeasonYear` newtypes; typed errors; conditional job transitions; documentation checker, terminology, self-audit and decisions.

- Completing an archive season matches games by date and unordered team pair, so the neutral-site NBA Cup final no longer blocks the 2024-25 sync. Compact Remove button with an Actions header in the coverage table.
- The results sync runs in the background; `GET /api/data/sync/status` and the Data page show live progress (requests, games, wait for the quota).
- A sync or upload of a season the archive covers only in part (2024-25) keeps the archive games and appends the later games with computed Elo, so the 2024-25 play-in and playoffs can be filled in.
- A results sync or upload with no games is refused instead of creating an empty season; computed seasons can be removed from the Data page (`DELETE /api/data/seasons/{season}`).
- Daily automatic sync of the current season at `RESULTS_SYNC_TIME` in `RESULTS_SYNC_TIMEZONE` (default 04:00 America/New_York), with catch-up at start.
- Results sync from the BALLDONTLIE API (`BALLDONTLIE_API_KEY`): `POST /api/data/sync`, the `sync-results` subcommand and a Data page button; paced to the plan's quota.
- Elo continuation: computed ratings for seasons after the archive (2025-26 and later) from a results CSV or a preseason projection, with the Elo source stored on each game.

- Security review: browser request guard against cross-site requests and DNS rebinding, security headers, decompression and download size caps, job work budget, read-only hardened container.

Full ASD-STE100 conformance is not verified.
