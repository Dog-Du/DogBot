# Postgres History Read Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace DogBot's SQLite history/session persistence with PostgreSQL-backed session/history storage and expose a conversation-scoped history read skill.

**Architecture:** `agent-runner` uses PostgreSQL for session mappings, text history ingest, and short-lived read grants. Claude receives fixed reader credentials plus a per-run token through Docker exec environment variables. The first-party history skill queries `agent_read.messages`, while PostgreSQL RLS enforces current-conversation or admin visibility.

**Tech Stack:** Rust 2024, `postgres` crate, PostgreSQL 15+, Docker Compose, Python 3 stdlib helper script, Claude prompt skills.

---

## File Structure

- Modify `agent-runner/Cargo.toml`: add `postgres`, `sha2`, `base64`, and `rand`.
- Modify `agent-runner/src/config.rs`: replace SQLite path settings with PostgreSQL and admin/history settings.
- Rewrite `agent-runner/src/session_store.rs`: keep the public session API, back it with `runner_sessions` and `runner_session_aliases`.
- Rewrite `agent-runner/src/history/store.rs`: store text messages in `history_messages`, create grants in `history_read_grants`, initialize schema/RLS/view.
- Modify `agent-runner/src/history/cleanup.rs`: purge expired grants and retained messages.
- Modify `agent-runner/src/docker_client.rs`: support per-exec environment variables.
- Modify `agent-runner/src/exec.rs`: generate history read grants and inject DB env into Claude exec.
- Modify `agent-runner/src/server.rs`: open PostgreSQL stores from settings, remove SQLite path setup in test helpers, and keep history mirroring behavior.
- Create `claude-prompt/skills/history-read/SKILL.md`: agent-facing instructions.
- Create `claude-prompt/skills/history-read/history_query.py`: search and SQL helper.
- Modify `deploy/dogbot.env.example`, `deploy/README.md`, `deploy/docker/docker-compose.yml`, and scripts to configure PostgreSQL instead of SQLite files.
- Update tests under `agent-runner/tests/` from SQLite path assumptions to PostgreSQL connection settings where covered in this implementation.

## Task 1: Configuration

**Files:**
- Modify: `agent-runner/Cargo.toml`
- Modify: `agent-runner/src/config.rs`
- Test: `agent-runner/tests/config_tests.rs`

- [ ] **Step 1: Write failing config tests**

Add expectations that defaults include:

```rust
assert_eq!(
    settings.database_url,
    "postgres://dogbot_admin:change-me@127.0.0.1:5432/dogbot"
);
assert_eq!(settings.postgres_agent_reader_user, "dogbot_agent_reader");
assert_eq!(settings.history_run_token_ttl_secs, 1800);
assert_eq!(settings.history_retention_days, 180);
assert!(settings.admin_actor_ids.is_empty());
```

Run: `cargo test --manifest-path agent-runner/Cargo.toml config_tests -- --nocapture`

Expected: compile failure or assertion failure because fields do not exist.

- [ ] **Step 2: Implement config fields**

Add fields to `Settings`:

```rust
pub database_url: String,
pub postgres_agent_reader_user: String,
pub postgres_agent_reader_password: String,
pub history_run_token_ttl_secs: i64,
pub history_retention_days: i64,
pub admin_actor_ids: Vec<String>,
```

Parse:

```rust
let postgres_host = string_or_default(&env_map, "POSTGRES_HOST", "127.0.0.1");
let postgres_port = parse_or_default(&env_map, "POSTGRES_PORT", 5432)?;
let postgres_db = string_or_default(&env_map, "POSTGRES_DB", "dogbot");
let postgres_admin_user = string_or_default(&env_map, "POSTGRES_ADMIN_USER", "dogbot_admin");
let postgres_admin_password =
    string_or_default(&env_map, "POSTGRES_ADMIN_PASSWORD", "change-me");
let database_url = optional_trimmed(&env_map, "DATABASE_URL").unwrap_or_else(|| {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        postgres_admin_user, postgres_admin_password, postgres_host, postgres_port, postgres_db
    )
});
```

