---
type: Engineering Standard
title: State machines and typestate
description: Runtime states, type-based states, and persisted transitions.
tags: [rust, fsm, typestate]
---

# State machines and typestate

<a id="reas-state-001"></a>
## REAS-STATE-001: Model runtime states

Use a finite-state machine for mutually exclusive workflow states.
Use one enum for the runtime state.
Do not encode that state with several Boolean flags.
Write each legal transition as a function.
Return a typed error when a runtime state does not permit the transition.
Match all variants of the state enum without a wildcard arm.

<a id="reas-state-002"></a>
## REAS-STATE-002: Reject invalid operations with typestate

Use typestate when operation order or resource initialization is an invariant.
Expose an operation only on the state types that permit it.
Bound the state parameter with a sealed trait when downstream crates must not add states.
Use one type parameter for each independent state dimension.
If the dimensions are not independent, use one state type or runtime enum.

Validate state from a file, database, or network at run time.
Use a runtime enum to select the state-specific operation.
After validation, you can construct a typestate value for later operations.
Typestate does not replace validation of external state.

<a id="reas-state-003"></a>
## REAS-STATE-003: Guard persisted transitions

A persisted state can change between a read and a write.
Use a conditional update or a row lock for each transition.
For a conditional update, include the record identifier and expected state in the condition.
For example, use `UPDATE jobs SET state = $1 WHERE id = $2 AND state = $3`.
Report success only when the update changes one row.
Report a conflict or missing record when the update changes no rows.

Use a version column when the operation must detect intervening changes.
This includes changes that return to the same state.
For a row lock, read the state with `SELECT ... FOR UPDATE`.
Make the transition decision after that read.
Write the new state in the same transaction.
Keep the lock until the transaction ends.
Typestate alone cannot prevent a stale read or a lost update.

The [typestate example](../examples/typestate/) demonstrates in-process state restrictions.
It does not demonstrate persisted transitions.
