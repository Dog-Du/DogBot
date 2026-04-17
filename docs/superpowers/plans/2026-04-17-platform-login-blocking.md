# Platform Login Blocking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make QQ and WeChat deployment block on fresh QR-code login for up to 100 seconds, fail cleanly on timeout, and remove the obsolete `astrbot/` tree plus its active doc references.

**Architecture:** `deploy_stack.sh` stays the only orchestration entrypoint, but login becomes an explicit blocking phase implemented inside the platform-specific login-prep scripts. Shared polling and timeout utilities live in `scripts/lib/common.sh`, platform scripts own QR refresh and login-status probing, and shell regression tests stub `docker` and `curl` so stale-QR replacement, timeout behavior, and success detection can be verified without real platform accounts.

**Tech Stack:** Bash, Docker CLI, curl, `uv run python`, shell regression tests

---

## File Map

- Modify: `scripts/lib/common.sh`
  - Add reusable deadline-based polling helpers shared by QQ and WeChat login scripts.
- Modify: `scripts/tests/test_common.sh`
  - Cover the new shared timeout/deadline helpers.
- Create: `scripts/tests/test_prepare_napcat_login.sh`
  - Stub `docker` and `curl` to prove stale local QR files are removed, fresh QR artifacts are written, and login must succeed before exit.
- Modify: `scripts/prepare_napcat_login.sh`
  - Replace the one-shot QR dump with “clear stale artifacts -> wait for fresh QR -> wait for login success”.
- Create: `scripts/tests/test_prepare_wechatpadpro_login.sh`
  - Stub `curl` to prove QR creation, QR refresh on expiration, login success detection, and timeout failure.
- Modify: `scripts/prepare_wechatpadpro_login.sh`
  - Replace the one-shot QR fetch with a blocking QR/login state machine.
- Modify: `scripts/deploy_stack.sh`
  - Keep platform boot order, but treat login success as a required phase before continuing.
- Modify: `scripts/check_structure.sh`
  - Run the new regression tests and keep executable checks current.
- Modify: `AGENTS.md`
  - Remove AstrBot from the current architecture narrative and reading order.
- Modify: `README.md`
  - Remove the stale “清理 AstrBot 遗留” TODO item and keep the architecture tree aligned with the live system.
- Modify: `deploy/README.md`
  - Document that deployment blocks for login and fails after 100 seconds.
- Delete: `astrbot/plugins/claude_runner_bridge/README.md`
- Delete: `astrbot/plugins/claude_runner_bridge/_conf_schema.json`
- Delete: `astrbot/plugins/claude_runner_bridge/main.py`
- Delete: `astrbot/plugins/claude_runner_bridge/requirements.txt`
- Delete: `astrbot/plugins/claude_runner_bridge/tests/test_main.py`

### Task 1: Add Shared Deadline Helpers

**Files:**
- Modify: `scripts/lib/common.sh`
- Modify: `scripts/tests/test_common.sh`

- [ ] **Step 1: Write the failing helper tests**

Append this block near the end of `scripts/tests/test_common.sh`:

```bash
timeout_start="$(date +%s)"
ready_file="$(mktemp)"
rm -f "$ready_file"
(
  sleep 2
  printf 'ready\n' >"$ready_file"
) &
delayed_writer_pid=$!

deadline_epoch="$(dogbot_deadline_in 5)"
if ! dogbot_wait_until_deadline "$deadline_epoch" test -f "$ready_file"; then
  echo "FAIL: dogbot_wait_until_deadline should succeed before the deadline" >&2
  exit 1
fi

elapsed=$(( $(date +%s) - timeout_start ))
if (( elapsed < 2 )); then
  echo "FAIL: dogbot_wait_until_deadline returned before the condition became true" >&2
  exit 1
fi

if dogbot_wait_until_deadline "$(dogbot_deadline_in 1)" false; then
  echo "FAIL: dogbot_wait_until_deadline should fail after deadline expiry" >&2
  exit 1
fi

wait "$delayed_writer_pid"
rm -f "$ready_file"
```

