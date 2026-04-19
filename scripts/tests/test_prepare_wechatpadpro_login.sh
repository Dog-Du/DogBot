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
      stale-key-regenerates|expired-key-regenerates)
        printf '{\"Code\":200,\"Data\":{\"authKeys\":[\"fresh-account-key\"]}}\n'
        ;;
      *)
        printf '{\"Code\":200,\"Data\":{\"authKeys\":[\"test-account-key\"]}}\n'
        ;;
    esac
    ;;
  *"/login/GetLoginQrCodePadX"*)
    case "$case_name" in
      prefer-newx-over-padx)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,cGFkeA==\",\"QrLink\":\"padx-link\",\"QrCodeUrl\":\"padx-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      success-refresh-online)
        qr_count=\$((\$(cat "\$state_dir/qr_count" 2>/dev/null || echo 0) + 1))
        echo "\$qr_count" >"\$state_dir/qr_count"
        if (( qr_count == 1 )); then
          printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,Zmlyc3Q=\",\"QrLink\":\"first-link\",\"QrCodeUrl\":\"first-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        else
          printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,c2Vjb25k\",\"QrLink\":\"second-link\",\"QrCodeUrl\":\"second-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        fi
        ;;
      stale-status-newx-success)
        printf '{\"Code\":300,\"Text\":\"检查扫码状态失败！err:SendCheckLoginQrcodeRequest err: wxconn.isConnected == false\"}\n'
        ;;
      padx-fallback-to-newx)
        printf '{\"Code\":300,\"Text\":\"padx failed\"}\n'
        ;;
      stale-key-regenerates|expired-key-regenerates)
        if [[ "\$*" == *"fresh-account-key"* ]]; then
          printf '{\"Code\":300,\"Text\":\"padx failed\"}\n'
        else
          printf '{\"Code\":300,\"Text\":\"获取二维码失败！err:DecodePackHeader err: len(respData) <= 32\"}\n'
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
      loginstate-online)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,bG9naW4=\",\"QrLink\":\"loginstate-link\",\"QrCodeUrl\":\"loginstate-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *"/login/GetLoginQrCodeNewX"*)
    case "$case_name" in
      success-refresh-online)
        exit 1
        ;;
      prefer-newx-over-padx)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,bmV3eA==\",\"QrLink\":\"newx-link\",\"QrCodeUrl\":\"newx-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      stale-status-newx-success)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,c3RhbGU=\",\"QrLink\":\"stale-link\",\"QrCodeUrl\":\"stale-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      padx-fallback-to-newx)
        printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,bmV3eA==\",\"QrLink\":\"newx-link\",\"QrCodeUrl\":\"newx-url\",\"expiredTime\":30,\"Key\":\"test-account-key\"}}\\n'
        ;;
      stale-key-regenerates|expired-key-regenerates)
        if [[ "\$*" == *"fresh-account-key"* ]]; then
          printf '{\"Code\":200,\"Data\":{\"qrCodeBase64\":\"data:image/png;base64,ZnJlc2g=\",\"QrLink\":\"fresh-link\",\"QrCodeUrl\":\"fresh-url\",\"expiredTime\":30,\"Key\":\"fresh-account-key\"}}\\n'
        else
          printf '{\"Code\":300,\"Text\":\"获取二维码失败！err:DecodePackHeader err: len(respData) <= 32\"}\n'
        fi
        ;;
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
      prefer-newx-over-padx)
        if (( status_count == 1 )); then
          printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        else
          printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        fi
        ;;
      success-refresh-online)
        if (( status_count == 1 )); then
          printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        elif (( status_count == 2 )); then
          printf '{\"Code\":200,\"Text\":\"二维码已过期\",\"Data\":{\"Status\":\"expired\"}}\\n'
        else
          printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        fi
        ;;
      stale-status-newx-success)
        if (( status_count == 1 )); then
          printf '{\"Code\":-2,\"Data\":null,\"Text\":\"test-account-key 该链接不存在！\"}\n'
        else
          printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        fi
        ;;
      padx-fallback-to-newx)
        if (( status_count == 1 )); then
          printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
        else
          printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        fi
        ;;
      already-logged-in-qr-fails)
        printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
        ;;
      stale-key-regenerates|expired-key-regenerates)
        if [[ "\$*" == *"stale-account-key"* ]]; then
          printf '{\"Code\":-2,\"Data\":null,\"Text\":\"stale-account-key 该链接不存在！\"}\n'
        elif [[ "\$*" == *"expired-account-key"* ]]; then
          printf '{\"Code\":300,\"Data\":null,\"Text\":\"该key已过期，请重新申请\"}\n'
        else
          fresh_status_count=\$((\$(cat "\$state_dir/fresh_status_count" 2>/dev/null || echo 0) + 1))
          echo "\$fresh_status_count" >"\$state_dir/fresh_status_count"
          if (( fresh_status_count == 1 )); then
            printf '{\"Code\":200,\"Text\":\"等待扫码\",\"Data\":{\"Status\":\"pending\"}}\\n'
          else
            printf '{\"Code\":200,\"Text\":\"已登录\",\"Data\":{\"Status\":\"online\",\"wxid\":\"wxid_bot\"}}\\n'
          fi
        fi
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
      loginstate-online)
        printf '{\"Code\":200,\"Text\":\"\",\"Data\":{\"loginState\":1,\"loginTime\":\"2026-04-19 05:36:04\"}}\\n'
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
  local diag_log_line=""

  mkdir -p "$case_dir"
  login_dir="$(mk_fake_runner "$case_name" "$case_dir")"
  case "$case_name" in
    already-logged-in-qr-fails|invalid-qr-payload|verify-required|timeout-pending|base-url-slow|slow-status-budget|client-version-too-low|stale-unrelated-blocker)
      admin_key_line=""
      ;;
    generate-key-budget)
      account_key_line=""
      ;;
    stale-key-regenerates)
      account_key_line="WECHATPADPRO_ACCOUNT_KEY=stale-account-key"
      ;;
    expired-key-regenerates)
      account_key_line="WECHATPADPRO_ACCOUNT_KEY=expired-account-key"
      ;;
  esac
  case "$case_name" in
    client-version-too-low)
      cat >"$state_dir/wechatpadpro.log" <<'EOF'
