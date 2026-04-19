# Deploy Content Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire repository-managed content bootstrap into the deployment flow so deploy syncs DogBot content into `DOGBOT_CONTENT_ROOT` and `agent-runner` receives all required control-plane environment variables.

**Architecture:** Keep bootstrap generation and runtime consumption separate. `deploy_stack.sh` optionally refreshes repository content packs, then always syncs repository `content/` into the external `DOGBOT_CONTENT_ROOT`. `start_agent_runner.sh` passes the full control-plane env surface into the Rust process. Script-level contract tests cover the new deploy behavior and env propagation.

**Tech Stack:** Bash, existing deploy scripts, Python content bootstrap tool, shell contract tests, `cargo test`, `pytest`

---

## File Structure

- Modify: `scripts/deploy_stack.sh`
  - refresh and sync content before starting services
- Modify: `scripts/start_agent_runner.sh`
  - pass content/control-plane/history/admin env vars to `agent-runner`
- Modify: `scripts/lib/common.sh`
  - add focused helper(s) for syncing repo content into runtime content root
- Modify: `deploy/dogbot.env.example`
  - document deploy-time content sync/refresh flags
- Modify: `deploy/README.md`
  - describe content bootstrap behavior during deploy
- Modify: `README.md`
  - mention deploy-time content bootstrap
- Modify: `scripts/check_structure.sh`
  - run new shell contract tests
- Create: `scripts/tests/test_start_agent_runner.sh`
  - verify env propagation for content/control-plane vars
- Create: `scripts/tests/test_deploy_content_bootstrap.sh`
  - verify deploy script refresh/sync behavior and env defaults

### Task 1: Add deploy-time content sync and refresh flags

**Files:**
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/lib/common.sh`
- Modify: `deploy/dogbot.env.example`
- Create: `scripts/tests/test_deploy_content_bootstrap.sh`
- Modify: `scripts/check_structure.sh`

- [ ] **Step 1: Write the failing deploy contract test**

Create `scripts/tests/test_deploy_content_bootstrap.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
deploy_script="$repo_root/scripts/deploy_stack.sh"
env_example="$repo_root/deploy/dogbot.env.example"

grep -q 'DOGBOT_SYNC_CONTENT_ON_DEPLOY' "$env_example" || {
  echo "FAIL: dogbot.env.example must declare DOGBOT_SYNC_CONTENT_ON_DEPLOY" >&2
  exit 1
}

grep -q '^DOGBOT_SYNC_CONTENT_ON_DEPLOY=1$' "$env_example" || {
  echo "FAIL: DOGBOT_SYNC_CONTENT_ON_DEPLOY must default to 1" >&2
  exit 1
}

grep -q '^DOGBOT_REFRESH_CONTENT_ON_DEPLOY=0$' "$env_example" || {
  echo "FAIL: DOGBOT_REFRESH_CONTENT_ON_DEPLOY must default to 0" >&2
  exit 1
}

grep -q 'scripts/sync_content_sources.py' "$deploy_script" || {
  echo "FAIL: deploy_stack.sh must optionally refresh repository content" >&2
  exit 1
}

grep -q 'dogbot_sync_content_root' "$deploy_script" || {
  echo "FAIL: deploy_stack.sh must sync repository content into DOGBOT_CONTENT_ROOT" >&2
  exit 1
}

echo "deploy content bootstrap checks passed."
```

- [ ] **Step 2: Register the new test in the structure check and verify red**

Add to `scripts/check_structure.sh`:

```bash
  "scripts/tests/test_deploy_content_bootstrap.sh"
```

and:

```bash
bash "$repo_root/scripts/tests/test_deploy_content_bootstrap.sh"
```

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- FAIL because the env example and deploy script do not contain the new content bootstrap integration yet

- [ ] **Step 3: Implement the minimal deploy-time content sync**

Add to `scripts/lib/common.sh`:

```bash
dogbot_sync_content_root() {
  local source_root="$1"
  local target_root="$2"

  mkdir -p "$target_root"
  rm -rf "$target_root"/local "$target_root"/packs "$target_root"/policies "$target_root"/upstream "$target_root"/sources.lock.json
  cp -a \
    "$source_root"/local \
    "$source_root"/packs \
    "$source_root"/policies \
    "$source_root"/upstream \
    "$source_root"/sources.lock.json \
    "$target_root"/
}
```

Update `deploy/dogbot.env.example` with:

```env
DOGBOT_SYNC_CONTENT_ON_DEPLOY=1
DOGBOT_REFRESH_CONTENT_ON_DEPLOY=0
```

Update `scripts/deploy_stack.sh` to:

```bash
repo_content_root="$repo_root/content"
runtime_content_root="${DOGBOT_CONTENT_ROOT:-/srv/dogbot/content}"

if [[ "${DOGBOT_REFRESH_CONTENT_ON_DEPLOY:-0}" == "1" ]]; then
  "$repo_root/scripts/sync_content_sources.py" --content-root "$repo_content_root"
