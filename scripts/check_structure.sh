#!/usr/bin/env bash
set -euo pipefail

files=(
  "compose/docker-compose.yml"
  "docker/claude-runner/Dockerfile"
  "docker/claude-runner/entrypoint.sh"
  "astrbot/plugins/claude_runner_bridge/main.py"
  "astrbot/plugins/claude_runner_bridge/_conf_schema.json"
  "astrbot/plugins/claude_runner_bridge/README.md"
  "astrbot/plugins/claude_runner_bridge/requirements.txt"
  "compose/platform-stack.yml"
  "deploy/dogbot.env.example"
  "scripts/deploy_stack.sh"
  "scripts/stop_stack.sh"
  "scripts/start_agent_runner.sh"
  "scripts/apply_runner_network_policy.sh"
  "scripts/remove_runner_network_policy.sh"
  "scripts/smoke_test_claude_runner.sh"
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
ensure_pattern "astrbot/plugins/claude_runner_bridge/main.py" "@register" || pattern_errors=$((pattern_errors+1))
ensure_pattern "astrbot/plugins/claude_runner_bridge/main.py" "agent-runner" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/platform-stack.yml" "soulter/astrbot:latest" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/platform-stack.yml" "mlikiowa/napcat-docker:latest" || pattern_errors=$((pattern_errors+1))
ensure_pattern "deploy/dogbot.env.example" "AGENT_RUNNER_BIND_ADDR" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/deploy_stack.sh" "docker compose --env-file" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/start_agent_runner.sh" "build --release --manifest-path" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/apply_runner_network_policy.sh" "INPUT" || pattern_errors=$((pattern_errors+1))
ensure_pattern "scripts/smoke_test_claude_runner.sh" "resolve_uv_bin" || pattern_errors=$((pattern_errors+1))

if [[ $pattern_errors -gt 0 ]]; then
  echo "Structure check failed due to missing scaffold markers." >&2
  exit 1
fi

bash -n "$repo_root/scripts/apply_runner_network_policy.sh"
bash -n "$repo_root/scripts/remove_runner_network_policy.sh"
bash -n "$repo_root/scripts/deploy_stack.sh"
bash -n "$repo_root/scripts/stop_stack.sh"
bash -n "$repo_root/scripts/start_agent_runner.sh"
bash -n "$repo_root/scripts/smoke_test_claude_runner.sh"
uv run python -m py_compile "$repo_root/astrbot/plugins/claude_runner_bridge/main.py"

echo "Structure check passed. All required files are present."
