#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
env_example="$repo_root/deploy/dogbot.env.example"
deploy_script="$repo_root/scripts/deploy_stack.sh"

if ! grep -q '^DOGBOT_PRUNE_LEGACY_CLAUDE_CONTENT_ON_DEPLOY=0$' "$env_example"; then
  echo "FAIL: deploy example config must keep legacy Claude content pruning opt-in" >&2
  exit 1
fi

if ! grep -q 'DOGBOT_PRUNE_LEGACY_CLAUDE_CONTENT_ON_DEPLOY' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must support DOGBOT_PRUNE_LEGACY_CLAUDE_CONTENT_ON_DEPLOY" >&2
  exit 1
fi

if ! grep -q 'cleanup_legacy_claude_content.py' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must be able to prune legacy Claude content via cleanup script" >&2
  exit 1
fi

echo "legacy Claude cleanup deploy checks passed."
