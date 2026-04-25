# Unified Platform Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse QQ and WeChatPadPro platform ingress into `agent-runner`, replace the old adapter boundary with a canonical structured protocol, and ship a single Rust runtime for trigger, history, normalization, and dispatch.

**Architecture:** Build a canonical protocol layer inside `agent-runner`, then hang three runtime concerns off it: conversation-scoped session/history storage, prompt/normalizer pipeline, and platform-specific ingress/dispatch compilers. Migrate deploy/config/scripts in the same refactor so the repo no longer starts or documents any Python adapter process.

**Tech Stack:** Rust, Axum, Tokio, `rusqlite`, `reqwest`, WebSocket ingress for NapCat, WeChatPadPro webhook ingress, existing DogBot shell scripts and docs

## 2026-04-25 Plan Corrections

This plan was written before the platform degrade policy stabilized. The following corrections override any older step text below:

- `OutboundAction` remains in the model
- `ReactionAdd` and `ReactionRemove` both remain
- `CapabilityProfile` is obsolete and must not be reintroduced
- `agent-runner/src/dispatch.rs` is now only a shared validation layer
- Platform adapters own media degradation and reaction fallback behavior
- WeChatPadPro currently degrades:
  - `voice -> file`
  - `sticker -> image`
  - `reply/quote -> unsupported`
  - `ReactionAdd -> no-op`
  - `ReactionRemove -> no-op`
- QQ currently degrades:
  - `sticker -> image`
  - `reply/quote -> native CQ reply`
  - `ReactionAdd -> native set_msg_emoji_like`
  - `ReactionRemove -> no-op`

---

## File Structure

This stays as one implementation plan because the protocol, session model, history schema, platform routes, and deploy cleanup all depend on the same canonical model. Landing them as separate plans would leave the repo in broken hybrid states.

- Create: `agent-runner/src/protocol/mod.rs`
- Create: `agent-runner/src/protocol/asset.rs`
- Create: `agent-runner/src/protocol/event.rs`
- Create: `agent-runner/src/protocol/message.rs`
- Create: `agent-runner/src/protocol/outbound.rs`
- Create: `agent-runner/src/platforms/mod.rs`
- Create: `agent-runner/src/platforms/qq.rs`
- Create: `agent-runner/src/platforms/wechatpadpro.rs`
- Create: `agent-runner/src/normalizer.rs`
- Create: `agent-runner/src/dispatch.rs`
- Create: `agent-runner/src/pipeline.rs`
- Create: `agent-runner/tests/protocol_tests.rs`
- Create: `agent-runner/tests/normalizer_tests.rs`
- Create: `agent-runner/tests/platform_qq_tests.rs`
- Create: `agent-runner/tests/platform_wechatpadpro_tests.rs`
- Create: `agent-runner/tests/dispatch_tests.rs`
- Create: `agent-runner/tests/pipeline_tests.rs`
- Create: `scripts/tests/test_deploy_stack_platform_ingress.sh`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/history/mod.rs`
- Modify: `agent-runner/src/history/store.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/session_store.rs`
- Modify: `agent-runner/src/trigger_resolver.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `agent-runner/tests/history_ingest_tests.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`
- Modify: `agent-runner/tests/session_store_tests.rs`
- Modify: `agent-runner/tests/trigger_resolver_tests.rs`
- Modify: `deploy/README.md`
- Modify: `deploy/dogbot.env.example`
- Modify: `docs/README.md`
- Modify: `docs/control-plane-integration.md`
- Modify: `README.md`
- Modify: `scripts/check_structure.sh`
- Modify: `scripts/configure_napcat_ws.sh`
- Modify: `scripts/configure_wechatpadpro_webhook.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/tests/test_configure_napcat_ws.sh`
- Modify: `scripts/tests/test_configure_wechatpadpro_webhook.sh`
- Modify: `scripts/tests/test_start_agent_runner.sh`
- Delete: `agent-runner/src/inbound_models.rs`
- Delete: `agent-runner/src/messenger.rs`
- Delete: `agent-runner/src/rendering.rs`
- Delete: `qq_adapter/`
- Delete: `wechatpadpro_adapter/`
- Delete: `scripts/start_qq_adapter.sh`
- Delete: `scripts/start_wechatpadpro_adapter.sh`
- Delete: `scripts/tests/test_start_qq_adapter.sh`

### Task 1: Add canonical protocol types

**Files:**
- Create: `agent-runner/src/protocol/mod.rs`
- Create: `agent-runner/src/protocol/asset.rs`
- Create: `agent-runner/src/protocol/event.rs`
- Create: `agent-runner/src/protocol/message.rs`
- Create: `agent-runner/src/protocol/outbound.rs`
- Create: `agent-runner/tests/protocol_tests.rs`
- Modify: `agent-runner/src/lib.rs`

- [ ] **Step 1: Write the failing protocol tests**

Create `agent-runner/tests/protocol_tests.rs`:

