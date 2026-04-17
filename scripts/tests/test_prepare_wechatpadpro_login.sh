#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir_root="$(mktemp -d)"
trap 'rm -rf "$tmpdir_root"' EXIT

mk_fake_runner() {
  local case_name="$1"
  local working_dir="$2"
  local state_dir="$working_dir/state"
  local login_dir="$working_dir/wechat-login"

  mkdir -p "$login_dir" "$state_dir" "$working_dir/bin"
  cat >"$working_dir/bin/curl" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
max_time=""
previous_arg=""
for arg in "\$@"; do
  if [[ "\$previous_arg" == "--max-time" ]]; then
    max_time="\$arg"
  fi
  previous_arg="\$arg"
done
case "\$*" in
  *"/admin/GenAuthKey1"*)
    case "$case_name" in
      generate-key-budget)
        echo "\$max_time" >"\$state_dir/genauth_timeout"
        if [[ "\$max_time" != 0.* ]]; then
          sleep 2
        fi
        printf '{\"Code\":200,\"Data\":{\"authKeys\":[\"test-account-key\"]}}\n'
        ;;
      *)
        printf '{\"Code\":200,\"Data\":{\"authKeys\":[\"test-account-key\"]}}\n'
        ;;
    esac
    ;;
  *"/login/GetLoginQrCodePadX"*)
    case "$case_name" in
      success-refresh-online)
        qr_count=\$((\$(cat "\$state_dir/qr_count" 2>/dev/null || echo 0) + 1))
        echo "\$qr_count" >"\$state_dir/qr_count"
        if (( qr_count == 1 )); then
          printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,Zmlyc3Q=\",\"QrLink\":\"first-link\",\"QrCodeUrl\":\"first-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        else
          printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,c2Vjb25k\",\"QrLink\":\"second-link\",\"QrCodeUrl\":\"second-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        fi
        ;;
      already-logged-in-qr-fails)
        exit 1
        ;;
      invalid-qr-payload)
        printf '{\"Code\":500,\"Text\":\"bad qr\"}\\n'
        ;;
      verify-required|timeout-pending|base-url-slow|slow-status-budget)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,c2Vjb25k\",\"QrLink\":\"timeout-link\",\"QrCodeUrl\":\"timeout-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *"/login/GetLoginQrCodeNewX"*)
    case "$case_name" in
      already-logged-in-qr-fails)
        exit 1
        ;;
      invalid-qr-payload)
        printf '{\"Code\":500,\"Text\":\"bad qr\"}\\n'
        ;;
      *)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,c2Vjb25k\",\"QrLink\":\"fallback-link\",\"QrCodeUrl\":\"fallback-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
    esac
    ;;
  *"/login/GetLoginStatus"*)
    status_count=\$((\$(cat "\$state_dir/status_count" 2>/dev/null || echo 0) + 1))
    echo "\$status_count" >"\$state_dir/status_count"
    case "$case_name" in
      success-refresh-online)
        if (( status_count == 1 )); then
          printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        elif (( status_count == 2 )); then
          printf '{\"Code\":200,\"Text\":\"二维码已过期\",\"Data\":{\"Status\":\"expired\"}}\\n'
        else
          printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        fi
        ;;
      already-logged-in-qr-fails)
        printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        ;;
      generate-key-budget)
        printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        ;;
      invalid-qr-payload)
        printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        ;;
      verify-required)
        printf '{\"Code\":200,\"Text\":\"请完成辅助验证\",\"Data\":{\"Status\":\"pending\"}}\\n'
        ;;
      slow-status-budget)
        echo "\$max_time" >"\$state_dir/status_timeout"
        if [[ "\$max_time" != 0.* ]]; then
          sleep 2
        fi
        printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        ;;
      timeout-pending|base-url-slow)
        printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        ;;
      *)
        printf '{\"Code\":200,\"Text\":\"等待\",\"Data\":{\"Status\":\"pending\"}}\\n'
        ;;
    esac
    ;;
  *)
    case "$case_name" in
      base-url-slow)
        ready_count=\$((\$(cat "\$state_dir/ready_count" 2>/dev/null || echo 0) + 1))
        echo "\$ready_count" >"\$state_dir/ready_count"
        sleep 1
        if (( ready_count < 2 )); then
          exit 1
        fi
        ;;
    esac
    printf '<html>ok</html>\n'
    ;;
esac
EOF
  chmod +x "$working_dir/bin/curl"
  printf '%s\n' "$login_dir"
}