- [ ] **Step 2: Run the helper test and verify RED**

Run: `bash scripts/tests/test_common.sh`

Expected: FAIL with `dogbot_deadline_in: command not found` or `dogbot_wait_until_deadline: command not found`

- [ ] **Step 3: Add the minimal shared implementation**

Insert these functions into `scripts/lib/common.sh` above `dogbot_resolve_compose_cmd()`:

```bash
dogbot_deadline_in() {
  local timeout_secs="${1:-0}"
  printf '%s\n' "$(( $(date +%s) + timeout_secs ))"
}

dogbot_wait_until_deadline() {
  local deadline_epoch="$1"
  shift
  local interval_secs="${DOGBOT_WAIT_INTERVAL_SECS:-1}"

  while true; do
    if "$@"; then
      return 0
    fi

    if (( $(date +%s) >= deadline_epoch )); then
      return 1
    fi

    sleep "$interval_secs"
  done
}
```

- [ ] **Step 4: Run the helper test and verify GREEN**

Run: `bash scripts/tests/test_common.sh`

Expected: PASS with `common.sh env resolution tests passed.`

- [ ] **Step 5: Commit**

```bash
git add scripts/lib/common.sh scripts/tests/test_common.sh
git commit -m "test: cover shared login deadline helpers"
```

### Task 2: Make QQ Login Fresh And Blocking

**Files:**
- Create: `scripts/tests/test_prepare_napcat_login.sh`
- Modify: `scripts/prepare_napcat_login.sh`

- [ ] **Step 1: Write the failing QQ login regression test**

Create `scripts/tests/test_prepare_napcat_login.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

env_file="$tmpdir/dogbot.env"
login_dir="$tmpdir/napcat-login"
state_dir="$tmpdir/state"
mkdir -p "$login_dir" "$state_dir" "$tmpdir/bin"

cat >"$env_file" <<EOF
ENABLE_QQ=1
NAPCAT_CONTAINER_NAME=napcat
NAPCAT_API_BASE_URL=http://127.0.0.1:3001
NAPCAT_LOGIN_OUTPUT_DIR=$login_dir
DOGBOT_LOGIN_TIMEOUT_SECS=5
DOGBOT_WAIT_INTERVAL_SECS=0.1
EOF

printf 'stale-qr\n' >"$login_dir/napcat-login-qr.png"

cat >"$tmpdir/bin/docker" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
case "$1" in
  logs)
    count=\$(($(cat "$state_dir/log_count" 2>/dev/null || echo 0) + 1))
    echo "\$count" >"$state_dir/log_count"
    if (( count >= 2 )); then
      printf 'scan https://txz.qq.com/p?k=fresh-link\n'
    fi
    ;;
  exec)
    if [[ "$4" == "test" ]]; then
      exit 0
    fi
    ;;
  cp)
    printf 'fresh-qr\n' >"${@: -1}"
    ;;
  *)
    ;;
esac
EOF

cat >"$tmpdir/bin/curl" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
count=\$(($(cat "$state_dir/login_count" 2>/dev/null || echo 0) + 1))
echo "\$count" >"$state_dir/login_count"
if (( count < 3 )); then
  printf '{"status":"failed","retcode":1,"data":{}}\n'
else
  printf '{"status":"ok","retcode":0,"data":{"user_id":3472283357}}\n'
fi
EOF

chmod +x "$tmpdir/bin/docker" "$tmpdir/bin/curl"

output="$(
  PATH="$tmpdir/bin:$PATH" \
    UV_BIN="$(command -v uv)" \
    "$repo_root/scripts/prepare_napcat_login.sh" "$env_file" 2>&1
)"

grep -q 'NapCat login URL: https://txz.qq.com/p?k=fresh-link' <<<"$output"
grep -q 'login_url=https://txz.qq.com/p?k=fresh-link' "$login_dir/napcat-login-meta.txt"
grep -q 'fresh-qr' "$login_dir/napcat-login-qr.png"
```

