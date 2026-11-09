---
paths:
  - "static/js/**"
  - "static/css/**"
  - "templates/**"
---

# Pages, scripts and templates

- Templates are Askama, compiled into the binary: a template error fails `cargo check --all-targets`, so run it after any template change. A page is a `Template` struct and a handler in `adapters/http/pages.rs` with `Layout::new(title, active)`, a route in `adapters/http/mod.rs`, a nav link in `templates/base.html` keyed on `layout.active`, and a row in the Pages table of `docs/concepts/application.md`.
- Scripts are plain JavaScript with no build step, one file per page, loaded with `defer` and versioned by a content hash (`?v={{ layout.assets }}`), so never bump a version by hand. Use the shared helpers on `window.NBA` from `static/js/app.js` (`table`, `cards`, `chart`, `dateScale`, `getJSON`, `esc`, `num`, `pct`, `labelOf`, `setStatus`) instead of writing new ones.
- Every value put into HTML goes through `esc`; a `table` column with `raw: true` must escape its own values.
- Chart.js is bundled (`static/js/chart.umd.js`); one chart per canvas through `NBA.chart`, which destroys the previous chart first. Charts use `responsive: true, maintainAspectRatio: false` inside a `.chart-box` or `.chart-wide` container.
- A table that polls or reloads must keep what the reader did: do not redraw on a click that changes nothing, and reopen a `<details>` that was open (the Jobs page is the pattern).
- Check a page in a headless browser before shipping it when the change is visible: Playwright is not installed system-wide, so install it in the session's scratchpad and serve the edited files over the running app's page shell.
- Run `node --check` on every script you change.
