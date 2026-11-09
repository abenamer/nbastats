---
paths:
  - "migrations/**"
---

# Migrations

- Migrations only add. Never drop a table or a column that holds data, and never rewrite an existing migration file: SQLx embeds the directory at compile time and checks the applied ones by checksum.
- A new file is numbered after the last one (`0011_...sql`) with a snake_case name that says what it adds. Start it with a comment that says why, in the voice of the other migrations.
- A check constraint that lists values is named `<table>_<column>_check`; to admit a value, drop that constraint and add it again with the full list (migration `0010` is the pattern).
- A data fix in a migration must set the rows to exactly what the derived rebuild (`application::imports::derived`) would set, so a later rebuild changes nothing.
- Update [docs/reference/schema.md](../../docs/reference/schema.md): the table's row and a `sources` entry for the migration. Note the migration in the changelog and the log.
