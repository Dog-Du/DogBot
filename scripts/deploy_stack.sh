#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"

selected_qq=""
selected_wechat=""
explicit_selection=0
env_file_override=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --qq)
      selected_qq=1
      explicit_selection=1
      shift
      ;;
    --no-qq)
      selected_qq=0
      explicit_selection=1
      shift
      ;;
    --wechat)
      selected_wechat=1
      explicit_selection=1
      shift
      ;;
    --no-wechat)
      selected_wechat=0
      explicit_selection=1
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
if ! dogbot_require_env_file "$env_file"; then
  echo "Missing env file: $env_file" >&2
  echo "请先复制 deploy/dogbot.env.example 为 deploy/dogbot.env，并完成本地配置。" >&2
  exit 1
fi

dogbot_load_env_file "$env_file"
runtime_state_file="$(dogbot_runtime_state_file)"
compose_project_name="${DOGBOT_COMPOSE_PROJECT_NAME:-dogbot}"

if [[ $explicit_selection -eq 0 ]]; then
  if [[ -t 0 ]]; then
    if dogbot_prompt_yes_no "是否启用 QQ？" "$(dogbot_bool_to_flag "${ENABLE_QQ:-1}")"; then
      selected_qq=1
    else
      selected_qq=0
    fi

    if dogbot_prompt_yes_no "是否启用微信？" "$(dogbot_bool_to_flag "${ENABLE_WECHATPADPRO:-0}")"; then
      selected_wechat=1
    else
      selected_wechat=0
    fi
  else
    selected_qq="$(dogbot_bool_to_flag "${ENABLE_QQ:-1}")"
    selected_wechat="$(dogbot_bool_to_flag "${ENABLE_WECHATPADPRO:-0}")"
  fi
fi

ENABLE_QQ="${selected_qq:-$(dogbot_bool_to_flag "${ENABLE_QQ:-1}")}"
ENABLE_WECHATPADPRO="${selected_wechat:-$(dogbot_bool_to_flag "${ENABLE_WECHATPADPRO:-0}")}"

if [[ "${ENABLE_QQ}" != "1" && "${ENABLE_WECHATPADPRO}" != "1" ]]; then
  rm -f "$runtime_state_file"
  echo "未选择任何平台，已退出，不执行部署。"
  exit 0
fi

print_compose_failure_hint() {
  local stderr_file="$1"
  local stderr_output=""
  if [[ -f "$stderr_file" ]]; then
    stderr_output="$(cat "$stderr_file")"
  fi

  echo >&2
  echo "Docker Compose up failed." >&2
  if [[ -n "$stderr_output" ]]; then
    echo "Original error:" >&2
    echo "$stderr_output" >&2
  fi

  if grep -Eqi \
    'pull access denied|failed to resolve reference|manifest unknown|context deadline exceeded|proxyconnect|TLS handshake timeout|i/o timeout|connection refused|Get \"https://registry-1.docker.io|Get \"https://auth.docker.io' \
    <<<"$stderr_output"; then
    echo >&2
    echo "Hint: image pull appears to have failed." >&2
    echo "Check Docker Hub connectivity first, especially your proxy / Docker Hub outbound access." >&2
  fi
}

run_compose_up() {
  local compose_file="$1"
  shift
  local stderr_file
  stderr_file="$(mktemp)"

  if [[ "$compose_cmd" == "docker compose" ]]; then
    if ! docker compose \
      --project-name "$compose_project_name" \
      --project-directory "$repo_root" \
      --env-file "$env_file" \
      -f "$compose_file" \
      up -d "$@" 2> >(tee "$stderr_file" >&2); then
      print_compose_failure_hint "$stderr_file"
      rm -f "$stderr_file"
      exit 1
    fi
  else
    if ! docker-compose \
      --project-name "$compose_project_name" \
      --project-directory "$repo_root" \
      --env-file "$env_file" \
      -f "$compose_file" \
      up -d "$@" 2> >(tee "$stderr_file" >&2); then
      print_compose_failure_hint "$stderr_file"
      rm -f "$stderr_file"
      exit 1
    fi
  fi

  rm -f "$stderr_file"
}

remove_legacy_compose_container_if_needed() {
  local container_name="$1"
  local expected_service="$2"
  local expected_config_file="$3"

  if ! docker inspect "$container_name" >/dev/null 2>&1; then
    return 0
  fi

  local current_project current_service current_config
  current_project="$(docker inspect "$container_name" --format '{{index .Config.Labels "com.docker.compose.project"}}' 2>/dev/null || true)"
  current_service="$(docker inspect "$container_name" --format '{{index .Config.Labels "com.docker.compose.service"}}' 2>/dev/null || true)"
  current_config="$(docker inspect "$container_name" --format '{{index .Config.Labels "com.docker.compose.project.config_files"}}' 2>/dev/null || true)"

  if [[ "$current_service" == "$expected_service" ]] && {
    [[ "$current_project" != "$compose_project_name" ]] || [[ "$current_config" != "$expected_config_file" ]];
  }; then
    echo "Removing legacy compose-managed container: $container_name"
    docker rm -f "$container_name" >/dev/null
    return 0
  fi

  if [[ "$current_service" == "$expected_service" ]]; then
    return 0
  fi

  echo "Container name conflict: $container_name already exists and is not managed by the current DogBot compose project." >&2
  echo "Remove it manually or set a different container name in deploy/dogbot.env." >&2
  exit 1
}

if ! compose_cmd="$(dogbot_resolve_compose_cmd)"; then
  echo "Docker Compose is not available." >&2
  echo "Install 'docker compose' plugin or 'docker-compose' first." >&2
  exit 1
