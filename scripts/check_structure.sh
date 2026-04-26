#!/usr/bin/env bash
set -euo pipefail

files=(
  "deploy/docker/README.md"
  "deploy/docker/docker-compose.yml"
  "deploy/docker/Dockerfile"
  "deploy/docker/entrypoint.sh"
  "deploy/docker/wechatpadpro-stack.yml"
  "claude-prompt/CLAUDE.md"
  "claude-prompt/persona.md"
  "claude-prompt/skills/reply-format/SKILL.md"
  "deploy/docker/platform-stack.yml"
  "deploy/dogbot.env.example"
  "scripts/deploy_stack.sh"
  "scripts/stop_stack.sh"
  "scripts/start_agent_runner.sh"
  "scripts/configure_napcat_ingress.sh"
  "scripts/prepare_napcat_login.sh"
  "scripts/prepare_wechatpadpro_login.sh"
  "scripts/configure_wechatpadpro_webhook.sh"
  "scripts/apply_runner_network_policy.sh"
  "scripts/remove_runner_network_policy.sh"
  "scripts/tests/smoke_test_claude_runner.sh"
  "scripts/tests/test_common.sh"
  "scripts/tests/test_configure_napcat_ingress.sh"
  "scripts/tests/test_prepare_napcat_login.sh"
  "scripts/tests/test_prepare_wechatpadpro_login.sh"
  "scripts/tests/test_wechatpadpro_defaults.sh"
  "scripts/tests/test_start_agent_runner.sh"
  "scripts/tests/test_deploy_stack_platform_ingress.sh"
  "scripts/tests/test_agent_runner_docker_e2e.sh"
)

executable_scripts=(
  "scripts/deploy_stack.sh"
  "scripts/stop_stack.sh"
  "scripts/start_agent_runner.sh"
  "scripts/configure_napcat_ingress.sh"
  "scripts/prepare_napcat_login.sh"
  "scripts/configure_wechatpadpro_webhook.sh"
  "scripts/prepare_wechatpadpro_login.sh"
  "scripts/tests/test_agent_runner_docker_e2e.sh"
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
ensure_pattern "deploy/docker/Dockerfile" "@anthropic-ai/claude-code" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/Dockerfile" "@maximhq/bifrost" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/Dockerfile" "tini" || pattern_errors=$((pattern_errors+1))
entrypoint_has_sudo=$(grep -q "sudo" "$repo_root/deploy/docker/entrypoint.sh" && echo yes || echo no)
entrypoint_has_gosu=$(grep -q "gosu" "$repo_root/deploy/docker/entrypoint.sh" && echo yes || echo no)
if [[ $entrypoint_has_sudo == yes ]]; then
  ensure_pattern "deploy/docker/Dockerfile" "sudo" || pattern_errors=$((pattern_errors+1))
fi
if [[ $entrypoint_has_gosu == yes ]]; then
  ensure_pattern "deploy/docker/Dockerfile" "gosu" || pattern_errors=$((pattern_errors+1))
fi
ensure_pattern "deploy/docker/Dockerfile" "claude-bootstrap.sh" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/entrypoint.sh" "/usr/local/bin/claude-bootstrap.sh" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/entrypoint.sh" "/state/claude-runner/launch.sh" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/lib/common.sh" "dogbot_write_claude_runner_runtime" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/lib/common.sh" "bifrost -host 127.0.0.1 -port" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/docker-compose.yml" "mem_limit" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/docker-compose.yml" "CLAUDE_CONFIG_DIR" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/docker/platform-stack.yml" "mlikiowa/napcat-docker:latest" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/configure_napcat_ingress.sh" "http://host.docker.internal:8787/v1/platforms/qq/napcat/events" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/configure_napcat_ingress.sh" "dogbot_resolve_uv_bin" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "AGENT_RUNNER_BIND_ADDR" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "PLATFORM_QQ_ACCOUNT_ID" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "PLATFORM_WECHATPADPRO_ACCOUNT_ID" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "NAPCAT_HTTP_CLIENT_URL=http://host.docker.internal:8787/v1/platforms/qq/napcat/events" || pattern_errors=$((pattern_errors+1))
if grep -q '^QQ_ADAPTER_QQ_BOT_ID=' "$repo_root/deploy/dogbot.env"; then
  echo "Stale QQ_ADAPTER_QQ_BOT_ID found in deploy/dogbot.env" >&2
  pattern_errors=$((pattern_errors+1))
fi

if [[ -f "$repo_root/deploy/dogbot.env" ]]; then
  example_norm="$(mktemp)"
  env_norm="$(mktemp)"
  sed -E 's/^([A-Z0-9_]+)=.*/\1=/' "$repo_root/deploy/dogbot.env.example" >"$example_norm"
  sed -E 's/^([A-Z0-9_]+)=.*/\1=/' "$repo_root/deploy/dogbot.env" >"$env_norm"
  if ! diff -u "$example_norm" "$env_norm" >/dev/null; then
    echo "deploy/dogbot.env structure is not aligned with deploy/dogbot.env.example" >&2
    diff -u "$example_norm" "$env_norm" >&2 || true
    pattern_errors=$((pattern_errors+1))
  fi
  rm -f "$example_norm" "$env_norm"
fi

ensure_pattern "scripts/deploy_stack.sh" "DOGBOT_COMPOSE_PROJECT_NAME" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/deploy_stack.sh" '--project-name "$compose_project_name"' || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/stop_stack.sh" "DOGBOT_COMPOSE_PROJECT_NAME" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/deploy_stack.sh" "dogbot_write_claude_runner_runtime" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/start_agent_runner.sh" "build --release --manifest-path" || pattern_errors=$((pattern_errors+1))
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
bash -n "$repo_root/scripts/configure_napcat_ingress.sh"
bash -n "$repo_root/scripts/prepare_napcat_login.sh"
bash -n "$repo_root/scripts/prepare_wechatpadpro_login.sh"
bash -n "$repo_root/scripts/configure_wechatpadpro_webhook.sh"
bash -n "$repo_root/scripts/tests/smoke_test_claude_runner.sh"
bash -n "$repo_root/scripts/tests/test_agent_runner_docker_e2e.sh"
bash "$repo_root/scripts/tests/test_common.sh"
bash "$repo_root/scripts/tests/test_configure_napcat_ingress.sh"
bash "$repo_root/scripts/tests/test_deploy_stack_platform_ingress.sh"
bash "$repo_root/scripts/tests/test_prepare_napcat_login.sh"
bash "$repo_root/scripts/tests/test_prepare_wechatpadpro_login.sh"
bash "$repo_root/scripts/tests/test_wechatpadpro_defaults.sh"
bash "$repo_root/scripts/tests/test_start_agent_runner.sh"

echo "Structure check passed. All required files are present."
