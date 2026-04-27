#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir_root="$(mktemp -d)"
trap 'rm -rf "$tmpdir_root"' EXIT

mk_fake_runtime() {
  local case_name="$1"
  local case_dir="$2"
  local login_dir="$case_dir/napcat-login"
  local state_dir="$case_dir/state"
  local qq_dir="$case_dir/napcat-qq"

  mkdir -p "$login_dir" "$state_dir" "$case_dir/bin" "$qq_dir"
  printf 'stale-qr\n' >"$login_dir/napcat-login-qr.png"
  cat >"$login_dir/napcat-login-meta.txt" <<EOF
container=stale
login_url=https://txz.qq.com/p?k=stale-link
qr_png_path=$login_dir/stale-qr.png
generated_at=2025-01-01T00:00:00Z
EOF

  cat >"$case_dir/bin/docker" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
case_name="$case_name"
case "\$1" in
  inspect)
    printf '2026-04-27T02:23:57Z\n'
    ;;
  logs)
    count=\$((\$(cat "\$state_dir/log_count" 2>/dev/null || echo 0) + 1))
    echo "\$count" >"\$state_dir/log_count"
    case "\$case_name" in
      fresh-qr-success)
        if [[ "\$*" == *"--since"* ]]; then
          if (( count >= 2 )); then
            printf 'scan https://txz.qq.com/p?k=fresh-link\n'
          fi
        fi
        ;;
      historical-log-filtering)
        if [[ "\$*" == *"--since"* ]]; then
          if (( count >= 2 )); then
            printf 'scan https://txz.qq.com/p?k=fresh-link\n'
          fi
        else
          printf 'scan https://txz.qq.com/p?k=stale-link\n'
        fi
        ;;
      runtime-log-during-login-times-out)
        if [[ "\$*" == *"--since"* ]]; then
          if (( count >= 2 )); then
            printf 'scan https://txz.qq.com/p?k=runtime-log-link\n'
          fi
        fi
        ;;
      preexisting-runtime-state-times-out)
        ;;
      already-logged-in)
        ;;
      slow-login-budget)
        sleep 1
        printf 'scan https://txz.qq.com/p?k=slow-link\n'
        ;;
      bounded-request-timeout-recovers)
        printf 'scan https://txz.qq.com/p?k=bounded-link\n'
        ;;
      existing-qr-rerun-times-out)
        if [[ "\$*" == *"--since"* ]]; then
          printf 'scan https://txz.qq.com/p?k=existing-link\n'
        fi
        ;;
    esac
    ;;
  exec)
    if [[ "\$3" == "test" ]]; then
      if [[ "\$case_name" == "already-logged-in" ]]; then
        exit 1
      fi
      echo "1" >"\$state_dir/exec_test_seen"
      exit 0
    fi
    ;;
  cp)
    case "\$case_name" in
      *)
        printf 'fresh-qr\n' >"\${@: -1}"
        ;;
    esac
    ;;
  *)
    ;;
esac
EOF

  cat >"$case_dir/bin/curl" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
case_name="$case_name"
max_time=""
previous_arg=""
for arg in "\$@"; do
  if [[ "\$previous_arg" == "--max-time" ]]; then
    max_time="\$arg"
  fi
  previous_arg="\$arg"
done

count=\$((\$(cat "\$state_dir/login_count" 2>/dev/null || echo 0) + 1))
echo "\$count" >"\$state_dir/login_count"

  case "\$case_name" in
    fresh-qr-success)
      if (( count < 3 )); then
        printf '{"status":"failed","retcode":1,"data":{}}\n'
      else
      printf '{"status":"ok","retcode":0,"data":{"user_id":3472283357}}\n'
    fi
    ;;
    historical-log-filtering)
      if (( count < 3 )); then
        printf '{"status":"failed","retcode":1,"data":{}}\n'
      else
        printf '{"status":"ok","retcode":0,"data":{"user_id":3472283357}}\n'
      fi
      ;;
  already-logged-in)
    printf '{"status":"ok","retcode":0,"data":{"user_id":3472283357}}\n'
    ;;
    slow-login-budget)
      echo "\$max_time" >"\$state_dir/login_timeout"
      if [[ "\$max_time" != "1" ]]; then
        sleep 2
      fi
      printf '{"status":"failed","retcode":1,"data":{}}\n'
      ;;
    runtime-log-during-login-times-out)
      if (( count < 2 )); then
        printf '{"status":"failed","retcode":1,"data":{}}\n'
      else
        mkdir -p "$qq_dir/nt_qq_dbtest/nt_data/log"
        printf 'qq-online\n' >"$qq_dir/nt_qq_dbtest/nt_data/log/qq-log_2026-04-17-19.qqxlog"
        printf '{"status":"failed","retcode":1,"data":{}}\n'
      fi
      ;;
    preexisting-runtime-state-times-out)
      printf '{"status":"failed","retcode":1,"data":{}}\n'
      ;;
    bounded-request-timeout-recovers)
      if (( count == 1 )); then
        echo "\$max_time" >"\$state_dir/first_login_timeout"
        python3 - <<'PY' "\$max_time"