fi

if [[ "${DOGBOT_SYNC_CONTENT_ON_DEPLOY:-1}" == "1" ]]; then
  dogbot_sync_content_root "$repo_content_root" "$runtime_content_root"
fi
```

Place this before `start_agent_runner.sh` is called.

- [ ] **Step 4: Run the structure check and confirm green**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/deploy_stack.sh scripts/lib/common.sh deploy/dogbot.env.example scripts/tests/test_deploy_content_bootstrap.sh scripts/check_structure.sh
git commit -m "feat: sync content bootstrap during deploy"
```

### Task 2: Pass content/control-plane env vars to `agent-runner`

**Files:**
- Modify: `scripts/start_agent_runner.sh`
- Create: `scripts/tests/test_start_agent_runner.sh`
- Modify: `scripts/check_structure.sh`

- [ ] **Step 1: Write the failing agent-runner env propagation test**

Create `scripts/tests/test_start_agent_runner.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
start_script="$repo_root/scripts/start_agent_runner.sh"

for pattern in \
  'DOGBOT_CONTENT_ROOT="${DOGBOT_CONTENT_ROOT:-./content}"' \
  'CONTROL_PLANE_DB_PATH="${CONTROL_PLANE_DB_PATH:-$AGENT_STATE_DIR/control.db}"' \
  'HISTORY_DB_PATH="${HISTORY_DB_PATH:-$AGENT_STATE_DIR/history.db}"' \
  'DOGBOT_ADMIN_ACTOR_IDS="${DOGBOT_ADMIN_ACTOR_IDS:-}"'
do
  grep -q "$pattern" "$start_script" || {
    echo "FAIL: missing agent-runner env propagation pattern: $pattern" >&2
    exit 1
  }
done

echo "start_agent_runner content env checks passed."
```

- [ ] **Step 2: Register the new test and verify red**

Add to `scripts/check_structure.sh`:

```bash
  "scripts/tests/test_start_agent_runner.sh"
```

and:

```bash
bash "$repo_root/scripts/tests/test_start_agent_runner.sh"
```

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- FAIL because `start_agent_runner.sh` does not pass those vars yet

- [ ] **Step 3: Implement the env propagation**

Update the `nohup env` block in `scripts/start_agent_runner.sh` to include:

```bash
  DOGBOT_CONTENT_ROOT="${DOGBOT_CONTENT_ROOT:-./content}" \
  CONTROL_PLANE_DB_PATH="${CONTROL_PLANE_DB_PATH:-$AGENT_STATE_DIR/control.db}" \
  HISTORY_DB_PATH="${HISTORY_DB_PATH:-$AGENT_STATE_DIR/history.db}" \
  DOGBOT_ADMIN_ACTOR_IDS="${DOGBOT_ADMIN_ACTOR_IDS:-}" \
```

Add:

```bash
mkdir -p "${DOGBOT_CONTENT_ROOT:-./content}"
```

alongside the existing directory setup.

- [ ] **Step 4: Run the structure check and confirm green**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/start_agent_runner.sh scripts/tests/test_start_agent_runner.sh scripts/check_structure.sh
git commit -m "feat: pass content bootstrap env to agent runner"
```

### Task 3: Update deploy docs and verify the integrated slice

**Files:**
- Modify: `deploy/README.md`
- Modify: `README.md`

- [ ] **Step 1: Update deploy docs**

Add to `deploy/README.md` a short section describing:

```markdown
### Deploy-Time Content Bootstrap

- `DOGBOT_REFRESH_CONTENT_ON_DEPLOY=1` refreshes repository packs from upstream sources before deploy
- `DOGBOT_SYNC_CONTENT_ON_DEPLOY=1` copies repository `content/` into `DOGBOT_CONTENT_ROOT`
- `agent-runner` reads runtime content from `DOGBOT_CONTENT_ROOT`, not directly from the repo worktree
```

- [ ] **Step 2: Update root README**

Add a short note to `README.md`:

```markdown
- deploy now syncs repository-managed `content/` into `DOGBOT_CONTENT_ROOT` before starting services
```

- [ ] **Step 3: Run integrated verification**

Run:

```bash
bash scripts/check_structure.sh
uv run --with pytest --with fastapi --with httpx python -m pytest qq_adapter/tests wechatpadpro_adapter/tests -q
cargo test --test config_tests --test context_run_tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- all listed checks pass

- [ ] **Step 4: Commit**

```bash
git add deploy/README.md README.md
git commit -m "docs: describe deploy content bootstrap flow"
```

## Self-Review Checklist

- Spec coverage:
  - deploy-time sync: Task 1
  - agent-runner env propagation: Task 2
  - documentation: Task 3
- No placeholders remain in task steps
- Type and function names are consistent:
  - `dogbot_sync_content_root`
  - `DOGBOT_SYNC_CONTENT_ON_DEPLOY`
  - `DOGBOT_REFRESH_CONTENT_ON_DEPLOY`
