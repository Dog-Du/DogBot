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

payload="$(cat <<JSON
{
  "URL": "$callback_url",
  "Enabled": true,
  "IncludeSelfMessage": $include_self_message,
  "MessageTypes": ["Text"],
  "RetryCount": 3,
  "Timeout": 5,
  "Secret": "$secret"
}
JSON
)"

response="$(curl -sS -X POST \
  "${base_url}/webhook/Config?key=${WECHATPADPRO_ACCOUNT_KEY}" \
  -H 'Content-Type: application/json' \
  -d "$payload")"

echo "$response"
if ! grep -q '"Code":200' <<<"$response"; then
  echo "WeChatPadPro webhook configuration failed." >&2
  exit 1
fi
