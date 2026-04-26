#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"
env_file="$(dogbot_resolve_env_file "${1:-}")"

resolve_rust_user_home() {
  local rust_user
  rust_user="${SUDO_USER:-$USER}"
  getent passwd "$rust_user" | cut -d: -f6
}

resolve_cargo_bin() {
  if command -v cargo >/dev/null 2>&1; then
    command -v cargo
    return 0
  fi

  local rust_user_home
  rust_user_home="$(resolve_rust_user_home)"
  if [[ -n "$rust_user_home" && -x "$rust_user_home/.cargo/bin/cargo" ]]; then
    echo "$rust_user_home/.cargo/bin/cargo"
    return 0
  fi

  return 1
}

dogbot_load_env_file "$env_file"

if ! cargo_bin="$(resolve_cargo_bin)"; then
  echo "cargo not found. Install Rust toolchain for the current user first." >&2
  exit 1
fi

rust_user_home="$(resolve_rust_user_home)"
if [[ -n "$rust_user_home" ]]; then
  export CARGO_HOME="${CARGO_HOME:-$rust_user_home/.cargo}"
  export RUSTUP_HOME="${RUSTUP_HOME:-$rust_user_home/.rustup}"
  export PATH="$CARGO_HOME/bin:$PATH"
fi

log_dir="${AGENT_RUNNER_LOG_DIR:-$AGENT_STATE_DIR/logs}"
pid_file="${AGENT_RUNNER_PID_FILE:-$AGENT_STATE_DIR/agent-runner.pid}"
claude_prompt_root="${DOGBOT_CLAUDE_PROMPT_ROOT:-$repo_root/claude-prompt}"
claude_runner_runtime_dir="${DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR:-$(dogbot_claude_runner_runtime_dir)}"
bind_addr="${AGENT_RUNNER_BIND_ADDR:-0.0.0.0:8787}"
bind_port="$(dogbot_bind_addr_port "$bind_addr")"
healthz_url="http://127.0.0.1:${bind_port}/healthz"

mkdir -p "$AGENT_WORKSPACE_DIR" "$AGENT_STATE_DIR" "$log_dir" "$claude_prompt_root" "$claude_runner_runtime_dir"
dogbot_write_claude_runner_runtime "$claude_runner_runtime_dir"

if [[ -f "$pid_file" ]]; then
  existing_pid="$(cat "$pid_file")"
  if kill -0 "$existing_pid" >/dev/null 2>&1; then
    existing_comm="$(ps -p "$existing_pid" -o comm= 2>/dev/null | tr -d '[:space:]')"
    if [[ "$existing_comm" == "agent-runner" ]] && dogbot_wait_for_http_ok "$healthz_url" 1; then
      echo "agent-runner already running with pid $existing_pid"
      exit 0
    fi
  fi
  rm -f "$pid_file"
fi

existing_listener_pid="$(dogbot_find_listener_pid "$bind_port" || true)"
if [[ -n "$existing_listener_pid" ]] && kill -0 "$existing_listener_pid" >/dev/null 2>&1; then
  existing_comm="$(ps -p "$existing_listener_pid" -o comm= 2>/dev/null | tr -d '[:space:]')"
  if [[ "$existing_comm" == "agent-runner" ]]; then
    printf '%s\n' "$existing_listener_pid" >"$pid_file"
    echo "agent-runner already listening on $bind_addr with pid $existing_listener_pid"
    exit 0
  fi

  echo "Port $bind_port is already in use by pid $existing_listener_pid ($existing_comm)." >&2
  exit 1
fi

"$cargo_bin" build --release --manifest-path "$repo_root/agent-runner/Cargo.toml"

