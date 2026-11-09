# Claude instructions

This project follows the [REAS standard](https://github.com/abenamer/reas). Its text is vendored at [standards/reas/standard/index.md](standards/reas/standard/index.md), and the rules for Rust load from `.claude/rules/rust.md` when you touch Rust. Start at [docs/index.md](docs/index.md).

@AGENTS.md

# Commands

- Format, check, lint: `cargo fmt --all`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`
- Unit tests, no database: `cargo test --lib`; one module: `cargo test --lib <module>`
- Integration tests: `DATABASE_URL=... cargo test`, each test in a fresh database (CI runs them; the compose database port is not published to the host)
- Docs: `python3 scripts/check_docs.py` and `python3 -m unittest discover -s tests/docs -t .`
- A query against the running database: `docker compose exec -T nba-postgres psql -U nba -d nba -c "..."`
- Ship: `/deploy` (push, wait for CI, pull, rebuild). Never rebuild before CI is green.

# Conventions

- Layout: `src/domain` (pure), `src/application` (use cases and ports), `src/adapters` (PostgreSQL, HTTP, CSV, dataset), `src/main.rs` (composition root). Add a port only at a genuine boundary, name the concrete adapter, add no forwarding layers.
- Lifecycles are enums with explicit transitions. Use the identifier newtypes; never pass bare `i32` keys.
- Every change updates the OKF bundle in `docs/` the same day: the log, the changelog and the concept or reference it touches, in the words of [docs/terminology.md](docs/terminology.md). ASD-STE100 Issue 9 is a target; never claim verified compliance.
- A model change is backtested from the command line and recorded in an ADR before it touches a page (ADR 0014 is the pattern). The market record judges the live model; nothing on a page may change the record's past rows.
- Report only the checks you executed, with their output.

# Gotchas

- Templates are Askama and compile into the binary: a template error fails `cargo check`.
- Migrations only add. A check constraint that lists values is named `<table>_<column>_check`.
- The hooks in `.claude/settings.json` format Rust after each edit and run the checks before a turn ends; `docker compose down -v`, forced pushes, and reading `.env` or the key files are denied there, not by this file.
- Static files are versioned by a content hash; never bump a version by hand.
