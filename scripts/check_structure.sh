#!/usr/bin/env bash
set -euo pipefail

files=(
  "compose/docker-compose.yml"
  "docker/claude-runner/Dockerfile"
  "docker/claude-runner/entrypoint.sh"
)

missing=()
for file in "${files[@]}"; do
  if [[ ! -f $file ]]; then
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