import sys
value = float(sys.argv[1])
raise SystemExit(0 if value <= 1.0 else 1)
PY
        if [[ \$? -ne 0 ]]; then
          sleep 2
          printf '{"status":"failed","retcode":1,"data":{}}\n'
        else
          printf '{"status":"failed","retcode":1,"data":{}}\n'
        fi
      else
        printf '{"status":"ok","retcode":0,"data":{"user_id":3472283357}}\n'
      fi
      ;;
    existing-qr-rerun-times-out)
      printf '{"status":"failed","retcode":1,"data":{}}\n'
      ;;
  esac
EOF

  chmod +x "$case_dir/bin/docker" "$case_dir/bin/curl"

  if [[ "$case_name" == "preexisting-runtime-state-times-out" ]]; then
    mkdir -p "$qq_dir/nt_qq_dbpreexisting/nt_data/log"
    printf 'qq-online\n' >"$qq_dir/nt_qq_dbpreexisting/nt_data/log/qq-log_2026-04-17-19.qqxlog"
  elif [[ "$case_name" == "stale-runtime-state-times-out" ]]; then
    mkdir -p "$qq_dir/nt_qq_dbstale/nt_data/log"
    printf 'qq-stale\n' >"$qq_dir/nt_qq_dbstale/nt_data/log/qq-log_2026-04-17-19.qqxlog"
    touch -d '2025-01-01 00:00:00Z' "$qq_dir/nt_qq_dbstale/nt_data/log/qq-log_2026-04-17-19.qqxlog"
  fi
}

run_case() {
  local case_name="$1"
  local expected_exit_code="$2"
  local expected_message="$3"
  local login_timeout="${4:-5}"
  local wait_interval="${5:-0.1}"

  local case_dir="$tmpdir_root/$case_name"
  local env_file="$case_dir/dogbot.env"
  local login_dir="$case_dir/napcat-login"
  local state_dir="$case_dir/state"
  local qq_dir="$case_dir/napcat-qq"

  mkdir -p "$case_dir"
  mk_fake_runtime "$case_name" "$case_dir"

  cat >"$env_file" <<EOF
ENABLE_QQ=1
NAPCAT_CONTAINER_NAME=napcat
NAPCAT_API_BASE_URL=http://127.0.0.1:3001
NAPCAT_LOGIN_OUTPUT_DIR=$login_dir
NAPCAT_QQ_DIR=$qq_dir
DOGBOT_LOGIN_TIMEOUT_SECS=$login_timeout
DOGBOT_WAIT_INTERVAL_SECS=$wait_interval
EOF

  local output
  local status
  set +e
  output="$(
    PATH="$case_dir/bin:$PATH" \
      UV_BIN="$(command -v uv)" \
      "$repo_root/scripts/prepare_napcat_login.sh" "$env_file" 2>&1
  )"
  status=$?
  set -e

  if [[ "$status" -ne "$expected_exit_code" ]]; then
    echo "FAIL: case '$case_name' expected exit $expected_exit_code but got $status" >&2
    echo "$output" >&2
    exit 1
  fi

  grep -q "$expected_message" <<<"$output" || {
    echo "FAIL: case '$case_name' missing expected output '$expected_message'" >&2
    echo "$output" >&2
    exit 1
  }

  if [[ "$case_name" != "already-logged-in" \
        && "$case_name" != "runtime-log-during-login-times-out" \
        && "$case_name" != "preexisting-runtime-state-times-out" \
        && "$case_name" != "stale-runtime-state-times-out" \
        && ! -f "$state_dir/exec_test_seen" ]]; then
    echo "FAIL: case '$case_name' did not validate qrcode presence through docker exec test -f" >&2
    exit 1
  fi

  case "$case_name" in
    fresh-qr-success)
      local login_count
      login_count="$(cat "$state_dir/login_count" 2>/dev/null || echo 0)"
      grep -q 'NapCat login confirmed.' <<<"$output"
      grep -q 'NapCat login URL: https://txz.qq.com/p?k=fresh-link' <<<"$output"
      grep -q 'login_url=https://txz.qq.com/p?k=fresh-link' "$login_dir/napcat-login-meta.txt"
      grep -q 'container=napcat' "$login_dir/napcat-login-meta.txt"
      grep -q "qr_png_path=$login_dir/napcat-login-qr.png" "$login_dir/napcat-login-meta.txt"
      grep -q 'generated_at=' "$login_dir/napcat-login-meta.txt"
      if grep -q 'login_url=https://txz.qq.com/p?k=stale-link' "$login_dir/napcat-login-meta.txt"; then
        echo "napcat-login-meta.txt still contains stale login_url" >&2
        exit 1
      fi
      if [[ "$login_count" -ne 3 ]]; then
        echo "Expected exactly 3 login polls, got: $login_count" >&2
        exit 1
      fi
      grep -q 'fresh-qr' "$login_dir/napcat-login-qr.png"
      ;;
    historical-log-filtering)
      grep -q 'NapCat login confirmed.' <<<"$output"
      grep -q 'NapCat login URL: https://txz.qq.com/p?k=fresh-link' <<<"$output"
      if grep -q 'https://txz.qq.com/p?k=stale-link' <<<"$output"; then
        echo "FAIL: case '$case_name' should ignore stale QR URLs from historical logs" >&2
        echo "$output" >&2
        exit 1
      fi
      grep -q 'login_url=https://txz.qq.com/p?k=fresh-link' "$login_dir/napcat-login-meta.txt"
      grep -q 'fresh-qr' "$login_dir/napcat-login-qr.png"
      ;;
    already-logged-in)
      if grep -q 'NapCat login URL:' <<<"$output"; then
        echo "FAIL: case '$case_name' should not require a QR when login is already confirmed" >&2
        echo "$output" >&2
        exit 1
      fi
      ;;
    slow-login-budget)
      python3 - <<'PY' "$state_dir/login_timeout"
