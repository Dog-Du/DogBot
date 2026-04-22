#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"
env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"
dogbot_require_env QQ_ADAPTER_QQ_BOT_ID

log_dir="${QQ_ADAPTER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"
pid_file="${QQ_ADAPTER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/qq-adapter.pid}"

mkdir -p "$log_dir"

host="${QQ_ADAPTER_HOST:-0.0.0.0}"
port="${QQ_ADAPTER_PORT:-19000}"
agent_runner_base_url="${QQ_ADAPTER_AGENT_RUNNER_BASE_URL:-${AGENT_RUNNER_BIND_ADDR:-http://127.0.0.1:8787}}"
if [[ "$agent_runner_base_url" != http* ]]; then
  agent_runner_base_url="http://${agent_runner_base_url}"
fi
agent_runner_base_url="${agent_runner_base_url/host.docker.internal/127.0.0.1}"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "qq-adapter already running with pid $(cat "$pid_file")"
  exit 0
fi

if existing_pid="$(dogbot_find_listener_pid "$port")" && [[ -n "${existing_pid:-}" ]]; then
  echo "$existing_pid" >"$pid_file"
  echo "qq-adapter already listening on :$port with pid $existing_pid"
  exit 0
fi

if ! uv_bin="$(dogbot_resolve_uv_bin)"; then
  exit 1
fi

(
  cd "$repo_root"
  export QQ_ADAPTER_BIND_ADDR="${QQ_ADAPTER_BIND_ADDR:-$host:$port}"
  export QQ_ADAPTER_QQ_BOT_ID="${QQ_ADAPTER_QQ_BOT_ID:-}"
  export QQ_ADAPTER_STATUS_COMMAND_NAME="${QQ_ADAPTER_STATUS_COMMAND_NAME:-agent-status}"
  export QQ_ADAPTER_DEFAULT_CWD="${QQ_ADAPTER_DEFAULT_CWD:-/workspace}"
  export QQ_ADAPTER_TIMEOUT_SECS="${QQ_ADAPTER_TIMEOUT_SECS:-120}"
  export AGENT_RUNNER_BASE_URL="${agent_runner_base_url}"
  export NAPCAT_API_BASE_URL="${NAPCAT_API_BASE_URL:-http://127.0.0.1:3001}"
  export NAPCAT_ACCESS_TOKEN="${NAPCAT_ACCESS_TOKEN:-}"

  exec setsid "$uv_bin" run --with fastapi --with uvicorn --with httpx --with websockets python -m uvicorn \
    qq_adapter.app:create_app \
    --factory \
    --host "$host" \
    --port "$port"
) >>"$log_dir/qq-adapter.log" 2>&1 < /dev/null &

if ! listener_pid="$(dogbot_wait_for_listener_pid "$port" 30)"; then
  echo "qq-adapter failed to start. See $log_dir/qq-adapter.log" >&2
  rm -f "$pid_file"
  exit 1
fi

echo "$listener_pid" >"$pid_file"
echo "Started qq-adapter with pid $listener_pid"
