---
type: Technical Reference
title: Documentation checker
description: Supported syntax and limits of the REAS documentation checker.
tags: [documentation, validation, tooling]
---

# Documentation checker

Run the checker from the repository root:

```bash
python -m pip install -r requirements-docs.txt
python scripts/check_docs.py
python -m unittest discover -s tests -v
```

## Supported checks

The checker reads `.md` files below the repository root.
It excludes `.git`, `target`, and `.venv` directories.
It parses CommonMark, including inline links, reference links, images, and ATX or Setext headings.
It also reads HTML `href`, `src`, `id`, and anchor `name` attributes.
Code blocks and code spans do not declare links or rules.

| Area | Check |
| --- | --- |
| Concept frontmatter | YAML mapping with unique keys and a non-empty `type` string. |
| Bundle index | Root `index.md` with only `okf_version: "0.2"` in its frontmatter. |
| Navigation | Each concept is reachable from the root index through links within the bundle. |
| Nested indexes | No frontmatter. |
| Logs | No frontmatter. Level-two date headings use valid `YYYY-MM-DD` dates. |
| Rule declarations | A heading that starts with `REAS-` must contain an uppercase family and a three-digit number. |
| Rule uniqueness | No duplicate declarations in the scanned Markdown files. |
| Local destinations | Referenced files and directories exist inside the repository. |
| Local fragments | A referenced Markdown heading or explicit HTML anchor exists. |
| Event metadata | `generated` and `verified` events have an actor and a datetime with a UTC offset. |
| Source metadata | Each `sources` entry has a non-empty `resource` string. |

For a directory link, fragment and navigation checks use `index.md`, then `README.md`, if either exists.
Heading fragments use lowercase text, hyphens for spaces, and numeric suffixes for duplicate headings.
Punctuation does not become part of the heading fragment.

In bundle documents, a path that starts with `/` is relative to `standard/`.
In documents outside the bundle, that path is relative to the repository root.
A path must remain inside the repository after resolution.

## Limits

This checker validates a REAS profile of OKF, not the complete OKF specification.
The profile requires an index and valid local destinations even where OKF permits unfinished knowledge.
It also requires `at` for each generated or verified event.
It does not establish the truth of metadata or source claims.

The parser does not implement GitHub extensions such as wiki links or generated footnote anchors.
The fragment algorithm does not reproduce every GitHub rendering extension.
Use explicit HTML anchors for stable rule links.
The checker does not fetch external URLs, including URLs in metadata.
It does not compare rule identifiers with repository history.
It does not validate the full STE vocabulary, grammar, or technical meaning.
