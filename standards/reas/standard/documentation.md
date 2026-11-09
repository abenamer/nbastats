---
type: Engineering Standard
title: Documentation and knowledge structure
description: ASD-STE100 language review and the REAS OKF profile.
tags: [documentation, asd-ste100, okf]
sources:
  - id: ste
    resource: https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf
    title: ASD-STE100 Issue 9
  - id: okf
    resource: https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md
    title: OKF v0.2 specification
---

# Documentation and knowledge structure

<a id="reas-doc-001"></a>
## REAS-DOC-001: Target ASD-STE100

Write procedures and reference text with the same terms and direct sentences.
Give one instruction in each procedural sentence.
Put conditions before the related instruction.
Keep explanations separate from instructions.
Use the official Issue 9 rules and dictionary for the language review.
Record its scope, findings, exceptions, and result in the [STE review record](ste-review.md).

Do not report full compliance from sentence length or a word list alone.
Do not change a code identifier to satisfy a language preference.
A technical correction and a language correction are separate review steps.

<a id="reas-doc-002"></a>
## REAS-DOC-002: Keep an OKF knowledge bundle

Use `standard/` as the knowledge bundle.
Give each concept valid YAML frontmatter with a non-empty `type` string.
Keep `index.md` at the bundle root with `okf_version: "0.2"`.
Make each concept reachable from that index through local Markdown links.
Use `index.md` and `log.md` only for their reserved purposes.

In bundle documents, resolve a path that starts with `/` against the bundle root.
In repository documents outside the bundle, resolve that path against the repository root.
Keep local link paths and heading fragments valid.
Attribute external sources.
For `generated` and `verified` events, include an actor in `by` and a timestamp with a UTC offset in `at`.
Make sure that the evidence supports each metadata claim.

<a id="reas-doc-003"></a>
## REAS-DOC-003: Use the same technical terms

Use the [terminology table](terminology.md) for terms with a Rust-specific or REAS-specific meaning.
Add a term when its meaning could change the interpretation of a rule.
Do not copy the general STE dictionary into the table.
Keep technical verbs and technical nouns in different tables.
Use established software terms.
Do not give one concept two different names without an explicit alias entry.
Write code identifiers, commands, and paths exactly as they appear in the code.

Keep each rule identifier bound to the same requirement.
Follow the [identifier policy](rule-identifiers.md) for historical references and retired rules.

<a id="reas-doc-004"></a>
## REAS-DOC-004: Update documentation with the code

Update domain, architecture, API, operation, and decision documents in the same change as the code.
Make instructions reproducible.
Give prerequisites, ordered steps, and expected results.
For a procedure that changes persistent state, give recovery steps.
Also give recovery steps when a failure can stop a procedure between its steps.

<a id="reas-doc-005"></a>
## REAS-DOC-005: Use different checks for structure and language

Run the documentation checker and its tests.
The [checker reference](checker.md) states the supported syntax and validation limits.
Keep different records for the technical review and the STE language review.
Do not use structural validity as proof of technical accuracy or STE compliance.