fi

agent_workspace_dir="${AGENT_WORKSPACE_DIR:-/srv/agent-workdir}"
agent_state_dir="${AGENT_STATE_DIR:-/srv/agent-state}"
runner_log_dir="${AGENT_RUNNER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"
postgres_data_dir="${POSTGRES_DATA_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/postgres}"
DOGBOT_CLAUDE_PROMPT_ROOT="${DOGBOT_CLAUDE_PROMPT_ROOT:-${AGENT_STATE_DIR:-/srv/agent-state}/claude-prompt}"
DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR="${DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR:-$(dogbot_claude_runner_runtime_dir)}"
dogbot_ensure_user_writable_dir "$agent_workspace_dir"
dogbot_ensure_user_writable_dir "$agent_state_dir"
dogbot_ensure_user_writable_dir "$DOGBOT_CLAUDE_PROMPT_ROOT"
dogbot_ensure_user_writable_dir "$DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR"
dogbot_ensure_user_writable_dir "$runner_log_dir"
dogbot_ensure_user_writable_dir "$postgres_data_dir"

dogbot_sync_claude_prompt_root "$repo_root/claude-prompt" "$DOGBOT_CLAUDE_PROMPT_ROOT"
dogbot_write_claude_runner_runtime "$DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR"

mkdir -p \
  "${NAPCAT_QQ_DIR:-/srv/napcat/qq}" \
  "${NAPCAT_CONFIG_DIR:-/srv/napcat/config}" \
  "${WECHATPADPRO_DATA_DIR:-/srv/wechatpadpro/data}" \
  "${WECHATPADPRO_MYSQL_DIR:-/srv/wechatpadpro/mysql}" \
  "${WECHATPADPRO_REDIS_DIR:-/srv/wechatpadpro/redis}"

dogbot_save_runtime_state "$runtime_state_file"

remove_legacy_compose_container_if_needed \
  "${POSTGRES_CONTAINER_NAME:-dogbot-postgres}" \
  "postgres" \
  "$repo_root/deploy/docker/docker-compose.yml"
run_compose_up "$repo_root/deploy/docker/docker-compose.yml" postgres

"$repo_root/scripts/start_agent_runner.sh" "$env_file"

remove_legacy_compose_container_if_needed \
  "${CLAUDE_CONTAINER_NAME:-claude-runner}" \
  "claude-runner" \
  "$repo_root/deploy/docker/docker-compose.yml"
run_compose_up "$repo_root/deploy/docker/docker-compose.yml" claude-runner

if [[ "${ENABLE_QQ}" == "1" ]]; then
  dogbot_require_env PLATFORM_QQ_BOT_ID
  "$repo_root/scripts/configure_napcat_ingress.sh" "$env_file"
  remove_legacy_compose_container_if_needed \
    "${NAPCAT_CONTAINER_NAME:-napcat}" \
    "napcat" \
    "$repo_root/deploy/docker/platform-stack.yml"
  run_compose_up "$repo_root/deploy/docker/platform-stack.yml" napcat
  echo "Waiting up to ${DOGBOT_LOGIN_TIMEOUT_SECS:-100}s for NapCat login..."
  "$repo_root/scripts/prepare_napcat_login.sh" "$env_file"
fi

if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  dogbot_require_env WECHATPADPRO_IMAGE
  dogbot_require_env WECHATPADPRO_ADMIN_KEY
  dogbot_require_env WECHATPADPRO_MYSQL_ROOT_PASSWORD
  dogbot_require_env WECHATPADPRO_MYSQL_PASSWORD

  remove_legacy_compose_container_if_needed \
    "${WECHATPADPRO_CONTAINER_NAME:-wechatpadpro}" \
    "wechatpadpro" \
    "$repo_root/deploy/docker/wechatpadpro-stack.yml"
  remove_legacy_compose_container_if_needed \
    "${WECHATPADPRO_MYSQL_CONTAINER_NAME:-wechatpadpro_mysql}" \
    "wechatpadpro_mysql" \
    "$repo_root/deploy/docker/wechatpadpro-stack.yml"
  remove_legacy_compose_container_if_needed \
    "${WECHATPADPRO_REDIS_CONTAINER_NAME:-wechatpadpro_redis}" \
    "wechatpadpro_redis" \
    "$repo_root/deploy/docker/wechatpadpro-stack.yml"
  run_compose_up "$repo_root/deploy/docker/wechatpadpro-stack.yml"
  echo "Waiting up to ${DOGBOT_LOGIN_TIMEOUT_SECS:-100}s for WeChatPadPro login..."
  "$repo_root/scripts/prepare_wechatpadpro_login.sh" "$env_file"
  if [[ "${WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK:-0}" == "1" ]]; then
    if ! "$repo_root/scripts/configure_wechatpadpro_webhook.sh" "$env_file"; then
      echo "WeChatPadPro webhook auto-configuration failed." >&2
      echo "If the account is not logged in yet, scan the QR code printed above first and re-run deploy." >&2
    fi
  fi
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
if [[ "${ENABLE_QQ}" == "1" ]]; then
  echo "NapCat WebUI: http://127.0.0.1:${NAPCAT_WEBUI_PORT:-6099}"
  echo "QQ platform ingress: http://${AGENT_RUNNER_BIND_ADDR:-127.0.0.1:8787}/v1/platforms/qq/napcat/events"
fi
if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  echo "WeChatPadPro API: http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}"
  echo "WeChatPadPro webhook ingress: http://${AGENT_RUNNER_BIND_ADDR:-127.0.0.1:8787}/v1/platforms/wechatpadpro/events"
fi