Add dependencies:

```toml
postgres = "0.19"
sha2 = "0.10"
base64 = "0.22"
rand = "0.8"
```

- [ ] **Step 3: Run config tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml config_tests -- --nocapture`

Expected: config tests pass.

## Task 2: PostgreSQL History Store

**Files:**
- Rewrite: `agent-runner/src/history/store.rs`
- Modify: `agent-runner/src/history/cleanup.rs`
- Test: `agent-runner/tests/history_ingest_tests.rs`

- [ ] **Step 1: Add history SQL initialization tests**

Add tests that call a pure SQL rendering helper and assert it contains:

```rust
assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_messages"));
assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_read_grants"));
assert!(sql.contains("ENABLE ROW LEVEL SECURITY"));
assert!(sql.contains("CREATE OR REPLACE VIEW agent_read.messages"));
```

Run: `cargo test --manifest-path agent-runner/Cargo.toml history_ingest_tests -- --nocapture`

Expected: failure until the helper exists.

- [ ] **Step 2: Implement store shape**

Define:

```rust
#[derive(Debug, Clone)]
pub struct HistoryStore {
    database_url: String,
    reader_database_url: String,
    retention_days: i64,
}
```

Public API:

```rust
pub fn open(settings: &crate::config::Settings) -> Result<Self, HistoryStoreError>;
pub fn initialize_schema(&self) -> Result<(), HistoryStoreError>;
pub fn insert_canonical_event(&self, event: &CanonicalEvent) -> Result<(), HistoryStoreError>;
pub fn create_read_grant(&self, grant: HistoryReadGrant) -> Result<HistoryReadGrantToken, HistoryStoreError>;
pub fn purge_expired(&self) -> Result<(), HistoryStoreError>;
```

Insert text-only message rows with `ON CONFLICT(platform_account, conversation_id, message_id) DO UPDATE`.

- [ ] **Step 3: Implement read grant tokens**

Generate 32 random bytes and base64url encode them:

```rust
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;

fn generate_run_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
```

Hash with SHA-256 and insert `token_hash` bytes.

- [ ] **Step 4: Run history tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml history_ingest_tests -- --nocapture`

Expected: SQL helper tests pass. Live PostgreSQL tests should be gated behind `DOGBOT_TEST_DATABASE_URL`.

## Task 3: PostgreSQL Session Store

**Files:**
- Rewrite: `agent-runner/src/session_store.rs`
- Test: `agent-runner/tests/session_store_tests.rs`
- Test: `agent-runner/src/exec.rs` unit tests

- [ ] **Step 1: Preserve API tests**

Keep the existing public methods:

```rust
get_or_create_conversation_session
reset_conversation_session
get_or_create_bound_session
bind_external_session_id
validate_external_session_binding
get_session
reset_bound_session
```

Convert tests to use a helper that reads `DOGBOT_TEST_DATABASE_URL`; skip live DB tests when unset with:

```rust
let Some(url) = std::env::var("DOGBOT_TEST_DATABASE_URL").ok() else {
    eprintln!("DOGBOT_TEST_DATABASE_URL unset; skipping postgres integration test");
    return;
};
```

- [ ] **Step 2: Implement schema**

Create `runner_sessions` and `runner_session_aliases` using the SQL from the spec.

- [ ] **Step 3: Implement operations**

Use `postgres::Client::connect(&self.database_url, postgres::NoTls)` per operation and preserve conflict behavior from the SQLite implementation.

