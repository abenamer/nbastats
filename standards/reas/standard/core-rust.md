---
type: Engineering Standard
title: Core Rust
description: Rules for Rust types, errors, ownership, and builds.
tags: [rust, types, traits, ownership, async]
---

# Core Rust

<a id="reas-rust-001"></a>
## REAS-RUST-001: Make invariants explicit

Use newtypes with private fields for domain values with extra invariants.
Use smart constructors to validate input.
Make sure that all safe public APIs preserve the invariants.
Use `Option` for an optional value.
Use `Result` for an operation that can fail.

Use enum variants for the legal cases.
For enums that this project controls, match all variants without a wildcard arm.
For an external non-exhaustive enum, handle the unknown case explicitly.
Do not expose a field or setter that can make a domain value invalid.
Record a new variant in a public exhaustive enum as a breaking API change.

<a id="reas-rust-002"></a>
## REAS-RUST-002: Select abstractions for the reader

Select an abstraction that makes the code path easy to follow.
Then measure its runtime cost.
A zero-cost abstraction adds no runtime cost compared with equivalent lower-level code.
The term does not describe readability or compilation time.

Generics and `impl Trait` do not add dynamic dispatch by themselves.
Generic code can also call a trait object.
A call to a statically known implementation uses static dispatch.
Trait objects use dynamic dispatch.

A new `Box<T>` allocates storage when `T` has nonzero size.
Conversion from `Box<T>` to `Box<dyn Trait>` does not add an allocation.
A reference to a trait object does not allocate storage for the value.
Each `Clone` implementation has its own cost.
For an abstraction with extra runtime cost, explain why it is necessary.
Do not remove an abstraction only to decrease compilation time.

<a id="reas-rust-003"></a>
## REAS-RUST-003: Make trait relationships explicit

Use a blanket implementation only when its behavior is correct for all types that meet its bounds.
Examine coherence and orphan rules before you add an implementation.
Record implementation conflicts that a future adapter could cause.
Do not build a generic repository framework before a second concrete implementation exists.

Do not give an inherent method the same name as a trait method on that type.
For the same candidate receiver type, Rust searches inherent methods before trait methods.
Automatic borrowing and dereferencing affect the candidate order.
A trait method can have priority.

<a id="reas-rust-004"></a>
## REAS-RUST-004: Make ownership explicit

Make responsibility for each owned value easy to find.
End each borrow after its last necessary use.
Use `Arc<Mutex<T>>` only for shared ownership and synchronized mutation.
If lifetime parameters obscure the data relationships, first examine a different data model.
Record the reason when the current model is necessary.

Do not use `unsafe` to violate aliasing or lifetime requirements.
The borrow checker also does its checks on references in an `unsafe` block.
When safe Rust cannot express an operation, isolate the unsafe implementation.
Document its safety conditions as [REAS-SEC-001](validation.md#reas-sec-001) specifies.

<a id="reas-rust-005"></a>
## REAS-RUST-005: Handle errors explicitly

Return `Result` with a typed error when the caller must handle a failure.
Use `?` to propagate an error that the current function cannot handle.
An inbound adapter can convert an error to a response or message after it handles that error.
Do not return success after a failure.
For a CLI program, return a failure exit status when the command fails.

Panic only for a bug.
For example, an invariant violation after validation can be a bug.
Do not call `unwrap` when `None` or `Err` can occur.
When failure is not possible at that point, use `expect` with the reason.
Do not substitute a default for a missing required input.

<a id="reas-rust-006"></a>
## REAS-RUST-006: Compile after each implementation step

Compile after each change to a type, function, or module.
When Cargo is available, run these commands from the workspace root:

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For packages outside a shared workspace, use `--manifest-path` for each package.
Report only the checks that ran.
If the hosted build cannot run Cargo, compile locally.
Do not make compilation speed a design objective.

<a id="reas-rust-007"></a>
## REAS-RUST-007: Separate async work and CPU work

Do not run blocking I/O or CPU-intensive work on an async executor thread.
Use `tokio::task::spawn_blocking`, a Rayon pool, or a bounded worker pool.
Set a limit on concurrent CPU work before submission.
Do not hold a `std::sync::Mutex` guard across an `.await`.

Record how each task responds to cancellation and shutdown.
A started `spawn_blocking` task does not stop when its handle receives `abort`.
For work that must stop, use cooperative cancellation.
Set a shutdown deadline.
Record the shutdown behavior for unfinished work after that deadline.
