#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"
env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"
uv_bin="$(dogbot_resolve_uv_bin)"

if [[ "${ENABLE_WECHATPADPRO:-0}" != "1" ]]; then
  echo "WeChatPadPro is disabled; skip login preparation."
  exit 0
fi

base_url="${WECHATPADPRO_BASE_URL:-http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}}"
login_dir="${WECHATPADPRO_LOGIN_OUTPUT_DIR:-${AGENT_STATE_DIR:-$repo_root/agent-state}/wechatpadpro-login}"
mkdir -p "$login_dir"
login_timeout_secs="${DOGBOT_LOGIN_TIMEOUT_SECS:-100}"
deadline_epoch="$(dogbot_deadline_in "$login_timeout_secs")"
deadline_epoch_ns="$(( $(date +%s%N) + login_timeout_secs * 1000000000 ))"
login_started_at_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

wechatpadpro_remaining_request_timeout() {
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

wechatpadpro_http_ready() {
  local request_timeout
  request_timeout="$(wechatpadpro_remaining_request_timeout)" || return 1
  curl -fsSL --max-time "$request_timeout" -o /dev/null "$base_url" >/dev/null 2>&1
}

if ! dogbot_wait_until_deadline "$deadline_epoch" wechatpadpro_http_ready; then
  echo "WeChatPadPro API did not become ready at $base_url within ${login_timeout_secs} seconds." >&2
  exit 1
fi

ensure_account_key() {
  if [[ -n "${WECHATPADPRO_ACCOUNT_KEY:-}" ]]; then
    return 0
  fi

  if [[ -z "${WECHATPADPRO_ADMIN_KEY:-}" ]]; then
    echo "WECHATPADPRO_ADMIN_KEY is not set." >&2
    exit 1
  fi

  generate_account_key "Generated"
}

generate_account_key() {
  local action_word="$1"
  local response account_key request_timeout
  if [[ -z "${WECHATPADPRO_ADMIN_KEY:-}" ]]; then
    echo "WECHATPADPRO_ADMIN_KEY is not set." >&2
    exit 1
  fi
  request_timeout="$(wechatpadpro_remaining_request_timeout)" || {
    echo "WeChatPadPro login did not complete within ${login_timeout_secs} seconds." >&2
    exit 1
  }
  response="$(curl --max-time "$request_timeout" -fsS -X POST \
    "${base_url}/admin/GenAuthKey1?key=${WECHATPADPRO_ADMIN_KEY}" \
    -H 'Content-Type: application/json' \
    -d '{"Count":1,"Days":365}')"

  account_key="$("$uv_bin" run python - <<'PY' "$response"
import json, sys
payload = json.loads(sys.argv[1])
keys = ((payload.get("Data") or {}).get("authKeys") or [])
if not keys:
    raise SystemExit("failed to create account key")
print(keys[0])
PY
)"

  WECHATPADPRO_ACCOUNT_KEY="$account_key"
  if grep -q '^WECHATPADPRO_ACCOUNT_KEY=' "$env_file"; then
    sed -i "s|^WECHATPADPRO_ACCOUNT_KEY=.*$|WECHATPADPRO_ACCOUNT_KEY=${account_key}|" "$env_file"
  else
    printf '\nWECHATPADPRO_ACCOUNT_KEY=%s\n' "$account_key" >>"$env_file"
  fi
  echo "${action_word} WECHATPADPRO_ACCOUNT_KEY and persisted it to $env_file"
}

refresh_account_key() {
  generate_account_key "Refreshed"
}

persist_account_key() {
  local account_key="$1"
  local action_word="${2:-Updated}"

  [[ -n "$account_key" ]] || return 0
  if [[ "${WECHATPADPRO_ACCOUNT_KEY:-}" == "$account_key" ]]; then
    return 0
  fi

  WECHATPADPRO_ACCOUNT_KEY="$account_key"
  if grep -q '^WECHATPADPRO_ACCOUNT_KEY=' "$env_file"; then
    sed -i "s|^WECHATPADPRO_ACCOUNT_KEY=.*$|WECHATPADPRO_ACCOUNT_KEY=${account_key}|" "$env_file"
  else
    printf '\nWECHATPADPRO_ACCOUNT_KEY=%s\n' "$account_key" >>"$env_file"
  fi
  echo "${action_word} WECHATPADPRO_ACCOUNT_KEY and persisted it to $env_file"
}

request_login_qr() {
  local endpoint="$1"
  local request_timeout
  request_timeout="$(wechatpadpro_remaining_request_timeout)" || return 1
  curl --max-time "$request_timeout" -fsS -X POST \
    "${base_url}${endpoint}?key=${WECHATPADPRO_ACCOUNT_KEY}" \
    -H 'Content-Type: application/json' \
    -d '{}'
}

