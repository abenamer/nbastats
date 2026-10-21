# Changelog

## 0.1.0 — 2026-10-21

- Initial application: Engines A, B and C, backtesting, dynasty laboratory, nine pages, PostgreSQL 17 persistence, Docker Compose, backup and restore scripts.
- Aligned with REAS 1.2.0-draft: domain, application and adapters layout; six ports and one `PostgresStore` adapter; `TeamId`, `FranchiseId` and `SeasonYear` newtypes; typed errors; conditional job transitions; documentation checker, terminology, self-audit and decisions.

- Results sync from the BALLDONTLIE API (`BALLDONTLIE_API_KEY`): `POST /api/data/sync`, the `sync-results` subcommand and a Data page button; paced to the plan's quota.
- Elo continuation: computed ratings for seasons after the archive (2025-26 and later) from a results CSV or a preseason projection, with the Elo source stored on each game.

- Security review: browser request guard against cross-site requests and DNS rebinding, security headers, decompression and download size caps, job work budget, read-only hardened container.

Full ASD-STE100 conformance is not verified.
