#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"

env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"

if [[ "${ENABLE_QQ:-0}" != "1" ]]; then
  echo "QQ is disabled; skip NapCat login preparation."
  exit 0
fi

uv_bin="$(dogbot_resolve_uv_bin)"
login_timeout_secs="${DOGBOT_LOGIN_TIMEOUT_SECS:-100}"
container_name="${NAPCAT_CONTAINER_NAME:-napcat}"
login_dir="${NAPCAT_LOGIN_OUTPUT_DIR:-${AGENT_STATE_DIR:-$repo_root/agent-state}/napcat-login}"
qr_png_path="$login_dir/napcat-login-qr.png"
meta_path="$login_dir/napcat-login-meta.txt"
mkdir -p "$login_dir"

deadline_epoch="$(dogbot_deadline_in "$login_timeout_secs")"
deadline_epoch_ns="$(( $(date +%s%N) + login_timeout_secs * 1000000000 ))"
login_started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
last_login_url=""

rm -f "$qr_png_path" "$meta_path"

napcat_remaining_request_timeout() {
  local remaining_ns=$(( deadline_epoch_ns - $(date +%s%N) ))
  if (( remaining_ns <= 0 )); then
    return 1
  fi
  local remaining_ms=$(( (remaining_ns + 999999) / 1000000 ))
  if (( remaining_ms <= 0 )); then
    remaining_ms=1
  fi
  printf '%s.%03d\n' "$(( remaining_ms / 1000 ))" "$(( remaining_ms % 1000 ))"
}

napcat_extract_login_url_from_logs() {
  docker logs "$@" 2>&1 \
    | grep -o 'https://txz\.qq\.com/p?k=[^[:space:]]*' \
    | tail -n1 || true
}

napcat_fetch_login_url() {
  local login_url
  login_url="$(napcat_extract_login_url_from_logs --since "$login_started_at" "$container_name")"
  if [[ -n "$login_url" ]]; then
    printf '%s\n' "$login_url"
    return 0
  fi

  napcat_extract_login_url_from_logs "$container_name"
}

napcat_write_artifacts() {
  local login_url="$1"
  [[ -n "$login_url" ]] || return 1
  docker exec "$container_name" test -f /app/napcat/cache/qrcode.png >/dev/null 2>&1 || return 1
  docker cp "$container_name:/app/napcat/cache/qrcode.png" "$qr_png_path" >/dev/null 2>&1 || return 1
  cat >"$meta_path" <<EOF
container=$container_name
login_url=$login_url
qr_png_path=$qr_png_path
generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
}

napcat_refresh_qr() {
  local login_url
  login_url="$(napcat_fetch_login_url)"
  [[ -n "$login_url" ]] || return 1
  napcat_write_artifacts "$login_url" || return 1
  if [[ "$login_url" != "$last_login_url" ]]; then
    echo "NapCat login QR image: $qr_png_path"
    echo "NapCat login meta: $meta_path"
    echo "NapCat login URL: $login_url"
    dogbot_print_qr_if_possible "$login_url"
    last_login_url="$login_url"
  fi
}

napcat_login_succeeded() {
  local response request_timeout
  request_timeout="$(napcat_remaining_request_timeout)" || return 1
  response="$(curl --connect-timeout "$request_timeout" --max-time "$request_timeout" -fsS -X POST \
    "${NAPCAT_API_BASE_URL%/}/get_login_info" \
    -H 'Content-Type: application/json' \
    -d '{}' 2>/dev/null)" || return 1

  "$uv_bin" run python - <<'PY' "$response"
import json, sys
try:
    payload = json.loads(sys.argv[1])
except json.JSONDecodeError:
    raise SystemExit(1)
data = payload.get("data") or {}
user_id = str(data.get("user_id") or "").strip()
raise SystemExit(0 if user_id else 1)
PY
}

qr_prepared=0

while (( $(date +%s%N) < deadline_epoch_ns )); do
  if napcat_login_succeeded; then
    echo "NapCat login confirmed."
    exit 0
  fi

  if napcat_refresh_qr; then
    qr_prepared=1
  fi

  sleep "${DOGBOT_WAIT_INTERVAL_SECS:-1}"
done

if [[ "$qr_prepared" != "1" ]]; then
  echo "NapCat login QR was not refreshed within ${login_timeout_secs} seconds." >&2
else
  echo "NapCat login did not complete within ${login_timeout_secs} seconds." >&2
fi
exit 1
