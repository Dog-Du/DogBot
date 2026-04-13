#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="${1:-$repo_root/deploy/myqqbot.env}"

if [[ ! -f "$env_file" ]]; then
  echo "Missing env file: $env_file" >&2
  exit 1
fi

set -a
source "$env_file"
set +a

docker compose --env-file "$env_file" -f "$repo_root/compose/platform-stack.yml" down
docker compose --env-file "$env_file" -f "$repo_root/compose/docker-compose.yml" down

pid_file="${AGENT_RUNNER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/agent-runner.pid}"
if [[ -f "$pid_file" ]]; then
  pid="$(cat "$pid_file")"
  if kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid"
  fi
  rm -f "$pid_file"
fi

if [[ "${APPLY_NETWORK_POLICY:-1}" == "1" ]]; then
  if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
    "$repo_root/scripts/remove_runner_network_policy.sh"
  else
    sudo "$repo_root/scripts/remove_runner_network_policy.sh"
  fi
fi

echo "Stack stopped."
