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
    if ! docker compose --env-file "$env_file" -f "$compose_file" up -d "$@" 2> >(tee "$stderr_file" >&2); then
      print_compose_failure_hint "$stderr_file"
      rm -f "$stderr_file"
      exit 1
    fi
  else
    if ! docker-compose --env-file "$env_file" -f "$compose_file" up -d "$@" 2> >(tee "$stderr_file" >&2); then
      print_compose_failure_hint "$stderr_file"
      rm -f "$stderr_file"
      exit 1
    fi
  fi

  rm -f "$stderr_file"
}

if ! compose_cmd="$(dogbot_resolve_compose_cmd)"; then
  echo "Docker Compose is not available." >&2
  echo "Install 'docker compose' plugin or 'docker-compose' first." >&2
  exit 1
fi

DOGBOT_CONTENT_ROOT="${DOGBOT_CONTENT_ROOT:-$repo_root/content}"
DOGBOT_SYNC_CONTENT_ON_DEPLOY="$(dogbot_bool_to_flag "${DOGBOT_SYNC_CONTENT_ON_DEPLOY:-1}")"
DOGBOT_REFRESH_CONTENT_ON_DEPLOY="$(dogbot_bool_to_flag "${DOGBOT_REFRESH_CONTENT_ON_DEPLOY:-0}")"

if [[ "$DOGBOT_REFRESH_CONTENT_ON_DEPLOY" == "1" ]]; then
  "$repo_root/scripts/sync_content_sources.py" --content-root "$repo_root/content"
fi

if [[ "$DOGBOT_SYNC_CONTENT_ON_DEPLOY" == "1" ]]; then
  dogbot_sync_content_root "$repo_root/content" "$DOGBOT_CONTENT_ROOT"
fi

mkdir -p \
  "${AGENT_WORKSPACE_DIR:-/srv/agent-workdir}" \
  "${AGENT_STATE_DIR:-/srv/agent-state}" \
  "$DOGBOT_CONTENT_ROOT" \
  "${NAPCAT_QQ_DIR:-/srv/napcat/qq}" \
  "${NAPCAT_CONFIG_DIR:-/srv/napcat/config}" \
  "${WECHATPADPRO_DATA_DIR:-/srv/wechatpadpro/data}" \
  "${WECHATPADPRO_MYSQL_DIR:-/srv/wechatpadpro/mysql}" \
  "${WECHATPADPRO_REDIS_DIR:-/srv/wechatpadpro/redis}" \
  "${AGENT_RUNNER_LOG_DIR:-${AGENT_STATE_DIR:-/srv/agent-state}/logs}"

dogbot_save_runtime_state "$runtime_state_file"

"$repo_root/scripts/start_agent_runner.sh" "$env_file"

run_compose_up "$repo_root/compose/docker-compose.yml" claude-runner

if [[ "${ENABLE_QQ}" == "1" ]]; then
  dogbot_require_env QQ_ADAPTER_QQ_BOT_ID
  "$repo_root/scripts/start_qq_adapter.sh" "$env_file"
  run_compose_up "$repo_root/compose/platform-stack.yml" napcat
  "$repo_root/scripts/configure_napcat_ws.sh" "$env_file"
  echo "Waiting up to ${DOGBOT_LOGIN_TIMEOUT_SECS:-100}s for NapCat login..."
  "$repo_root/scripts/prepare_napcat_login.sh" "$env_file"
fi

if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  dogbot_require_env WECHATPADPRO_IMAGE
  dogbot_require_env WECHATPADPRO_ADMIN_KEY
  dogbot_require_env WECHATPADPRO_MYSQL_ROOT_PASSWORD
  dogbot_require_env WECHATPADPRO_MYSQL_PASSWORD

  run_compose_up "$repo_root/compose/wechatpadpro-stack.yml"
  "$repo_root/scripts/start_wechatpadpro_adapter.sh" "$env_file"
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
  echo "QQ adapter: http://${QQ_ADAPTER_BIND_ADDR:-127.0.0.1:19000}"
fi
if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  echo "WeChatPadPro API: http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}"
  echo "WeChatPadPro adapter: http://${WECHATPADPRO_ADAPTER_BIND_ADDR:-127.0.0.1:18999}"
fi
