# DogBot Control Plane Phase A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first control-plane slice inside `agent-runner`: identity normalization, scope resolution, permission enforcement, repository-loaded content, and memory-isolated context loading.

**Architecture:** Keep the current `/v1/runs` execution path, but insert a new internal `context` subsystem before Claude execution. Use SQLite in the existing runner state directory for persistent objects and continue using repository files for DogBot-managed `resource`, `skill`, and `policy` content.

**Tech Stack:** Rust, `rusqlite`, `axum`, `serde`, existing Python adapters, SQLite, `cargo test`, `pytest`

---

## File Structure

- Create: `agent-runner/src/context/mod.rs`
  - module exports and shared types
- Create: `agent-runner/src/context/identity.rs`
  - `platform_account`, `actor`, `conversation` normalization helpers
- Create: `agent-runner/src/context/scope.rs`
  - readable and writable scope resolution
- Create: `agent-runner/src/context/policy.rs`
  - admin whitelist and permission checks
- Create: `agent-runner/src/context/object_store.rs`
  - SQLite schema and CRUD for `memory`, `resource`, `skill`, `policy`, `memory_candidate`
- Create: `agent-runner/src/context/context_pack.rs`
  - render loaded context into Claude-facing prompt text
- Create: `agent-runner/src/context/repo_loader.rs`
  - load repository-managed resources, skills, and policies
- Create: `agent-runner/src/context/memory_intent.rs`
  - parse structured memory-write intents from Claude output
- Create: `agent-runner/tests/context_scope_tests.rs`
  - scope and permission regression coverage
- Create: `agent-runner/tests/context_store_tests.rs`
  - SQLite schema and repo-loader coverage
- Create: `agent-runner/tests/context_run_tests.rs`
  - `/v1/runs` integration coverage for context-pack injection
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`
- Modify: `qq_adapter/config.py`
- Modify: `qq_adapter/mapper.py`
- Modify: `qq_adapter/tests/test_mapper.py`
- Modify: `wechatpadpro_adapter/config.py`
- Modify: `wechatpadpro_adapter/mapper.py`
- Modify: `wechatpadpro_adapter/tests/test_mapper.py`
- Create: `content/resources/.gitkeep`
- Create: `content/skills/.gitkeep`
- Create: `content/policies/defaults.json`

### Task 1: Add control-plane config and request surface

**Files:**
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `qq_adapter/config.py`
- Modify: `qq_adapter/mapper.py`
- Modify: `qq_adapter/tests/test_mapper.py`
- Modify: `wechatpadpro_adapter/config.py`
- Modify: `wechatpadpro_adapter/mapper.py`
- Modify: `wechatpadpro_adapter/tests/test_mapper.py`

- [ ] **Step 1: Write the failing Rust config test**

Add to `agent-runner/tests/config_tests.rs`:

```rust
#[test]
fn settings_parse_control_plane_fields() {
    let settings = agent_runner::config::Settings::from_env_map(std::collections::HashMap::from([
        ("DOGBOT_CONTENT_ROOT".into(), "/srv/dogbot/content".into()),
        ("CONTROL_PLANE_DB_PATH".into(), "/srv/dogbot/runtime/agent-state/control.db".into()),
        ("DOGBOT_ADMIN_ACTOR_IDS".into(), "qq:user:1,wechat:user:wxid_admin".into()),
    ]))
    .unwrap();

    assert_eq!(settings.content_root, "/srv/dogbot/content");
    assert_eq!(settings.control_plane_db_path, "/srv/dogbot/runtime/agent-state/control.db");
    assert_eq!(
        settings.admin_actor_ids,
        vec!["qq:user:1".to_string(), "wechat:user:wxid_admin".to_string()]
    );
}
```

- [ ] **Step 2: Write the failing adapter mapper tests**

Add to `qq_adapter/tests/test_mapper.py`:

```python
def test_group_payload_includes_platform_account_id():
    event = {
        "message_type": "group",
        "raw_message": "[CQ:at,qq=123] /agent hi",
        "user_id": 1,
        "group_id": 2,
        "message_id": 9,
    }
    payload = build_run_payload(
        event,
        prompt="hi",
        default_cwd="/workspace",
        timeout_secs=120,
        platform_account_id="qq:bot_uin:123",
    )
    assert payload["platform_account_id"] == "qq:bot_uin:123"
