# DogBot Control Plane Phase B Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move trigger recognition, reply rendering, and platform delivery metadata into `agent-runner`, while keeping `/agent` mandatory and adding reply-aware and mention-aware group invocation.

**Architecture:** Introduce a canonical `InboundMessage` API in `agent-runner`. Adapters stop making trigger decisions and stop sending final replies themselves; they normalize platform events and hand them to the runner, which then resolves triggers, executes the agent, renders plain-text replies, and sends QQ or WeChat payloads.

**Tech Stack:** Rust, `axum`, `rusqlite`, Python adapters, NapCat HTTP API, WeChatPadPro HTTP API, `cargo test`, `pytest`

---

## File Structure

- Create: `agent-runner/src/inbound_models.rs`
  - canonical inbound message types
- Create: `agent-runner/src/trigger_resolver.rs`
  - `/agent` detection, mention gating, reply gating
- Create: `agent-runner/src/provenance_store.rs`
  - minimal bot-message provenance lookup
- Create: `agent-runner/src/rendering.rs`
  - markdown degradation and media-action extraction
- Create: `agent-runner/src/wechat_messenger.rs`
  - WeChatPadPro outbound delivery implementation
- Create: `agent-runner/tests/trigger_resolver_tests.rs`
- Create: `agent-runner/tests/rendering_tests.rs`
- Create: `agent-runner/tests/inbound_api_tests.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/lib.rs`
- Modify: `agent-runner/src/messenger.rs`
- Modify: `agent-runner/tests/http_api_tests.rs`
- Modify: `qq_adapter/runner_client.py`
- Modify: `qq_adapter/app.py`
- Modify: `qq_adapter/mapper.py`
- Modify: `qq_adapter/napcat_client.py`
- Modify: `qq_adapter/tests/test_app.py`
- Modify: `qq_adapter/tests/test_mapper.py`
- Modify: `wechatpadpro_adapter/runner_client.py`
- Modify: `wechatpadpro_adapter/processor.py`
- Modify: `wechatpadpro_adapter/mapper.py`
- Modify: `wechatpadpro_adapter/command_policy.py`
- Modify: `wechatpadpro_adapter/tests/test_app.py`
- Modify: `wechatpadpro_adapter/tests/test_mapper.py`

### Task 1: Add the inbound message API and failing trigger tests

**Files:**
- Create: `agent-runner/src/inbound_models.rs`
- Create: `agent-runner/tests/trigger_resolver_tests.rs`
- Modify: `agent-runner/src/lib.rs`

- [ ] **Step 1: Write the failing trigger tests**

Create `agent-runner/tests/trigger_resolver_tests.rs`:

```rust
use agent_runner::inbound_models::InboundMessage;
use agent_runner::trigger_resolver::{TriggerDecision, TriggerResolver};

#[test]
fn private_message_requires_agent_token_anywhere_in_normalized_text() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "请帮我 /agent 总结一下".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_requires_agent_token_and_mention_or_reply() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m2".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent 看这个".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test trigger_resolver_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because the inbound models and resolver do not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/inbound_models.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboundMessage {
    pub platform: String,
    pub platform_account: String,
    pub conversation_id: String,
    pub actor_id: String,
    pub message_id: String,
    pub reply_to_message_id: Option<String>,
    pub raw_segments_json: String,
    pub normalized_text: String,
    pub mentions: Vec<String>,
    pub is_group: bool,
    pub is_private: bool,
    pub timestamp_epoch_secs: i64,
}
```

Create `agent-runner/src/trigger_resolver.rs`:

```rust
use crate::inbound_models::InboundMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerDecision {
    Run,
    Status,
    Ignore,
}

#[derive(Default)]
pub struct TriggerResolver;

impl TriggerResolver {
    pub fn resolve(&self, message: &InboundMessage) -> TriggerDecision {
        if message.normalized_text.contains("/agent-status") {
            return TriggerDecision::Status;
        }

        if !message.normalized_text.contains("/agent") {
            return TriggerDecision::Ignore;
        }

        if message.is_private {
            return TriggerDecision::Run;
        }

        if !message.mentions.is_empty() || message.reply_to_message_id.is_some() {
            return TriggerDecision::Run;
        }

        TriggerDecision::Ignore
    }
}
```

