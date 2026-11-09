# Rust examples

Each subdirectory is an independent Cargo package.
The examples commit `Cargo.lock` and use the root `rust-toolchain.toml`.

- [Ports and adapters](ports-and-adapters/) includes a CLI command function, use case, port trait, and in-memory outbound adapter.
  One composition root connects them.
  The tests include input boundaries, missing records, and repository failures.
- [Typestate](typestate/) includes two legal transitions and a sealed state trait.
  Three compile-fail doctests cover an invalid operation, an invalid state argument, and an attempted downstream state implementation.

The CLI example returns rendered text from a library function.
A CLI executable must also set the exit status for a failed command.
The examples do not include a database adapter, persisted transitions, transaction ownership, or an async port.

Run the stable checks:

```bash
cargo check --locked --manifest-path examples/ports-and-adapters/Cargo.toml --all-targets
cargo check --locked --manifest-path examples/typestate/Cargo.toml --all-targets
cargo test --locked --manifest-path examples/ports-and-adapters/Cargo.toml
cargo test --locked --manifest-path examples/typestate/Cargo.toml
cargo clippy --locked --manifest-path examples/ports-and-adapters/Cargo.toml --all-targets -- -D warnings
cargo clippy --locked --manifest-path examples/typestate/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path examples/ports-and-adapters/Cargo.toml --all -- --check
cargo fmt --manifest-path examples/typestate/Cargo.toml --all -- --check
```

Stable rustdoc tests that a `compile_fail` snippet does not compile.
It does not test the expected error code.
The nightly job also tests the expected diagnostic codes:

```bash
rustup toolchain install nightly-2026-10-23 --profile minimal
cargo +nightly-2026-10-23 test --locked --manifest-path examples/typestate/Cargo.toml --doc
```

Nightly rustdoc fails a doctest when an expected error code is absent.
It can accept extra diagnostics.
The [self-audit](../standard/self-audit.md) identifies the code revision and commands for each recorded result.
