# DogBot Control Plane Phase C Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add history capture, limited QQ backfill, per-conversation retention, and conversation-scoped history retrieval without turning raw history into shared memory.

**Architecture:** Reuse the Phase B inbound message pipeline so enabled conversations are mirrored into SQLite. Keep retrieval local and conversation-scoped with reply anchoring, recent-window selection, FTS lookup, and attachment stubs. Use QQ backfill only as a bounded sync aid and keep WeChat on realtime mirror only.

**Tech Stack:** Rust, SQLite, `rusqlite`, Python adapters, NapCat history API, existing WeChatPadPro webhook flow, `cargo test`, `pytest`

---

## File Structure

- Create: `agent-runner/src/history/mod.rs`
- Create: `agent-runner/src/history/store.rs`
- Create: `agent-runner/src/history/retrieval.rs`
- Create: `agent-runner/src/history/cleanup.rs`
- Create: `agent-runner/tests/history_ingest_tests.rs`
- Create: `agent-runner/tests/history_retrieval_tests.rs`
- Create: `agent-runner/tests/history_cleanup_tests.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/context/context_pack.rs`
- Modify: `agent-runner/src/rendering.rs`
- Modify: `qq_adapter/napcat_client.py`
- Create: `qq_adapter/history_sync.py`
- Modify: `qq_adapter/app.py`
- Modify: `qq_adapter/tests/test_app.py`
- Modify: `wechatpadpro_adapter/processor.py`
- Modify: `wechatpadpro_adapter/tests/test_app.py`

### Task 1: Add history store schema and ingest-state tests

**Files:**
- Create: `agent-runner/src/history/mod.rs`
- Create: `agent-runner/src/history/store.rs`
- Create: `agent-runner/tests/history_ingest_tests.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/config.rs`

- [ ] **Step 1: Write the failing schema tests**

Create `agent-runner/tests/history_ingest_tests.rs`:

```rust
use agent_runner::history::store::HistoryStore;

#[test]
fn history_store_creates_message_and_ingest_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_attachment".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
    assert!(tables.contains(&"conversation_ingest_state".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test history_ingest_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the `history` module does not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/history/mod.rs`:

```rust
pub mod cleanup;
pub mod retrieval;
pub mod store;
```

Create `agent-runner/src/history/store.rs`:

```rust
use rusqlite::Connection;
use std::path::Path;

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS message_store (
                message_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                normalized_text TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS message_attachment (
                attachment_id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                attachment_type TEXT NOT NULL,
                asset_id TEXT
            );
            CREATE TABLE IF NOT EXISTS asset_store (
                asset_id TEXT PRIMARY KEY,
                storage_path TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                availability_status TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversation_ingest_state (
                conversation_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                retention_days INTEGER NOT NULL
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

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test history_ingest_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/history/mod.rs agent-runner/src/history/store.rs agent-runner/tests/history_ingest_tests.rs agent-runner/src/lib.rs agent-runner/src/config.rs
git commit -m "feat: add history store schema"
```

### Task 2: Mirror enabled conversations into history storage

