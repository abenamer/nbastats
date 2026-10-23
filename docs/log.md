# Bundle update log

## 2026-10-23
* **Update**: The results sync fetches only the days since the last played game after the first sync of a season; [Elo continuation](/concepts/elo-continuation.md) and the import runbook describe the window. The term Fetch window is defined.
* **Update**: The Elo Explorer season chart shows one-game teams as a dot and the games not played yet as a shaded strip; noted in the [Application overview](/concepts/application.md).

## 2026-10-22
* **Update**: Historical division alignment back to 1946-47 described in [Historical data](/concepts/historical-data.md).
* **Update**: Records page gains eras by league size and the 3-point line; the term Era is defined.
* **Update**: Added the Records page to the [Application overview](/concepts/application.md), the `/api/records` route, and the terms Decade and League size.
* **Update**: Elo Explorer charts repaired (date axis, per-game tooltip). Franchise charts use the franchise's uniform colours; the API reference lists the `colors` field.
* **Update**: Curated champions now cover every season the archive has; [Historical data](/concepts/historical-data.md) says which seasons are derived and which are curated.
* **Update**: Added the Franchise History page to the [Application overview](/concepts/application.md).
* **Update**: Archive matching ignores home and away order; noted in [Elo continuation](/concepts/elo-continuation.md).
* **Update**: The results sync runs in the background with a status route; the import runbook and the API reference describe the live progress line.
* **Update**: Syncs and uploads now complete a season that the archive covers only in part (2024-25 playoffs). [Elo continuation](/concepts/elo-continuation.md) and the import runbook describe the rule.
* **Update**: Added season removal to the import runbook and the API reference; an empty results fetch no longer creates a season.

## 2026-10-21
* **Update**: Added the daily results sync (`RESULTS_SYNC_*` settings) to the import runbook, configuration and API references.
* **Update**: Added the `ResultsSource` port and the BALLDONTLIE adapter: results sync runbook, configuration keys, API route, security rows and ADR 0001 update.
* **Update**: Added [Elo continuation](/concepts/elo-continuation.md): computed ratings for seasons after the archive, the results layout, preseason projections, the Elo source of each game, and the new API routes.
* **Update**: Security review. Added the [Security model](/concepts/security.md), the browser request guard, security headers, decompression and download size caps, the job work budget and container hardening.
* **Update**: Aligned the project with REAS 1.2.0-draft: ports and adapters layout, identifier newtypes, typed errors, documentation checker, terminology, self-audit, STE review status and three decisions.

## 2026-10-21
* **Initialization**: Created the bundle with concepts, runbooks and reference documents for application version 0.1.0.
