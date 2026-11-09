---
type: Engineering Standard
title: Validation, security, and evidence
description: Tests, security requirements, and evidence for reported results.
tags: [testing, security, evidence]
---

# Validation, security, and evidence

<a id="reas-test-001"></a>
## REAS-TEST-001: Test invariants and failures

Write unit tests for domain invariants and each state transition.
Do tests for error paths and invalid inputs.
Use a failing test double for error propagation tests through a port.
Use property tests when generated inputs can exercise an invariant.
Use simulations for behavior that depends on sequences of events.
Record the seed when a simulation fails.
Use that seed to reproduce the failure.

For a database adapter, use a container with the production database engine for integration tests.
Do not use an in-memory adapter as evidence for database behavior.
Add a compile-fail test for an attempted downstream implementation of each sealed state trait.

<a id="reas-test-002"></a>
## REAS-TEST-002: Report observable evidence

Report a check as passed only when execution evidence shows that result.
Report passed, failed, and not run as separate outcomes.
For each passed check, record the date and commit or immutable workflow run.
For evidence from a different environment, identify the source and the commands that ran.
Do not apply a passing result to code that changed after that execution.

<a id="reas-sec-001"></a>
## REAS-SEC-001: Test security properties

Validate external input before it reaches a domain operation.
Use parameterized queries for data values.
Keep credentials out of source code and logs.
Set limits on request size, connection count, and execution time.

Document each endpoint as anonymous or authenticated.
Record the reason for anonymous access.
Enforce authorization for each protected operation.
Document each trust assumption based on local access.
Do not trust a caller only because it uses a loopback address.

For each `unsafe` block, add a nearby `SAFETY` comment.
State its safety conditions and explain why they hold.
For each public unsafe function, document the caller's obligations in a `# Safety` section.
For an unsafe abstraction, do boundary-condition tests.
Record the Miri and sanitizer results, including checks that did not run.
Do a dependency vulnerability scan with `cargo audit` or `cargo deny`.
