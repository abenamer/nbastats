# Contributing

1. Read [docs/index.md](docs/index.md) and the [REAS standard](https://github.com/abenamer/reas).
2. Put domain rules in `src/domain`, use cases and ports in `src/application`, infrastructure in `src/adapters`.
3. Add tests for invariants, state transitions and invalid input. Integration tests run against PostgreSQL.
4. Run `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
5. Update the documentation bundle and run `python scripts/check_docs.py`.
6. Record an architecture decision in `docs/decisions/` when you change a boundary.
7. Do not claim ASD-STE100 compliance without a review against the official standard.
