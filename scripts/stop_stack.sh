#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"

keep_qq=0
keep_wechat=0
env_file_override=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-qq)
      keep_qq=1
      shift
      ;;
    --keep-wechat)
      keep_wechat=1
      shift
      ;;
    --env-file)
      env_file_override="$2"
      shift 2
      ;;
    -*)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
    *)
      if [[ -z "$env_file_override" ]]; then
        env_file_override="$1"
      else
        echo "Unexpected argument: $1" >&2
        exit 1
      fi
      shift
      ;;
  esac
done

env_file="$(dogbot_resolve_env_file "${env_file_override:-}")"
dogbot_load_env_file "$env_file"
runtime_state_file="$(dogbot_runtime_state_file)"
dogbot_load_runtime_state_if_present "$runtime_state_file"
compose_project_name="${DOGBOT_COMPOSE_PROJECT_NAME:-dogbot}"

if ! compose_cmd="$(dogbot_resolve_compose_cmd)"; then
  echo "Docker Compose is not available." >&2
  echo "Install 'docker compose' plugin or 'docker-compose' first." >&2
  exit 1
fi

if [[ "$compose_cmd" == "docker compose" ]]; then
  if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" && "$keep_wechat" != "1" ]]; then
    docker compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/wechatpadpro-stack.yml" down
  fi
  if [[ "${ENABLE_QQ:-1}" == "1" && "$keep_qq" != "1" ]]; then
    docker compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/platform-stack.yml" down
  fi
  docker compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/docker-compose.yml" down
else
  if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" && "$keep_wechat" != "1" ]]; then
    docker-compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/wechatpadpro-stack.yml" down
  fi
  if [[ "${ENABLE_QQ:-1}" == "1" && "$keep_qq" != "1" ]]; then
    docker-compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/platform-stack.yml" down
  fi
  docker-compose --project-name "$compose_project_name" --project-directory "$repo_root" --env-file "$env_file" -f "$repo_root/deploy/docker/docker-compose.yml" down
fi

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

rm -f "$runtime_state_file"

echo "Stack stopped."