Export both modules in `agent-runner/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test trigger_resolver_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/inbound_models.rs agent-runner/src/trigger_resolver.rs agent-runner/src/lib.rs agent-runner/tests/trigger_resolver_tests.rs
git commit -m "feat: add canonical inbound message trigger resolver"
```

### Task 2: Add provenance lookup and the inbound API route

**Files:**
- Create: `agent-runner/src/provenance_store.rs`
- Create: `agent-runner/tests/inbound_api_tests.rs`
- Modify: `agent-runner/src/server.rs`
- Modify: `agent-runner/src/models.rs`

- [ ] **Step 1: Write the failing inbound API test**

Create `agent-runner/tests/inbound_api_tests.rs`:

```rust
use agent_runner::inbound_models::InboundMessage;
use agent_runner::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::server::Runner;
use async_trait::async_trait;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

struct AcceptingRunner;

#[async_trait]
impl Runner for AcceptingRunner {
    async fn run(
        &self,
        _request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
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

#[tokio::test]
async fn inbound_api_accepts_group_message_and_returns_accepted() {
    let app = agent_runner::server::build_test_app(std::sync::Arc::new(AcceptingRunner));
    let payload = serde_json::to_vec(&InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m1".into(),
        reply_to_message_id: Some("bot-msg-1".into()),
        raw_segments_json: "[]".into(),
        normalized_text: "/agent hi".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because `/v1/inbound-messages` does not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/provenance_store.rs`:

```rust
#[derive(Debug, Clone)]
pub struct ProvenanceEntry {
    pub message_id: String,
    pub sender_role: String,
}
```

Add to `agent-runner/src/server.rs`:

```rust
.route("/v1/inbound-messages", post(handle_inbound_message))
```

and implement:

```rust
async fn handle_inbound_message(State(state): State<AppState>, body: Bytes) -> Response {
    let message: crate::inbound_models::InboundMessage = match serde_json::from_slice(&body) {
        Ok(message) => message,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, "invalid_json", &err.to_string()).into_response(),
    };

    match crate::trigger_resolver::TriggerResolver::default().resolve(&message) {
        crate::trigger_resolver::TriggerDecision::Ignore => {
            (StatusCode::OK, Json(serde_json::json!({ "status": "ignored" }))).into_response()
        }
        _ => (StatusCode::OK, Json(serde_json::json!({ "status": "accepted" }))).into_response(),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/provenance_store.rs agent-runner/src/server.rs agent-runner/tests/inbound_api_tests.rs
git commit -m "feat: add inbound message API route"
```

### Task 3: Implement rendering and outbound image action parsing

**Files:**
- Create: `agent-runner/src/rendering.rs`
- Create: `agent-runner/tests/rendering_tests.rs`
- Modify: `agent-runner/src/server.rs`

- [ ] **Step 1: Write the failing rendering tests**

Create `agent-runner/tests/rendering_tests.rs`:

```rust
use agent_runner::rendering::{degrade_markdown, parse_media_actions};

#[test]
fn markdown_degradation_keeps_lists_and_urls() {
    let rendered = degrade_markdown("## Title\n\n- item\n\n[link](https://example.com)");
    assert!(rendered.contains("Title"));
    assert!(rendered.contains("- item"));
    assert!(rendered.contains("https://example.com"));
}

#[test]
fn media_action_parser_extracts_send_image_block() {
    let stdout = r#"before
```dogbot-action
{"type":"send_image","source_type":"remote_url","source_value":"https://example.com/a.png","caption_text":"done","target_conversation":"qq:group:100"}
```
"#;

    let actions = parse_media_actions(stdout);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_type, "send_image");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --test rendering_tests --manifest-path agent-runner/Cargo.toml
```

Expected: FAIL because `rendering` does not exist

- [ ] **Step 3: Write the minimal implementation**

Create `agent-runner/src/rendering.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MediaAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub source_type: String,
    pub source_value: String,
    pub caption_text: Option<String>,
    pub target_conversation: String,
}

pub fn degrade_markdown(input: &str) -> String {
    input
        .replace("## ", "")
        .replace("**", "")
        .replace('`', "")
        .replace("[link](", "")
        .replace(')', "")
}

