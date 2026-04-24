#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
env_example="$repo_root/deploy/dogbot.env.example"
start_script="$repo_root/scripts/start_wechatpadpro_adapter.sh"
wechat_compose="$repo_root/compose/wechatpadpro-stack.yml"

if ! grep -q '^WECHATPADPRO_AGENT_RUNNER_BASE_URL=http://127.0.0.1:8787$' "$env_example"; then
  echo "FAIL: WeChat example config must point to the default agent-runner port 8787" >&2
  exit 1
fi

if ! grep -q '^WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1$' "$env_example"; then
  echo "FAIL: WeChat example config must enable webhook auto-configuration by default" >&2
  exit 1
fi

if grep -q '^WECHATPADPRO_BOT_MENTION_NAMES=$' "$env_example"; then
  echo "FAIL: WeChat example config must set at least one default bot mention name" >&2
  exit 1
fi

if ! grep -q 'WECHATPADPRO_AGENT_RUNNER_BASE_URL:-${AGENT_RUNNER_BIND_ADDR:-127.0.0.1:8787}' "$start_script"; then
  echo "FAIL: start_wechatpadpro_adapter.sh must fall back to AGENT_RUNNER_BIND_ADDR / 8787" >&2
  exit 1
fi

if grep -q 'WECHATPADPRO_WECHAT_PORT:-8080}:8080' "$wechat_compose"; then
  echo "FAIL: WeChatPadPro compose must not publish container port 8080 to the host by default" >&2
  exit 1
fi

echo "wechatpadpro default config checks passed."