- [ ] **Step 2: Run the QQ regression test and verify RED**

Run: `bash scripts/tests/test_prepare_napcat_login.sh`

Expected: FAIL because `scripts/prepare_napcat_login.sh` exits before it removes the stale file, writes the fresh QR, and waits for the third successful login poll.

- [ ] **Step 3: Implement blocking QQ login with fresh artifact replacement**

Replace the body of `scripts/prepare_napcat_login.sh` after the environment checks with this structure:

```bash
uv_bin="$(dogbot_resolve_uv_bin)"
login_timeout_secs="${DOGBOT_LOGIN_TIMEOUT_SECS:-100}"
deadline_epoch="$(dogbot_deadline_in "$login_timeout_secs")"
login_started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
last_login_url=""

rm -f "$qr_png_path" "$meta_path"

napcat_fetch_login_url() {
  docker logs --since "$login_started_at" "$container_name" 2>&1 \
    | grep -o 'https://txz\.qq\.com/p?k=[^[:space:]]*' \
    | tail -n1
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
  local response
  response="$(curl -fsS -X POST \
    "${NAPCAT_API_BASE_URL%/}/get_login_info" \
    -H 'Content-Type: application/json' \
    -d '{}')"

  "$uv_bin" run python - <<'PY' "$response"
import json, sys
payload = json.loads(sys.argv[1])
data = payload.get("data") or {}
user_id = str(data.get("user_id") or "").strip()
raise SystemExit(0 if user_id else 1)
PY
}

if ! dogbot_wait_until_deadline "$deadline_epoch" napcat_refresh_qr; then
  echo "NapCat login QR was not refreshed within ${login_timeout_secs} seconds." >&2
  exit 1
fi

if ! dogbot_wait_until_deadline "$deadline_epoch" napcat_login_succeeded; then
  echo "NapCat login did not complete within ${login_timeout_secs} seconds." >&2
  exit 1
fi

echo "NapCat login confirmed."
```

- [ ] **Step 4: Run the QQ regression test and verify GREEN**

Run: `bash scripts/tests/test_prepare_napcat_login.sh`

Expected: PASS with no output except shell success.

- [ ] **Step 5: Commit**

```bash
git add scripts/prepare_napcat_login.sh scripts/tests/test_prepare_napcat_login.sh
git commit -m "feat: block deployment on fresh qq login"
```

### Task 3: Make WeChat Login Fresh And Blocking

**Files:**
- Create: `scripts/tests/test_prepare_wechatpadpro_login.sh`
- Modify: `scripts/prepare_wechatpadpro_login.sh`

- [ ] **Step 1: Write the failing WeChat login regression test**