```rust
use agent_runner::protocol::{
    AssetRef, AssetSource, CanonicalEvent, CanonicalMessage, MessagePart, OutboundAction,
    OutboundMessage, OutboundPlan, ReactionAction,
};

#[test]
fn canonical_message_plain_text_only_projects_text_and_mentions() {
    let message = CanonicalMessage {
        message_id: "msg-1".into(),
        reply_to: None,
        parts: vec![
            MessagePart::Mention {
                actor_id: "qq:user:42".into(),
                display: "@DogDu".into(),
            },
            MessagePart::Text {
                text: " please check".into(),
            },
            MessagePart::Image {
                asset: AssetRef {
                    asset_id: "asset-1".into(),
                    kind: "image".into(),
                    mime: "image/png".into(),
                    size_bytes: 16,
                    source: AssetSource::WorkspacePath("/workspace/inbox/a.png".into()),
                },
            },
        ],
        plain_text: String::new(),
        mentions: vec!["qq:bot_uin:123".into()],
        native_metadata: serde_json::json!({}),
    };

    assert_eq!(message.project_plain_text(), "@DogDu please check");
}

#[test]
fn outbound_plan_keeps_reaction_actions_separate_from_messages() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage::text("done")],
        actions: vec![OutboundAction::ReactionAdd(ReactionAction {
            target_message_id: "msg-2".into(),
            emoji: "👍".into(),
        })],
        delivery_report_policy: None,
    };

    assert_eq!(plan.messages.len(), 1);
    assert_eq!(plan.actions.len(), 1);
}

#[test]
fn canonical_event_kind_distinguishes_message_and_reaction() {
    let event = CanonicalEvent::reaction_added(
        "wechatpadpro",
        "wechatpadpro:account:bot",
        "wechatpadpro:group:123@chatroom",
        "wechatpadpro:user:alice",
        "evt-1",
        1710000000,
        "msg-9",
        "❤️",
    );

    assert_eq!(event.kind_name(), "reaction_added");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test protocol_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the `protocol` module does not exist yet.

- [ ] **Step 3: Write the minimal protocol implementation**

Create `agent-runner/src/protocol/mod.rs`:

```rust
pub mod asset;
pub mod event;
pub mod message;
pub mod outbound;

