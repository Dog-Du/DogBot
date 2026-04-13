#!/usr/bin/env bash
set -euo pipefail

files=(
  "compose/docker-compose.yml"
  "docker/claude-runner/Dockerfile"
  "docker/claude-runner/entrypoint.sh"
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

echo "Structure check passed. All required files are present."

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
ensure_pattern "compose/docker-compose.yml" "mem_limit" || pattern_errors=$((pattern_errors+1))
ensure_pattern "compose/docker-compose.yml" "CLAUDE_CONFIG_DIR" || pattern_errors=$((pattern_errors+1))

if [[ $pattern_errors -gt 0 ]]; then
  echo "Structure check failed due to missing scaffold markers." >&2
  exit 1
fi
