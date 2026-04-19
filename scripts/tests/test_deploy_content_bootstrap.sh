#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
common_sh="$repo_root/scripts/lib/common.sh"
deploy_script="$repo_root/scripts/deploy_stack.sh"
env_example="$repo_root/deploy/dogbot.env.example"

# shellcheck source=../lib/common.sh
source "$common_sh"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

src_dir="$tmpdir/source"
dest_dir="$tmpdir/dest"
mkdir -p "$src_dir/packs/base" "$dest_dir/obsolete"
printf '{"pack_id":"base"}\n' >"$src_dir/packs/base/manifest.json"
printf 'stale\n' >"$dest_dir/obsolete/old.txt"

dogbot_sync_content_root "$src_dir" "$dest_dir"

if [[ ! -f "$dest_dir/packs/base/manifest.json" ]]; then
  echo "FAIL: dogbot_sync_content_root should copy content packs into DOGBOT_CONTENT_ROOT" >&2
  exit 1
fi

if [[ -e "$dest_dir/obsolete/old.txt" ]]; then
  echo "FAIL: dogbot_sync_content_root should remove stale files from DOGBOT_CONTENT_ROOT" >&2
  exit 1
fi

if ! grep -q '^DOGBOT_SYNC_CONTENT_ON_DEPLOY=1$' "$env_example"; then
  echo "FAIL: deploy example config must enable content sync on deploy by default" >&2
  exit 1
fi

if ! grep -q '^DOGBOT_REFRESH_CONTENT_ON_DEPLOY=0$' "$env_example"; then
  echo "FAIL: deploy example config must keep upstream refresh opt-in by default" >&2
  exit 1
fi

if ! grep -q 'DOGBOT_REFRESH_CONTENT_ON_DEPLOY' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must support DOGBOT_REFRESH_CONTENT_ON_DEPLOY" >&2
  exit 1
fi

if ! grep -q 'sync_content_sources.py' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must be able to refresh content packs via sync_content_sources.py" >&2
  exit 1
fi

if ! grep -q 'dogbot_sync_content_root' "$deploy_script"; then
  echo "FAIL: deploy_stack.sh must sync repo content into DOGBOT_CONTENT_ROOT before startup" >&2
  exit 1
fi

echo "deploy content bootstrap checks passed."
