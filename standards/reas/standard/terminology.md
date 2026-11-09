---
type: Technical Reference
title: REAS terminology
description: Software terms with a specific meaning in REAS.
tags: [terminology, rust, ste]
---

# REAS terminology

This table gives the meanings of terms that affect a REAS rule.
It is not the ASD-STE100 dictionary.
The terms use established software meanings, with explicit REAS conventions where necessary.
The STE language review must examine their use in context.

## Technical nouns

| Term | Meaning |
| --- | --- |
| Adapter | Code that converts between an external interface and the application. An adapter can be a function, module, or concrete type. |
| Aliasing | Access to the same memory through more than one reference or pointer. Rust places conditions on such access. |
| Application use case | An application operation that coordinates domain behavior and ports. REAS also calls this a use case. |
| Async executor | A runtime component that polls futures and schedules tasks. |
| Authentication | Establishment of a caller's identity. |
| Authorization | A decision that controls a caller's access to an operation. |
| Blanket implementation | A trait implementation that covers all types in its stated bounds, for example `impl<T: Display> Describe for T`. |
| Blocking work | Work that occupies an executor thread without returning control to the executor. This includes blocking I/O and CPU-intensive work. |
| Borrow | Temporary access to a value through a shared or exclusive reference. A borrow does not transfer ownership of the value. |
| Cancellation | A request or mechanism to stop work before completion. Each task implementation supplies its cancellation behavior. |
| Cargo | The Rust package and build tool. A Cargo package can contain more than one crate. |
| Coherence | Rules that reject overlapping trait implementations and implementations that fail the orphan rules. Trait arguments are part of the implementation identity. |
| Composition root | Code that constructs adapters and connects them to use cases. Each executable has one composition root. |
| Conditional update | A database update with a condition on the expected stored state or version. The caller must examine the affected-row count. |
| Crate | A Rust compilation unit, such as a library or binary target. |
| Database constraint | A database rule for stored values, such as a unique key, foreign key, or check constraint. |
| Domain | Types, invariants, and business rules that do not depend on infrastructure. |
| Dyn-compatible trait | A trait that can serve as the base trait of a trait object under Rust's dyn-compatibility rules. |
| Dynamic dispatch | Selection of a call target at run time. Trait-object calls use a vtable for this selection. |
| Endpoint | An external entry point to an application, such as an HTTP route. |
| Enum | A Rust type with a declared set of variants. Each value has one variant. |
| Forwarding layer | Code that only delegates a call and adds no separate responsibility. A declared application boundary can delegate directly. |
| Hosted build | A build in a CI service or remote development environment. |
| Human-maintained code | Code that persons read, change, and review. REAS-ARCH-003 describes architecture exceptions for private applications. |
| Idempotency | The property that a repeated operation has the same intended effect as one execution. |
| Inbound adapter | Code that receives external input and calls a use case. Examples include an HTTP handler and a CLI command function. |
| Inherent method | A method declared in an `impl Type` block, rather than an implementation of a trait. |
| Invariant | A condition that a valid domain value or operation must satisfy. |
| Lifetime | A region for which a reference must remain valid. Rust can infer lifetimes. A lifetime has no runtime representation. |
| Lifetime parameter | A generic parameter, such as `'a`, that expresses a lifetime relationship in a type or function signature. |
| Lost update | A write that replaces a different writer's change without detecting that change. |
| Migration | A versioned change to a database schema. |
| Newtype | A new type that wraps a different type, for example `struct TeamId(u32)`. |
| Non-exhaustive enum | An enum with a declaration that permits future variants without a breaking API change. Downstream matches must include an unknown-case arm. |
| Offline query metadata | SQLx data in `.sqlx/` that lets checked-query macros compile without a live database. It does not prove migration consistency. |
| OKF concept | A non-reserved Markdown document in an OKF knowledge bundle. Its frontmatter includes a non-empty `type` string. |
| OKF knowledge bundle | A directory of linked Markdown documents that follows Open Knowledge Format. In this repository, the bundle is `standard/`. |
| Orphan rules | Restrictions on trait implementations across crates. For a foreign trait, a local type must occur in `Self` or a trait argument. Other restrictions apply to uncovered type parameters before the first local type. |
| Outbound adapter | An implementation of a port for an external dependency. An in-memory adapter can substitute for that dependency in tests. |
| Ownership | Rules for moves and responsibility for dropping values. `Rc` and `Arc` provide shared ownership of an allocation. |
| Parameterized query | A SQL query with parameters for data values. Parameters do not replace SQL identifiers. |
| Port | A trait that the application declares for a dependency. REAS uses port for the outbound side and use case for the inbound side. |
| Ports and adapters | Architecture that separates application behavior from external systems through explicit boundaries. Hexagonal architecture is an established alias. |
| Property test | A test of a stated invariant with generated inputs. |
| Pure function | A function with no side effects. The same arguments give the same result. |
| Row lock | A database lock that coordinates access to a row until the transaction ends. |
| Schema | The database structure, including tables, columns, types, and constraints. |
| Sealed trait | A public trait that downstream crates cannot implement. One pattern uses a supertrait in a private module, without implementations for external types. |
| Shutdown | The process that stops new work before the application exits. It completes, cancels, or discards existing work. |
| Simulation | A test that executes a model with a sequence of events. A recorded random seed can help reproduce a failure. |
| Smart constructor | A function that validates input before it returns a domain value. All safe public construction paths must preserve the same invariants. |
| SQLx | An async Rust SQL library. Its checked-query macros can use a database or saved offline metadata at compile time. |
| Stale read | A read with a result that does not match the state used by the next operation. |
| State machine | A finite set of states and permitted transitions, with exactly one current state. Finite-state machine is an established alias. |
| Static dispatch | A call with a target that the compiler selects at compile time. Monomorphization specializes generic code for concrete instantiations. |
| Test double | An implementation that a test controls, such as a failing repository or an in-memory adapter. |
| Trait | A Rust interface of associated functions, types, or constants. A type can implement the trait. |
| Trait object | A dynamically sized value accessed through a dyn-compatible base trait, with optional auto-trait and lifetime bounds. Its pointers include dispatch metadata. |
| Transaction | A database boundary for operations that must commit together or roll back together. A local transaction does not include remote side effects. |
| Typed error | An error struct or enum with variants or fields that the caller can examine. REAS excludes type-erased `Box<dyn Error>` and `anyhow::Error`. |
| Typestate | Representation of a value's state in its type. State-specific APIs can reject invalid operations at compile time. |
| Variant | One case of an enum, optionally with associated data. |
| Wildcard arm | A match arm with `_` that accepts all remaining cases. |
| YAML frontmatter | A YAML metadata block between `---` delimiters at the start of a Markdown document. |
| Zero-cost abstraction | An abstraction with no additional runtime cost compared with equivalent lower-level code. The term does not describe readability or compilation time. |

Ownership does not guarantee execution of all destructors.
For example, `ManuallyDrop` suppresses automatic destruction of its contents.
Process termination can also skip destructors.
The Rust Reference states the complete drop and trait-implementation rules.

## Technical verbs

These terms describe software operations.
The list records their intended meaning; it does not establish STE approval in every sentence.

| Verb | Software meaning |
| --- | --- |
| Allocate | Obtain storage for a value. |
| Borrow | Create a reference without moving ownership of the referenced value. |
| Compile | Translate source code and perform the compiler's required checks. |
| Deserialize | Convert an external data representation into program values. |
| Dispatch | Select and invoke an implementation for a call. |
| Drop | Perform Rust's destruction operation for a value. |
| Implement | Supply the associated items of a trait for a type. |
| Instantiate | Substitute concrete arguments into a generic definition. |
| Monomorphize | Specialize generic code for concrete type and const arguments during compilation. |
| Propagate | Return an error from a called operation without local error handling. |
| Serialize | Convert program values into an external data representation. |
| Validate | Test data or structure against specified constraints. |

Use ports and adapters, state machine, and application use case as the preferred terms in new rule text.
Use established aliases only to explain terminology or preserve a historical reference.
