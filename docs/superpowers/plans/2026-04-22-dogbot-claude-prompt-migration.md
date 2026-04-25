# DogBot Claude Prompt Migration Plan

## 2026-04-26 Plan Corrections

This migration plan predates the current prompt exposure model. The following corrections override older step text below:

- `claude-prompt/skills/**` is now the source-of-truth skill directory
- `.claude` is obsolete and must not be reintroduced
- `claude-runner` no longer projects prompt files into `/workspace`
- `agent-runner` now relies on:
  - `--add-dir /state/claude-prompt`
  - system prompt instructions that require reading `/state/claude-prompt/CLAUDE.md`
  - the reply protocol skill at `/state/claude-prompt/skills/reply-format/SKILL.md`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove DogBot's repository-managed content bootstrap pipeline and replace it with a lightweight Claude-native `claude-prompt/` source directory that deploys into the Claude runtime.

**Architecture:** Delete the `content/` pack/sync/cleanup chain end-to-end. Keep `agent-runner` focused on dynamic runtime context and container/session orchestration. Introduce `claude-prompt/` as the only static Claude-facing content source, sync it into a runtime directory during deploy, and expose that directory to Claude Code inside the container.

**Tech Stack:** Rust, Bash, Docker Compose, Claude Code CLI, repository-managed Markdown skills, existing shell contract tests, existing Rust tests

---

## File Structure

- Delete: `content/`
- Delete: `scripts/sync_content_sources.py`
- Delete: `scripts/audit_legacy_runtime_memory.py`
- Delete: `scripts/cleanup_legacy_claude_content.py`
- Delete: `scripts/tests/test_sync_content_sources.py`
- Delete: `scripts/tests/test_audit_legacy_runtime_memory.py`
- Delete: `scripts/tests/test_cleanup_legacy_claude_content.py`
- Delete: `scripts/tests/test_claude_legacy_cleanup_deploy.sh`
- Delete: `scripts/tests/test_deploy_content_bootstrap.sh`
- Delete: `agent-runner/src/context/repo_loader.rs`
- Delete: `agent-runner/tests/repo_loader_tests.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/context/context_pack.rs`
- Modify: `agent-runner/src/context/mod.rs`
- Modify: `agent-runner/src/docker_client.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `agent-runner/tests/context_run_tests.rs`
- Modify: `agent-runner/tests/docker_client_tests.rs`
- Create: `claude-prompt/CLAUDE.md`
- Create: `claude-prompt/persona.md`
- Create: `claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md`
- Modify: `compose/docker-compose.yml`
- Modify: `deploy/README.md`
- Modify: `deploy/dogbot.env.example`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/control-plane-integration.md`
- Delete: `docs/superpowers/specs/2026-04-19-dogbot-content-bootstrap-design.md`
- Delete: `docs/superpowers/plans/2026-04-19-dogbot-content-bootstrap.md`
- Delete: `docs/superpowers/plans/2026-04-19-deploy-content-bootstrap.md`
- Modify: `scripts/check_structure.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/lib/common.sh`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/tests/test_start_agent_runner.sh`

### Task 1: Replace config and runtime wiring from `content` to `claude-prompt`

**Files:**
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `deploy/dogbot.env.example`

- [ ] **Step 1: Write the failing config test**

Add a test to `agent-runner/tests/config_tests.rs` asserting:

- `DOGBOT_CLAUDE_PROMPT_ROOT` overrides the default
- `DOGBOT_CONTENT_ROOT` is no longer read

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- FAIL because `Settings` still exposes `content_root`

- [ ] **Step 3: Implement the minimal config rename**

Change `Settings` in `agent-runner/src/config.rs`:

- rename `content_root` to `claude_prompt_root`
- read from `DOGBOT_CLAUDE_PROMPT_ROOT`
- default to `./claude-prompt`

Update `scripts/start_agent_runner.sh` and `deploy/dogbot.env.example` to pass/use the new variable only.

- [ ] **Step 4: Re-run the config test**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- PASS

### Task 2: Remove pack manifest loading from `agent-runner`

**Files:**
- Delete: `agent-runner/src/context/repo_loader.rs`
- Delete: `agent-runner/tests/repo_loader_tests.rs`
- Modify: `agent-runner/src/context/context_pack.rs`
- Modify: `agent-runner/src/context/mod.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/tests/context_run_tests.rs`

- [ ] **Step 1: Write the failing runtime context test**

Replace the current pack-item-focused test in `agent-runner/tests/context_run_tests.rs` with a test that asserts:

- `/v1/runs` still injects readable scopes
- `/v1/runs` still injects history evidence when present
- no `"Enabled pack items:"` marker is expected

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- FAIL because server/context code still loads enabled pack items

- [ ] **Step 3: Delete pack-loading code**

Implement the minimum removal:

- remove `repo_loader` module export from `agent-runner/src/context/mod.rs`
- simplify `context_pack.rs` so it only renders scopes + optional history evidence
- remove `RepoContentLoader` usage from `server.rs`
- delete `repo_loader.rs` and its tests

- [ ] **Step 4: Re-run the context tests**

Run:

```bash
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- PASS

### Task 3: Expose `claude-prompt` to Claude Code

**Files:**
- Modify: `agent-runner/src/docker_client.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/tests/docker_client_tests.rs`
- Modify: `compose/docker-compose.yml`

- [ ] **Step 1: Write the failing container/command tests**

