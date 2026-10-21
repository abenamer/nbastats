---
type: Architecture Decision
title: Bundle the dataset in the image
description: Why the game log ships inside the container image.
tags: [decision, data]
generated: { by: "human:abenamer", at: "2026-10-21T19:00:00Z" }
---

# ADR 0003 — Bundle the dataset in the image

## Context

The original FiveThirtyEight host no longer serves the game log. The application must work without internet access at run time.

## Decision

Ship `data/nba_elo_neil_paine.csv.gz` (4 MB) in the repository and the image. Import it on the first start when the database is empty. Keep refresh and upload available for updates.

## Consequences

* The first start works offline.
* The repository grows by 4 MB. A new dataset version is a commit.
