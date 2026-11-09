---
type: Engineering Standard
title: Ports and adapters
description: Application boundaries and visible adapter implementations.
tags: [architecture, rust, hexagonal]
---

# Ports and adapters

<a id="reas-arch-001"></a>
## REAS-ARCH-001: Make the code path visible

Use ports and adapters for public or human-maintained applications.
For private applications, record an exception through [REAS-ARCH-003](#reas-arch-003).
For operations with external I/O, make this code path easy to follow:

1. Inbound adapter.
2. Application use case.
3. Port trait.
4. Outbound adapter.

Construct the adapters and use cases in one composition root for each executable.
A use case can delegate directly to a port when it establishes the application boundary.
Do not add a forwarding layer without a different responsibility.

<a id="reas-arch-002"></a>
## REAS-ARCH-002: Direct dependencies toward the application

Keep frameworks, database drivers, transports, and deployment tools out of domain code.
Examples include Axum, SQLx, HTTP, and Docker.
Declare the necessary ports in application code.
Use outbound adapters to implement those ports.
Use inbound adapters to convert external input for application use cases.

<a id="reas-arch-003"></a>
## REAS-ARCH-003: Select the architecture for private projects

Before you select the architecture, ask these questions:

1. Who will read and maintain the code?
2. What external systems does it use?
3. For how long must the project remain in use?
4. How likely are the boundaries to change?

Use a flat module layout for a private personal tool with no replaceable external dependency.
Keep domain behavior separate from I/O even in that layout.
For other exceptions to REAS-ARCH-001, record the answers and the selected boundaries in an architecture decision.

<a id="reas-arch-004"></a>
## REAS-ARCH-004: Keep concrete implementations visible

Declare a port trait where a test double or a replaceable implementation is necessary.
Use a port to isolate a system outside your control.
Do not add factories that build factories.
Do not use a service locator or hidden dependency registration.
Do not add duplicate interfaces or forwarding layers without a different responsibility.
Name each adapter for the system that it adapts, for example `PostgresTeamRepository`.

<a id="reas-arch-005"></a>
## REAS-ARCH-005: Separate rules from infrastructure

Write business rules and mathematical operations as pure functions.
Supply time, random values, and external data as inputs to those functions.
If a side effect is necessary for a rule, record the reason and its boundary.
Put HTTP serialization, SQLx row mapping, and third-party API clients in adapters.
Pass dependencies through constructors.

<a id="reas-arch-006"></a>
## REAS-ARCH-006: Use generic adapter parameters by default

Give a use case an adapter type parameter with a port trait bound.
For example, use `FindTeam<R: TeamRepository>`.
Here, `TeamRepository` is the port and `R` is the adapter type.
The use case's type shows the concrete adapter at the composition root.

Use a trait object for an implementation that must vary without a change to the enclosing type.
Also consider a trait object when adapter parameters appear in unrelated public APIs.
Record that decision in the composition root.
Use `Box<dyn Port>` for one owner.
Use `Arc<dyn Port + Send + Sync>` for shared ownership across threads.
`Arc` alone does not make the adapter thread-safe.

Make sure that the port is dyn-compatible before you use a trait object.
For a statically dispatched async port, use native `async fn` or an explicit future return type.
Record the `Send` requirement for returned futures.
For a dynamically dispatched async port, use an explicit boxed future with the necessary lifetime and thread bounds.
If a macro supplies this representation, record its name.

The [ports and adapters example](../examples/ports-and-adapters/) uses a synchronous port.
It includes a CLI inbound adapter, one use case, and an in-memory outbound adapter.
It does not demonstrate an async port or a database transaction.
