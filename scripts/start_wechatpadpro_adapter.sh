#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
default_env_file="$repo_root/deploy/dogbot.env"
if [[ $# -ge 1 ]]; then
  env_file="$1"
else
  env_file="$default_env_file"
fi

if [[ ! -f "$env_file" ]]; then
  echo "Missing env file: $env_file" >&2
  exit 1
fi

set -a
source "$env_file"
set +a

log_dir="${WECHATPADPRO_ADAPTER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"
pid_file="${WECHATPADPRO_ADAPTER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/wechatpadpro-adapter.pid}"

mkdir -p "$log_dir"

host="${WECHATPADPRO_ADAPTER_HOST:-127.0.0.1}"
port="${WECHATPADPRO_ADAPTER_PORT:-18999}"
agent_runner_base_url="${WECHATPADPRO_AGENT_RUNNER_BASE_URL:-${AGENT_RUNNER_BASE_URL:-http://127.0.0.1:11451}}"
agent_runner_base_url="${agent_runner_base_url/host.docker.internal/127.0.0.1}"

find_listener_pid() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n1
    return 0
  fi

  ss -ltnp "( sport = :$port )" 2>/dev/null | sed -n 's/.*pid=\([0-9]\+\).*/\1/p' | head -n1
}

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "wechatpadpro-adapter already running with pid $(cat "$pid_file")"
  exit 0
fi

if existing_pid="$(find_listener_pid "$port")" && [[ -n "${existing_pid:-}" ]]; then
  echo "$existing_pid" >"$pid_file"
  echo "wechatpadpro-adapter already listening on :$port with pid $existing_pid"
  exit 0
fi

resolve_uv() {
  if command -v uv >/dev/null 2>&1; then
    command -v uv
    return 0
  fi

  if [[ -n "${SUDO_USER:-}" ]]; then
    local sudo_home
    sudo_home="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
    if [[ -n "$sudo_home" && -x "$sudo_home/.local/bin/uv" ]]; then
      echo "$sudo_home/.local/bin/uv"
      return 0
    fi
  fi

  if [[ -x "$HOME/.local/bin/uv" ]]; then
    echo "$HOME/.local/bin/uv"
    return 0
  fi

  return 1
}

if ! uv_bin="$(resolve_uv)"; then
  echo "uv not found. Install uv or make sure it is available in PATH." >&2
  exit 1
fi

(
  cd "$repo_root"
  export WECHATPADPRO_ADAPTER_BIND_ADDR="${WECHATPADPRO_ADAPTER_BIND_ADDR:-$host:$port}"
  export WECHATPADPRO_BASE_URL="${WECHATPADPRO_BASE_URL:-http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}}"
  export WECHATPADPRO_ACCOUNT_KEY="${WECHATPADPRO_ACCOUNT_KEY:-}"
  export WECHATPADPRO_ADAPTER_SHARED_TOKEN="${WECHATPADPRO_ADAPTER_SHARED_TOKEN:-}"
  export WECHATPADPRO_WEBHOOK_SECRET="${WECHATPADPRO_WEBHOOK_SECRET:-}"
  export WECHATPADPRO_ADAPTER_WEBHOOK_URL="${WECHATPADPRO_ADAPTER_WEBHOOK_URL:-http://host.docker.internal:${WECHATPADPRO_ADAPTER_PORT:-18999}/wechatpadpro/events}"
  export WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK="${WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK:-0}"
  export WECHATPADPRO_DEFAULT_CWD="${WECHATPADPRO_DEFAULT_CWD:-/workspace}"
  export WECHATPADPRO_DEFAULT_TIMEOUT_SECS="${WECHATPADPRO_DEFAULT_TIMEOUT_SECS:-120}"
  export WECHATPADPRO_COMMAND_NAME="${WECHATPADPRO_COMMAND_NAME:-agent}"
  export WECHATPADPRO_STATUS_COMMAND_NAME="${WECHATPADPRO_STATUS_COMMAND_NAME:-agent-status}"
  export AGENT_RUNNER_BASE_URL="${agent_runner_base_url}"

  exec setsid "$uv_bin" run --with fastapi --with uvicorn --with httpx python -m uvicorn \
    wechatpadpro_adapter.app:create_app \
    --factory \
    --host "$host" \
    --port "$port"
) >>"$log_dir/wechatpadpro-adapter.log" 2>&1 < /dev/null &

adapter_pid=$!
sleep 2

listener_pid="$(find_listener_pid "$port")"
if [[ -z "${listener_pid:-}" ]] || ! kill -0 "$listener_pid" >/dev/null 2>&1; then
  echo "wechatpadpro-adapter failed to start. See $log_dir/wechatpadpro-adapter.log" >&2
  rm -f "$pid_file"
  exit 1
fi

echo "$listener_pid" >"$pid_file"
echo "Started wechatpadpro-adapter with pid $listener_pid"
