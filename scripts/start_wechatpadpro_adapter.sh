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

log_dir="${WECHATPADPRO_ADAPTER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"
pid_file="${WECHATPADPRO_ADAPTER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/wechatpadpro-adapter.pid}"

mkdir -p "$log_dir"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "wechatpadpro-adapter already running with pid $(cat "$pid_file")"
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

host="${WECHATPADPRO_ADAPTER_HOST:-127.0.0.1}"
port="${WECHATPADPRO_ADAPTER_PORT:-18999}"

nohup env \
  WECHATPADPRO_ADAPTER_BIND_ADDR="${WECHATPADPRO_ADAPTER_BIND_ADDR:-$host:$port}" \
  WECHATPADPRO_BASE_URL="${WECHATPADPRO_BASE_URL:-http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}}" \
  WECHATPADPRO_ACCOUNT_KEY="${WECHATPADPRO_ACCOUNT_KEY:-}" \
  WECHATPADPRO_ADAPTER_SHARED_TOKEN="${WECHATPADPRO_ADAPTER_SHARED_TOKEN:-}" \
  WECHATPADPRO_WEBHOOK_SECRET="${WECHATPADPRO_WEBHOOK_SECRET:-}" \
  WECHATPADPRO_ADAPTER_WEBHOOK_URL="${WECHATPADPRO_ADAPTER_WEBHOOK_URL:-http://host.docker.internal:${WECHATPADPRO_ADAPTER_PORT:-18999}/wechatpadpro/events}" \
  WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK="${WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK:-0}" \
  WECHATPADPRO_DEFAULT_CWD="${WECHATPADPRO_DEFAULT_CWD:-/workspace}" \
  WECHATPADPRO_DEFAULT_TIMEOUT_SECS="${WECHATPADPRO_DEFAULT_TIMEOUT_SECS:-120}" \
  AGENT_RUNNER_BASE_URL="${AGENT_RUNNER_BASE_URL:-http://127.0.0.1:11451}" \
  "$uv_bin" run --with fastapi --with uvicorn --with httpx python -m uvicorn \
  wechatpadpro_adapter.app:create_app \
  --factory \
  --host "$host" \
  --port "$port" \
  >>"$log_dir/wechatpadpro-adapter.log" 2>&1 &

echo $! >"$pid_file"
echo "Started wechatpadpro-adapter with pid $(cat "$pid_file")"
