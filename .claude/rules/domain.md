---
paths:
  - "src/domain/**"
---

# The domain layer

- Pure. No SQLx, Axum, reqwest, Tokio or clock reads here: a function takes slices and values and returns values. The application layer loads the data and calls it; the HTTP adapter serialises the result.
- Identifiers are the newtypes in `domain::ids` (`TeamId`, `FranchiseId`, `PlayerId`, `SeasonYear`), never a bare `i32`.
- A lifecycle or a stored label is an enum with explicit transitions. A stored text enum uses the `text_enum!` macro in `domain::models`, which gives it `as_str`, `FromStr` and `ALL`; a new variant needs the database constraint that admits it (see the migrations rule).
- A change that alters stored results or forecasts bumps the module's `MODEL_VERSION` constant, so old rows read as their own version instead of blending in.
- No `unwrap` or `expect` outside tests, apart from an invariant the code itself guarantees, with the reason in the message.
- Doc comments say why, in the voice of the module headers: what the computation means and what it assumes, not a restatement of the code.
- Unit tests live in the module, named as sentences in snake_case (`a_favourite_that_wins_returns_the_posted_payout_and_the_fair_one`), each checking a few related facts with hand-computed values. Run them with `cargo test --lib <module>`.
