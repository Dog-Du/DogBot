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
