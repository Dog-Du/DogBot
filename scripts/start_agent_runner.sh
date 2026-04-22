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

mkdir -p "$AGENT_WORKSPACE_DIR" "$AGENT_STATE_DIR" "$log_dir" "$claude_prompt_root"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "agent-runner already running with pid $(cat "$pid_file")"
  exit 0
fi

"$cargo_bin" build --release --manifest-path "$repo_root/agent-runner/Cargo.toml"

nohup env \
  BIND_ADDR="${AGENT_RUNNER_BIND_ADDR:-127.0.0.1:8787}" \
  API_PROXY_BIND_ADDR="${API_PROXY_BIND_ADDR:-0.0.0.0:9000}" \
  DEFAULT_TIMEOUT_SECS="${DEFAULT_TIMEOUT_SECS:-120}" \
  MAX_TIMEOUT_SECS="${MAX_TIMEOUT_SECS:-300}" \
  CLAUDE_CONTAINER_NAME="${CLAUDE_CONTAINER_NAME:-claude-runner}" \
  CLAUDE_IMAGE_NAME="${CLAUDE_IMAGE_NAME:-dogbot/claude-runner:local}" \
  AGENT_WORKSPACE_DIR="$AGENT_WORKSPACE_DIR" \
  AGENT_STATE_DIR="$AGENT_STATE_DIR" \
  ANTHROPIC_BASE_URL="${ANTHROPIC_BASE_URL:-http://host.docker.internal:9000}" \
  API_PROXY_AUTH_TOKEN="${API_PROXY_AUTH_TOKEN:-local-proxy-token}" \
  API_PROXY_UPSTREAM_BASE_URL="${API_PROXY_UPSTREAM_BASE_URL:-}" \
  API_PROXY_UPSTREAM_TOKEN="${API_PROXY_UPSTREAM_TOKEN:-}" \
  API_PROXY_UPSTREAM_AUTH_HEADER="${API_PROXY_UPSTREAM_AUTH_HEADER:-x-api-key}" \
  API_PROXY_UPSTREAM_AUTH_SCHEME="${API_PROXY_UPSTREAM_AUTH_SCHEME:-}" \
  API_PROXY_UPSTREAM_MODEL="${API_PROXY_UPSTREAM_MODEL:-}" \
  NAPCAT_API_BASE_URL="${NAPCAT_API_BASE_URL:-http://127.0.0.1:3001}" \
  NAPCAT_ACCESS_TOKEN="${NAPCAT_ACCESS_TOKEN:-}" \
  MAX_CONCURRENT_RUNS="${MAX_CONCURRENT_RUNS:-10}" \
  MAX_QUEUE_DEPTH="${MAX_QUEUE_DEPTH:-20}" \
  GLOBAL_RATE_LIMIT_PER_MINUTE="${GLOBAL_RATE_LIMIT_PER_MINUTE:-10}" \
  USER_RATE_LIMIT_PER_MINUTE="${USER_RATE_LIMIT_PER_MINUTE:-3}" \
  CONVERSATION_RATE_LIMIT_PER_MINUTE="${CONVERSATION_RATE_LIMIT_PER_MINUTE:-5}" \
  SESSION_DB_PATH="${SESSION_DB_PATH:-$AGENT_STATE_DIR/runner.db}" \
  DOGBOT_CLAUDE_PROMPT_ROOT="${DOGBOT_CLAUDE_PROMPT_ROOT:-$repo_root/claude-prompt}" \
  HISTORY_DB_PATH="${HISTORY_DB_PATH:-$AGENT_STATE_DIR/history.db}" \
  CLAUDE_CONTAINER_CPU_CORES="${CLAUDE_CONTAINER_CPU_CORES:-4}" \
  CLAUDE_CONTAINER_MEMORY_MB="${CLAUDE_CONTAINER_MEMORY_MB:-4096}" \
  CLAUDE_CONTAINER_DISK_GB="${CLAUDE_CONTAINER_DISK_GB:-50}" \
  CLAUDE_CONTAINER_PIDS_LIMIT="${CLAUDE_CONTAINER_PIDS_LIMIT:-256}" \
  "$repo_root/agent-runner/target/release/agent-runner" \
  >>"$log_dir/agent-runner.log" 2>&1 &

echo $! > "$pid_file"
echo "Started agent-runner with pid $(cat "$pid_file")"
