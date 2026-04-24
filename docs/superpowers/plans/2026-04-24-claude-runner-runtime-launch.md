# Claude Runner Runtime Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `claude-runner` startup logic out of the image entrypoint and into runtime-generated files under `/state/claude-runner`.

**Architecture:** Keep `docker/claude-runner/entrypoint.sh` minimal so the image only bootstraps permissions and then `exec`s a runtime launch script mounted from `/state`. Generate that runtime launch script from host-side shell helpers so future Bifrost launch changes only require script updates and container restart, not image rebuild.

**Tech Stack:** Bash, Docker Compose, Rust contract tests, existing DogBot shell test harness

---

### Task 1: Generate claude-runner runtime scripts from host-side helpers

**Files:**
- Modify: `scripts/lib/common.sh`
- Test: `scripts/tests/test_common.sh`

- [ ] Add a helper that writes `launch.sh` into a caller-provided runtime directory.
- [ ] Make the generated `launch.sh` contain the current Bifrost config materialization and startup logic.
- [ ] Verify the generated file is executable and contains the expected Bifrost launch markers.

### Task 2: Make entrypoint thin and route startup through `/state`

**Files:**
- Modify: `docker/claude-runner/entrypoint.sh`
- Modify: `agent-runner/tests/compose_contract_tests.rs`

- [ ] Replace direct Bifrost startup in the image entrypoint with a thin wrapper that runs bootstrap and `exec`s `/state/claude-runner/launch.sh`.
- [ ] Update contract tests so they assert the entrypoint stays thin and the generated runtime helper owns Bifrost startup.

### Task 3: Materialize runtime launch scripts before claude-runner startup

**Files:**
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/check_structure.sh`
- Modify: `scripts/tests/test_start_agent_runner.sh`
- Modify: `scripts/tests/smoke_test_claude_runner.sh`

- [ ] Ensure host-side startup scripts generate `/state/claude-runner/launch.sh` before any claude-runner container start.
- [ ] Keep existing deploy behavior otherwise unchanged.
- [ ] Update smoke and structure checks to use and validate the generated runtime launch path.

### Task 4: Run regression verification

**Files:**
- Test only

- [ ] Run focused shell tests for common helpers and start script behavior.
- [ ] Run Rust contract and api-proxy tests.
- [ ] Run structure and smoke checks to confirm the new thin-entrypoint flow works end-to-end.
