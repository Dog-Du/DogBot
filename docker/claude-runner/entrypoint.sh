#!/usr/bin/env bash
set -euo pipefail

sudo /usr/local/bin/claude-bootstrap.sh
launch_script="${DOGBOT_CLAUDE_RUNNER_LAUNCH_SCRIPT:-/state/claude-runner/launch.sh}"

if [[ ! -x "$launch_script" ]]; then
  echo "claude-runner launch script is missing or not executable: $launch_script" >&2
  exit 1
fi

exec "$launch_script"
