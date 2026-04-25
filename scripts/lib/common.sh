#!/usr/bin/env bash

dogbot_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dogbot_repo_root="$(cd "$dogbot_script_dir/.." && pwd)"
dogbot_default_env_file="$dogbot_repo_root/deploy/dogbot.env"

dogbot_resolve_env_file() {
  if [[ $# -ge 1 && -n "${1:-}" ]]; then
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

dogbot_find_listener_pid() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n1
    return 0
  fi

  ss -ltnp "( sport = :$port )" 2>/dev/null | sed -n 's/.*pid=\([0-9]\+\).*/\1/p' | head -n1
}

dogbot_wait_for_listener_pid() {
  local port="$1"
  local timeout_secs="${2:-30}"
  local attempts=$(( timeout_secs * 2 ))
  local attempt=0
  local listener_pid=""

  while (( attempt < attempts )); do
    listener_pid="$(dogbot_find_listener_pid "$port")"
    if [[ -n "$listener_pid" ]] && kill -0 "$listener_pid" >/dev/null 2>&1; then
      printf '%s\n' "$listener_pid"
      return 0
    fi

    sleep 0.5
    attempt=$(( attempt + 1 ))
  done

  return 1
}

dogbot_wait_for_http_ok() {
  local url="$1"
  local timeout_secs="${2:-30}"
  local attempts=$timeout_secs
  local attempt=0

  while (( attempt < attempts )); do
    if curl -fsSL --max-time 5 -o /dev/null "$url" >/dev/null 2>&1; then
      return 0
    fi

    sleep 1
    attempt=$(( attempt + 1 ))
  done

  return 1
}

dogbot_deadline_in() {
  local timeout_secs="${1:-0}"
  printf '%s\n' "$(( $(date +%s) + timeout_secs ))"
}

dogbot_wait_until_deadline() {
  local deadline_epoch="$1"
  shift
  local interval_secs="${DOGBOT_WAIT_INTERVAL_SECS:-1}"

  while true; do
    if "$@"; then
      return 0
    fi

    if (( $(date +%s) >= deadline_epoch )); then
      return 1
    fi

    sleep "$interval_secs"
  done
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

dogbot_sync_claude_prompt_root() {
  local source_dir="$1"
  local dest_dir="$2"
  local source_abs
  local dest_abs

  if [[ ! -d "$source_dir" ]]; then
    echo "Claude prompt source directory does not exist: $source_dir" >&2
    return 1
  fi

  mkdir -p "$dest_dir"

  source_abs="$(cd "$source_dir" && pwd -P)"
  dest_abs="$(cd "$dest_dir" && pwd -P)"

  if [[ "$source_abs" == "$dest_abs" ]]; then
    return 0
  fi

  find "$dest_dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  cp -a "$source_dir"/. "$dest_dir"/
}

dogbot_claude_runner_runtime_dir() {
  local state_dir="${AGENT_STATE_DIR:-/srv/agent-state}"
  printf '%s\n' "$state_dir/claude-runner"
}

dogbot_write_claude_runner_runtime() {
  local runtime_dir="$1"
  local launch_path="$runtime_dir/launch.sh"

  mkdir -p "$runtime_dir"

  cat >"$launch_path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

bifrost_dir="/state/bifrost"
config_path="$bifrost_dir/config.json"
log_path="$bifrost_dir/bifrost.log"
prompt_root="/state/claude-prompt"
port="${BIFROST_PORT:-8080}"
provider_name="${BIFROST_PROVIDER_NAME:-primary}"
default_model="${BIFROST_MODEL:-primary/model-id}"
stripped_model="${default_model#*/}"
upstream_base_url="${BIFROST_UPSTREAM_BASE_URL:-https://example.com}"
upstream_provider_type="${BIFROST_UPSTREAM_PROVIDER_TYPE:-openai}"
upstream_api_key="${BIFROST_UPSTREAM_API_KEY:-replace-me}"

mkdir -p "$bifrost_dir"

if [[ -z "${BIFROST_ENCRYPTION_KEY:-}" ]]; then
  export BIFROST_ENCRYPTION_KEY
  BIFROST_ENCRYPTION_KEY="$(openssl rand -hex 32)"
fi

export BIFROST_UPSTREAM_API_KEY="$upstream_api_key"

jq -n \
  --arg schema "https://www.getbifrost.ai/schema" \
  --arg provider_name "$provider_name" \
  --arg default_model "$default_model" \
  --arg stripped_model "$stripped_model" \
  --arg upstream_base_url "$upstream_base_url" \
  --arg upstream_provider_type "$upstream_provider_type" \
  '{
    "$schema": $schema,
    "encryption_key": "env.BIFROST_ENCRYPTION_KEY",
    "providers": {
      ($provider_name): {
        "network_config": {
          "base_url": $upstream_base_url
        },
        "custom_provider_config": {
          "base_provider_type": $upstream_provider_type,
          "allowed_requests": {
            "chat_completion": true,
            "chat_completion_stream": true,
            "responses": true,
            "responses_stream": true
          }
        },
        "keys": [
          {
            "name": "default-key",
            "value": "env.BIFROST_UPSTREAM_API_KEY",
            "models": (
              [$default_model, $stripped_model]
              | map(select(length > 0))
              | unique
            ),
            "weight": 1
          }
        ]
      }
    },
    "config_store": {
      "enabled": false
    }
  }' >"$config_path"

bifrost -host 127.0.0.1 -port "$port" -app-dir "$bifrost_dir" >>"$log_path" 2>&1 &
bifrost_pid=$!

cleanup() {
  if kill -0 "$bifrost_pid" >/dev/null 2>&1; then
    kill "$bifrost_pid" >/dev/null 2>&1 || true
    wait "$bifrost_pid" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT INT TERM

for _ in $(seq 1 60); do
  if ! kill -0 "$bifrost_pid" >/dev/null 2>&1; then
    cat "$log_path" >&2 || true
    exit 1
  fi

  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    wait "$bifrost_pid"
    exit $?
  fi

  sleep 1
done

echo "bifrost did not become ready on 127.0.0.1:${port}" >&2
cat "$log_path" >&2 || true
exit 1
EOF

  chmod +x "$launch_path"
}
