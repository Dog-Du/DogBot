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

log_dir="${AGENT_RUNNER_LOG_DIR:-$AGENT_STATE_DIR/logs}"
pid_file="${AGENT_RUNNER_PID_FILE:-$AGENT_STATE_DIR/agent-runner.pid}"

mkdir -p "$AGENT_WORKSPACE_DIR" "$AGENT_STATE_DIR" "$log_dir"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "agent-runner already running with pid $(cat "$pid_file")"
  exit 0
fi

cargo build --release --manifest-path "$repo_root/agent-runner/Cargo.toml"

nohup env \
  BIND_ADDR="${AGENT_RUNNER_BIND_ADDR:-127.0.0.1:8787}" \
  DEFAULT_TIMEOUT_SECS="${DEFAULT_TIMEOUT_SECS:-120}" \
  MAX_TIMEOUT_SECS="${MAX_TIMEOUT_SECS:-300}" \
  CLAUDE_CONTAINER_NAME="${CLAUDE_CONTAINER_NAME:-claude-runner}" \
  CLAUDE_IMAGE_NAME="${CLAUDE_IMAGE_NAME:-myqqbot/claude-runner:local}" \
  AGENT_WORKSPACE_DIR="$AGENT_WORKSPACE_DIR" \
  AGENT_STATE_DIR="$AGENT_STATE_DIR" \
  ANTHROPIC_BASE_URL="${ANTHROPIC_BASE_URL:-http://host.docker.internal:9000}" \
  NAPCAT_API_BASE_URL="${NAPCAT_API_BASE_URL:-http://127.0.0.1:3001}" \
  NAPCAT_ACCESS_TOKEN="${NAPCAT_ACCESS_TOKEN:-}" \
  MAX_CONCURRENT_RUNS="${MAX_CONCURRENT_RUNS:-10}" \
  MAX_QUEUE_DEPTH="${MAX_QUEUE_DEPTH:-20}" \
  GLOBAL_RATE_LIMIT_PER_MINUTE="${GLOBAL_RATE_LIMIT_PER_MINUTE:-10}" \
  USER_RATE_LIMIT_PER_MINUTE="${USER_RATE_LIMIT_PER_MINUTE:-3}" \
  CONVERSATION_RATE_LIMIT_PER_MINUTE="${CONVERSATION_RATE_LIMIT_PER_MINUTE:-5}" \
  SESSION_DB_PATH="${SESSION_DB_PATH:-$AGENT_STATE_DIR/runner.db}" \
  CLAUDE_CONTAINER_CPU_CORES="${CLAUDE_CONTAINER_CPU_CORES:-4}" \
  CLAUDE_CONTAINER_MEMORY_MB="${CLAUDE_CONTAINER_MEMORY_MB:-4096}" \
  CLAUDE_CONTAINER_DISK_GB="${CLAUDE_CONTAINER_DISK_GB:-50}" \
  CLAUDE_CONTAINER_PIDS_LIMIT="${CLAUDE_CONTAINER_PIDS_LIMIT:-256}" \
  "$repo_root/agent-runner/target/release/agent-runner" \
  >>"$log_dir/agent-runner.log" 2>&1 &

echo $! > "$pid_file"
echo "Started agent-runner with pid $(cat "$pid_file")"