from pathlib import Path
value = float(Path(__import__("sys").argv[1]).read_text().strip())
if not (0.0 < value < 1.0):
    raise SystemExit(f"expected /get_login_info --max-time to be < 1.0s near the deadline, got {value}")
PY
      ;;
    bounded-request-timeout-recovers)
      python3 - <<'PY' "$state_dir/first_login_timeout"
from pathlib import Path
value = float(Path(__import__("sys").argv[1]).read_text().strip())
if not (0.0 < value <= 1.0):
    raise SystemExit(f"expected first /get_login_info --max-time to be <= 1.0s, got {value}")
PY
      grep -q 'NapCat login URL: https://txz.qq.com/p?k=bounded-link' <<<"$output"
      ;;
    existing-qr-rerun-times-out)
      grep -q 'NapCat login URL: https://txz.qq.com/p?k=existing-link' <<<"$output"
      grep -q 'login_url=https://txz.qq.com/p?k=existing-link' "$login_dir/napcat-login-meta.txt"
      grep -q 'fresh-qr' "$login_dir/napcat-login-qr.png"
      ;;
    runtime-log-during-login-times-out)
      if grep -q 'NapCat login confirmed.' <<<"$output"; then
        echo "FAIL: case '$case_name' should not confirm login from runtime log activity alone" >&2
        echo "$output" >&2
        exit 1
      fi
      grep -q 'qq-online' "$qq_dir/nt_qq_dbtest/nt_data/log/qq-log_2026-04-17-19.qqxlog"
      ;;
    preexisting-runtime-state-times-out)
      if grep -q 'NapCat login confirmed.' <<<"$output"; then
        echo "FAIL: case '$case_name' should not confirm login from preexisting runtime state alone" >&2
        echo "$output" >&2
        exit 1
      fi
      grep -q 'qq-online' "$qq_dir/nt_qq_dbpreexisting/nt_data/log/qq-log_2026-04-17-19.qqxlog"
      ;;
    stale-runtime-state-times-out)
      grep -q 'qq-stale' "$qq_dir/nt_qq_dbstale/nt_data/log/qq-log_2026-04-17-19.qqxlog"
      if grep -q 'NapCat login confirmed.' <<<"$output"; then
        echo "FAIL: case '$case_name' should not confirm login from stale runtime state" >&2
        echo "$output" >&2
        exit 1
      fi
      ;;
  esac
}

run_case fresh-qr-success 0 "NapCat login confirmed."
run_case historical-log-filtering 0 "NapCat login confirmed."
run_case already-logged-in 0 "NapCat login confirmed."
run_case slow-login-budget 1 "NapCat login did not complete within 1 seconds." 1 0.05
run_case bounded-request-timeout-recovers 0 "NapCat login confirmed." 2 0.05
run_case existing-qr-rerun-times-out 1 "NapCat login did not complete within 5 seconds."
run_case runtime-log-during-login-times-out 1 "NapCat login did not complete within 5 seconds."
run_case preexisting-runtime-state-times-out 1 "NapCat login QR was not refreshed within 5 seconds."
run_case stale-runtime-state-times-out 1 "NapCat login QR was not refreshed within 5 seconds."
