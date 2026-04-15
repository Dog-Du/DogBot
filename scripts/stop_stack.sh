#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
default_env_file="$repo_root/deploy/dogbot.env"
legacy_env_file="$repo_root/deploy/myqqbot.env"
if [[ $# -ge 1 ]]; then
  env_file="$1"
elif [[ -f "$default_env_file" ]]; then
  env_file="$default_env_file"
else
  env_file="$legacy_env_file"
fi

resolve_compose_cmd() {
  if docker compose version >/dev/null 2>&1; then
    echo "docker compose"
    return 0
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    echo "docker-compose"
    return 0
  fi

  return 1
}

if [[ ! -f "$env_file" ]]; then
  echo "Missing env file: $env_file" >&2
  exit 1
fi

set -a
source "$env_file"
set +a

if ! compose_cmd="$(resolve_compose_cmd)"; then
  echo "Docker Compose is not available." >&2
  echo "Install 'docker compose' plugin or 'docker-compose' first." >&2
  exit 1
fi

if [[ "$compose_cmd" == "docker compose" ]]; then
  if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
    docker compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" down
  fi
  docker compose --env-file "$env_file" -f "$repo_root/compose/platform-stack.yml" down
  docker compose --env-file "$env_file" -f "$repo_root/compose/docker-compose.yml" down
else
  if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
    docker-compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" down
  fi
  docker-compose --env-file "$env_file" -f "$repo_root/compose/platform-stack.yml" down
  docker-compose --env-file "$env_file" -f "$repo_root/compose/docker-compose.yml" down
fi

pid_file="${AGENT_RUNNER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/agent-runner.pid}"
if [[ -f "$pid_file" ]]; then
  pid="$(cat "$pid_file")"
  if kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid"
  fi
  rm -f "$pid_file"
fi

wechatpadpro_adapter_pid_file="${WECHATPADPRO_ADAPTER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/wechatpadpro-adapter.pid}"
if [[ -f "$wechatpadpro_adapter_pid_file" ]]; then
  pid="$(cat "$wechatpadpro_adapter_pid_file")"
  if kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid"
  fi
  rm -f "$wechatpadpro_adapter_pid_file"
fi

pkill -f 'uvicorn wechatpadpro_adapter.app:create_app' >/dev/null 2>&1 || true

if [[ "${APPLY_NETWORK_POLICY:-1}" == "1" ]]; then
  if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
    "$repo_root/scripts/remove_runner_network_policy.sh"
  else
    sudo "$repo_root/scripts/remove_runner_network_policy.sh"
  fi
fi

echo "Stack stopped."