ensure_account_key

last_qr_link=""
login_err_log="$(mktemp -p "${TMPDIR:-$repo_root}" wechatpadpro_login_err.XXXXXX.log 2>/dev/null || mktemp -p "$repo_root" wechatpadpro_login_err.XXXXXX.log)"

cleanup_login_err_log() {
  rm -f "$login_err_log"
}

trap cleanup_login_err_log EXIT

write_login_artifacts() {
  local response="$1"
  local qr_info_output
  qr_info_output="$("$uv_bin" run python - <<'PY' "$response" "$login_dir" "$WECHATPADPRO_ACCOUNT_KEY"
import base64, json, pathlib, sys
import datetime
payload = json.loads(sys.argv[1])
login_dir = pathlib.Path(sys.argv[2])
account_key = sys.argv[3]
data = payload.get("Data") or {}
if str(payload.get("Code") or "") != "200":
    raise SystemExit("qr request did not return Code=200")
img = (data.get("qrCodeBase64") or "").removeprefix("data:image/png;base64,")
if not img:
    raise SystemExit("qrCodeBase64 missing from response")
png_path = login_dir / "wechatpadpro-login-qr.png"
meta_path = login_dir / "wechatpadpro-login-meta.json"
png_path.write_bytes(base64.b64decode(img))
meta = {
    "account_key": data.get("Key") or account_key,
    "qr_link": data.get("QrLink"),
    "qr_code_url": data.get("QrCodeUrl"),
    "expires_in": data.get("expiredTime"),
    "png_path": str(png_path),
    "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
}
meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2))
print(meta["qr_link"])
print(png_path)
print(meta_path)
print(meta["account_key"] or "")
PY
)"
  local -a qr_info=()
  mapfile -t qr_info <<<"$qr_info_output"
  if (( ${#qr_info[@]} < 4 )); then
    return 1
  fi
  printf '%s\n' "${qr_info[@]}"
}

try_fetch_login_qr_endpoint() {
  local endpoint="$1"
  local response qr_info_output
  response="$(request_login_qr "$endpoint" 2>>"$login_err_log")" || return 1
  qr_info_output="$(write_login_artifacts "$response" 2>>"$login_err_log")" || return 1
  printf '%s\n' "$qr_info_output"
}

fetch_login_qr() {
  local qr_info_output qr_link png_path meta_path
  local -a qr_info=()
  : >"$login_err_log"
  qr_info_output="$(try_fetch_login_qr_endpoint "/login/GetLoginQrCodeNewX")" \
    || qr_info_output="$(try_fetch_login_qr_endpoint "/login/GetLoginQrCodePadX")" \
    || return 1
  mapfile -t qr_info <<<"$qr_info_output"
  if (( ${#qr_info[@]} < 4 )); then
    return 1
  fi
  persist_account_key "${qr_info[3]:-}" "Updated"
  qr_link="${qr_info[0]}"
  png_path="${qr_info[1]}"
  meta_path="${qr_info[2]}"

  if [[ "$qr_link" != "$last_qr_link" ]]; then
    echo "WeChatPadPro login QR image: $png_path"
    echo "WeChatPadPro login QR meta: $meta_path"
    echo "WeChatPadPro QR link: $qr_link"
    dogbot_print_qr_if_possible "$qr_link"
    last_qr_link="$qr_link"
  fi
}

fetch_login_qr_with_key_recovery() {
  if fetch_login_qr; then
    return 0
  fi

  if [[ -z "${WECHATPADPRO_ADMIN_KEY:-}" ]]; then
    return 1
  fi

  if (( account_key_refresh_count >= account_key_refresh_limit )); then
    return 1
  fi

  refresh_account_key
  account_key_refresh_count=$((account_key_refresh_count + 1))
  last_qr_link=""
  fetch_login_qr
}

current_login_state() {
  local response request_timeout
  request_timeout="$(wechatpadpro_remaining_request_timeout)" || return 1
  response="$(curl --max-time "$request_timeout" -fsS "${base_url}/login/GetLoginStatus?key=${WECHATPADPRO_ACCOUNT_KEY}")"
  "$uv_bin" run python - <<'PY' "$response"
import json, sys
payload = json.loads(sys.argv[1])
code = str(payload.get("Code") or "")
text = str(payload.get("Text") or "")
data = payload.get("Data") or {}
status = str(data.get("Status") or data.get("status") or "")
wxid = str(data.get("wxid") or data.get("Wxid") or "")
login_state = str(data.get("loginState") or data.get("LoginState") or "")
combined = f"{text} {status}".lower()
if code == "-2" or "不存在" in text or ("key已过期" in text) or ("key 已过期" in text) or ("请重新申请" in text):
    print("stale-key")
elif "退出微信" in text or "已退出" in text or "logged out" in combined:
    print("logged-out")
elif "验证码" in text or "辅助" in text:
    print("verify-required")
elif "过期" in text or "expired" in combined:
    print("expired")
elif login_state == "1" or "已登录" in text or "在线" in text or "已绑定" in text or "online" in combined or "bound" in combined or wxid.startswith("wxid_"):
    print("online")
else:
    print("pending")
PY
}

recent_wechatpadpro_diag_logs() {
  if [[ -n "${WECHATPADPRO_DIAG_LOG_FILE:-}" ]]; then
    [[ -f "$WECHATPADPRO_DIAG_LOG_FILE" ]] || return 1
    tail -n 200 "$WECHATPADPRO_DIAG_LOG_FILE"
    return 0
  fi

  command -v docker >/dev/null 2>&1 || return 1

  local container_name="${WECHATPADPRO_CONTAINER_NAME:-wechatpadpro}"
  docker logs --since "$login_started_at_iso" "$container_name" 2>&1 || return 1
}

current_login_blocker() {
  local diag_logs
  diag_logs="$(recent_wechatpadpro_diag_logs 2>/dev/null || true)"
  [[ -n "$diag_logs" ]] || return 0

  "$uv_bin" run python - <<'PY' "${WECHATPADPRO_ACCOUNT_KEY:-}" "$diag_logs"
import re
import sys

current_key = sys.argv[1]
log_text = sys.argv[2]

if not current_key or not log_text.strip():
    raise SystemExit(0)

key_markers = (
    current_key,
    f"UUID={current_key}",
    f"AuthKey: {current_key}",
    f"by {current_key}",
)
blocker_re = re.compile(r"当前客户端版本过低|版本过低")
key_window = 20
lines = log_text.splitlines()
last_key_index = -10**9

for index, line in enumerate(lines):
    if any(marker in line for marker in key_markers):
        last_key_index = index
    if blocker_re.search(line) and index - last_key_index <= key_window:
        print(f"client-version-too-low\t{line}")
        raise SystemExit(0)
PY
}

report_qr_failure() {
  local action="$1"
  local state_after_failure=""
  state_after_failure="$(current_login_state 2>/dev/null || true)"
  if [[ "$state_after_failure" == "online" ]]; then
    echo "WeChatPadPro account is already logged in for key: $WECHATPADPRO_ACCOUNT_KEY"
    exit 0
  fi

  if [[ "$action" == "refresh" ]]; then
    echo "Failed to refresh WeChatPadPro login QR." >&2
  else
    echo "Failed to fetch WeChatPadPro login QR." >&2
  fi
  cat "$login_err_log" >&2 || true
  exit 1
}

qr_prepared=0
account_key_refresh_count=0
account_key_refresh_limit=3

while (( $(date +%s) < deadline_epoch )); do
  blocker_info="$(current_login_blocker 2>/dev/null || true)"
  if [[ -n "$blocker_info" ]]; then
    blocker_type="${blocker_info%%$'\t'*}"
    blocker_detail="${blocker_info#*$'\t'}"
    case "$blocker_type" in
      client-version-too-low)
        echo "WeChatPadPro login blocked: current client version is too low." >&2
        exit 1
        ;;
    esac
  fi

  case "$(current_login_state 2>/dev/null || echo pending)" in
    online)
      echo "WeChatPadPro account is already logged in for key: $WECHATPADPRO_ACCOUNT_KEY"
      exit 0
      ;;
    expired)
      fetch_login_qr_with_key_recovery || report_qr_failure refresh
      qr_prepared=1
      ;;
    verify-required)
      echo "WeChatPadPro login requires additional verification." >&2
      exit 1
      ;;
    logged-out)
      if [[ "$qr_prepared" != "1" ]]; then
        fetch_login_qr_with_key_recovery || report_qr_failure fetch
        qr_prepared=1
      fi
      ;;
    stale-key|pending)
      if [[ "$qr_prepared" != "1" ]]; then
        fetch_login_qr_with_key_recovery || report_qr_failure fetch
        qr_prepared=1
      fi
      ;;
  esac
  sleep "${DOGBOT_WAIT_INTERVAL_SECS:-1}"
done

echo "WeChatPadPro login did not complete within ${login_timeout_secs} seconds." >&2
exit 1
