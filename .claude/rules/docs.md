---
paths:
  - "docs/**"
  - "CHANGELOG.md"
  - "README.md"
---

# The documentation bundle

- `docs/` is an Open Knowledge Format bundle. Every file under `concepts/`, `runbooks/`, `reference/` and `decisions/` starts with YAML front matter: `type`, `title`, `description`, `tags`, `generated`, and `sources` that point at the code or migrations it describes. Links between bundle files are bundle-relative with a leading slash (`/concepts/elo-model.md`). `docs/index.md` lists every file.
- Every change to the application gets, the same day: an entry at the top of [docs/log.md](../../docs/log.md) under today's date (`* **New**:` or `* **Update**:`, naming the files changed, ending "Noted in the changelog."), and a bullet at the top of the current version's list in `CHANGELOG.md`. A concept or reference that the change touches is updated in the same commit.
- A new model, rule or external dependency gets an Architecture Decision Record in `docs/decisions/` (Context, Decision, Consequences, Alternatives, and a Runs table when it is measured), and a line in `docs/index.md`.
- Use the vocabulary of [docs/terminology.md](../../docs/terminology.md); add a row for a new term, in the table's order.
- The text targets ASD-STE100 Simplified Technical English: short sentences, one instruction per sentence, the same word for the same thing. Never claim verified compliance; the [self-audit](../../docs/self-audit.md) says what was checked.
- Record a measurement with its date and the store it was run on; the bundle's numbers are the runs recorded in the decision records.
- Before you finish: `python3 scripts/check_docs.py` and `python3 -m unittest discover -s tests/docs -t .`.
