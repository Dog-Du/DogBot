#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
start_script="$repo_root/scripts/start_agent_runner.sh"

patterns=(
  'DOGBOT_CLAUDE_PROMPT_ROOT="${DOGBOT_CLAUDE_PROMPT_ROOT:-'
  'HISTORY_DB_PATH="${HISTORY_DB_PATH:-'
)

for pattern in "${patterns[@]}"; do
  if ! grep -q "$pattern" "$start_script"; then
    echo "FAIL: start_agent_runner.sh must export $pattern into agent-runner" >&2
    exit 1
  fi
done

if ! grep -q 'mkdir -p "$AGENT_WORKSPACE_DIR" "$AGENT_STATE_DIR" "$log_dir" "$claude_prompt_root"' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must prepare DOGBOT_CLAUDE_PROMPT_ROOT before launch" >&2
  exit 1
fi

if grep -q 'DOGBOT_CONTENT_ROOT' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must not export legacy DOGBOT_CONTENT_ROOT" >&2
  exit 1
fi

echo "start_agent_runner claude prompt env checks passed."
