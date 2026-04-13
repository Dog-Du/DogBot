# Default Chat Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make AstrBot route normal messages to the agent by default, isolate sessions per actual platform conversation, mention the original sender on group replies, and run Claude Code with bypassed permission prompts inside the sandboxed container.

**Architecture:** Keep platform-specific message parsing inside the AstrBot plugin while leaving execution, timeout, queueing, and outbound delivery inside `agent-runner`. Session isolation is strengthened by using AstrBot's platform-provided session identity (`unified_msg_origin`) instead of synthetic IDs, while group reply mentions are injected through both passive plugin replies and the proactive `/v1/messages` path.

**Tech Stack:** Rust (`axum`, `tokio`, `serde`), Python plugin code in AstrBot, `uv` + `pytest`, Dockerized Claude Code CLI

---

## File Structure

- Modify: `astrbot/plugins/claude_runner_bridge/main.py`
  Switch from command-only routing to default message routing, preserve explicit status command handling, use platform-provided conversation identity, and emit group replies as `@sender + text`.
- Create: `astrbot/plugins/claude_runner_bridge/tests/test_main.py`
  Plugin-focused tests for routing decisions, payload/session construction, and group reply formatting.
- Modify: `agent-runner/src/exec.rs`
  Add Claude CLI flags for bypassed permission prompts and broader in-container directory access.
- Modify: `agent-runner/src/server.rs`
  Default proactive group sends to mentioning the session owner when no explicit mention is provided.
- Modify: `agent-runner/tests/http_api_tests.rs`
  Lock the proactive group-mention default behavior.
- Modify: `agent-runner/src/exec.rs` tests
  Lock the new Claude CLI command shape.

### Task 1: Lock the new behavior with tests

**Files:**
- Create: `astrbot/plugins/claude_runner_bridge/tests/test_main.py`
- Modify: `agent-runner/tests/http_api_tests.rs`
- Modify: `agent-runner/src/exec.rs`

- [ ] **Step 1: Add failing Python plugin tests**
- [ ] **Step 2: Run `uv run --with pytest python -m pytest astrbot/plugins/claude_runner_bridge/tests/test_main.py -q` and confirm failure**
- [ ] **Step 3: Add failing Rust tests for default group mentions and Claude permission flags**
- [ ] **Step 4: Run `cargo test --test http_api_tests exec::tests::build_claude_command_uses_resume_for_existing_sessions --manifest-path agent-runner/Cargo.toml` and confirm failure**

### Task 2: Implement plugin routing and reply metadata

**Files:**
- Modify: `astrbot/plugins/claude_runner_bridge/main.py`

- [ ] **Step 1: Route normal messages to the agent unless they are special slash commands**
- [ ] **Step 2: Build conversation IDs from AstrBot session identity and sender metadata**
- [ ] **Step 3: Reply to group messages with `@sender` at the beginning**
- [ ] **Step 4: Re-run Python tests and confirm pass**

### Task 3: Implement runner-side permission and mention defaults

**Files:**
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`

- [ ] **Step 1: Add Claude CLI permission-bypass flags and shared directory access**
- [ ] **Step 2: Default proactive group sends to mentioning the stored session user**
- [ ] **Step 3: Re-run targeted Rust tests and confirm pass**

### Task 4: Verify the integrated behavior

**Files:**
- Modify: `astrbot/plugins/claude_runner_bridge/README.md`

- [ ] **Step 1: Update plugin docs for default routing and special commands**
- [ ] **Step 2: Run `cargo test --manifest-path agent-runner/Cargo.toml`**
- [ ] **Step 3: Run `uv run --with pytest python -m pytest astrbot/plugins/claude_runner_bridge/tests/test_main.py -q`**
- [ ] **Step 4: Run `./scripts/check_structure.sh`**