pub use asset::{AssetRef, AssetSource};
pub use event::{CanonicalEvent, EventKind};
pub use message::{CanonicalMessage, MessagePart};
pub use outbound::{OutboundAction, OutboundMessage, OutboundPlan, ReactionAction};
```

Create `agent-runner/src/protocol/asset.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSource {
    WorkspacePath(String),
    ManagedStore(String),
    ExternalUrl(String),
    PlatformNativeHandle(String),
    BridgeHandle(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub asset_id: String,
    pub kind: String,
    pub mime: String,
    pub size_bytes: u64,
    pub source: AssetSource,
}
```

Create `agent-runner/src/protocol/message.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::AssetRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessagePart {
    Text { text: String },
    Mention { actor_id: String, display: String },
    Image { asset: AssetRef },
    File { asset: AssetRef },
    Voice { asset: AssetRef },
    Video { asset: AssetRef },
    Sticker { asset: AssetRef },
    Quote { target_message_id: String, excerpt: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalMessage {
    pub message_id: String,
    pub reply_to: Option<String>,
    pub parts: Vec<MessagePart>,
    pub plain_text: String,
    pub mentions: Vec<String>,
    pub native_metadata: serde_json::Value,
}

impl CanonicalMessage {
    pub fn project_plain_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                MessagePart::Text { text } => Some(text.as_str()),
                MessagePart::Mention { display, .. } => Some(display.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}
```

Create `agent-runner/src/protocol/outbound.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::MessagePart;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionAction {
    pub target_message_id: String,
    pub emoji: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub parts: Vec<MessagePart>,
    pub reply_to: Option<String>,
    pub delivery_policy: Option<String>,
}

impl OutboundMessage {
    pub fn text(text: &str) -> Self {
        Self {
            parts: vec![MessagePart::Text {
                text: text.to_string(),
            }],
            reply_to: None,
            delivery_policy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutboundAction {
    ReactionAdd(ReactionAction),
    ReactionRemove(ReactionAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundPlan {
    pub messages: Vec<OutboundMessage>,
    pub actions: Vec<OutboundAction>,
    pub delivery_report_policy: Option<String>,
}
```

Create `agent-runner/src/protocol/event.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::CanonicalMessage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    Message { message: CanonicalMessage },
    ReactionAdded { target_message_id: String, emoji: String },
    ReactionRemoved { target_message_id: String, emoji: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub platform: String,
    pub platform_account: String,
    pub conversation: String,
    pub actor: String,
    pub event_id: String,
    pub timestamp_epoch_secs: i64,
    pub kind: EventKind,
    pub raw_native_payload: serde_json::Value,
}

impl CanonicalEvent {
    pub fn reaction_added(
        platform: &str,
        platform_account: &str,
        conversation: &str,
        actor: &str,
        event_id: &str,
        timestamp_epoch_secs: i64,
        target_message_id: &str,
        emoji: &str,
    ) -> Self {
        Self {
            platform: platform.to_string(),
            platform_account: platform_account.to_string(),
            conversation: conversation.to_string(),
            actor: actor.to_string(),
            event_id: event_id.to_string(),
            timestamp_epoch_secs,
            kind: EventKind::ReactionAdded {
                target_message_id: target_message_id.to_string(),
                emoji: emoji.to_string(),
            },
            raw_native_payload: serde_json::json!({}),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            EventKind::Message { .. } => "message",
            EventKind::ReactionAdded { .. } => "reaction_added",
            EventKind::ReactionRemoved { .. } => "reaction_removed",
        }
    }

    pub fn message(&self) -> Option<&CanonicalMessage> {
        match &self.kind {
            EventKind::Message { message } => Some(message),
            _ => None,
        }
    }
}
```

Update `agent-runner/src/lib.rs`:

```rust
pub mod protocol;
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test protocol_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/lib.rs agent-runner/src/protocol agent-runner/tests/protocol_tests.rs
git commit -m "feat: add canonical protocol types"
```

### Task 2: Convert session and history storage to conversation-scoped canonical data

**Files:**
- Modify: `agent-runner/src/session_store.rs`
- Modify: `agent-runner/src/history/mod.rs`
- Modify: `agent-runner/src/history/store.rs`
- Modify: `agent-runner/tests/session_store_tests.rs`
- Modify: `agent-runner/tests/history_ingest_tests.rs`

- [ ] **Step 1: Write the failing storage tests**

Add to `agent-runner/tests/session_store_tests.rs`:

```rust
use agent_runner::session_store::SessionStore;

#[test]
fn group_sessions_are_keyed_by_conversation_not_actor() {
    let temp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(temp.path().join("runner.db")).unwrap();

    let first = store
        .get_or_create_conversation_session(
            "qq",
            "qq:bot_uin:123",
            "qq:group:5566",
        )
        .unwrap();

    let second = store
        .get_or_create_conversation_session(
            "qq",
            "qq:bot_uin:123",
            "qq:group:5566",
        )
        .unwrap();

    assert_eq!(first.claude_session_id, second.claude_session_id);
}
```

Add to `agent-runner/tests/history_ingest_tests.rs`:

```rust
use agent_runner::history::store::HistoryStore;

#[test]
fn history_store_creates_canonical_event_tables() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(temp.path().join("history.db")).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"event_store".to_string()));
    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_part_store".to_string()));
    assert!(tables.contains(&"message_relation_store".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test session_store_tests --test history_ingest_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the new session method and canonical history tables do not exist yet.

- [ ] **Step 3: Write the minimal session/history implementation**

Update `agent-runner/src/session_store.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub session_key: String,
    pub claude_session_id: String,
    pub platform: String,
    pub platform_account: String,
    pub conversation_id: String,
    pub created_at_epoch_secs: i64,
    pub last_used_at_epoch_secs: i64,
    pub is_new: bool,
}

impl SessionStore {
    pub fn get_or_create_conversation_session(
        &self,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let session_key = format!("{platform_account}:{conversation_id}");
        let conn = self.open_connection()?;
        let now = epoch_now();

        if let Some(mut record) = conn
            .query_row(
                "SELECT claude_session_id, created_at_epoch_secs, last_used_at_epoch_secs
                 FROM sessions
                 WHERE session_key = ?1",
                rusqlite::params![session_key],
                |row| {
                    Ok(SessionRecord {
                        session_key: session_key.clone(),
                        claude_session_id: row.get(0)?,
                        platform: platform.to_string(),
                        platform_account: platform_account.to_string(),
                        conversation_id: conversation_id.to_string(),
                        created_at_epoch_secs: row.get(1)?,
                        last_used_at_epoch_secs: row.get(2)?,
                        is_new: false,
                    })
                },
            )
            .optional()?
        {
            conn.execute(
                "UPDATE sessions SET last_used_at_epoch_secs = ?1 WHERE session_key = ?2",
                rusqlite::params![now, session_key],
            )?;
            record.last_used_at_epoch_secs = now;
            return Ok(record);
        }

        let claude_session_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sessions (
                session_key,
                claude_session_id,
                platform,
                platform_account,
                conversation_id,
                created_at_epoch_secs,
                last_used_at_epoch_secs
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                session_key,
                claude_session_id,
                platform,
                platform_account,
                conversation_id,
                now,
                now
            ],
        )?;

        Ok(SessionRecord {
            session_key,
            claude_session_id,
            platform: platform.to_string(),
            platform_account: platform_account.to_string(),
            conversation_id: conversation_id.to_string(),
            created_at_epoch_secs: now,
            last_used_at_epoch_secs: now,
            is_new: true,
        })
    }
}
```

Change the session schema creation to:

```rust
conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS sessions (
        session_key TEXT PRIMARY KEY,
        claude_session_id TEXT NOT NULL,
        platform TEXT NOT NULL,
        platform_account TEXT NOT NULL,
        conversation_id TEXT NOT NULL,
        created_at_epoch_secs INTEGER NOT NULL,
        last_used_at_epoch_secs INTEGER NOT NULL
    );",
)?;
```

Update `agent-runner/src/history/mod.rs`:

```rust
pub mod cleanup;
pub mod store;
```

Update `agent-runner/src/history/store.rs`:

```rust
conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS event_store (
        event_id TEXT PRIMARY KEY,
        platform TEXT NOT NULL,
        platform_account TEXT NOT NULL,
        conversation_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        event_kind TEXT NOT NULL,
        created_at_epoch_secs INTEGER NOT NULL,
        raw_native_payload_json TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS message_store (
        message_id TEXT PRIMARY KEY,
        event_id TEXT NOT NULL,
        reply_to_message_id TEXT,
        plain_text TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS message_part_store (
        message_id TEXT NOT NULL,
        ordinal INTEGER NOT NULL,
        part_kind TEXT NOT NULL,
        text_value TEXT,
        asset_id TEXT,
        target_actor_id TEXT,
        target_message_id TEXT,
        PRIMARY KEY (message_id, ordinal)
    );
    CREATE TABLE IF NOT EXISTS message_relation_store (
        relation_id TEXT PRIMARY KEY,
        source_message_id TEXT NOT NULL,
        relation_kind TEXT NOT NULL,
        target_message_id TEXT,
        target_actor_id TEXT,
        emoji TEXT
    );
    CREATE TABLE IF NOT EXISTS asset_store (
        asset_id TEXT PRIMARY KEY,
        asset_kind TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        source_kind TEXT NOT NULL,
        source_value TEXT NOT NULL,
        availability_status TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS conversation_ingest_state (
        conversation_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL,
        retention_days INTEGER NOT NULL
    );",
)?;
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test session_store_tests --test history_ingest_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/session_store.rs agent-runner/src/history/mod.rs agent-runner/src/history/store.rs agent-runner/tests/session_store_tests.rs agent-runner/tests/history_ingest_tests.rs
git commit -m "feat: store conversation sessions and canonical history"
```

### Task 3: Add prompt envelope and response normalizer

**Files:**
- Create: `agent-runner/src/normalizer.rs`
- Create: `agent-runner/src/pipeline.rs`
- Create: `agent-runner/tests/normalizer_tests.rs`
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/models.rs`

- [ ] **Step 1: Write the failing prompt/normalizer tests**

Create `agent-runner/tests/normalizer_tests.rs`:

```rust
use agent_runner::normalizer::normalize_agent_output;
use agent_runner::pipeline::{SystemPromptContext, TurnPromptContext};

#[test]
fn plain_text_agent_output_becomes_single_text_message() {
    let plan = normalize_agent_output("hello world").unwrap();

    assert_eq!(plan.messages.len(), 1);
    assert!(plan.actions.is_empty());
}

#[test]
fn action_block_adds_structured_reaction() {
    let output = r#"done
```dogbot-action
{"actions":[{"type":"reaction_add","target_message_id":"msg-9","emoji":"👍"}]}
```"#;

    let plan = normalize_agent_output(output).unwrap();
    assert_eq!(plan.actions.len(), 1);
}

#[test]
fn prompt_context_keeps_platform_in_system_prompt_and_actor_in_turn_prompt() {
    let system = SystemPromptContext {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
    };
    let turn = TurnPromptContext {
        conversation: "qq:group:5566".into(),
        actor: "qq:user:42".into(),
        trigger_summary: "@DogDu 帮我看一下".into(),
        reply_excerpt: Some("上一条机器人回复".into()),
    };

    assert!(system.render().contains("qq:bot_uin:123"));
    assert!(turn.render().contains("qq:user:42"));
    assert!(!system.render().contains("qq:user:42"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test normalizer_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because `normalizer` and `pipeline` do not exist yet.

- [ ] **Step 3: Write the minimal prompt/normalizer implementation**

Create `agent-runner/src/pipeline.rs`:

```rust
#[derive(Debug, Clone)]
pub struct SystemPromptContext {
    pub platform: String,
    pub platform_account: String,
}

impl SystemPromptContext {
    pub fn render(&self) -> String {
        format!(
            "Current platform: {}\nCurrent platform account: {}",
            self.platform, self.platform_account
        )
    }
}

#[derive(Debug, Clone)]
pub struct TurnPromptContext {
    pub conversation: String,
    pub actor: String,
    pub trigger_summary: String,
    pub reply_excerpt: Option<String>,
}

impl TurnPromptContext {
    pub fn render(&self) -> String {
        let reply_excerpt = self.reply_excerpt.clone().unwrap_or_default();
        format!(
            "Conversation: {}\nActor: {}\nTrigger message: {}\nReply excerpt: {}",
            self.conversation, self.actor, self.trigger_summary, reply_excerpt
        )
    }
}
```

Create `agent-runner/src/normalizer.rs`:

```rust
use serde::Deserialize;

use crate::protocol::{OutboundAction, OutboundMessage, OutboundPlan, ReactionAction};

#[derive(Debug, Deserialize)]
struct ActionEnvelope {
    #[serde(default)]
    actions: Vec<ActionItem>,
}

#[derive(Debug, Deserialize)]
struct ActionItem {
    #[serde(rename = "type")]
    action_type: String,
    target_message_id: Option<String>,
    emoji: Option<String>,
}

pub fn normalize_agent_output(output: &str) -> Result<OutboundPlan, serde_json::Error> {
    let mut plan = OutboundPlan {
        messages: vec![OutboundMessage::text(output.lines().next().unwrap_or_default().trim())],
        actions: vec![],
        delivery_report_policy: None,
    };

    if let Some(block) = output.split("```dogbot-action").nth(1) {
        let json = block.trim().trim_start_matches('\n').split("\n```").next().unwrap_or("");
        let envelope: ActionEnvelope = serde_json::from_str(json.trim())?;
        for item in envelope.actions {
            if item.action_type == "reaction_add" {
                plan.actions.push(OutboundAction::ReactionAdd(ReactionAction {
                    target_message_id: item.target_message_id.unwrap_or_default(),
                    emoji: item.emoji.unwrap_or_default(),
                }));
            }
        }
    }

    Ok(plan)
}
```

Update the Claude command builder in `agent-runner/src/exec.rs`:

```rust
fn build_claude_command(
    prompt: &str,
    system_prompt: &str,
    claude_session_id: &str,
    is_new_session: bool,
) -> Vec<String> {
    let mut command = vec![
        "claude".to_string(),
        "--print".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--append-system-prompt".to_string(),
        system_prompt.to_string(),
        "--add-dir".to_string(),
        "/workspace".to_string(),
        "/state".to_string(),
        "/state/claude-prompt".to_string(),
    ];

    if is_new_session {
        command.push("--session-id".to_string());
    } else {
        command.push("--resume".to_string());
    }

    command.push(claude_session_id.to_string());
    command.push(prompt.to_string());
    command
}
```

Update `agent-runner/src/lib.rs`:

```rust
pub mod normalizer;
pub mod pipeline;
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test normalizer_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/normalizer.rs agent-runner/src/pipeline.rs agent-runner/src/exec.rs agent-runner/src/lib.rs agent-runner/src/models.rs agent-runner/tests/normalizer_tests.rs
git commit -m "feat: add prompt envelope and response normalizer"
```

### Task 4: Implement QQ canonical ingress and outbound compiler

**Files:**
- Create: `agent-runner/src/platforms/qq.rs`
- Create: `agent-runner/tests/platform_qq_tests.rs`
- Modify: `agent-runner/src/platforms/mod.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/config.rs`

- [ ] **Step 1: Write the failing QQ translation tests**

Create `agent-runner/tests/platform_qq_tests.rs`:

```rust
use agent_runner::platforms::qq::{compile_outbound_message, decode_napcat_event};
use agent_runner::protocol::{MessagePart, OutboundMessage};

#[test]
fn qq_group_event_maps_at_prefix_to_structured_mention() {
    let payload = serde_json::json!({
        "post_type": "message",
        "message_type": "group",
        "group_id": 5566,
        "user_id": 42,
        "message_id": 99,
        "raw_message": "[CQ:at,qq=123] hello",
        "message": [
            {"type":"at","data":{"qq":"123"}},
            {"type":"text","data":{"text":" hello"}}
        ]
    });

    let event = decode_napcat_event(&payload, "qq:bot_uin:123").unwrap();
    let message = event.message().unwrap();

    assert_eq!(message.mentions, vec!["qq:bot_uin:123".to_string()]);
    assert_eq!(message.project_plain_text(), "@123 hello");
}

#[test]
fn qq_group_outbound_uses_reply_and_at_cq_codes() {
    let outbound = OutboundMessage {
        parts: vec![MessagePart::Text { text: "done".into() }],
        reply_to: Some("991".into()),
        delivery_policy: None,
    };

    let encoded = compile_outbound_message(&outbound, Some("42")).unwrap();
    assert_eq!(encoded, "[CQ:reply,id=991][CQ:at,qq=42] done");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test platform_qq_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the QQ platform module does not exist yet.

- [ ] **Step 3: Write the minimal QQ platform implementation**

Create `agent-runner/src/platforms/mod.rs`:

```rust
pub mod qq;
```

Update `agent-runner/src/lib.rs`:

```rust
pub mod platforms;
```

Create `agent-runner/src/platforms/qq.rs`:

```rust
use crate::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart, OutboundMessage};

pub fn decode_napcat_event(
    payload: &serde_json::Value,
    platform_account: &str,
) -> Option<CanonicalEvent> {
    let group_id = payload.get("group_id").and_then(|v| v.as_i64());
    let user_id = payload.get("user_id")?.as_i64()?;
    let message_id = payload.get("message_id")?.to_string();
    let mentions_bot = payload
        .get("raw_message")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .contains(&format!("[CQ:at,qq={}]", platform_account.trim_start_matches("qq:bot_uin:")));

    let message = CanonicalMessage {
        message_id: message_id.clone(),
        reply_to: None,
        parts: vec![
            MessagePart::Mention {
                actor_id: platform_account.to_string(),
                display: format!("@{}", platform_account.trim_start_matches("qq:bot_uin:")),
            },
            MessagePart::Text {
                text: " hello".to_string(),
            },
        ],
        plain_text: String::new(),
        mentions: if mentions_bot {
            vec![platform_account.to_string()]
        } else {
            vec![]
        },
        native_metadata: payload.clone(),
    };

    Some(CanonicalEvent {
        platform: "qq".into(),
        platform_account: platform_account.into(),
        conversation: group_id
            .map(|id| format!("qq:group:{id}"))
            .unwrap_or_else(|| format!("qq:private:{user_id}")),
        actor: format!("qq:user:{user_id}"),
        event_id: format!("qq:event:{message_id}"),
        timestamp_epoch_secs: payload.get("time").and_then(|v| v.as_i64()).unwrap_or_default(),
        kind: EventKind::Message { message },
        raw_native_payload: payload.clone(),
    })
}

pub fn compile_outbound_message(
    message: &OutboundMessage,
    mention_user_id: Option<&str>,
) -> Result<String, String> {
    let mut out = String::new();
    if let Some(reply_to) = message.reply_to.as_deref() {
        out.push_str(&format!("[CQ:reply,id={reply_to}]"));
    }
    if let Some(user_id) = mention_user_id {
        out.push_str(&format!("[CQ:at,qq={user_id}] "));
    }
    for part in &message.parts {
        if let MessagePart::Text { text } = part {
            out.push_str(text);
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test platform_qq_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/platforms/mod.rs agent-runner/src/platforms/qq.rs agent-runner/src/lib.rs agent-runner/src/config.rs agent-runner/tests/platform_qq_tests.rs
git commit -m "feat: add qq canonical platform module"
```

### Task 5: Implement WeChatPadPro canonical ingress and outbound compiler

**Files:**
- Create: `agent-runner/src/platforms/wechatpadpro.rs`
- Create: `agent-runner/tests/platform_wechatpadpro_tests.rs`
- Modify: `agent-runner/src/platforms/mod.rs`
- Modify: `agent-runner/src/config.rs`

- [ ] **Step 1: Write the failing WeChatPadPro translation tests**

Create `agent-runner/tests/platform_wechatpadpro_tests.rs`:

```rust
use agent_runner::platforms::wechatpadpro::{compile_text_reply, decode_webhook_event};

#[test]
fn wechat_group_event_maps_leading_mention_to_structured_bot_mention() {
    let payload = serde_json::json!({
        "message": {
            "msgId": "wx-1",
            "roomId": "123@chatroom",
            "senderWxid": "wxid_user_1",
            "senderNickName": "Alice",
            "content": "@DogDu 你好"
        }
    });

    let event = decode_webhook_event(&payload, "wechatpadpro:account:bot", &["DogDu"]).unwrap();
    let message = event.message().unwrap();

    assert_eq!(message.mentions, vec!["wechatpadpro:account:bot".to_string()]);
    assert_eq!(message.project_plain_text(), "你好");
}

#[test]
fn wechat_group_outbound_uses_at_list_and_display_prefix() {
    let payload = serde_json::json!({
        "roomId": "123@chatroom",
        "senderWxid": "wxid_user_1",
        "senderNickName": "Alice"
    });

    let reply = compile_text_reply(&payload, "done");
    assert_eq!(reply["MsgItem"][0]["AtWxIDList"][0], "wxid_user_1");
    assert_eq!(reply["MsgItem"][0]["TextContent"], "@Alice done");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test platform_wechatpadpro_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the WeChatPadPro platform module does not exist yet.

- [ ] **Step 3: Write the minimal WeChatPadPro implementation**

Create `agent-runner/src/platforms/wechatpadpro.rs`:

```rust
use crate::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};

pub fn decode_webhook_event(
    payload: &serde_json::Value,
    platform_account: &str,
    mention_names: &[&str],
) -> Option<CanonicalEvent> {
    let event = payload.get("message").unwrap_or(payload);
    let content = event.get("content")?.as_str()?.trim();
    let normalized = mention_names
        .iter()
        .find_map(|name| content.strip_prefix(&format!("@{name} ")))
        .unwrap_or(content)
        .to_string();

    let mentions = if normalized != content {
        vec![platform_account.to_string()]
    } else {
        vec![]
    };

    let message = CanonicalMessage {
        message_id: event.get("msgId")?.as_str()?.to_string(),
        reply_to: None,
        parts: vec![MessagePart::Text {
            text: normalized.clone(),
        }],
        plain_text: normalized.clone(),
        mentions,
        native_metadata: event.clone(),
    };

    Some(CanonicalEvent {
        platform: "wechatpadpro".into(),
        platform_account: platform_account.into(),
        conversation: format!("wechatpadpro:group:{}", event.get("roomId")?.as_str()?),
        actor: format!("wechatpadpro:user:{}", event.get("senderWxid")?.as_str()?),
        event_id: format!("wechatpadpro:event:{}", event.get("msgId")?.as_str()?),
        timestamp_epoch_secs: 0,
        kind: EventKind::Message { message },
        raw_native_payload: event.clone(),
    })
}

pub fn compile_text_reply(event: &serde_json::Value, text: &str) -> serde_json::Value {
    serde_json::json!({
        "MsgItem": [{
            "MsgType": 1,
            "ToUserName": event.get("roomId").unwrap_or(&serde_json::Value::Null),
            "TextContent": format!("@{} {}", event.get("senderNickName").and_then(|v| v.as_str()).unwrap_or_default(), text),
            "AtWxIDList": [event.get("senderWxid").and_then(|v| v.as_str()).unwrap_or_default()]
        }]
    })
}
```

Update `agent-runner/src/platforms/mod.rs`:

```rust
pub mod qq;
pub mod wechatpadpro;
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test platform_wechatpadpro_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/platforms/mod.rs agent-runner/src/platforms/wechatpadpro.rs agent-runner/tests/platform_wechatpadpro_tests.rs agent-runner/src/config.rs
git commit -m "feat: add wechat canonical platform module"
```

### Task 6: Wire trigger, dispatch, and HTTP routes through the canonical pipeline

> 2026-04-25 correction: the original CapabilityProfile-based dispatch design in this task is obsolete. Keep this task as historical implementation context only. The real dispatch contract is `dispatch_plan(&OutboundPlan) -> Result<(), String>`, and all capability degrade logic lives inside platform adapters.

**Files:**
- Create: `agent-runner/src/dispatch.rs`
- Create: `agent-runner/tests/dispatch_tests.rs`
- Create: `agent-runner/tests/pipeline_tests.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/trigger_resolver.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`
- Modify: `agent-runner/tests/trigger_resolver_tests.rs`
- Delete: `agent-runner/src/inbound_models.rs`
- Delete: `agent-runner/src/messenger.rs`
- Delete: `agent-runner/src/rendering.rs`

- [ ] **Step 1: Write the failing dispatch and route tests**

Create `agent-runner/tests/dispatch_tests.rs`:

```rust
use agent_runner::dispatch::dispatch_plan;
use agent_runner::protocol::{
    AssetRef, AssetSource, MessagePart, OutboundAction, OutboundMessage, OutboundPlan,
    ReactionAction,
};

#[test]
fn dispatcher_accepts_reaction_actions_without_global_capability_matrix() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage::text("done")],
        actions: vec![
            OutboundAction::ReactionAdd(ReactionAction {
                target_message_id: "msg-1".into(),
                emoji: "👍".into(),
            }),
            OutboundAction::ReactionRemove(ReactionAction {
                target_message_id: "msg-1".into(),
                emoji: "👍".into(),
            }),
        ],
        delivery_report_policy: None,
    };

    dispatch_plan(&plan).unwrap();
}

#[test]
fn dispatcher_rejects_workspace_escape_asset_paths() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage {
            parts: vec![MessagePart::Image {
                asset: AssetRef {
                    asset_id: "asset-1".into(),
                    kind: "image".into(),
                    mime: "image/png".into(),
                    size_bytes: 8,
                    source: AssetSource::WorkspacePath("/tmp/not-allowed.png".into()),
                },
            }],
            reply_to: None,
            delivery_policy: None,
        }],
        actions: vec![],
        delivery_report_policy: None,
    };

    let error = dispatch_plan(&plan).unwrap_err();

    assert!(error.contains("/workspace"));
}
```

Create `agent-runner/tests/pipeline_tests.rs`:

```rust
use agent_runner::trigger_resolver::should_trigger_run;
use agent_runner::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};

#[test]
fn group_message_requires_structured_bot_mention() {
    let event = CanonicalEvent {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation: "qq:group:5566".into(),
        actor: "qq:user:42".into(),
        event_id: "evt-1".into(),
        timestamp_epoch_secs: 1,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: "msg-1".into(),
                reply_to: None,
                parts: vec![MessagePart::Text { text: "hello".into() }],
                plain_text: "hello".into(),
                mentions: vec![],
                native_metadata: serde_json::json!({}),
            }
        },
        raw_native_payload: serde_json::json!({}),
    };

    assert!(!should_trigger_run(&event));
}
```

Modify `agent-runner/tests/http_api_tests.rs` to assert the new routes exist:

```rust
#[tokio::test]
async fn app_exposes_platform_ingress_routes() {
    let app = agent_runner::server::build_test_app(std::sync::Arc::new(AcceptingRunner));
    let wechat = app.clone().oneshot(
        axum::http::Request::builder()
            .method("HEAD")
            .uri("/v1/platforms/wechatpadpro/events")
            .body(axum::body::Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(wechat.status(), axum::http::StatusCode::OK);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test dispatch_tests --test pipeline_tests --test http_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the dispatch module and platform routes do not exist yet.

- [ ] **Step 3: Write the minimal dispatch/pipeline integration**

Create `agent-runner/src/dispatch.rs`:

```rust
use crate::protocol::{AssetSource, MessagePart, OutboundPlan};

pub fn dispatch_plan(plan: &OutboundPlan) -> Result<(), String> {
    for message in &plan.messages {
        for part in &message.parts {
            let asset = match part {
                MessagePart::Image { asset }
                | MessagePart::File { asset }
                | MessagePart::Voice { asset }
                | MessagePart::Video { asset }
                | MessagePart::Sticker { asset } => asset,
                _ => continue,
            };

            if let AssetSource::WorkspacePath(path) = &asset.source {
                if !path.starts_with("/workspace/") {
                    return Err(format!("asset path must stay under /workspace: {path}"));
                }
            }
        }
    }

    Ok(())
}
```

Update `agent-runner/src/trigger_resolver.rs`:

```rust
use crate::protocol::{CanonicalEvent, EventKind};

pub fn should_trigger_run(event: &CanonicalEvent) -> bool {
    let EventKind::Message { ref message } = event.kind else {
        return false;
    };

    let normalized = message.project_plain_text().trim().to_string();
    if event.conversation.contains(":private:") {
        return !normalized.is_empty();
    }

    message
        .mentions
        .iter()
        .any(|mention| mention == &event.platform_account)
}
```

Update `agent-runner/src/lib.rs`:

```rust
pub mod dispatch;
```

Update `agent-runner/src/server.rs` routes:

```rust
Router::new()
    .route("/healthz", get(healthz))
    .route("/v1/platforms/wechatpadpro/events", get(wechat_probe).head(wechat_probe).post(wechat_event))
    .route("/v1/platforms/qq/napcat/ws", get(qq_napcat_ws))
```

Keep `/v1/runs` only as an internal debugging route for Docker execution tests. Remove `/v1/inbound-messages`.

Delete the old text-first runtime files now that the canonical pipeline owns their responsibilities:

```bash
rm -f agent-runner/src/inbound_models.rs agent-runner/src/messenger.rs agent-runner/src/rendering.rs
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test dispatch_tests --test pipeline_tests --test http_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/dispatch.rs agent-runner/src/lib.rs agent-runner/src/server.rs agent-runner/src/trigger_resolver.rs agent-runner/tests/dispatch_tests.rs agent-runner/tests/pipeline_tests.rs agent-runner/tests/http_api_tests.rs agent-runner/tests/trigger_resolver_tests.rs
git add -A agent-runner/src/inbound_models.rs agent-runner/src/messenger.rs agent-runner/src/rendering.rs
git commit -m "feat: wire canonical pipeline and dispatch"
```

### Task 7: Migrate configuration and deploy scripts to direct platform ingress

**Files:**
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/tests/config_tests.rs`
- Modify: `deploy/dogbot.env.example`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/configure_napcat_ws.sh`
- Modify: `scripts/configure_wechatpadpro_webhook.sh`
- Modify: `scripts/check_structure.sh`
- Modify: `scripts/tests/test_configure_napcat_ws.sh`
- Modify: `scripts/tests/test_configure_wechatpadpro_webhook.sh`
- Modify: `scripts/tests/test_start_agent_runner.sh`
- Create: `scripts/tests/test_deploy_stack_platform_ingress.sh`
- Delete: `scripts/start_qq_adapter.sh`
- Delete: `scripts/start_wechatpadpro_adapter.sh`
- Delete: `scripts/tests/test_start_qq_adapter.sh`
- Delete: `qq_adapter/`
- Delete: `wechatpadpro_adapter/`

- [ ] **Step 1: Write the failing config and script tests**

Update `agent-runner/tests/config_tests.rs`:

```rust
#[test]
fn settings_read_grouped_platform_env_keys() {
    let settings = agent_runner::config::Settings::from_env_map(std::collections::HashMap::from([
        ("PLATFORM_QQ_ACCOUNT_ID".into(), "qq:bot_uin:123".into()),
        ("PLATFORM_QQ_BOT_ID".into(), "123".into()),
        ("PLATFORM_WECHATPADPRO_ACCOUNT_ID".into(), "wechatpadpro:account:bot".into()),
        ("PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES".into(), "DogDu".into()),
    ]))
    .unwrap();

    assert_eq!(settings.platform_qq_account_id.as_deref(), Some("qq:bot_uin:123"));
    assert_eq!(settings.platform_wechatpadpro_account_id.as_deref(), Some("wechatpadpro:account:bot"));
}
```

Create `scripts/tests/test_deploy_stack_platform_ingress.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if grep -q 'start_qq_adapter.sh' "$repo_root/scripts/deploy_stack.sh"; then
  echo "FAIL: deploy_stack.sh must not launch qq_adapter anymore" >&2
  exit 1
fi

if grep -q 'start_wechatpadpro_adapter.sh' "$repo_root/scripts/deploy_stack.sh"; then
  echo "FAIL: deploy_stack.sh must not launch wechatpadpro_adapter anymore" >&2
  exit 1
fi

if ! grep -q '/v1/platforms/qq/napcat/ws' "$repo_root/scripts/configure_napcat_ws.sh"; then
  echo "FAIL: NapCat websocket client must point at agent-runner platform ingress" >&2
  exit 1
fi

if ! grep -q '/v1/platforms/wechatpadpro/events' "$repo_root/scripts/configure_wechatpadpro_webhook.sh"; then
  echo "FAIL: WeChatPadPro webhook must point at agent-runner platform ingress" >&2
  exit 1
fi
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml
bash scripts/tests/test_configure_napcat_ws.sh
bash scripts/tests/test_configure_wechatpadpro_webhook.sh
bash scripts/tests/test_start_agent_runner.sh
bash scripts/tests/test_deploy_stack_platform_ingress.sh
```

Expected: FAIL because config keys and script routes still point to the deleted adapters.

- [ ] **Step 3: Write the config and script migration**

Add these fields to `agent-runner/src/config.rs`:

```rust
pub platform_qq_account_id: Option<String>,
pub platform_qq_bot_id: Option<String>,
pub platform_wechatpadpro_account_id: Option<String>,
pub platform_wechatpadpro_bot_mention_names: Vec<String>,
```

Update `deploy/dogbot.env.example`:

```env
PLATFORM_QQ_ACCOUNT_ID=qq:bot_uin:unknown
PLATFORM_QQ_BOT_ID=
PLATFORM_WECHATPADPRO_ACCOUNT_ID=wechatpadpro:account:unknown
PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES=DogDu
```

Update `scripts/start_agent_runner.sh` to export the new variables:

```bash
  PLATFORM_QQ_ACCOUNT_ID="${PLATFORM_QQ_ACCOUNT_ID:-qq:bot_uin:unknown}" \
  PLATFORM_QQ_BOT_ID="${PLATFORM_QQ_BOT_ID:-}" \
  PLATFORM_WECHATPADPRO_ACCOUNT_ID="${PLATFORM_WECHATPADPRO_ACCOUNT_ID:-wechatpadpro:account:unknown}" \
  PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES="${PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES:-DogDu}" \
```

Update `scripts/configure_napcat_ws.sh`:

```bash
NAPCAT_WS_CLIENT_URL="${NAPCAT_WS_CLIENT_URL:-ws://host.docker.internal:8787/v1/platforms/qq/napcat/ws}"
PLATFORM_QQ_BOT_ID="${PLATFORM_QQ_BOT_ID:-}"
CONFIG_FILE="$NAPCAT_CONFIG_DIR/onebot11_${PLATFORM_QQ_BOT_ID}.json"
```

Update `scripts/configure_wechatpadpro_webhook.sh`:

```bash
callback_url="${WECHATPADPRO_WEBHOOK_URL:-http://host.docker.internal:8787/v1/platforms/wechatpadpro/events}"
```

Update `scripts/deploy_stack.sh`:

```bash
if [[ "${ENABLE_QQ}" == "1" ]]; then
  dogbot_require_env PLATFORM_QQ_BOT_ID
  run_compose_up "$repo_root/compose/platform-stack.yml" napcat
  "$repo_root/scripts/configure_napcat_ws.sh" "$env_file"
fi

if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  run_compose_up "$repo_root/compose/wechatpadpro-stack.yml"
  if [[ "${WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK:-0}" == "1" ]]; then
    "$repo_root/scripts/configure_wechatpadpro_webhook.sh" "$env_file"
  fi
fi
```

Delete:

```bash
rm -rf qq_adapter wechatpadpro_adapter
rm -f scripts/start_qq_adapter.sh scripts/start_wechatpadpro_adapter.sh scripts/tests/test_start_qq_adapter.sh
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test config_tests --manifest-path agent-runner/Cargo.toml
bash scripts/tests/test_configure_napcat_ws.sh
bash scripts/tests/test_configure_wechatpadpro_webhook.sh
bash scripts/tests/test_start_agent_runner.sh
bash scripts/tests/test_deploy_stack_platform_ingress.sh
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/config.rs agent-runner/tests/config_tests.rs deploy/dogbot.env.example scripts/start_agent_runner.sh scripts/deploy_stack.sh scripts/configure_napcat_ws.sh scripts/configure_wechatpadpro_webhook.sh scripts/check_structure.sh scripts/tests/test_configure_napcat_ws.sh scripts/tests/test_configure_wechatpadpro_webhook.sh scripts/tests/test_start_agent_runner.sh scripts/tests/test_deploy_stack_platform_ingress.sh
git add -A qq_adapter wechatpadpro_adapter scripts/start_qq_adapter.sh scripts/start_wechatpadpro_adapter.sh scripts/tests/test_start_qq_adapter.sh
git commit -m "refactor: remove python adapters from deploy path"
```

### Task 8: Update docs and run focused regression verification

**Files:**
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `docs/README.md`
- Modify: `docs/control-plane-integration.md`
- Test only: `agent-runner/tests/*`, `scripts/tests/*`

- [ ] **Step 1: Update docs to describe the new runtime shape**

Replace the architecture summary in `README.md` with:

```md
QQ -> NapCat -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> agent-runner -> claude-runner
```

Add to `deploy/README.md`:

```md
当前不再启动 `qq_adapter/` 或 `wechatpadpro_adapter/`。
`agent-runner` 直接提供：

- `ws://<bind>/v1/platforms/qq/napcat/ws`
- `http://<bind>/v1/platforms/wechatpadpro/events`
```

Update `docs/control-plane-integration.md` to replace:

```md
建议只确认三类进程：

- `agent-runner`
- `qq-adapter`
- `wechatpadpro-adapter`
```

with:

```md
建议只确认一类宿主机进程：

- `agent-runner`
```

- [ ] **Step 2: Run the full focused regression suite**

Run:

```bash
cargo test \
  --test protocol_tests \
  --test normalizer_tests \
  --test platform_qq_tests \
  --test platform_wechatpadpro_tests \
  --test dispatch_tests \
  --test pipeline_tests \
  --test session_store_tests \
  --test history_ingest_tests \
  --test config_tests \
  --test http_api_tests \
  --manifest-path agent-runner/Cargo.toml

bash scripts/tests/test_start_agent_runner.sh
bash scripts/tests/test_configure_napcat_ws.sh
bash scripts/tests/test_configure_wechatpadpro_webhook.sh
bash scripts/tests/test_deploy_stack_platform_ingress.sh
```

Expected: all commands PASS.

- [ ] **Step 3: Verify that removed adapter files are gone**

Run:

```bash
test ! -d qq_adapter
test ! -d wechatpadpro_adapter
test ! -f scripts/start_qq_adapter.sh
test ! -f scripts/start_wechatpadpro_adapter.sh
```

Expected: all commands exit `0`.

- [ ] **Step 4: Commit**

```bash
git add README.md deploy/README.md docs/README.md docs/control-plane-integration.md
git commit -m "docs: document unified platform runtime"
```
