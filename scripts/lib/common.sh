#!/usr/bin/env bash

dogbot_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dogbot_repo_root="$(cd "$dogbot_script_dir/.." && pwd)"
dogbot_default_env_file="$dogbot_repo_root/deploy/dogbot.env"

dogbot_resolve_env_file() {
  if [[ $# -ge 1 ]]; then
    printf '%s\n' "$1"
  else
    printf '%s\n' "$dogbot_default_env_file"
  fi
}

dogbot_require_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    echo "Missing env file: $env_file" >&2
    return 1
  fi
}

dogbot_runtime_state_file() {
  local state_dir="${AGENT_STATE_DIR:-/srv/agent-state}"
  printf '%s\n' "$state_dir/deploy-state.env"
}

dogbot_load_env_file() {
  local env_file="$1"
  dogbot_require_env_file "$env_file" || return 1
  set -a
  # shellcheck disable=SC1090
  source "$env_file"
  set +a
}

dogbot_require_env() {
  local key="$1"
  if [[ -z "${!key:-}" ]]; then
    echo "Missing required environment variable: $key" >&2
    return 1
  fi
}

dogbot_resolve_compose_cmd() {
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

dogbot_resolve_uv_bin() {
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

  echo "uv not found. Please install uv first." >&2
  return 1
}

dogbot_prompt_yes_no() {
  local prompt="$1"
  local default_answer="${2:-y}"
  local reply
  local suffix="[Y/n]"
  if [[ "$default_answer" =~ ^(n|N)$ ]]; then
    suffix="[y/N]"
  fi

  while true; do
    read -r -p "$prompt $suffix " reply || return 1
    reply="${reply:-$default_answer}"
    case "$reply" in
      y|Y|yes|YES) return 0 ;;
      n|N|no|NO) return 1 ;;
      *) echo "请输入 y 或 n。" >&2 ;;
    esac
  done
}

dogbot_bool_to_flag() {
  local value="${1:-0}"
  if [[ "$value" =~ ^(1|true|TRUE|yes|YES|on|ON)$ ]]; then
    echo "1"
  else
    echo "0"
  fi
}

dogbot_save_runtime_state() {
  local output_file="$1"
  mkdir -p "$(dirname "$output_file")"
  cat >"$output_file" <<EOF
ENABLE_QQ=${ENABLE_QQ:-0}
ENABLE_WECHATPADPRO=${ENABLE_WECHATPADPRO:-0}
EOF
}

dogbot_load_runtime_state_if_present() {
  local state_file="$1"
  if [[ -f "$state_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$state_file"
    set +a
  fi
}

dogbot_print_qr_if_possible() {
  local url="$1"
  if [[ -z "$url" ]]; then
    return 0
  fi
  if command -v qrencode >/dev/null 2>&1; then
    qrencode -t ANSIUTF8 "$url"
  fi
}