Update tests to assert:

- container env includes `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`
- `build_claude_command` allows `/state/claude-prompt`

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test --test docker_client_tests --manifest-path agent-runner/Cargo.toml
cargo test exec::tests --manifest-path agent-runner/Cargo.toml
```

Expected:
- FAIL because env/command still only mention `/workspace` and `/state`

- [ ] **Step 3: Implement the minimal Claude-native wiring**

Update:

- `docker_client.rs` env to include `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`
- `exec.rs` command builder to add `/state/claude-prompt`
- `compose/docker-compose.yml` env surface to match

- [ ] **Step 4: Re-run the tests**

Run:

```bash
cargo test --test docker_client_tests --manifest-path agent-runner/Cargo.toml
cargo test --manifest-path agent-runner/Cargo.toml exec::tests
```

Expected:
- PASS

### Task 4: Replace deploy-time content sync with `claude-prompt` sync

**Files:**
- Modify: `scripts/lib/common.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/tests/test_start_agent_runner.sh`
- Modify: `scripts/check_structure.sh`
- Delete: `scripts/tests/test_deploy_content_bootstrap.sh`
- Delete: `scripts/tests/test_claude_legacy_cleanup_deploy.sh`

- [ ] **Step 1: Write the failing shell contract expectations**

Update shell tests so they expect:

- `DOGBOT_CLAUDE_PROMPT_ROOT`
- a sync helper for `claude-prompt/`
- no references to `sync_content_sources.py` or `cleanup_legacy_claude_content.py`

- [ ] **Step 2: Run the shell tests to verify they fail**

Run:

```bash
bash scripts/tests/test_start_agent_runner.sh
bash scripts/check_structure.sh
```

Expected:
- FAIL because old content/bootstrap expectations still exist

- [ ] **Step 3: Implement deploy script cleanup**

Update scripts to:

- remove all content/bootstrap/cleanup flags
- add a helper that syncs `claude-prompt/` into `DOGBOT_CLAUDE_PROMPT_ROOT`
- create the runtime prompt directory during deploy/start

- [ ] **Step 4: Re-run the shell tests**

Run:

```bash
bash scripts/tests/test_start_agent_runner.sh
bash scripts/check_structure.sh
```

Expected:
- PASS

### Task 5: Add the new `claude-prompt/` source tree

**Files:**
- Create: `claude-prompt/CLAUDE.md`
- Create: `claude-prompt/persona.md`
- Create: `claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md`
- Modify: `scripts/check_structure.sh`

- [ ] **Step 1: Write the failing structure check**

Update `scripts/check_structure.sh` so it requires:

```text
claude-prompt/CLAUDE.md
claude-prompt/persona.md
claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md
```

and no longer requires `content/...` or removed Python tools.

- [ ] **Step 2: Run the structure check to verify it fails**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- FAIL because `claude-prompt/` does not exist yet

- [ ] **Step 3: Create the minimal Claude-native source files**

Create:

- `claude-prompt/CLAUDE.md`
  - DogBot runtime role
  - explicit trigger rules
  - image/output boundaries
  - `@persona.md`
  - `dogbot-memory` fenced block contract
- `claude-prompt/persona.md`
  - default conversational persona
  - concise voice rules
- `claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md`
  - when to emit memory
  - exact JSON fields required by runner

- [ ] **Step 4: Re-run the structure check**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- PASS

### Task 6: Delete obsolete files and docs, then update the remaining docs

**Files:**
- Delete: `content/`
- Delete: old bootstrap/cleanup scripts and tests
- Delete: old content-bootstrap docs
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `deploy/dogbot.env.example`
- Modify: `docs/README.md`
- Modify: `docs/control-plane-integration.md`

- [ ] **Step 1: Remove obsolete files**

Delete the old `content/` tree, content bootstrap scripts/tests, and the three content-bootstrap docs.

- [ ] **Step 2: Update user-facing docs**

Update surviving docs so they describe:

- `claude-prompt/` as the static source of truth
- deploy-time sync into runtime prompt dir
- no upstream content refresh
- no legacy Claude content cleanup flow

- [ ] **Step 3: Run the targeted regression set**

Run:

```bash
cargo test --test config_tests --test context_run_tests --test docker_client_tests --manifest-path agent-runner/Cargo.toml
bash scripts/tests/test_start_agent_runner.sh
bash scripts/check_structure.sh
```

Expected:
- PASS

### Task 7: Final verification

**Files:**
- Review only

- [ ] **Step 1: Check repository status**

Run:

```bash
git status --short
```

Expected:
- no unexpected old content/bootstrap files left behind

- [ ] **Step 2: Check for stale references**

Run:

```bash
rg -n "DOGBOT_CONTENT_ROOT|sync_content_sources|cleanup_legacy_claude_content|content/packs|sources.lock|DOGBOT_SYNC_CONTENT_ON_DEPLOY|DOGBOT_REFRESH_CONTENT_ON_DEPLOY|DOGBOT_PRUNE_LEGACY_CLAUDE_CONTENT_ON_DEPLOY" README.md deploy docs scripts agent-runner compose
```

Expected:
- no remaining references outside intentionally deleted-history files

- [ ] **Step 3: Summarize residual risk**

Confirm only these residual risks remain:

- Claude Code additional-directory `CLAUDE.md` behavior is exercised indirectly, not by a full integration test
- initial skill set is intentionally minimal
