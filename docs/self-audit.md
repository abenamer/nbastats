---
type: Audit Report
title: Self-audit against REAS
description: Evidence and limits of this project's compliance with the REAS standard.
tags: [quality, audit, reas]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
sources:
  - id: reas
    resource: https://github.com/abenamer/reas
    title: REAS, Rust Engineering and Architecture Standard, 1.2.0-draft
---

# Self-audit against REAS

**Status:** Structural checks run locally on 2026-10-21. Formal ASD-STE100 review and remote CI execution are pending.

| Rule | Evidence | Status |
| --- | --- | --- |
| REAS-RUST-001 invariants | `TeamId`, `FranchiseId`, validated `SeasonYear`; enum states; `SeriesState`; private fields | Implemented |
| REAS-RUST-002 zero-cost abstractions | `Progress`, `GameModel`/`SeriesModel` generics; `ModelSpec` dispatches once | Implemented |
| REAS-RUST-003 coherent traits | Two blanket implementations: `Progress` for closures, `SeriesModel` for every `GameModel`; no generic repository | Implemented |
| REAS-RUST-004 ownership | No `unsafe`; `Arc<App>` shared state with `RwLock` caches; engines borrow slices | Implemented |
| REAS-RUST-005 typed errors | `StoreError`, `ImportError`, `JobError`, `ValidationError`, `ReconstructError`, `SeasonError`; no `unwrap` outside tests | Implemented |
| REAS-RUST-006 incremental compilation | `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` run locally | Checked locally |
| REAS-STATE-001 runtime states | `JobStatus::transition`, `SeriesState::record`, `PlayInState::advance`, ordered `PlayoffResult` | Implemented |
| REAS-STATE-002 typestate | Not used; persisted and externally loaded state uses runtime enums | Decision, see [ADR 0002](/decisions/0002-runtime-enums-over-typestate.md) |
| REAS-STATE-003 persisted transitions | `claim_next_job` uses `FOR UPDATE SKIP LOCKED`; `finish_job` and `apply_job_event` are conditional updates | Implemented, tested |
| REAS-ARCH-001 visible code path | inbound HTTP adapter → use case → port → `PostgresStore`; composition root `main.rs` | Implemented |
| REAS-ARCH-002 dependencies inward | `domain` has no Axum, SQLx or HTTP imports | Implemented |
| REAS-ARCH-003 private project questions | Asked in [ADR 0001](/decisions/0001-ports-at-three-boundaries.md); answers pending from the maintainer | Open |
| REAS-ARCH-004 no hidden abstraction | One adapter type per boundary; no factories; ports are named traits | Implemented |
| REAS-ARCH-005 algorithms separate | Engines are pure functions over slices | Implemented |
| REAS-DATA-001 explicit storage | Explicit SQL; row mapping in `adapters::postgres::records` | Implemented |
| REAS-DATA-002 transactions | Batch upserts in one transaction; conditional job updates | Implemented |
| REAS-DATA-003 durable data | Named volumes; backup, restore and persistence scripts | Tested 2026-10-21 |
| REAS-DATA-004 reproducible builds | Run-time checked queries; no database needed to compile | Implemented |
| REAS-TEST-001 tests | 92 unit tests, 9 PostgreSQL integration tests (one with a fake results source), seeded simulations, invalid-input tests | Pass locally |
| REAS-TEST-002 async and CPU work | Engines run on `spawn_blocking` with Rayon; no lock held across `.await` | Implemented |
| REAS-TEST-003 security | Security review done 2026-10-21: request guard (CSRF, DNS rebinding), security headers, decompression and download caps, work budget, container hardening; see [Security model](/concepts/security.md) | Reviewed; \`cargo audit\` result recorded in the log |
| REAS-TEST-004 evidence | This table lists only executed checks | Maintained |
| REAS-DOC-001 ASD-STE100 | Target only; see [STE review](/ste-review.md) | Unverified |
| REAS-DOC-002 OKF bundle | `docs/` with `okf_version: "0.2"`; `scripts/check_docs.py` passes | Checked locally |
| REAS-DOC-003 terminology | [Project terminology](/terminology.md) | Initial |
| REAS-DOC-004 document while implementing | Concepts, runbooks, reference and decisions updated with the code | Maintained |
| REAS-DOC-005 independent validation | `python scripts/check_docs.py` and `tests/docs` | Checked locally |

Do not treat this table as proof of compliance. Update it after each release or major change.