成功添加连接: UUID=test-account-key, ConnID=1
GET Connection locfree success by test-account-key
2026-04-17 11:39:39: <e>
<ShowType>1</ShowType>
<Content><![CDATA[当前客户端版本过低，请前往应用商店升级到最新版本客户端后再登录。]]></Content>
</e>
EOF
      diag_log_line="WECHATPADPRO_DIAG_LOG_FILE=$state_dir/wechatpadpro.log"
      ;;
    stale-unrelated-blocker)
      cat >"$state_dir/wechatpadpro.log" <<'EOF'
成功添加连接: UUID=some-other-key, ConnID=1
2026-04-17 11:39:39: <e>
<ShowType>1</ShowType>
<Content><![CDATA[当前客户端版本过低，请前往应用商店升级到最新版本客户端后再登录。]]></Content>
</e>
EOF
      diag_log_line="WECHATPADPRO_DIAG_LOG_FILE=$state_dir/wechatpadpro.log"
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
$diag_log_line
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
    stale-status-newx-success)
      grep -q 'WeChatPadPro QR link: stale-link' <<<"$output"
      grep -q '^WECHATPADPRO_ACCOUNT_KEY=test-account-key$' "$env_file"
      printf 'c3RhbGU=' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
      ;;
    padx-fallback-to-newx)
      grep -q 'WeChatPadPro QR link: newx-link' <<<"$output"
      grep -q '"qr_link": "newx-link"' "$login_dir/wechatpadpro-login-meta.json"
      printf 'bmV3eA==' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
      ;;
    prefer-newx-over-padx)
      grep -q 'WeChatPadPro QR link: newx-link' <<<"$output"
      grep -q '"qr_link": "newx-link"' "$login_dir/wechatpadpro-login-meta.json"
      printf 'bmV3eA==' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
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
    stale-key-regenerates)
      grep -q 'WeChatPadPro QR link: fresh-link' <<<"$output"
      grep -q '^WECHATPADPRO_ACCOUNT_KEY=fresh-account-key$' "$env_file"
      printf 'ZnJlc2g=' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
      ;;
    expired-key-regenerates)
      grep -q 'WeChatPadPro QR link: fresh-link' <<<"$output"
      grep -q '^WECHATPADPRO_ACCOUNT_KEY=fresh-account-key$' "$env_file"
      printf 'ZnJlc2g=' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
      ;;
    client-version-too-low)
      if grep -Fq '<Content><![CDATA[' <<<"$output"; then
        echo "FAIL: case '$case_name' should not print raw XML blocker details" >&2
        echo "$output" >&2
        exit 1
      fi
      ;;
    stale-unrelated-blocker)
      if grep -q 'current client version is too low' <<<"$output"; then
        echo "FAIL: case '$case_name' should ignore blocker logs unrelated to the current account key" >&2
        echo "$output" >&2
        exit 1
      fi
      ;;
    loginstate-online)
      grep -q 'WeChatPadPro account is already logged in for key: test-account-key' <<<"$output"
      ;;
  esac

  if compgen -G "$repo_root/wechatpadpro_login_err.*.log" >/dev/null; then
    echo "FAIL: case '$case_name' leaked wechatpadpro_login_err temporary files into the repo root" >&2
    ls "$repo_root"/wechatpadpro_login_err.*.log >&2
    exit 1
  fi
}

run_case success-refresh-online 0 "WeChatPadPro QR link: second-link" 5 0.1
run_case stale-status-newx-success 0 "WeChatPadPro QR link: stale-link" 5 0.1
run_case padx-fallback-to-newx 0 "WeChatPadPro QR link: newx-link" 5 0.1
run_case prefer-newx-over-padx 0 "WeChatPadPro QR link: newx-link" 5 0.1

run_case already-logged-in-qr-fails 0 "WeChatPadPro account is already logged in for key: test-account-key" 5 0.1
run_case invalid-qr-payload 1 "Failed to fetch WeChatPadPro login QR." 5 0.1
run_case verify-required 1 "WeChatPadPro login requires additional verification."
run_case timeout-pending 1 "WeChatPadPro login did not complete within 1 seconds." 1 0.05
run_case base-url-slow 1 "WeChatPadPro API did not become ready at http://127.0.0.1:38849 within 1 seconds." 1 0.05
run_case slow-status-budget 1 "WeChatPadPro login did not complete within 1 seconds." 1 0.05
run_case client-version-too-low 1 "WeChatPadPro login blocked: current client version is too low."
run_case stale-unrelated-blocker 1 "WeChatPadPro login did not complete within 1 seconds." 1 0.05
run_case loginstate-online 0 "WeChatPadPro account is already logged in for key: test-account-key" 5 0.1
run_case generate-key-budget 0 "Generated WECHATPADPRO_ACCOUNT_KEY and persisted it to" 1 0.05
run_case stale-key-regenerates 0 "WeChatPadPro QR link: fresh-link" 5 0.1
run_case expired-key-regenerates 0 "WeChatPadPro QR link: fresh-link" 5 0.1
