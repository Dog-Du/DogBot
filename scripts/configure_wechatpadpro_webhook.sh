#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"
env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"

if [[ "${ENABLE_WECHATPADPRO:-0}" != "1" ]]; then
  echo "WeChatPadPro is disabled; skip webhook configuration."
  exit 0
fi

if [[ -z "${WECHATPADPRO_ACCOUNT_KEY:-}" ]]; then
  echo "WECHATPADPRO_ACCOUNT_KEY is not set; skip webhook configuration." >&2
  exit 0
fi

callback_url="${WECHATPADPRO_ADAPTER_WEBHOOK_URL:-http://host.docker.internal:${WECHATPADPRO_ADAPTER_PORT:-18999}/wechatpadpro/events}"
base_url="${WECHATPADPRO_BASE_URL:-http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}}"
include_self_message="${WECHATPADPRO_WEBHOOK_INCLUDE_SELF_MESSAGE:-false}"
secret="${WECHATPADPRO_WEBHOOK_SECRET:-}"
message_types_json="${WECHATPADPRO_WEBHOOK_MESSAGE_TYPES_JSON:-[\"1\"]}"

payload="$(cat <<JSON
{
  "URL": "$callback_url",
  "Enabled": true,
  "IncludeSelfMessage": $include_self_message,
  "MessageTypes": $message_types_json,
  "RetryCount": 3,
  "Timeout": 5,
  "Secret": "$secret",
  "UseDirectStream": true,
  "UseRedisSync": false,
  "IndependentMode": true
}
JSON
)"

api_configured=0
db_synced=0

if response="$(curl --max-time 15 -sS -X POST \
  "${base_url}/webhook/Config?key=${WECHATPADPRO_ACCOUNT_KEY}" \
  -H 'Content-Type: application/json' \
  -d "$payload")"; then
  echo "$response"
  if grep -q '"Code":200' <<<"$response"; then
    api_configured=1
  else
    echo "WeChatPadPro webhook configuration API did not return Code=200; attempting direct database sync." >&2
  fi
else
  echo "WeChatPadPro webhook configuration API request failed; attempting direct database sync." >&2
fi

if [[ -n "${WECHATPADPRO_MYSQL_ROOT_PASSWORD:-}" ]]; then
  mysql_container="${WECHATPADPRO_MYSQL_CONTAINER_NAME:-wechatpadpro_mysql}"
  mysql_database="${WECHATPADPRO_MYSQL_DATABASE:-weixin}"
  escaped_callback_url="${callback_url//\'/\'\\\'\'}"
  escaped_secret="${secret//\'/\'\\\'\'}"
  escaped_types="${message_types_json//\'/\'\\\'\'}"
  include_self_bit=0
  if [[ "$include_self_message" =~ ^(1|true|TRUE|yes|YES|on|ON)$ ]]; then
    include_self_bit=1
  fi

  docker_exec_cmd=(docker exec "$mysql_container" mysql -uroot "-p${WECHATPADPRO_MYSQL_ROOT_PASSWORD}" -D "$mysql_database" -e "
    UPDATE webhook_config
    SET url='${escaped_callback_url}',
        secret='${escaped_secret}',
        enabled=1,
        timeout=5,
        retry_count=3,
        message_types='${escaped_types}',
        include_self_message=${include_self_bit}
    WHERE webhook_key='${WECHATPADPRO_ACCOUNT_KEY}';
  ")

  if "${docker_exec_cmd[@]}" >/dev/null 2>&1; then
    db_synced=1
  else
    if command -v sudo >/dev/null 2>&1; then
      if sudo "${docker_exec_cmd[@]}" >/dev/null 2>&1; then
        db_synced=1
      else
        echo "warning: failed to sync webhook_config directly through MySQL" >&2
      fi
    else
      echo "warning: failed to sync webhook_config directly through MySQL" >&2
    fi
  fi
fi

if [[ "$api_configured" != "1" && "$db_synced" != "1" ]]; then
  echo "WeChatPadPro webhook configuration failed." >&2
  exit 1
fi

echo "WeChatPadPro webhook configuration synced."
