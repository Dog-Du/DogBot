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
