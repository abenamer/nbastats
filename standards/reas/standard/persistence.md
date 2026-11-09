---
type: Engineering Standard
title: Persistence and transactions
description: PostgreSQL storage, transactions, and reproducible builds.
tags: [postgresql, sqlx, transactions]
---

# Persistence and transactions

<a id="reas-data-001"></a>
## REAS-DATA-001: Use explicit storage operations

Write a specific SQL query for each operation.
Keep SQL in named concrete adapters.
Version each schema change as a migration.
When a SQLx row type has different invariants, map it to the domain type through validation.
Use database constraints for invariants that PostgreSQL can express as constraints.
Use parameterized queries for data values.

Select indexes from representative query plans and workload measurements.
Record the query that each index serves.
Include storage and write cost in that decision.
Do not add an index only because a query filters a column.

<a id="reas-data-002"></a>
## REAS-DATA-002: Set transaction boundaries

Put related database writes that must succeed or fail together in one transaction.
Declare the atomic operation in an application port.
Let its concrete adapter own the transaction, including commit and rollback.
For writes across repositories, use one operation-level port with an adapter that shares the transaction.
Do not expose SQLx transaction types in domain or application code.

Guard workflow transitions as [REAS-STATE-003](state-machines.md#reas-state-003) specifies.
Do not include HTTP calls or other external side effects in the guarantee of a local database transaction.
For such work, record the retry, idempotency, and partial-failure policy.
Do not build a generic repository framework before a second concrete implementation exists.

<a id="reas-data-003"></a>
## REAS-DATA-003: Keep data beyond the container lifecycle

Store PostgreSQL data in a named Docker volume.
Keep backups in storage that is independent of the database volume.
Do a test of the restore procedure.
Make sure that a container rebuild or replacement preserves the records.
Do not use `docker compose down -v` against a database that you must keep.

The `-v` flag removes named volumes declared by the Compose project.
It also removes anonymous volumes attached to the project's containers.
It does not remove external volumes.
A volume is not a backup.

<a id="reas-data-004"></a>
## REAS-DATA-004: Make builds reproducible

For SQLx checked queries, commit the metadata that `cargo sqlx prepare` writes to `.sqlx/`.
Set `SQLX_OFFLINE=true` in builds that must not connect to a database.
Do not make compilation depend on a developer's local database.
Offline compilation does not establish that the metadata matches the current migrations.

In database integration CI, apply migrations to a new empty database.
Then run `cargo sqlx prepare --check` for the supported targets and features.
Keep the SQLx CLI version compatible with the project's SQLx version.

Commit `Cargo.lock` for application workspaces.
Use `--locked` for release builds.
Record the Rust toolchain, target, enabled features, and native dependencies.
Record the lockfile policy for library examples.