- [ ] **Step 4: Run session tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml session_store_tests exec::tests -- --nocapture`

Expected: non-DB tests pass; DB integration tests skip unless `DOGBOT_TEST_DATABASE_URL` is set.

## Task 4: Runtime Grant Injection

**Files:**
- Modify: `agent-runner/src/docker_client.rs`
- Modify: `agent-runner/src/exec.rs`
- Test: `agent-runner/tests/docker_client_tests.rs`
- Test: `agent-runner/src/exec.rs`

- [ ] **Step 1: Write env injection tests**

Assert `CreateExecOptions.env` contains supplied per-exec values in a pure helper.

- [ ] **Step 2: Add per-exec env support**

Change:

```rust
pub async fn create_claude_exec(
    &self,
    container_name: &str,
    cwd: &str,
    command: Vec<String>,
    env: Vec<String>,
)
```

Set `env: Some(env)` in `CreateExecOptions`.

- [ ] **Step 3: Create grant before exec**

In `DockerRunner::execute_once`, create a history read grant and inject:

```text
DOGBOT_HISTORY_DATABASE_URL=<reader url>
DOGBOT_HISTORY_RUN_TOKEN=<token>
PGOPTIONS=-c dogbot.run_token=<token> -c statement_timeout=5000
```

For admin private runs, issue admin grant rows using configured admin actor IDs.

- [ ] **Step 4: Run runtime tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml docker_client_tests exec::tests -- --nocapture`

Expected: env command assembly tests pass.

## Task 5: History Skill

**Files:**
- Create: `claude-prompt/skills/history-read/SKILL.md`
- Create: `claude-prompt/skills/history-read/history_query.py`
- Modify: `claude-prompt/CLAUDE.md`
- Test: `agent-runner/tests/prompt_contract_tests.rs`

- [ ] **Step 1: Add prompt contract test**

Assert `claude-prompt/CLAUDE.md` mentions `skills/history-read/SKILL.md`.

- [ ] **Step 2: Implement Python helper**

The helper supports:

```bash
python3 history_query.py search --since ... --until ... --sender ... --contains ... --limit 20
python3 history_query.py sql "select ... from agent_read.messages ..."
```

It reads `DOGBOT_HISTORY_DATABASE_URL` and `DOGBOT_HISTORY_RUN_TOKEN`, builds
`PGOPTIONS`, invokes `psql`, and prints query output.

- [ ] **Step 3: Implement skill docs**

Tell the agent to use the helper when it needs prior text messages and to avoid
inventing history when no rows are returned.

- [ ] **Step 4: Run prompt tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml prompt_contract_tests -- --nocapture`

Expected: prompt contract tests pass.

## Task 6: Deployment and Docs

**Files:**
- Modify: `deploy/docker/docker-compose.yml`
- Modify: `deploy/dogbot.env.example`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `README.md`
- Modify: `deploy/README.md`
- Test: `scripts/tests/test_start_agent_runner.sh`
- Test: `scripts/tests/test_deploy_stack_platform_ingress.sh`

- [ ] **Step 1: Add Postgres service**

Add `postgres` service with image `postgres:15`, volume under
`${AGENT_STATE_DIR}/postgres`, and env vars from `dogbot.env`.

- [ ] **Step 2: Remove SQLite file preparation**

Stop creating `runner.db` and `history.db` files in scripts. Export PostgreSQL
settings to `agent-runner`.

- [ ] **Step 3: Update docs**

Replace `SESSION_DB_PATH` and `HISTORY_DB_PATH` references with PostgreSQL
configuration and note that old SQLite data is discarded.

- [ ] **Step 4: Run script tests**

Run: `bash scripts/tests/test_start_agent_runner.sh && bash scripts/tests/test_deploy_stack_platform_ingress.sh`

Expected: script tests pass after expected string updates.

## Task 7: Verification

**Files:**
- All touched files

- [ ] **Step 1: Format**

Run: `cargo fmt --manifest-path agent-runner/Cargo.toml`

Expected: no formatting diff required afterward.

- [ ] **Step 2: Rust tests**

Run: `cargo test --manifest-path agent-runner/Cargo.toml -- --nocapture`

Expected: tests that do not require `DOGBOT_TEST_DATABASE_URL` pass; PostgreSQL
integration tests skip when the env var is unset.

- [ ] **Step 3: Shell tests**

Run: `bash scripts/tests/test_start_agent_runner.sh`

Expected: pass.

- [ ] **Step 4: Git review**

Run: `git status --short && git diff --stat`

Expected: only intentional files changed.
