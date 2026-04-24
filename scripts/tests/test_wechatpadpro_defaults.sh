#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
env_example="$repo_root/deploy/dogbot.env.example"
deploy_script="$repo_root/scripts/deploy_stack.sh"
configure_script="$repo_root/scripts/configure_wechatpadpro_webhook.sh"
wechat_compose="$repo_root/compose/wechatpadpro-stack.yml"

if ! grep -q '^WECHATPADPRO_WEBHOOK_URL=http://host.docker.internal:8787/v1/platforms/wechatpadpro/events$' "$env_example"; then
  echo "FAIL: WeChat example config must point webhook delivery to agent-runner direct ingress" >&2
  exit 1
fi

if ! grep -q '^WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1$' "$env_example"; then
  echo "FAIL: WeChat example config must enable webhook auto-configuration by default" >&2
  exit 1
fi

if ! grep -q '^PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES=DogDu$' "$env_example"; then
  echo "FAIL: WeChat example config must set at least one default platform mention name" >&2
  exit 1
fi

if grep -q 'start_wechatpadpro_adapter.sh' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must not launch the deleted WeChatPadPro adapter" >&2
  exit 1
fi

if ! grep -q 'WECHATPADPRO_WEBHOOK_URL:-http://host.docker.internal:8787/v1/platforms/wechatpadpro/events' "$configure_script"; then
  echo "FAIL: configure_wechatpadpro_webhook.sh must default to agent-runner direct ingress" >&2
  exit 1
fi

if grep -q 'WECHATPADPRO_WECHAT_PORT:-8080}:8080' "$wechat_compose"; then
  echo "FAIL: WeChatPadPro compose must not publish container port 8080 to the host by default" >&2
  exit 1
fi

echo "wechatpadpro default config checks passed."