pub fn parse_media_actions(stdout: &str) -> Vec<MediaAction> {
    stdout
        .split("```dogbot-action\n")
        .skip(1)
        .filter_map(|chunk| chunk.split("\n```").next())
        .filter_map(|json| serde_json::from_str::<MediaAction>(json).ok())
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test --test rendering_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/rendering.rs agent-runner/tests/rendering_tests.rs
git commit -m "feat: add reply rendering and media action parsing"
```

### Task 4: Add WeChat delivery and move adapters to the inbound API

**Files:**
- Create: `agent-runner/src/wechat_messenger.rs`
- Modify: `agent-runner/src/config.rs`
- Modify: `agent-runner/src/messenger.rs`
- Modify: `qq_adapter/runner_client.py`
- Modify: `qq_adapter/app.py`
- Modify: `qq_adapter/mapper.py`
- Modify: `qq_adapter/tests/test_app.py`
- Modify: `wechatpadpro_adapter/runner_client.py`
- Modify: `wechatpadpro_adapter/processor.py`
- Modify: `wechatpadpro_adapter/tests/test_app.py`

- [ ] **Step 1: Write the failing adapter tests**

Add to `qq_adapter/tests/test_app.py`:

```python
def test_group_message_is_forwarded_to_inbound_api(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["payload"] = payload
        return {"status": "accepted"}

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
```

Add to `wechatpadpro_adapter/tests/test_app.py`:

```python
def test_wechat_webhook_uses_inbound_api(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["payload"] = payload
        return {"status": "accepted"}

    monkeypatch.setattr("wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests/test_app.py -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests/test_app.py -q
```

Expected: FAIL because `send_inbound_message()` does not exist

- [ ] **Step 3: Write the minimal implementation**

Add to both runner clients:

```python
async def send_inbound_message(self, payload: dict[str, object]) -> dict[str, object]:
    async with httpx.AsyncClient(base_url=self.base_url, timeout=15) as client:
        response = await client.post("/v1/inbound-messages", json=payload)
    response.raise_for_status()
    return response.json()
```

Update `qq_adapter/mapper.py` and `wechatpadpro_adapter/mapper.py` to expose a canonical builder:

```python
def build_inbound_payload(... ) -> dict[str, object]:
    return {
        "platform": "qq",
        "platform_account": platform_account_id,
        "conversation_id": conversation_id,
        "actor_id": user_id,
        "message_id": message_id,
        "reply_to_message_id": reply_to_message_id,
        "raw_segments_json": json.dumps(raw_segments, ensure_ascii=False),
        "normalized_text": normalized_text,
        "mentions": mentions,
        "is_group": is_group,
        "is_private": not is_group,
        "timestamp_epoch_secs": int(time.time()),
    }
```

Update `qq_adapter/app.py` to call `send_inbound_message()` and stop sending NapCat replies directly.

Update `wechatpadpro_adapter/processor.py` to call `send_inbound_message()` and stop calling `run_agent()` plus `send_reply()` for accepted paths.

Add `agent-runner/src/wechat_messenger.rs` with the same request shape currently used by `wechatpadpro_adapter/wechat_client.py`.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests -q
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent-runner/src/wechat_messenger.rs agent-runner/src/config.rs agent-runner/src/messenger.rs qq_adapter/runner_client.py qq_adapter/app.py qq_adapter/mapper.py qq_adapter/tests/test_app.py wechatpadpro_adapter/runner_client.py wechatpadpro_adapter/processor.py wechatpadpro_adapter/tests/test_app.py
git commit -m "feat: route platform messages through inbound runner API"
```

### Task 5: Phase B verification

**Files:**
- Modify: `agent-runner/src/*`
- Modify: `qq_adapter/*`
- Modify: `wechatpadpro_adapter/*`

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
cargo test --test trigger_resolver_tests --manifest-path agent-runner/Cargo.toml
cargo test --test rendering_tests --manifest-path agent-runner/Cargo.toml
cargo test --test inbound_api_tests --manifest-path agent-runner/Cargo.toml
cargo test --test http_api_tests --manifest-path agent-runner/Cargo.toml
```

Expected: PASS

- [ ] **Step 2: Run adapter regression suite**

Run:

```bash
uv run --with pytest python -m pytest qq_adapter/tests -q
uv run --with pytest python -m pytest wechatpadpro_adapter/tests -q
```

Expected: PASS

- [ ] **Step 3: Review diff**

Run:

```bash
git diff --stat HEAD~5..HEAD
```

Expected:
- diff only covers inbound API, trigger resolution, rendering, delivery, and adapter handoff
