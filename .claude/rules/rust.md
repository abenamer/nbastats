---
paths:
  - "src/**/*.rs"
  - "tests/**/*.rs"
  - "Cargo.toml"
---

# REAS rules for Rust

The project follows the REAS standard, version 1.4.0-draft, vendored at `standards/reas/` (its documents under `standard/`, its example packages under `examples/`). Each rule has an identifier; read the file named for the full text before you apply one, and cite the identifier when a decision rests on it. The lines below are the requirements the standard states, condensed.

## Core Rust (`standards/reas/standard/core-rust.md`)

- **REAS-RUST-001, invariants explicit.** Newtypes with private fields and smart constructors for domain values with invariants; `Option` for optional, `Result` for fallible; enum variants for the legal cases, matched without a wildcard arm for enums this project controls; no field or setter that can make a value invalid.
- **REAS-RUST-002, abstractions for the reader.** Pick the abstraction that makes the code path easy to follow, then measure its runtime cost; explain any abstraction with extra runtime cost; never remove one only to compile faster.
- **REAS-RUST-003, trait relationships explicit.** A blanket implementation only when it is correct for every type that meets its bounds; check coherence and orphan rules; no generic repository framework before a second concrete implementation exists; no inherent method named like a trait method on the same type.
- **REAS-RUST-004, ownership explicit.** Make the owner of each value easy to find; end each borrow after its last use; `Arc<Mutex<T>>` only for shared ownership with synchronised mutation; if lifetimes obscure the data, change the data model first; no `unsafe` here.
- **REAS-RUST-005, errors explicit.** `Result` with a typed error where the caller must handle failure; `?` to propagate; an inbound adapter converts an error to a response; never success after a failure; the CLI returns a failure status. Panic only for a bug; no `unwrap` where `None` or `Err` can occur; `expect` with the reason when failure is impossible; no default in place of a missing required input.
- **REAS-RUST-006, compile after each step.** `cargo fmt --all`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test` after each change to a type, function or module; report only the checks that ran.
- **REAS-RUST-007, async and CPU work apart.** No blocking I/O or CPU work on an executor thread: `spawn_blocking` or the Rayon pool, with a bound on concurrent work; never hold a `std::sync::Mutex` guard across an `.await`; record how a task responds to cancellation and shutdown.

## State machines (`standards/reas/standard/state-machines.md`; the typestate example is `standards/reas/examples/typestate/`)

- **REAS-STATE-001, runtime states.** One enum per workflow state, never several Boolean flags; each legal transition is a function; a typed error for a transition the state does not permit; match every variant.
- **REAS-STATE-002, typestate.** Only where operation order or initialisation is an invariant; this project keeps persisted and externally loaded state in runtime enums (ADR 0002) and validates external state at run time.
- **REAS-STATE-003, persisted transitions.** A conditional update (`... WHERE id = $1 AND state = $2`) or a row lock (`FOR UPDATE`) for each transition; success only when exactly one row changed; a conflict otherwise. The job store is the pattern.

## Architecture (`standards/reas/standard/ports-and-adapters.md`; the example is `standards/reas/examples/ports-and-adapters/`)

- **REAS-ARCH-001, visible code path.** Inbound adapter, application use case, port trait, outbound adapter; one composition root (`src/main.rs`); a use case may call a port directly; no forwarding layer without a different responsibility.
- **REAS-ARCH-002, dependencies inward.** Axum, SQLx, HTTP and Docker stay out of `src/domain`; ports are declared in `src/application/ports.rs` and implemented in `src/adapters`.
- **REAS-ARCH-004, concrete implementations visible.** A port only where a test double or a replaceable implementation is needed or to isolate a system outside our control; no factories of factories, no service locator, no duplicate interfaces; adapters named for what they adapt (`PostgresStore`, `OddsApi`, `EspnScoreboard`).
- **REAS-ARCH-005, rules apart from infrastructure.** Business rules and mathematics are pure functions that take time, randomness and data as inputs; serialisation, row mapping and API clients live in adapters; dependencies come through constructors.
- **REAS-ARCH-006, adapter parameters.** A use case takes its adapter as a type parameter bound by the port by default; a trait object (`Arc<dyn Port + Send + Sync>`) where the implementation must vary at run time, with the decision recorded at the composition root; check dyn-compatibility first.

## Persistence (`standards/reas/standard/persistence.md`)

- **REAS-DATA-001, explicit storage.** One specific SQL query per operation, in a named adapter; every schema change is a migration; database constraints for the invariants PostgreSQL can express; parameterised queries; an index only for a measured query, with the query it serves recorded.
- **REAS-DATA-002, transaction boundaries.** Writes that must succeed or fail together in one transaction, declared as one port operation whose adapter owns commit and rollback; no SQLx transaction types in domain or application code; no external side effects inside a database transaction's guarantee.
- **REAS-DATA-003, data outlives the container.** Named volumes, backups outside the volume, a tested restore; never `docker compose down -v` against data to keep.
- **REAS-DATA-004, reproducible builds.** `Cargo.lock` committed and `--locked` in CI; no compile-time dependency on a developer's database.

## Validation, security and evidence (`standards/reas/standard/validation.md`)

- **REAS-TEST-001, invariants and failures.** Unit tests for each invariant and each transition, for error paths and invalid input; a failing test double for error propagation through a port; integration tests against the real PostgreSQL, never an in-memory stand-in; record the seed of a failing simulation.
- **REAS-TEST-002, observable evidence.** A check is passed only when its execution shows it; report passed, failed and not run apart, with the date and commit; a passing result does not carry over to code changed since.
- **REAS-SEC-001, security properties.** Validate external input before it reaches the domain; parameterised queries; credentials out of source and logs; limits on request size, connections and execution time; each endpoint documented as anonymous or authenticated with the reason; no trust from a loopback address alone; `cargo audit` in CI.