```

Add to `wechatpadpro_adapter/tests/test_mapper.py`:

```python
def test_private_payload_includes_platform_account_id():
    payload = build_run_payload(
        {"message": {"content": "/agent hi", "fromUserName": "wxid_user"}},
        default_cwd="/workspace",
        timeout_secs=120,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
    )
    assert payload["platform_account_id"] == "wechatpadpro:account:wxid_bot_1"
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml settings_parse_control_plane_fields
uv run --with pytest python -m pytest qq_adapter/tests/test_mapper.py -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests/test_mapper.py -q
```

Expected:
- Rust test fails because `Settings` has no control-plane fields
- Python tests fail because `build_run_payload()` has no `platform_account_id` parameter

- [ ] **Step 4: Write the minimal implementation**

Update `agent-runner/src/config.rs` to add:

```rust
pub content_root: String,
pub control_plane_db_path: String,
pub admin_actor_ids: Vec<String>,
```

and parse them with:

```rust
let content_root = string_or_default(&env_map, "DOGBOT_CONTENT_ROOT", "./content");
let control_plane_db_path = optional_trimmed(&env_map, "CONTROL_PLANE_DB_PATH")
    .unwrap_or_else(|| format!("{state_dir}/control.db"));
let admin_actor_ids = optional_trimmed(&env_map, "DOGBOT_ADMIN_ACTOR_IDS")
    .map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
```

Update `agent-runner/src/models.rs`:

```rust
pub platform_account_id: String,
```

Update adapter settings and payload builders to pass a stable platform account identifier:

```python
platform_account_id=os.getenv("QQ_PLATFORM_ACCOUNT_ID", "qq:bot_uin:unknown").strip()
```

```python
"platform_account_id": platform_account_id,
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml settings_parse_control_plane_fields
uv run --with pytest python -m pytest qq_adapter/tests/test_mapper.py -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests/test_mapper.py -q
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add agent-runner/src/config.rs agent-runner/src/models.rs agent-runner/tests/config_tests.rs qq_adapter/config.py qq_adapter/mapper.py qq_adapter/tests/test_mapper.py wechatpadpro_adapter/config.py wechatpadpro_adapter/mapper.py wechatpadpro_adapter/tests/test_mapper.py
git commit -m "feat: add control plane config surface"
```

### Task 2: Build identity, scope, and permission resolution

**Files:**
- Create: `agent-runner/src/context/mod.rs`
- Create: `agent-runner/src/context/identity.rs`
- Create: `agent-runner/src/context/scope.rs`
- Create: `agent-runner/src/context/policy.rs`
- Create: `agent-runner/tests/context_scope_tests.rs`
- Modify: `agent-runner/src/lib.rs`

- [ ] **Step 1: Write the failing scope tests**

Create `agent-runner/tests/context_scope_tests.rs` with:

```rust
use agent_runner::context::{
    policy::PermissionPolicy,
    scope::{ReadableScopes, ScopeKind, ScopeResolver},
};

#[test]
fn scope_resolver_orders_readable_scopes_from_local_to_global() {
    let scopes = ScopeResolver::new().readable_scopes(
        "qq:user:1",
        "qq:group:100",
        "qq:bot_uin:123",
    );

    assert_eq!(
        scopes,
        vec![
            ReadableScopes::new(ScopeKind::UserPrivate, "qq:user:1"),
            ReadableScopes::new(ScopeKind::ConversationShared, "qq:group:100"),
            ReadableScopes::new(ScopeKind::PlatformAccountShared, "qq:bot_uin:123"),
            ReadableScopes::new(ScopeKind::BotGlobalAdmin, "dogbot"),
        ]
    );
}

