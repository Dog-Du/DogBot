#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
start_script="$repo_root/scripts/start_agent_runner.sh"

patterns=(
  'DOGBOT_CONTENT_ROOT="${DOGBOT_CONTENT_ROOT:-'
  'CONTROL_PLANE_DB_PATH="${CONTROL_PLANE_DB_PATH:-'
  'HISTORY_DB_PATH="${HISTORY_DB_PATH:-'
  'DOGBOT_ADMIN_ACTOR_IDS="${DOGBOT_ADMIN_ACTOR_IDS:-}'
)

for pattern in "${patterns[@]}"; do
  if ! grep -q "$pattern" "$start_script"; then
    echo "FAIL: start_agent_runner.sh must export $pattern into agent-runner" >&2
    exit 1
  fi
done

if ! grep -q 'mkdir -p "$AGENT_WORKSPACE_DIR" "$AGENT_STATE_DIR" "$log_dir" "$content_root"' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must prepare DOGBOT_CONTENT_ROOT before launch" >&2
  exit 1
fi

echo "start_agent_runner content env checks passed."
