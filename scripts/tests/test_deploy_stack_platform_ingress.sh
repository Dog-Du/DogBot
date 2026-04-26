#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if grep -q 'start_qq_adapter.sh' "$repo_root/scripts/deploy_stack.sh"; then
  echo "FAIL: deploy_stack.sh must not launch qq_adapter anymore" >&2
  exit 1
fi

if grep -q 'start_wechatpadpro_adapter.sh' "$repo_root/scripts/deploy_stack.sh"; then
  echo "FAIL: deploy_stack.sh must not launch wechatpadpro_adapter anymore" >&2
  exit 1
fi

if ! grep -q '/v1/platforms/qq/napcat/events' "$repo_root/scripts/configure_napcat_ingress.sh"; then
  echo "FAIL: NapCat HTTP client must point at agent-runner platform ingress" >&2
  exit 1
fi

if ! grep -q '/v1/platforms/wechatpadpro/events' "$repo_root/scripts/configure_wechatpadpro_webhook.sh"; then
  echo "FAIL: WeChatPadPro webhook must point at agent-runner platform ingress" >&2
  exit 1
fi

if ! grep -Fq '${AGENT_WORKSPACE_DIR:-/srv/agent-workdir}:/workspace:ro' "$repo_root/deploy/docker/platform-stack.yml"; then
  echo "FAIL: NapCat container must mount AGENT_WORKSPACE_DIR read-only at /workspace for local media delivery" >&2
  exit 1
fi

if ! grep -Fq 'ACCOUNT: ${PLATFORM_QQ_BOT_ID:-}' "$repo_root/deploy/docker/platform-stack.yml"; then
  echo "FAIL: NapCat container must default to quick login with PLATFORM_QQ_BOT_ID" >&2
  exit 1
fi

if ! grep -q 'DOGBOT_COMPOSE_PROJECT_NAME' "$repo_root/scripts/deploy_stack.sh"; then
  echo "FAIL: deploy_stack.sh must pin a stable Docker Compose project name" >&2
  exit 1
fi

configure_line="$(grep -n '"\$repo_root/scripts/configure_napcat_ingress.sh" "\$env_file"' "$repo_root/scripts/deploy_stack.sh" | cut -d: -f1)"
compose_line="$(grep -n 'run_compose_up "\$repo_root/deploy/docker/platform-stack.yml" napcat' "$repo_root/scripts/deploy_stack.sh" | cut -d: -f1)"

if [[ -z "$configure_line" || -z "$compose_line" || "$configure_line" -ge "$compose_line" ]]; then
  echo "FAIL: deploy_stack.sh must configure NapCat ingress before starting the NapCat container" >&2
  exit 1
fi

if grep -q 'docker restart "\$NAPCAT_CONTAINER_NAME"' "$repo_root/scripts/configure_napcat_ingress.sh"; then
  echo "FAIL: configure_napcat_ingress.sh must not restart NapCat during deploy" >&2
  exit 1
fi

required_runtime_repairs=(
  'dogbot_ensure_user_writable_dir "$agent_workspace_dir"'
  'dogbot_ensure_user_writable_dir "$agent_state_dir"'
  'dogbot_ensure_user_writable_dir "$DOGBOT_CLAUDE_PROMPT_ROOT"'
  'dogbot_ensure_user_writable_dir "$DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR"'
  'dogbot_ensure_user_writable_dir "$runner_log_dir"'
  'dogbot_ensure_user_writable_file_path "$session_db_path"'
  'dogbot_ensure_user_writable_file_path "$history_db_path"'
)

for pattern in "${required_runtime_repairs[@]}"; do
  if ! grep -Fq "$pattern" "$repo_root/scripts/deploy_stack.sh"; then
    echo "FAIL: deploy_stack.sh must repair agent-runner runtime directory ownership before launch" >&2
    exit 1
  fi
done

if ! grep -q 'DOGBOT_COMPOSE_PROJECT_NAME' "$repo_root/scripts/stop_stack.sh"; then
  echo "FAIL: stop_stack.sh must pin a stable Docker Compose project name" >&2
  exit 1
fi

if ! grep -q -- '--keep-qq' "$repo_root/scripts/stop_stack.sh"; then
  echo "FAIL: stop_stack.sh must support keeping NapCat running during runner-only restarts" >&2
  exit 1
fi

if ! grep -q -- '--keep-wechat' "$repo_root/scripts/stop_stack.sh"; then
  echo "FAIL: stop_stack.sh must support keeping WeChatPadPro running during runner-only restarts" >&2
  exit 1
fi

echo "deploy_stack platform ingress checks passed."
