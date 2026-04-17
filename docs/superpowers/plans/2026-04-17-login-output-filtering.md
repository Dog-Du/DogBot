# Login Output Filtering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make interactive platform login output cleaner by hiding WeChatPadPro raw XML blocker noise and by printing only the current NapCat QR for the active login attempt.

**Architecture:** Keep the fix inside the two login preparation shell scripts. Extend the existing script-level regression tests first, then make minimal logic changes: suppress raw blocker detail in the WeChat script and remove pre-window NapCat QR fallback in the QQ script.

**Tech Stack:** Bash, curl, docker logs, existing shell-based regression tests

---

### Task 1: WeChat blocker output normalization

**Files:**
- Modify: `scripts/tests/test_prepare_wechatpadpro_login.sh`
- Modify: `scripts/prepare_wechatpadpro_login.sh`

- [ ] **Step 1: Write the failing test**

Add an assertion to the existing `client-version-too-low` case so the captured script output contains the normalized blocker line but does not contain the raw XML text such as `<Content><![CDATA[`.

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/tests/test_prepare_wechatpadpro_login.sh`
Expected: FAIL in the `client-version-too-low` case because the raw XML detail is still printed.

- [ ] **Step 3: Write minimal implementation**

Update `scripts/prepare_wechatpadpro_login.sh` so the `client-version-too-low` blocker path prints only:

```bash
echo "WeChatPadPro login blocked: current client version is too low." >&2
exit 1
```

and no longer echoes the raw blocker detail line to stderr.

- [ ] **Step 4: Run test to verify it passes**

Run: `bash scripts/tests/test_prepare_wechatpadpro_login.sh`
Expected: PASS

### Task 2: NapCat QR filtering

**Files:**
- Modify: `scripts/tests/test_prepare_napcat_login.sh`
- Modify: `scripts/prepare_napcat_login.sh`

- [ ] **Step 1: Write the failing test**

Extend the NapCat shell regression test to simulate:

- an old QR URL present only in historical container logs
- a new QR URL present only in `--since "$login_started_at"` logs

Assert that the script prints and persists only the new QR URL.

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/tests/test_prepare_napcat_login.sh`
Expected: FAIL because the current implementation falls back to full container history and emits the stale QR first.

- [ ] **Step 3: Write minimal implementation**

Update `scripts/prepare_napcat_login.sh` so `napcat_fetch_login_url` only returns QR URLs from logs emitted since `login_started_at`. If none exist yet, return failure and keep polling; do not fall back to older container history.

- [ ] **Step 4: Run test to verify it passes**

Run: `bash scripts/tests/test_prepare_napcat_login.sh`
Expected: PASS

### Task 3: Regression verification

**Files:**
- Modify: `scripts/prepare_wechatpadpro_login.sh`
- Modify: `scripts/prepare_napcat_login.sh`
- Modify: `scripts/tests/test_prepare_wechatpadpro_login.sh`
- Modify: `scripts/tests/test_prepare_napcat_login.sh`

- [ ] **Step 1: Run focused regression suite**

Run:

```bash
bash scripts/tests/test_prepare_wechatpadpro_login.sh
bash scripts/tests/test_prepare_napcat_login.sh
bash scripts/tests/test_common.sh
bash -n scripts/prepare_wechatpadpro_login.sh
bash -n scripts/prepare_napcat_login.sh
```

Expected: all commands succeed.

- [ ] **Step 2: Review diff**

Run:

```bash
git diff -- scripts/prepare_wechatpadpro_login.sh scripts/prepare_napcat_login.sh scripts/tests/test_prepare_wechatpadpro_login.sh scripts/tests/test_prepare_napcat_login.sh
```

Expected: diff only covers the intended output-filtering and QR-windowing changes.
