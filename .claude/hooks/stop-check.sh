#!/bin/bash
# Stop hook: the checks that must pass before a turn counts as done, run only over what the
# session changed. The Rust check runs when a Rust, template, migration or Cargo file differs
# from the pushed branch; the docs check when the bundle, the README or the changelog does.
# Exit 2 blocks the stop and sends the output to Claude; stop_hook_active stops the loop
# once Claude has already been sent back.
INPUT=$(cat)
if [ "$(echo "$INPUT" | jq -r '.stop_hook_active // false')" = "true" ]; then
  exit 0
fi
cd "$CLAUDE_PROJECT_DIR" 2>/dev/null || exit 0

changed=$( { git diff --name-only HEAD 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null; } )
upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null)
if [ -n "$upstream" ]; then
  changed="$changed"$'\n'"$(git diff --name-only "$upstream...HEAD" 2>/dev/null)"
fi

rust=$(echo "$changed" | grep -E '^(src|tests|migrations|templates)/|^Cargo\.(toml|lock)$' | head -1)
docs=$(echo "$changed" | grep -E '^docs/|^README\.md$|^CHANGELOG\.md$|^scripts/check_docs\.py$' | head -1)

failed=0
if [ -n "$rust" ]; then
  cargo check --all-targets 1>&2 || failed=1
fi
if [ -n "$docs" ]; then
  python3 scripts/check_docs.py 1>&2 || failed=1
fi

if [ "$failed" = 1 ]; then
  echo "A check above failed. Fix it before finishing (cargo check --all-targets; python3 scripts/check_docs.py)." >&2
  exit 2
fi
exit 0
