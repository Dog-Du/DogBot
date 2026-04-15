#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"

env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"

if [[ "${ENABLE_QQ:-0}" != "1" ]]; then
  echo "QQ is disabled; skip NapCat login preparation."
  exit 0
fi

container_name="${NAPCAT_CONTAINER_NAME:-napcat}"
login_dir="${NAPCAT_LOGIN_OUTPUT_DIR:-${AGENT_STATE_DIR:-$dogbot_repo_root/agent-state}/napcat-login}"
mkdir -p "$login_dir"

log_output="$(docker logs --tail 200 "$container_name" 2>&1 || true)"
login_url="$(grep -o 'https://txz\.qq\.com/p?k=[^[:space:]]*' <<<"$log_output" | tail -n1 || true)"
qr_png_path="$login_dir/napcat-login-qr.png"
meta_path="$login_dir/napcat-login-meta.txt"

if docker exec "$container_name" test -f /app/napcat/cache/qrcode.png >/dev/null 2>&1; then
  docker cp "$container_name:/app/napcat/cache/qrcode.png" "$qr_png_path" >/dev/null 2>&1 || true
fi

{
  echo "container=$container_name"
  echo "login_url=$login_url"
  echo "qr_png_path=$qr_png_path"
} >"$meta_path"

echo "NapCat login QR image: $qr_png_path"
echo "NapCat login meta: $meta_path"
if [[ -n "$login_url" ]]; then
  echo "NapCat login URL: $login_url"
  dogbot_print_qr_if_possible "$login_url"
else
  echo "NapCat login URL not found in recent logs yet." >&2
fi
