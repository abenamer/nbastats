---
type: Review Procedure
title: Simplified Technical English review
description: Issue 9 review evidence, scope, and remaining language work.
tags: [documentation, ste, review]
sources:
  - id: ste9
    resource: https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf
    title: ASD-STE100 Issue 9, 2025-01-15
---

# Simplified Technical English review

**Overall result: Full ASD-STE100 compliance is not established.**

The technical corrections and the language review are separate steps.
A shorter sentence can still contain a vocabulary or grammar error.
A terminology table does not automatically approve every use of its terms.

## Review record: 2026-10-23

Reviewer: Codex, AI-assisted review.
Reference: the official ASD-STE100 Issue 9 PDF, dated 2025-01-15.
The reference has 434 PDF pages.
Its SHA-256 digest is `d1f4ea9e7cd6e46b47aa9057209f99e78c0e9cfc4e27a5b07895b05c1a166431`.
The reference was downloaded and inspected locally.
It is not copied into this repository.

### Scope

The primary language pass covers these documents:

- [Core Rust](core-rust.md).
- [Ports and adapters](ports-and-adapters.md).
- [State machines](state-machines.md).
- [Persistence](persistence.md).
- [Validation](validation.md).
- [Documentation rules](documentation.md).
- [Terminology](terminology.md).

The README, agent instructions, templates, and examples README also received plain-language edits.
They did not receive a complete dictionary review.
Code identifiers, commands, paths, rule identifiers, and diagnostic codes retain their exact spelling.
Historical changelog entries and license text are outside this language-review scope.

### Completed checks

| Area | Result | Evidence and limit |
| --- | --- | --- |
| Reference access | Passed | Official Issue 9 PDF, with the digest above. |
| Procedural sentence length | Passed for the six rule documents | A conservative count treats each code span as one token and counts other space-separated tokens. No sentence exceeds 20 tokens. This is a screening count, not a complete implementation of STE word counting. |
| Glossary sentence length | Passed for the definitions | The same screening count gives no sentence above 25 tokens. |
| Instruction structure | Reviewed and corrected | Compound instructions were divided. Conditions precede related instructions. Code-path stages use vertical lists. |
| Voice and verb form | Reviewed and corrected in the rule documents | Known-agent passive instructions were rewritten. A complete contextual grammar audit remains open. |
| Vocabulary samples | Reviewed and corrected | Dictionary entries include `check`, `test`, `consistent`, `need`, and `verify`. General verb uses of `check` and `test` were removed from rule instructions. |
| Technical terminology | Reviewed for Rust meaning | Noun and verb tables are separate. This is not a full STE category approval of each occurrence. |

The language review used Part 1 sections 1 through 9 and selected Part 2 dictionary entries.
It did not inspect every dictionary decision for every word occurrence.
The table above must not be read as a full-conformance result.

### Open checks

| Item | Required work | Current outcome |
| --- | --- | --- |
| Vocabulary and part of speech | Review every occurrence against Issue 9 rules 1.1 through 1.4 and 9.2. | Not run in full. |
| Technical nouns | Confirm the category, meaning, and form of each noun term under rules 1.5 through 1.11. | Not run in full. Software terms such as `trait object` must keep their established meaning. |
| Technical verbs | Confirm category and context under rules 1.12 and 1.13. | Not run in full. Software uses of `compile`, `return`, `dispatch`, and `implement` need contextual decisions. |
| Grammar and multi-word nouns | Complete the contextual checks in sections 2 through 4 and 9. | Not run in full. |
| Complete repository scope | Review the supporting instructions, new reference pages, and Rust doc comments against all applicable language rules. | Not run in full. |

There is no blanket exception for software words.
Adding a word to the project glossary does not waive the official language rules.
Do not report full compliance while these checks remain open.

## Procedure for the next review

1. Identify the document revision and review scope.
2. Use the official Issue 9 rules and dictionary.
3. Resolve each vocabulary occurrence by meaning and part of speech.
4. Record technical-noun and technical-verb decisions with their applicable categories.
5. Correct the text without a change to its technical meaning.
6. Do the structural checks and applicable Rust tests after the edits.
7. Record the reviewer, date, revision, findings, and remaining exceptions.
8. Report full compliance only when the complete review supports that result.

The automated documentation checker does not perform this language review.