**Files:**
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/history/store.rs`
- Modify: `agent-runner/tests/inbound_api_tests.rs`
- Modify: `wechatpadpro_adapter/processor.py`
- Modify: `wechatpadpro_adapter/tests/test_app.py`

- [ ] **Step 1: Write the failing mirror test**

Add to `agent-runner/tests/inbound_api_tests.rs`:

```rust
#[tokio::test]
async fn inbound_api_persists_enabled_conversation_messages() {
    let settings = agent_runner::config::Settings {
        bind_addr: "127.0.0.1:8787".into(),
        default_timeout_secs: 120,
        max_timeout_secs: 300,
        container_name: "claude-runner".into(),
        image_name: "dogbot/claude-runner:local".into(),
        workspace_dir: "/tmp/agent-runner-tests/workdir".into(),
        state_dir: "/tmp/agent-runner-tests/state".into(),
        anthropic_base_url: "http://host.docker.internal:9000".into(),
        api_proxy_auth_token: "local-proxy-token".into(),
        napcat_api_base_url: "http://127.0.0.1:3001".into(),
        napcat_access_token: None,
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        global_rate_limit_per_minute: 10,
        user_rate_limit_per_minute: 3,
        conversation_rate_limit_per_minute: 5,
        session_db_path: "/tmp/agent-runner-tests/state/runner.db".into(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
        content_root: "./content".into(),
        control_plane_db_path: "/tmp/agent-runner-tests/state/control.db".into(),
        admin_actor_ids: vec![],
    };
    let history_store = agent_runner::history::store::HistoryStore::open("/tmp/agent-runner-tests/state/history.db").unwrap();
    history_store.upsert_ingest_state("qq:group:100", true, 180).unwrap();

    let app = agent_runner::server::build_test_app_with_settings(
        std::sync::Arc::new(AcceptingRunner),
        settings,
    );
    let payload = serde_json::to_vec(&agent_runner::inbound_models::InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m-enabled-1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent summarize".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    })
    .unwrap();

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(history_store.message_count("qq:group:100").unwrap(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml inbound_api_persists_enabled_conversation_messages
```

Expected: FAIL because inbound messages are not mirrored yet

- [ ] **Step 3: Write the minimal implementation**

Add to `agent-runner/src/history/store.rs`:

```rust
pub fn upsert_ingest_state(&self, conversation_id: &str, enabled: bool, retention_days: i64) -> rusqlite::Result<()> {
    self.conn.execute(
        "INSERT INTO conversation_ingest_state (conversation_id, enabled, retention_days)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(conversation_id) DO UPDATE SET enabled = excluded.enabled, retention_days = excluded.retention_days",
        rusqlite::params![conversation_id, enabled as i64, retention_days],
    )?;
    Ok(())
}

pub fn insert_message(&self, message_id: &str, conversation_id: &str, actor_id: &str, normalized_text: &str, created_at_epoch_secs: i64) -> rusqlite::Result<()> {
    self.conn.execute(
        "INSERT OR IGNORE INTO message_store (message_id, conversation_id, actor_id, normalized_text, created_at_epoch_secs)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![message_id, conversation_id, actor_id, normalized_text, created_at_epoch_secs],
    )?;
    Ok(())
}
```

Call these methods from `/v1/inbound-messages` after trigger resolution for enabled conversations.

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml inbound_api_persists_enabled_conversation_messages
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/server.rs agent-runner/src/history/store.rs agent-runner/tests/inbound_api_tests.rs wechatpadpro_adapter/processor.py wechatpadpro_adapter/tests/test_app.py
git commit -m "feat: mirror enabled inbound messages into history storage"
```

### Task 3: Add limited QQ backfill support

**Files:**
- Modify: `qq_adapter/napcat_client.py`
- Create: `qq_adapter/history_sync.py`
- Modify: `qq_adapter/app.py`
- Modify: `qq_adapter/tests/test_app.py`

- [ ] **Step 1: Write the failing QQ backfill test**

Add to `qq_adapter/tests/test_app.py`:

```python
def test_group_enablement_triggers_limited_history_backfill(monkeypatch):
    calls = {"history": 0, "forwarded": 0}

    async def fake_history(self, group_id: str, count: int = 50):
        calls["history"] += 1
        return [{"message_id": 1, "raw_message": "old text", "user_id": 7, "group_id": int(group_id)}]

    async def fake_inbound(self, payload):
        calls["forwarded"] += 1
        return {"status": "accepted"}
```

Assert:

```python
assert calls == {"history": 1, "forwarded": 2}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests/test_app.py -q
```

Expected: FAIL because NapCat history fetching and sync do not exist

- [ ] **Step 3: Write the minimal implementation**

Add to `qq_adapter/napcat_client.py`:

```python
async def get_group_msg_history(self, group_id: str, count: int = 50) -> list[dict[str, object]]:
    async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
        response = await client.post(
            "/get_group_msg_history",
            headers=self._headers(),
            json={"group_id": int(group_id), "count": count},
        )
    response.raise_for_status()
    body = response.json()
    return list(body.get("data") or [])
```

Create `qq_adapter/history_sync.py`:

```python
async def sync_group_history(napcat, runner, group_id: str, default_cwd: str, timeout_secs: int, platform_account_id: str) -> None:
    for event in await napcat.get_group_msg_history(group_id, count=50):
        payload = build_inbound_payload(
            event,
            default_cwd=default_cwd,
            timeout_secs=timeout_secs,
            platform_account_id=platform_account_id,
        )
        await runner.send_inbound_message(payload)
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests/test_app.py -q
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add qq_adapter/napcat_client.py qq_adapter/history_sync.py qq_adapter/app.py qq_adapter/tests/test_app.py
git commit -m "feat: add limited QQ history backfill"
```

### Task 4: Build retrieval evidence packs and context injection

**Files:**
- Create: `agent-runner/src/history/retrieval.rs`
- Create: `agent-runner/tests/history_retrieval_tests.rs`
- Modify: `agent-runner/src/context/context_pack.rs`
- Modify: `agent-runner/src/server.rs`

- [ ] **Step 1: Write the failing retrieval test**

Create `agent-runner/tests/history_retrieval_tests.rs`:

```rust
use agent_runner::history::retrieval::build_history_evidence_pack;

#[test]
fn retrieval_pack_includes_anchor_recent_window_and_attachment_stub() {
    let pack = build_history_evidence_pack(
        "qq:group:100",
        "当前问题 /agent 总结",
        &[
            ("m1", "机器人上一条消息", false),
            ("m2", "用户补充消息", true),
        ],
    );

    assert!(pack.contains("Anchor message"));
    assert!(pack.contains("Recent context"));
    assert!(pack.contains("attachment present"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test history_retrieval_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the retrieval builder does not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/history/retrieval.rs`:

```rust
pub fn build_history_evidence_pack(
    conversation_id: &str,
    query: &str,
    rows: &[(&str, &str, bool)],
) -> String {
    let mut output = format!("History evidence for {conversation_id}\nQuery: {query}\n");
    output.push_str("Anchor message\n");
    output.push_str("Recent context\n");
    for (_message_id, text, has_attachment) in rows {
        output.push_str(&format!("- {text}\n"));
        if *has_attachment {
            output.push_str("  - attachment present\n");
        }
    }
    output
}
```

Update `agent-runner/src/context/context_pack.rs` to append the evidence pack after the readable scopes block.

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test history_retrieval_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/history/retrieval.rs agent-runner/src/context/context_pack.rs agent-runner/tests/history_retrieval_tests.rs agent-runner/src/server.rs
git commit -m "feat: add history evidence pack retrieval"
```

### Task 5: Add TTL cleanup and attachment safety checks

**Files:**
- Create: `agent-runner/src/history/cleanup.rs`
- Create: `agent-runner/tests/history_cleanup_tests.rs`
- Modify: `agent-runner/src/history/store.rs`
- Modify: `agent-runner/src/rendering.rs`

- [ ] **Step 1: Write the failing cleanup tests**

Create `agent-runner/tests/history_cleanup_tests.rs`:

```rust
use agent_runner::history::{cleanup::purge_expired_history, store::HistoryStore};

#[test]
fn cleanup_removes_expired_messages_but_keeps_live_assets() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();

    store.insert_expired_message_for_test("m1", "qq:group:100").unwrap();
    store.insert_live_asset_for_test("asset-1", "/tmp/a.png").unwrap();

    purge_expired_history(&store).unwrap();

    assert_eq!(store.message_count("qq:group:100").unwrap(), 0);
    assert_eq!(store.asset_count().unwrap(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test history_cleanup_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because cleanup does not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/history/cleanup.rs`:

```rust
use super::store::HistoryStore;

pub fn purge_expired_history(store: &HistoryStore) -> rusqlite::Result<()> {
    store.delete_expired_messages()?;
    store.delete_orphaned_assets()?;
    Ok(())
}
```

Add guard code in `agent-runner/src/rendering.rs` before sending `stored_asset` actions:

```rust
if !authorized_asset_ids.contains(&action.source_value) {
    return Err("asset_not_authorized".into());
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test history_cleanup_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/history/cleanup.rs agent-runner/src/history/store.rs agent-runner/src/rendering.rs agent-runner/tests/history_cleanup_tests.rs
git commit -m "feat: add history retention cleanup and asset guards"
```

### Task 6: Phase C verification

**Files:**
- Modify: `agent-runner/src/history/*`
- Modify: `qq_adapter/*`
- Modify: `wechatpadpro_adapter/*`

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
cargo test --test history_ingest_tests --manifest-path agent-runner/Cargo.toml
cargo test --test history_retrieval_tests --manifest-path agent-runner/Cargo.toml
cargo test --test history_cleanup_tests --manifest-path agent-runner/Cargo.toml
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml
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
- diff is limited to history mirror, retrieval, cleanup, QQ backfill, and related tests
