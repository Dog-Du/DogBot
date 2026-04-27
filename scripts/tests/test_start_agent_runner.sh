#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
start_script="$repo_root/scripts/start_agent_runner.sh"
env_example="$repo_root/deploy/dogbot.env.example"

patterns=(
  'DOGBOT_CLAUDE_PROMPT_ROOT="${DOGBOT_CLAUDE_PROMPT_ROOT:-'
  'DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR="${DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR:-'
  'PLATFORM_QQ_ACCOUNT_ID="${PLATFORM_QQ_ACCOUNT_ID:-'
  'PLATFORM_QQ_BOT_ID="${PLATFORM_QQ_BOT_ID:-'
  'PLATFORM_WECHATPADPRO_ACCOUNT_ID="${PLATFORM_WECHATPADPRO_ACCOUNT_ID:-'
  'PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES="${PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES:-'
  'DATABASE_URL="${DATABASE_URL:-}"'
  'POSTGRES_HOST="${POSTGRES_HOST:-127.0.0.1}"'
  'POSTGRES_AGENT_READER_DATABASE_URL="$postgres_agent_reader_database_url"'
  'HISTORY_RUN_TOKEN_TTL_SECS="${HISTORY_RUN_TOKEN_TTL_SECS:-1800}"'
  'DOGBOT_ADMIN_ACTOR_IDS="${DOGBOT_ADMIN_ACTOR_IDS:-}"'
)

for pattern in "${patterns[@]}"; do
  if ! grep -q "$pattern" "$start_script"; then
    echo "FAIL: start_agent_runner.sh must export $pattern into agent-runner" >&2
    exit 1
  fi
done

extra_patterns=(
  'nohup setsid env \'
  'dogbot_ensure_user_writable_dir "$AGENT_WORKSPACE_DIR"'
  'dogbot_ensure_user_writable_dir "$AGENT_STATE_DIR"'
  'dogbot_ensure_user_writable_dir "$log_dir"'
  'dogbot_ensure_user_writable_dir "$claude_prompt_root"'
  'dogbot_ensure_user_writable_dir "$claude_runner_runtime_dir"'
  'bind_port="$(dogbot_bind_addr_port "$bind_addr")"'
  'healthz_url="http://127.0.0.1:${bind_port}/healthz"'
  'postgres_agent_reader_database_url="${POSTGRES_AGENT_READER_DATABASE_URL:-postgres://'
  'existing_pid="$(cat "$pid_file")"'
  'dogbot_wait_for_http_ok "$healthz_url" 1'
  'existing_listener_pid="$(dogbot_find_listener_pid "$bind_port" || true)"'
  'dogbot_wait_for_http_ok "$healthz_url" 15'
  'printf '\''%s\n'\'' "$launched_pid" >"$pid_file"'
)

for pattern in "${extra_patterns[@]}"; do
  if ! grep -Fq "$pattern" "$start_script"; then
    echo "FAIL: start_agent_runner.sh must verify health and reconcile existing listeners before writing pid file" >&2
    exit 1
  fi
done

if ! grep -q 'dogbot_write_claude_runner_runtime "$claude_runner_runtime_dir"' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must materialize the claude-runner launch script before startup" >&2
  exit 1
fi

if grep -q 'DOGBOT_CONTENT_ROOT' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must not export legacy DOGBOT_CONTENT_ROOT" >&2
  exit 1
fi

if ! grep -q '^BIFROST_UPSTREAM_BASE_URL=http://host.docker.internal:9000$' "$env_example"; then
  echo "FAIL: dogbot.env.example must point Bifrost to the host api-proxy by default" >&2
  exit 1
fi

if ! grep -q '^BIFROST_UPSTREAM_API_KEY=local-proxy-token$' "$env_example"; then
  echo "FAIL: dogbot.env.example must use local-proxy-token for bifrost -> api-proxy auth" >&2
  exit 1
fi

if ! grep -q '^API_PROXY_AUTH_TOKEN=local-proxy-token$' "$env_example"; then
  echo "FAIL: dogbot.env.example must keep the api-proxy auth token aligned with bifrost" >&2
  exit 1
fi

if ! grep -q '^API_PROXY_UPSTREAM_BASE_URL=https://example.com$' "$env_example"; then
  echo "FAIL: dogbot.env.example must keep the real upstream base URL on the host api-proxy side" >&2
  exit 1
fi

if ! grep -q '^API_PROXY_UPSTREAM_TOKEN=replace-me$' "$env_example"; then
  echo "FAIL: dogbot.env.example must keep the real upstream token on the host api-proxy side" >&2
  exit 1
fi

if ! grep -q '^AGENT_RUNNER_BIND_ADDR=0.0.0.0:8787$' "$env_example"; then
  echo "FAIL: dogbot.env.example must bind agent-runner on 0.0.0.0:8787 so platform containers can reach it" >&2
  exit 1
fi

if ! grep -q '^POSTGRES_DB=dogbot$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define PostgreSQL database settings" >&2
  exit 1
fi

if ! grep -q '^POSTGRES_PORT=15432$' "$env_example"; then
  echo "FAIL: dogbot.env.example must avoid the common host PostgreSQL port 5432 by default" >&2
  exit 1
fi

if ! grep -q 'postgres:5432/dogbot' "$env_example"; then
  echo "FAIL: dogbot.env.example reader URL must use the internal PostgreSQL service address" >&2
  exit 1
fi

if ! grep -q '@postgres:5432/${POSTGRES_DB:-dogbot}' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must default the reader URL to the internal PostgreSQL service address" >&2
  exit 1
fi

if ! grep -q 'POSTGRES_PORT="${POSTGRES_PORT:-15432}"' "$start_script"; then
  echo "FAIL: start_agent_runner.sh must default PostgreSQL host port to 15432" >&2
  exit 1
fi

if grep -q '^SESSION_DB_PATH=' "$env_example" || grep -q '^HISTORY_DB_PATH=' "$env_example"; then
  echo "FAIL: dogbot.env.example must not configure obsolete SQLite paths" >&2
  exit 1
fi

if ! grep -q '^PLATFORM_QQ_ACCOUNT_ID=qq:bot_uin:unknown$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define PLATFORM_QQ_ACCOUNT_ID with the direct-ingress default" >&2
  exit 1
fi

if ! grep -q '^PLATFORM_QQ_BOT_ID=$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define PLATFORM_QQ_BOT_ID for NapCat websocket config" >&2
  exit 1
fi

if ! grep -q '^PLATFORM_WECHATPADPRO_ACCOUNT_ID=wechatpadpro:account:unknown$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define PLATFORM_WECHATPADPRO_ACCOUNT_ID with the direct-ingress default" >&2
  exit 1
fi

if ! grep -q '^PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES=DogDu$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES for mention matching" >&2
  exit 1
fi

echo "start_agent_runner claude prompt env checks passed."