Create `scripts/tests/test_prepare_wechatpadpro_login.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

env_file="$tmpdir/dogbot.env"
login_dir="$tmpdir/wechat-login"
state_dir="$tmpdir/state"
mkdir -p "$login_dir" "$state_dir" "$tmpdir/bin"

cat >"$env_file" <<EOF
ENABLE_WECHATPADPRO=1
WECHATPADPRO_ADMIN_KEY=test-admin-key
WECHATPADPRO_ACCOUNT_KEY=test-account-key
WECHATPADPRO_BASE_URL=http://127.0.0.1:38849
WECHATPADPRO_LOGIN_OUTPUT_DIR=$login_dir
DOGBOT_LOGIN_TIMEOUT_SECS=5
DOGBOT_WAIT_INTERVAL_SECS=0.1
EOF

cat >"$tmpdir/bin/curl" <<EOF
#!/usr/bin/env bash
state_dir="$state_dir"
case "$*" in
  *"/login/GetLoginQrCodePadX"*)
    qr_count=\$(($(cat "$state_dir/qr_count" 2>/dev/null || echo 0) + 1))
    echo "\$qr_count" >"$state_dir/qr_count"
    if (( qr_count == 1 )); then
      printf '{"Code":200,"Data":{"qrCodeBase64":"data:image/png;base64,Zmlyc3Q=","QrLink":"first-link","QrCodeUrl":"first-url","expiredTime":30,"Key":"test-account-key"}}\n'
    else
      printf '{"Code":200,"Data":{"qrCodeBase64":"data:image/png;base64,c2Vjb25k","QrLink":"second-link","QrCodeUrl":"second-url","expiredTime":30,"Key":"test-account-key"}}\n'
    fi
    ;;
  *"/login/GetLoginStatus"*)
    status_count=\$(($(cat "$state_dir/status_count" 2>/dev/null || echo 0) + 1))
    echo "\$status_count" >"$state_dir/status_count"
    if (( status_count == 1 )); then
      printf '{"Code":200,"Text":"二维码已过期","Data":{"Status":"expired"}}\n'
    else
      printf '{"Code":200,"Text":"已登录","Data":{"Status":"online","wxid":"wxid_bot"}}\n'
    fi
    ;;
  *)
    printf '<html>ok</html>\n'
    ;;
esac
EOF

chmod +x "$tmpdir/bin/curl"

output="$(
  PATH="$tmpdir/bin:$PATH" \
    UV_BIN="$(command -v uv)" \
    "$repo_root/scripts/prepare_wechatpadpro_login.sh" "$env_file" 2>&1
)"

grep -q 'WeChatPadPro QR link: second-link' <<<"$output"
grep -q '"qr_link": "second-link"' "$login_dir/wechatpadpro-login-meta.json"
printf 'second' | base64 -d | cmp -s - "$login_dir/wechatpadpro-login-qr.png"
```

- [ ] **Step 2: Run the WeChat regression test and verify RED**

Run: `bash scripts/tests/test_prepare_wechatpadpro_login.sh`

Expected: FAIL because the current script exits after the first QR fetch instead of refreshing expired QR codes and waiting for online status.

- [ ] **Step 3: Implement blocking WeChat login with QR refresh**

Refactor `scripts/prepare_wechatpadpro_login.sh` so the bottom half follows this structure:

```bash
login_timeout_secs="${DOGBOT_LOGIN_TIMEOUT_SECS:-100}"
deadline_epoch="$(dogbot_deadline_in "$login_timeout_secs")"
last_qr_link=""

write_login_artifacts() {
  local response="$1"
  "$uv_bin" run python - <<'PY' "$response" "$login_dir" "$WECHATPADPRO_ACCOUNT_KEY"
import base64, json, pathlib, sys
payload = json.loads(sys.argv[1])
login_dir = pathlib.Path(sys.argv[2])
account_key = sys.argv[3]
data = payload.get("Data") or {}
img = (data.get("qrCodeBase64") or "").removeprefix("data:image/png;base64,")
png_path = login_dir / "wechatpadpro-login-qr.png"
meta_path = login_dir / "wechatpadpro-login-meta.json"
png_path.write_bytes(base64.b64decode(img))
meta = {
    "account_key": data.get("Key") or account_key,
    "qr_link": data.get("QrLink"),
    "qr_code_url": data.get("QrCodeUrl"),
    "expires_in": data.get("expiredTime"),
    "png_path": str(png_path),
    "generated_at": __import__("datetime").datetime.utcnow().isoformat() + "Z",
}
meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2))
print(meta["qr_link"])
print(png_path)
print(meta_path)
PY
}

fetch_login_qr() {
  local response qr_link png_path meta_path
  response="$(request_login_qr "/login/GetLoginQrCodePadX" 2>/tmp/wechatpadpro_login_err.log)" \
    || response="$(request_login_qr "/login/GetLoginQrCodeNewX" 2>>/tmp/wechatpadpro_login_err.log)" \
    || return 1

  mapfile -t qr_info < <(write_login_artifacts "$response")
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

current_login_state() {
  local response
  response="$(curl --max-time 15 -fsS "${base_url}/login/GetLoginStatus?key=${WECHATPADPRO_ACCOUNT_KEY}")"
  "$uv_bin" run python - <<'PY' "$response"
import json, sys
payload = json.loads(sys.argv[1])
text = str(payload.get("Text") or "")
data = payload.get("Data") or {}
status = str(data.get("Status") or data.get("status") or "")
wxid = str(data.get("wxid") or data.get("Wxid") or "")
combined = f"{text} {status}".lower()
if "验证码" in text or "辅助" in text:
    print("verify-required")
elif "过期" in text or "expired" in combined:
    print("expired")
elif "已登录" in text or "在线" in text or "online" in combined or wxid.startswith("wxid_"):
    print("online")
else:
    print("pending")
PY
}

fetch_login_qr || {
  echo "Failed to fetch WeChatPadPro login QR." >&2
  cat /tmp/wechatpadpro_login_err.log >&2 || true
  exit 1
}

while (( $(date +%s) < deadline_epoch )); do
  case "$(current_login_state)" in
    online)
      echo "WeChatPadPro account is already logged in for key: $WECHATPADPRO_ACCOUNT_KEY"
      exit 0
      ;;
    expired)
      fetch_login_qr || {
        echo "Failed to refresh WeChatPadPro login QR." >&2
        exit 1
      }
      ;;
    verify-required)
      echo "WeChatPadPro login requires additional verification." >&2
      exit 1
      ;;
  esac
  sleep "${DOGBOT_WAIT_INTERVAL_SECS:-1}"
done

echo "WeChatPadPro login did not complete within ${login_timeout_secs} seconds." >&2
exit 1
```

