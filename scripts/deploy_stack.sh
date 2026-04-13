#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="${1:-$repo_root/deploy/myqqbot.env}"

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
  echo "Copy deploy/myqqbot.env.example to deploy/myqqbot.env and edit it first." >&2
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

mkdir -p \
  "${AGENT_WORKSPACE_DIR:-/srv/agent-workdir}" \
  "${AGENT_STATE_DIR:-/srv/agent-state}" \
  "${NAPCAT_QQ_DIR:-/srv/napcat/qq}" \
  "${NAPCAT_CONFIG_DIR:-/srv/napcat/config}" \
  "${ASTRBOT_DATA_DIR:-/srv/astrbot/data}" \
  "${AGENT_RUNNER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"

"$repo_root/scripts/start_agent_runner.sh" "$env_file"

if [[ "$compose_cmd" == "docker compose" ]]; then
  docker compose --env-file "$env_file" -f "$repo_root/compose/docker-compose.yml" up -d claude-runner
  docker compose --env-file "$env_file" -f "$repo_root/compose/platform-stack.yml" up -d napcat astrbot
else
  docker-compose --env-file "$env_file" -f "$repo_root/compose/docker-compose.yml" up -d claude-runner
  docker-compose --env-file "$env_file" -f "$repo_root/compose/platform-stack.yml" up -d napcat astrbot
fi

if [[ "${APPLY_NETWORK_POLICY:-1}" == "1" ]]; then
  if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
    API_PROXY_PORT="${API_PROXY_PORT:-9000}" \
      CLAUDE_CONTAINER_NAME="${CLAUDE_CONTAINER_NAME:-claude-runner}" \
      "$repo_root/scripts/apply_runner_network_policy.sh"
  else
    echo "Applying runner network policy via sudo..."
    sudo \
      API_PROXY_PORT="${API_PROXY_PORT:-9000}" \
      CLAUDE_CONTAINER_NAME="${CLAUDE_CONTAINER_NAME:-claude-runner}" \
      "$repo_root/scripts/apply_runner_network_policy.sh"
  fi
fi

echo "Deployment finished."
echo "NapCat WebUI: http://127.0.0.1:${NAPCAT_WEBUI_PORT:-6099}"
echo "AstrBot WebUI: http://127.0.0.1:${ASTRBOT_WEBUI_PORT:-6185}"
