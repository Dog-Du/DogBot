# Login Output Filtering Design

## Context

The current platform login flow has two user-facing issues during interactive bring-up:

1. `prepare_wechatpadpro_login.sh` correctly detects the upstream blocker "current client version is too low", but it also echoes the raw XML payload from WeChatPadPro logs into the terminal.
2. `prepare_napcat_login.sh` can print two QR codes for a single login attempt. The first QR is stale because the script falls back to older container logs when no QR has appeared yet for the current login window.

These two issues make interactive login noisy and misleading.

## Goals

- Show one clean, actionable WeChatPadPro blocker message when the client version is too low.
- Expose only the current NapCat login QR for the active login attempt.
- Preserve existing deploy flow, timeout behavior, and artifact locations.

## Non-Goals

- Changing upstream WeChatPadPro behavior or fixing the upstream client-version issue.
- Changing NapCat's internal login behavior or retry policy.
- Refactoring deploy orchestration outside the two login preparation scripts.

## Chosen Approach

### WeChatPadPro blocker output

`prepare_wechatpadpro_login.sh` will keep its existing blocker detection based on recent WeChatPadPro diagnostic logs, but terminal output will be normalized:

- Print only: `WeChatPadPro login blocked: current client version is too low.`
- Do not echo the raw XML detail line to stderr.
- Keep raw detail available in container logs for manual diagnosis.

This keeps the terminal output actionable without duplicating upstream noise.

### NapCat QR filtering

`prepare_napcat_login.sh` will treat the current login attempt as bounded by `login_started_at`.

- QR extraction will only read NapCat container logs emitted since `login_started_at`.
- If no QR is present yet in that window, the script will keep polling and will not fall back to older container history.
- Artifact overwrite behavior remains unchanged: the latest QR and metadata for the active login attempt still replace any previous files on disk.
- Because older QR URLs are no longer eligible, the script will print only the QR generated for the active post-restart login attempt.

This addresses the real root cause of the duplicate QR output: historical-log fallback crossing login attempts.

## Expected Behavior

### WeChatPadPro

- When the upstream service reports "当前客户端版本过低", the script exits with a single clean blocker line.
- No raw XML snippet is printed to the terminal.

### NapCat

- After `configure_napcat_ws.sh` restarts NapCat, `prepare_napcat_login.sh` waits for the first QR generated after that restart.
- Old QR URLs from pre-restart logs are ignored.
- Terminal output contains at most one NapCat QR URL per active login attempt unless NapCat genuinely rotates the QR within the same attempt after the script has already printed one.

## Testing

- Extend `scripts/tests/test_prepare_wechatpadpro_login.sh` to assert that the normalized blocker message is printed without raw XML content.
- Extend `scripts/tests/test_prepare_napcat_login.sh` to simulate:
  - an old QR present before `login_started_at`
  - a new QR present after `login_started_at`
  - expected behavior: only the new QR is written and printed

## Risks

- If NapCat fails to emit any QR after the current login window starts, the script will now wait instead of surfacing an old stale QR. This is intended and matches the desired semantics.
- We rely on container log timestamps being sufficient to separate pre-restart and post-restart QR emissions; this matches current observed behavior.
