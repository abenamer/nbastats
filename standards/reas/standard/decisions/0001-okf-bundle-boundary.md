---
type: Architecture Decision
title: Define the OKF bundle boundary
description: Explain why standard/ is the documentation knowledge bundle.
tags: [decision, documentation, okf]
---

# ADR 0001 — Define the OKF bundle boundary

## Context
REAS needs a machine-readable knowledge collection and ordinary repository files, including `README.md`, `CLAUDE.md`, and `AGENTS.md`.

## Decision
Treat `standard/` as the OKF v0.2 bundle. Use `standard/index.md` for navigation. Require a `type` field on concept documents below this path. Validate all local Markdown links in the repository.

## Consequences
The document validator can distinguish knowledge concepts from root repository instructions. Future contributors must keep frontmatter in standard concept documents and maintain links.

## Alternatives
Placing all repository Markdown in one OKF bundle was rejected because agent instructions and project landing pages have different functions.
