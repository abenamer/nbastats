#!/bin/bash
# PreToolUse hook on Bash: refuse the commands that must never run here, whatever the
# session decided. Exit 2 blocks the command and sends the message to Claude as the reason.
#   - `docker compose down -v` (or --volumes) deletes the named volumes with every game,
#     job and result; the upgrade runbook forbids it for routine work.
#   - A forced push to main rewrites the branch CI deploys from.
# A pattern matches only in command position (the start of a line, or after ; && || | or an
# opening parenthesis), so a commit message or an echo that mentions the command passes.
INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
AT_COMMAND='(^|[;&|(]|\$\()[[:space:]]*(sudo[[:space:]]+)?'

if echo "$COMMAND" | grep -Eq "${AT_COMMAND}docker([[:space:]]+compose|-compose)[^|;&]*[[:space:]]down([[:space:]][^|;&]*)?[[:space:]](-v|--volumes)([[:space:]);&|]|$)"; then
  echo "Blocked: 'docker compose down -v' deletes the data volumes. Use 'docker compose up -d --build' to upgrade, or 'docker compose down' without -v to stop (see docs/runbooks/upgrade.md)." >&2
  exit 2
fi

if echo "$COMMAND" | grep -Eq "${AT_COMMAND}git[[:space:]]+push[^|;&]*[[:space:]](--force|-f|--force-with-lease)([[:space:]]|=|$)"; then
  echo "Blocked: a forced push rewrites the branch CI deploys from. Make a new commit instead." >&2
  exit 2
fi

exit 0