- [ ] **Step 4: Run the WeChat regression test and verify GREEN**

Run: `bash scripts/tests/test_prepare_wechatpadpro_login.sh`

Expected: PASS with no output except shell success.

- [ ] **Step 5: Commit**

```bash
git add scripts/prepare_wechatpadpro_login.sh scripts/tests/test_prepare_wechatpadpro_login.sh
git commit -m "feat: block deployment on wechat login"
```

### Task 4: Wire Blocking Login Into Deployment Checks

**Files:**
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/check_structure.sh`

- [ ] **Step 1: Write the failing structure/deploy assertions**

Update `scripts/check_structure.sh` so the `files` list and executed checks include the two new regression tests:

```bash
  "scripts/tests/test_prepare_napcat_login.sh"
  "scripts/tests/test_prepare_wechatpadpro_login.sh"
```

And add these execution lines near the end:

```bash
bash "$repo_root/scripts/tests/test_common.sh"
bash "$repo_root/scripts/tests/test_prepare_napcat_login.sh"
bash "$repo_root/scripts/tests/test_prepare_wechatpadpro_login.sh"
```

- [ ] **Step 2: Run the structure check and verify RED**

Run: `bash scripts/check_structure.sh`

Expected: FAIL because the new test files do not exist yet in the index or because `deploy_stack.sh` still prints the old “scan QR and rerun” recovery message.

- [ ] **Step 3: Update deployment orchestration and checks**

In `scripts/deploy_stack.sh`, replace the platform login sections with explicit blocking messages:

```bash
if [[ "${ENABLE_QQ}" == "1" ]]; then
  dogbot_require_env QQ_ADAPTER_QQ_BOT_ID
  "$repo_root/scripts/start_qq_adapter.sh" "$env_file"
  run_compose_up "$repo_root/compose/platform-stack.yml" napcat
  "$repo_root/scripts/configure_napcat_ws.sh" "$env_file"
  echo "Waiting up to 100 seconds for NapCat login..."
  "$repo_root/scripts/prepare_napcat_login.sh" "$env_file"
fi

if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  dogbot_require_env WECHATPADPRO_IMAGE
  dogbot_require_env WECHATPADPRO_ADMIN_KEY
  dogbot_require_env WECHATPADPRO_MYSQL_ROOT_PASSWORD
  dogbot_require_env WECHATPADPRO_MYSQL_PASSWORD

  run_compose_up "$repo_root/compose/wechatpadpro-stack.yml"
  "$repo_root/scripts/start_wechatpadpro_adapter.sh" "$env_file"
  echo "Waiting up to 100 seconds for WeChatPadPro login..."
  "$repo_root/scripts/prepare_wechatpadpro_login.sh" "$env_file"
  if [[ "${WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK:-0}" == "1" ]]; then
    "$repo_root/scripts/configure_wechatpadpro_webhook.sh" "$env_file"
  fi
