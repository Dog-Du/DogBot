#!/usr/bin/env bash
set -euo pipefail

files=(
  "compose/docker-compose.yml"
  "docker/claude-runner/Dockerfile"
  "docker/claude-runner/entrypoint.sh"
  "content/sources.lock.json"
  "content/packs/base/manifest.json"
  "content/packs/qq/manifest.json"
  "content/packs/wechat/manifest.json"
  "content/packs/starter-skills/manifest.json"
  "content/packs/memory-baseline/manifest.json"
  "content/packs/ov-examples/manifest.json"
  "qq_adapter/app.py"
  "qq_adapter/config.py"
  "qq_adapter/mapper.py"
  "qq_adapter/napcat_client.py"
  "qq_adapter/runner_client.py"
  "compose/platform-stack.yml"
  "deploy/dogbot.env.example"
  "scripts/deploy_stack.sh"
  "scripts/stop_stack.sh"
  "scripts/start_agent_runner.sh"
  "scripts/start_qq_adapter.sh"
  "scripts/start_wechatpadpro_adapter.sh"
  "scripts/configure_napcat_ws.sh"
  "scripts/prepare_napcat_login.sh"
  "scripts/prepare_wechatpadpro_login.sh"
  "scripts/configure_wechatpadpro_webhook.sh"
  "scripts/sync_content_sources.py"
  "scripts/audit_legacy_runtime_memory.py"
  "scripts/apply_runner_network_policy.sh"
  "scripts/remove_runner_network_policy.sh"
  "scripts/tests/smoke_test_claude_runner.sh"
  "scripts/tests/test_common.sh"
  "scripts/tests/test_configure_napcat_ws.sh"
  "scripts/tests/test_prepare_napcat_login.sh"
  "scripts/tests/test_prepare_wechatpadpro_login.sh"
  "scripts/tests/test_wechatpadpro_defaults.sh"
  "scripts/tests/test_deploy_content_bootstrap.sh"
  "scripts/tests/test_start_agent_runner.sh"
  "scripts/tests/test_sync_content_sources.py"
  "scripts/tests/test_audit_legacy_runtime_memory.py"
)

executable_scripts=(
  "scripts/deploy_stack.sh"
  "scripts/stop_stack.sh"
  "scripts/start_agent_runner.sh"
  "scripts/start_qq_adapter.sh"
  "scripts/start_wechatpadpro_adapter.sh"
  "scripts/configure_napcat_ws.sh"
  "scripts/prepare_napcat_login.sh"
  "scripts/configure_wechatpadpro_webhook.sh"
  "scripts/prepare_wechatpadpro_login.sh"
)

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

missing=()
for file in "${files[@]}"; do
  full_path="$repo_root/$file"
  if [[ ! -f $full_path ]]; then
    missing+=("$file")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Structure check failed: missing files:" >&2
  for path in "${missing[@]}"; do
    echo "  - $path" >&2
  done
  exit 1
fi

non_executable=()
for script in "${executable_scripts[@]}"; do
  if [[ ! -x "$repo_root/$script" ]]; then
    non_executable+=("$script")
  fi
done