run_case() {
  local case_name="$1"
  local expected_exit_code="$2"
  local expected_message="$3"
  local login_timeout="${4:-5}"
  local wait_interval="${5:-0.1}"

  local case_dir="$tmpdir_root/$case_name"
  local env_file="$case_dir/dogbot.env"
  local login_dir
  local state_dir="$case_dir/state"
  local admin_key_line="WECHATPADPRO_ADMIN_KEY=test-admin-key"
  local account_key_line="WECHATPADPRO_ACCOUNT_KEY=test-account-key"

  mkdir -p "$case_dir"
  login_dir="$(mk_fake_runner "$case_name" "$case_dir")"
  case "$case_name" in
    already-logged-in-qr-fails|invalid-qr-payload|verify-required|timeout-pending|base-url-slow|slow-status-budget)
      admin_key_line=""
      ;;
    generate-key-budget)
      account_key_line=""
      ;;
  esac
  cat >"$env_file" <<EOF
ENABLE_WECHATPADPRO=1
$admin_key_line
$account_key_line
WECHATPADPRO_BASE_URL=http://127.0.0.1:38849
WECHATPADPRO_LOGIN_OUTPUT_DIR=$login_dir
DOGBOT_LOGIN_TIMEOUT_SECS=$login_timeout
DOGBOT_WAIT_INTERVAL_SECS=$wait_interval
EOF

  local output
  local status
  set +e
  output="$(
    PATH="$case_dir/bin:$PATH" \
      UV_BIN="$(command -v uv)" \
      "$repo_root/scripts/prepare_wechatpadpro_login.sh" "$env_file" 2>&1
  )"
  status=$?
  set -e

  if [[ "$status" -ne "$expected_exit_code" ]]; then
    echo "FAIL: case '$case_name' expected exit $expected_exit_code but got $status" >&2
    echo "$output" >&2
    exit 1
  fi

  if [[ "$expected_message" ]]; then
    grep -q "$expected_message" <<<"$output" || {
      echo "FAIL: case '$case_name' missing expected output '$expected_message'" >&2
      echo "$output" >&2
      exit 1
    }
  fi

  echo "$output"

  case "$case_name" in
    success-refresh-online)
      grep -q 'WeChatPadPro QR link: second-link' <<<"$output"
      grep -q '"qr_link": "second-link"' "$login_dir/wechatpadpro-login-meta.json"
      printf 'c2Vjb25k' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
      ;;
    already-logged-in-qr-fails)
      if [[ -f "$state_dir/qr_count" ]]; then
        echo "FAIL: case '$case_name' should not request a QR code when status is already online" >&2
        cat "$state_dir/qr_count" >&2
        exit 1
      fi
      if grep -q 'WeChatPadPro QR link:' <<<"$output"; then
        echo "FAIL: case '$case_name' should not print QR output when status is already online" >&2
        echo "$output" >&2
        exit 1
      fi
      ;;
    slow-status-budget)
      python3 - <<'PY' "$state_dir/status_timeout"
from pathlib import Path
value = float(Path(__import__("sys").argv[1]).read_text().strip())
if not (0.0 < value < 1.0):
    raise SystemExit(f"expected GetLoginStatus --max-time to be < 1.0s near the deadline, got {value}")
PY
      ;;
    generate-key-budget)
      python3 - <<'PY' "$state_dir/genauth_timeout"
from pathlib import Path
value = float(Path(__import__("sys").argv[1]).read_text().strip())
if not (0.0 < value < 1.0):
    raise SystemExit(f"expected GenAuthKey1 --max-time to be < 1.0s near the deadline, got {value}")
PY
      grep -q '^WECHATPADPRO_ACCOUNT_KEY=test-account-key$' "$env_file"
      ;;
  esac

  if compgen -G "$repo_root/wechatpadpro_login_err.*.log" >/dev/null; then
    echo "FAIL: case '$case_name' leaked wechatpadpro_login_err temporary files into the repo root" >&2
    ls "$repo_root"/wechatpadpro_login_err.*.log >&2
    exit 1
  fi
}

run_case success-refresh-online 0 "WeChatPadPro QR link: second-link" 5 0.1

run_case already-logged-in-qr-fails 0 "WeChatPadPro account is already logged in for key: test-account-key" 5 0.1
run_case invalid-qr-payload 1 "Failed to fetch WeChatPadPro login QR." 5 0.1
run_case verify-required 1 "WeChatPadPro login requires additional verification."
run_case timeout-pending 1 "WeChatPadPro login did not complete within 1 seconds." 1 0.05
run_case base-url-slow 1 "WeChatPadPro API did not become ready at http://127.0.0.1:38849 within 1 seconds." 1 0.05
run_case slow-status-budget 1 "WeChatPadPro login did not complete within 1 seconds." 1 0.05
run_case generate-key-budget 0 "Generated WECHATPADPRO_ACCOUNT_KEY and persisted it to" 1 0.05