fi
```

Also extend `scripts/check_structure.sh` so it syntax-checks the two login-prep scripts:

```bash
bash -n "$repo_root/scripts/prepare_napcat_login.sh"
bash -n "$repo_root/scripts/prepare_wechatpadpro_login.sh"
```

- [ ] **Step 4: Run the structure check and verify GREEN**

Run: `bash scripts/check_structure.sh`

Expected: PASS with `Structure check passed. All required files are present.`

- [ ] **Step 5: Commit**

```bash
git add scripts/deploy_stack.sh scripts/check_structure.sh
git commit -m "feat: enforce blocking platform login during deploy"
```

### Task 5: Remove `astrbot/` And Update Active Docs

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `deploy/README.md`
- Delete: `astrbot/plugins/claude_runner_bridge/README.md`
- Delete: `astrbot/plugins/claude_runner_bridge/_conf_schema.json`
- Delete: `astrbot/plugins/claude_runner_bridge/main.py`
- Delete: `astrbot/plugins/claude_runner_bridge/requirements.txt`
- Delete: `astrbot/plugins/claude_runner_bridge/tests/test_main.py`

- [ ] **Step 1: Write the failing cleanup check**

Run this grep command from the repo root:

```bash
rg -n "astrbot|claude_runner_bridge" AGENTS.md README.md deploy/README.md scripts qq_adapter wechatpadpro_adapter compose deploy -S
```

Expected: matches in `AGENTS.md`, `README.md`, and possibly active deploy docs still mention AstrBot cleanup as pending.

- [ ] **Step 2: Update the active docs**

Apply these exact text updates:

In `AGENTS.md`, replace the current QQ chain block with:

```text
QQ -> NapCat -> qq-adapter -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner
```

Remove the `AstrBot` / `claude_runner_bridge` bullets from the QQ 接入层 section, replace the important directory entry with:

```markdown
- `qq_adapter/`
  - QQ 适配器
```

Update the known-problems/future-direction sections so they no longer say QQ still depends on AstrBot or that removing AstrBot is future work.

In `README.md`, delete this TODO block:

```markdown
- [ ] 清理 AstrBot 遗留
  - 删除不再使用的插件和部署残留
  - 彻底收敛到双 adapter 架构
```

In `deploy/README.md`, update the login sections to say the deploy script blocks for login and exits after 100 seconds if the operator does not finish scanning in time.

- [ ] **Step 3: Remove the obsolete source tree**

Run:

```bash
git rm -r astrbot
```

- [ ] **Step 4: Verify cleanup**

Run:

```bash
rg -n "astrbot|claude_runner_bridge" AGENTS.md README.md deploy/README.md scripts qq_adapter wechatpadpro_adapter compose deploy -S
```

Expected: no matches in active runtime/docs files.

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md README.md deploy/README.md
git commit -m "refactor: remove obsolete astrbot artifacts"
```

## Self-Review

- Spec coverage:
  - Fresh QQ QR replacement: Task 2
  - Blocking WeChat QR/login flow: Task 3
  - Shared 100-second timeout behavior: Task 1 + Tasks 2/3
  - Deploy only continues after login success: Task 4
  - `astrbot/` cleanup: Task 5
- Placeholder scan:
  - No `TODO`, `TBD`, or “similar to previous task” placeholders remain.
- Type/command consistency:
  - Shared helper names are consistent across tasks: `dogbot_deadline_in`, `dogbot_wait_until_deadline`
  - Platform scripts both use `DOGBOT_LOGIN_TIMEOUT_SECS` with default `100`
  - Test entrypoints consistently use `bash scripts/tests/...`