if [[ ${#non_executable[@]} -gt 0 ]]; then
  echo "Structure check failed: scripts missing executable bit:" >&2
  for path in "${non_executable[@]}"; do
    echo "  - $path" >&2
  done
  exit 1
fi

ensure_pattern() {
  local file="$1"
  local pattern="$2"
  if ! grep -q -- "$pattern" "$repo_root/$file"; then
    echo "Pattern '$pattern' missing from $file" >&2
    return 1
  fi
  return 0
}

pattern_errors=0
ensure_pattern "docker/claude-runner/Dockerfile" "@anthropic-ai/claude-code" || pattern_errors=$((pattern_errors+1))
ensure_pattern "docker/claude-runner/Dockerfile" "tini" || pattern_errors=$((pattern_errors+1))
entrypoint_has_sudo=$(grep -q "sudo" "$repo_root/docker/claude-runner/entrypoint.sh" && echo yes || echo no)
entrypoint_has_gosu=$(grep -q "gosu" "$repo_root/docker/claude-runner/entrypoint.sh" && echo yes || echo no)
if [[ $entrypoint_has_sudo == yes ]]; then
  ensure_pattern "docker/claude-runner/Dockerfile" "sudo" || pattern_errors=$((pattern_errors+1))
fi
if [[ $entrypoint_has_gosu == yes ]]; then
  ensure_pattern "docker/claude-runner/Dockerfile" "gosu" || pattern_errors=$((pattern_errors+1))
fi
ensure_pattern "docker/claude-runner/Dockerfile" "claude-bootstrap.sh" || pattern_errors=$((pattern_errors+1))
ensure_pattern "docker/claude-runner/entrypoint.sh" "/usr/local/bin/claude-bootstrap.sh" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/docker-compose.yml" "mem_limit" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/docker-compose.yml" "CLAUDE_CONFIG_DIR" || pattern_errors=$((pattern_errors+1))
ensure_pattern "qq_adapter/app.py" "/napcat/ws" || pattern_errors=$((pattern_errors+1))
ensure_pattern "qq_adapter/napcat_client.py" "/send_group_msg" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/platform-stack.yml" "mlikiowa/napcat-docker:latest" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/configure_napcat_ws.sh" "host.docker.internal:19000/napcat/ws" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/configure_napcat_ws.sh" "dogbot_resolve_uv_bin" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "AGENT_RUNNER_BIND_ADDR" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/deploy_stack.sh" "docker compose --env-file" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/start_agent_runner.sh" "build --release --manifest-path" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/start_qq_adapter.sh" "qq_adapter.app:create_app" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/apply_runner_network_policy.sh" "INPUT" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/tests/smoke_test_claude_runner.sh" "resolve_uv_bin" || pattern_errors=$((pattern_errors+1))

if [[ $pattern_errors -gt 0 ]]; then
  echo "Structure check failed due to missing scaffold markers." >&2
  exit 1
fi

bash -n "$repo_root/scripts/apply_runner_network_policy.sh"
bash -n "$repo_root/scripts/remove_runner_network_policy.sh"
bash -n "$repo_root/scripts/deploy_stack.sh"
bash -n "$repo_root/scripts/stop_stack.sh"
bash -n "$repo_root/scripts/start_agent_runner.sh"
bash -n "$repo_root/scripts/start_qq_adapter.sh"
bash -n "$repo_root/scripts/start_wechatpadpro_adapter.sh"
bash -n "$repo_root/scripts/configure_napcat_ws.sh"
bash -n "$repo_root/scripts/prepare_napcat_login.sh"
bash -n "$repo_root/scripts/prepare_wechatpadpro_login.sh"
bash -n "$repo_root/scripts/configure_wechatpadpro_webhook.sh"
bash -n "$repo_root/scripts/tests/smoke_test_claude_runner.sh"
bash "$repo_root/scripts/tests/test_common.sh"
bash "$repo_root/scripts/tests/test_configure_napcat_ws.sh"
bash "$repo_root/scripts/tests/test_prepare_napcat_login.sh"
bash "$repo_root/scripts/tests/test_prepare_wechatpadpro_login.sh"
bash "$repo_root/scripts/tests/test_wechatpadpro_defaults.sh"
bash "$repo_root/scripts/tests/test_deploy_content_bootstrap.sh"
bash "$repo_root/scripts/tests/test_start_agent_runner.sh"
uv run python -m py_compile "$repo_root/qq_adapter/app.py"
uv run python -m py_compile "$repo_root/scripts/sync_content_sources.py"
uv run python -m py_compile "$repo_root/scripts/audit_legacy_runtime_memory.py"

echo "Structure check passed. All required files are present."