nohup setsid env \
  BIND_ADDR="$bind_addr" \
  DEFAULT_TIMEOUT_SECS="${DEFAULT_TIMEOUT_SECS:-120}" \
  MAX_TIMEOUT_SECS="${MAX_TIMEOUT_SECS:-300}" \
  CLAUDE_CONTAINER_NAME="${CLAUDE_CONTAINER_NAME:-claude-runner}" \
  CLAUDE_IMAGE_NAME="${CLAUDE_IMAGE_NAME:-dogbot/claude-runner:local}" \
  AGENT_WORKSPACE_DIR="$AGENT_WORKSPACE_DIR" \
  AGENT_STATE_DIR="$AGENT_STATE_DIR" \
  BIFROST_PORT="${BIFROST_PORT:-8080}" \
  BIFROST_PROVIDER_NAME="${BIFROST_PROVIDER_NAME:-primary}" \
  BIFROST_MODEL="${BIFROST_MODEL:-primary/model-id}" \
  BIFROST_UPSTREAM_PROVIDER_TYPE="${BIFROST_UPSTREAM_PROVIDER_TYPE:-openai}" \
  BIFROST_UPSTREAM_BASE_URL="${BIFROST_UPSTREAM_BASE_URL:-https://example.com}" \
  BIFROST_UPSTREAM_API_KEY="${BIFROST_UPSTREAM_API_KEY:-replace-me}" \
  ANTHROPIC_BASE_URL="${ANTHROPIC_BASE_URL:-http://127.0.0.1:${BIFROST_PORT:-8080}/anthropic}" \
  ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-dummy}" \
  NAPCAT_API_BASE_URL="${NAPCAT_API_BASE_URL:-http://127.0.0.1:3001}" \
  NAPCAT_ACCESS_TOKEN="${NAPCAT_ACCESS_TOKEN:-}" \
  PLATFORM_QQ_ACCOUNT_ID="${PLATFORM_QQ_ACCOUNT_ID:-qq:bot_uin:unknown}" \
  PLATFORM_QQ_BOT_ID="${PLATFORM_QQ_BOT_ID:-}" \
  PLATFORM_WECHATPADPRO_ACCOUNT_ID="${PLATFORM_WECHATPADPRO_ACCOUNT_ID:-wechatpadpro:account:unknown}" \
  PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES="${PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES:-DogDu}" \
  MAX_CONCURRENT_RUNS="${MAX_CONCURRENT_RUNS:-10}" \
  MAX_QUEUE_DEPTH="${MAX_QUEUE_DEPTH:-20}" \
  GLOBAL_RATE_LIMIT_PER_MINUTE="${GLOBAL_RATE_LIMIT_PER_MINUTE:-10}" \
  USER_RATE_LIMIT_PER_MINUTE="${USER_RATE_LIMIT_PER_MINUTE:-3}" \
  CONVERSATION_RATE_LIMIT_PER_MINUTE="${CONVERSATION_RATE_LIMIT_PER_MINUTE:-5}" \
  SESSION_DB_PATH="${SESSION_DB_PATH:-$AGENT_STATE_DIR/runner.db}" \
  DOGBOT_CLAUDE_PROMPT_ROOT="${DOGBOT_CLAUDE_PROMPT_ROOT:-$repo_root/claude-prompt}" \
  DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR="${DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR:-$(dogbot_claude_runner_runtime_dir)}" \
  HISTORY_DB_PATH="${HISTORY_DB_PATH:-$AGENT_STATE_DIR/history.db}" \
  CLAUDE_CONTAINER_CPU_CORES="${CLAUDE_CONTAINER_CPU_CORES:-4}" \
  CLAUDE_CONTAINER_MEMORY_MB="${CLAUDE_CONTAINER_MEMORY_MB:-4096}" \
  CLAUDE_CONTAINER_DISK_GB="${CLAUDE_CONTAINER_DISK_GB:-50}" \
  CLAUDE_CONTAINER_PIDS_LIMIT="${CLAUDE_CONTAINER_PIDS_LIMIT:-256}" \
  "$repo_root/agent-runner/target/release/agent-runner" \
  >>"$log_dir/agent-runner.log" 2>&1 &

launched_pid="$!"

if ! dogbot_wait_for_http_ok "$healthz_url" 15; then
  if kill -0 "$launched_pid" >/dev/null 2>&1; then
    kill "$launched_pid" >/dev/null 2>&1 || true
    wait "$launched_pid" 2>/dev/null || true
  fi
  echo "agent-runner failed to become healthy at $healthz_url" >&2
  tail -n 80 "$log_dir/agent-runner.log" >&2 || true
  exit 1
fi

printf '%s\n' "$launched_pid" >"$pid_file"
echo "Started agent-runner with pid $launched_pid"
