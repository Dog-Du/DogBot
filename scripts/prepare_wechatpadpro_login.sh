#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="${1:-$repo_root/deploy/myqqbot.env}"

if [[ ! -f "$env_file" ]]; then
  echo "Missing env file: $env_file" >&2
  exit 1
fi

set -a
source "$env_file"
set +a

if [[ "${ENABLE_WECHATPADPRO:-0}" != "1" ]]; then
  echo "WeChatPadPro is disabled; skip login preparation."
  exit 0
fi

if [[ -z "${WECHATPADPRO_ADMIN_KEY:-}" ]]; then
  echo "WECHATPADPRO_ADMIN_KEY is not set." >&2
  exit 1
fi

base_url="${WECHATPADPRO_BASE_URL:-http://127.0.0.1:${WECHATPADPRO_HOST_PORT:-38849}}"
login_dir="${WECHATPADPRO_LOGIN_OUTPUT_DIR:-${AGENT_STATE_DIR:-$repo_root/agent-state}/wechatpadpro-login}"
mkdir -p "$login_dir"

ensure_account_key() {
  if [[ -n "${WECHATPADPRO_ACCOUNT_KEY:-}" ]]; then
    return 0
  fi

  local response account_key
  response="$(curl --max-time 15 -fsS -X POST \
    "${base_url}/admin/GenAuthKey1?key=${WECHATPADPRO_ADMIN_KEY}" \
    -H 'Content-Type: application/json' \
    -d '{"Count":1,"Days":365}')"

  account_key="$(python3 - <<'PY' "$response"
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
  echo "Generated WECHATPADPRO_ACCOUNT_KEY and persisted it to $env_file"
}

request_login_qr() {
  local endpoint="$1"
  curl --max-time 20 -fsS -X POST \
    "${base_url}${endpoint}?key=${WECHATPADPRO_ACCOUNT_KEY}" \
    -H 'Content-Type: application/json' \
    -d '{}'
}

ensure_account_key

response=""
if ! response="$(request_login_qr "/login/GetLoginQrCodePadX" 2>/tmp/wechatpadpro_login_err.log)"; then
  if ! response="$(request_login_qr "/login/GetLoginQrCodeNewX" 2>>/tmp/wechatpadpro_login_err.log)"; then
    echo "Failed to fetch WeChatPadPro login QR." >&2
    cat /tmp/wechatpadpro_login_err.log >&2 || true
    exit 1
  fi
fi

python3 - <<'PY' "$response" "$login_dir" "$WECHATPADPRO_ACCOUNT_KEY"
import base64
import json
import pathlib
import sys

payload = json.loads(sys.argv[1])
login_dir = pathlib.Path(sys.argv[2])
account_key = sys.argv[3]

if payload.get("Code") != 200:
    text = payload.get("Text", "")
    if "已登录" in text or "在线" in text or "已绑定微信号" in text:
        print(f"WeChatPadPro account is already logged in for key: {account_key}")
        raise SystemExit(0)
    raise SystemExit(text or f"unexpected response: {payload}")

data = payload.get("Data") or {}
img = data.get("qrCodeBase64", "")
prefix = "data:image/png;base64,"
if img.startswith(prefix):
    img = img[len(prefix):]

png_path = login_dir / "wechatpadpro-login-qr.png"
meta_path = login_dir / "wechatpadpro-login-meta.json"

png_path.write_bytes(base64.b64decode(img))
meta = {
    "uuid": data.get("uuid"),
    "qr_link": data.get("QrLink"),
    "qr_code_url": data.get("QrCodeUrl"),
    "expires_in": data.get("expiredTime"),
    "account_key": data.get("Key") or account_key,
    "png_path": str(png_path),
}
meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2))

print(f"WeChatPadPro account key: {meta['account_key']}")
print(f"WeChatPadPro login QR image: {png_path}")
print(f"WeChatPadPro login QR meta: {meta_path}")
print(f"WeChatPadPro QR link: {meta['qr_link']}")
print(f"WeChatPadPro QR image URL: {meta['qr_code_url']}")
print(f"WeChatPadPro QR expires in: {meta['expires_in']} seconds")
PY
