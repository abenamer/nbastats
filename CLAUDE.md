# Claude instructions

This project follows the [REAS standard](https://github.com/abenamer/reas). Start at [docs/index.md](docs/index.md).

1. Keep the layout: `src/domain` (pure), `src/application` (use cases and ports), `src/adapters` (PostgreSQL, HTTP, CSV, dataset), `src/main.rs` (composition root).
2. Model lifecycles as enums with explicit transitions. Use the identifier newtypes; do not pass bare `i32` keys.
3. Add a port only at a genuine boundary. Name the concrete adapter. Do not add forwarding layers.
4. Compile in small increments: `cargo fmt --all`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (integration tests need `DATABASE_URL`).
5. Update the OKF bundle in `docs/` with each change. Use [docs/terminology.md](docs/terminology.md). Target ASD-STE100 Issue 9; do not claim verified compliance.
6. Run `python scripts/check_docs.py` and `python -m unittest discover -s tests/docs -t .`.
7. Report only checks you executed. Never use `docker compose down -v` in instructions for routine maintenance.
