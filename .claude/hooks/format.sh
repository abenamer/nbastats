#!/bin/bash
# PostToolUse hook on Edit|Write: format the crate after a Rust file changes, so formatting
# never depends on the session remembering `cargo fmt`. Never blocks: a formatter failure
# is reported by the Stop hook's check instead.
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

case "$FILE_PATH" in
  *.rs)
    cd "$CLAUDE_PROJECT_DIR" 2>/dev/null || exit 0
    cargo fmt --all >/dev/null 2>&1
    ;;
esac

exit 0