#[test]
fn permission_policy_blocks_unapproved_conversation_shared_write() {
    let policy = PermissionPolicy::new(vec!["qq:user:admin".into()]);

    let result = policy.can_write_conversation_shared(
        "qq:user:1",
        "qq:group:100",
        &std::collections::BTreeSet::new(),
    );

    assert!(!result.allowed);
    assert_eq!(result.reason.as_deref(), Some("actor_not_authorized_for_conversation"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test context_scope_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the `context` module does not exist yet

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/context/mod.rs`:

```rust
pub mod identity;
pub mod policy;
pub mod scope;
```

Create `agent-runner/src/context/scope.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    UserPrivate,
    ConversationShared,
    PlatformAccountShared,
    BotGlobalAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableScopes {
    pub kind: ScopeKind,
    pub id: String,
}

impl ReadableScopes {
    pub fn new(kind: ScopeKind, id: impl Into<String>) -> Self {
        Self { kind, id: id.into() }
    }
}

pub struct ScopeResolver;

impl ScopeResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn readable_scopes(
        &self,
        actor_id: &str,
        conversation_id: &str,
        platform_account_id: &str,
    ) -> Vec<ReadableScopes> {
        vec![
            ReadableScopes::new(ScopeKind::UserPrivate, actor_id),
            ReadableScopes::new(ScopeKind::ConversationShared, conversation_id),
            ReadableScopes::new(ScopeKind::PlatformAccountShared, platform_account_id),
            ReadableScopes::new(ScopeKind::BotGlobalAdmin, "dogbot"),
        ]
    }
}
```

Create `agent-runner/src/context/policy.rs`:

```rust
use std::collections::BTreeSet;

pub struct PermissionPolicy {
    admin_actor_ids: BTreeSet<String>,
}

pub struct PermissionDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl PermissionPolicy {
    pub fn new(admin_actor_ids: Vec<String>) -> Self {
        Self {
            admin_actor_ids: admin_actor_ids.into_iter().collect(),
        }
    }

    pub fn can_write_conversation_shared(
        &self,
        actor_id: &str,
        _conversation_id: &str,
        authorized_actor_ids: &BTreeSet<String>,
    ) -> PermissionDecision {
        if self.admin_actor_ids.contains(actor_id) || authorized_actor_ids.contains(actor_id) {
            return PermissionDecision { allowed: true, reason: None };
        }

        PermissionDecision {
            allowed: false,
            reason: Some("actor_not_authorized_for_conversation".into()),
        }
    }
}
```

Export the module in `agent-runner/src/lib.rs`:

```rust
pub mod context;
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test context_scope_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/lib.rs agent-runner/src/context/mod.rs agent-runner/src/context/identity.rs agent-runner/src/context/scope.rs agent-runner/src/context/policy.rs agent-runner/tests/context_scope_tests.rs
git commit -m "feat: add scope and permission resolution"
```

### Task 3: Add persistent object storage and repository loaders

**Files:**
- Create: `agent-runner/src/context/object_store.rs`
- Create: `agent-runner/src/context/repo_loader.rs`
- Create: `agent-runner/tests/context_store_tests.rs`
- Create: `content/resources/.gitkeep`
- Create: `content/skills/.gitkeep`
- Create: `content/policies/defaults.json`

- [ ] **Step 1: Write the failing storage tests**

Create `agent-runner/tests/context_store_tests.rs`:

```rust
use agent_runner::context::object_store::ContextObjectStore;

#[test]
fn object_store_creates_required_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("control.db");

    let store = ContextObjectStore::open(&db_path).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"context_objects".to_string()));
    assert!(tables.contains(&"memory_candidates".to_string()));
    assert!(tables.contains(&"conversation_authorizations".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test context_store_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because `ContextObjectStore` does not exist yet

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/context/object_store.rs`:

```rust
use rusqlite::Connection;
use std::path::Path;

pub struct ContextObjectStore {
    conn: Connection,
}

impl ContextObjectStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS context_objects (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                scope_kind TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                body_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_candidates (
                id TEXT PRIMARY KEY,
                actor_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                candidate_json TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversation_authorizations (
                conversation_id TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                PRIMARY KEY (conversation_id, actor_id)
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn table_names(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }
}
```

Create `agent-runner/src/context/repo_loader.rs`:

```rust
pub struct RepoContentLoader {
    pub root: String,
}

impl RepoContentLoader {
    pub fn new(root: impl Into<String>) -> Self {
        Self { root: root.into() }
    }
}
```

Create `content/policies/defaults.json`:

```json
{
  "memory": {
    "allow_auto_commit_user_private": true,
    "allow_auto_commit_conversation_shared": false
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test context_store_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/context/object_store.rs agent-runner/src/context/repo_loader.rs agent-runner/tests/context_store_tests.rs content/resources/.gitkeep content/skills/.gitkeep content/policies/defaults.json
git commit -m "feat: add context object storage and repo loader scaffold"
```

### Task 4: Inject context packs into the run path

**Files:**
- Create: `agent-runner/src/context/context_pack.rs`
- Create: `agent-runner/tests/context_run_tests.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`

- [ ] **Step 1: Write the failing run-path test**

Create `agent-runner/tests/context_run_tests.rs`:

```rust
use std::sync::{Arc, Mutex};
use agent_runner::models::{RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::server::{build_test_app_with_settings, Runner};
use async_trait::async_trait;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[derive(Default)]
struct CapturingRunner {
    prompt: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl Runner for CapturingRunner {
    async fn run(
        &self,
        request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, agent_runner::models::ErrorResponse> {
        *self.prompt.lock().unwrap() = Some(request.prompt.clone());
        Ok(RunResponse {
            status: "ok".into(),
            stdout: "ok".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 0,
        })
    }
}
```

Add the assertion:

```rust
assert!(captured.contains("Readable scopes:"));
assert!(captured.contains("qq:user:1"));
assert!(captured.contains("qq:private:1"));
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because no context pack is prepended to the prompt

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/context/context_pack.rs`:

```rust
use super::scope::ReadableScopes;

pub fn render_context_pack(scopes: &[ReadableScopes]) -> String {
    let mut output = String::from("Readable scopes:\n");
    for scope in scopes {
        output.push_str(&format!("- {:?}: {}\n", scope.kind, scope.id));
    }
    output.push('\n');
    output
}
```

Update `agent-runner/src/server.rs` before queueing the run:

```rust
let scopes = ScopeResolver::new().readable_scopes(
    &request.user_id,
    &request.conversation_id,
    &request.platform_account_id,
);
request.prompt = format!("{}{}", render_context_pack(&scopes), request.prompt);
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml
cargo test --test http_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/context/context_pack.rs agent-runner/src/server.rs agent-runner/src/exec.rs agent-runner/tests/context_run_tests.rs agent-runner/tests/http_api_tests.rs
git commit -m "feat: inject control plane context into runs"
```

### Task 5: Add structured memory-write intent handling

**Files:**
- Create: `agent-runner/src/context/memory_intent.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/context/object_store.rs`
- Modify: `agent-runner/tests/context_store_tests.rs`

- [ ] **Step 1: Write the failing parser test**

Add to `agent-runner/tests/context_store_tests.rs`:

```rust
#[test]
fn parse_memory_write_intent_from_stdout_block() {
    let stdout = r#"normal text
```dogbot-memory
{"scope":"user-private","summary":"prefers rust","raw_evidence":"I prefer Rust"}
```
"#;

    let intent = agent_runner::context::memory_intent::parse_memory_intent(stdout).unwrap();
    assert_eq!(intent.scope, "user-private");
    assert_eq!(intent.summary, "prefers rust");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test context_store_tests --manifest-path agent-runner/Cargo.toml parse_memory_write_intent_from_stdout_block
```

Expected: FAIL because the parser does not exist yet

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/context/memory_intent.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MemoryIntent {
    pub scope: String,
    pub summary: String,
    pub raw_evidence: String,
}

pub fn parse_memory_intent(stdout: &str) -> Option<MemoryIntent> {
    let start = stdout.find("```dogbot-memory\n")?;
    let body = &stdout[start + "```dogbot-memory\n".len()..];
    let end = body.find("\n```")?;
    serde_json::from_str(&body[..end]).ok()
}
```

Add a store method:

```rust
pub fn insert_memory_candidate(&self, actor_id: &str, conversation_id: &str, candidate_json: &str) -> rusqlite::Result<()> {
    self.conn.execute(
        "INSERT INTO memory_candidates (id, actor_id, conversation_id, candidate_json, created_at_epoch_secs)
         VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), actor_id, conversation_id, candidate_json],
    )?;
    Ok(())
}
```

Hook the parser into `agent-runner/src/exec.rs` after stdout collection and before returning the response.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --test context_store_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/context/memory_intent.rs agent-runner/src/context/object_store.rs agent-runner/src/exec.rs agent-runner/tests/context_store_tests.rs
git commit -m "feat: capture memory write intents as candidates"
```

### Task 6: Phase A verification

**Files:**
- Modify: `agent-runner/src/context/*`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `qq_adapter/*`
- Modify: `wechatpadpro_adapter/*`

- [ ] **Step 1: Run focused Rust test suite**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml
cargo test --test context_scope_tests --manifest-path agent-runner/Cargo.toml
cargo test --test context_store_tests --manifest-path agent-runner/Cargo.toml
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml
cargo test --test http_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 2: Run adapter regression tests**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests -q
```

Expected: PASS

- [ ] **Step 3: Review final diff**

Run:

```bash
git diff --stat HEAD~6..HEAD
```

Expected:
- changes are limited to control-plane config, context modules, adapters, and tests

