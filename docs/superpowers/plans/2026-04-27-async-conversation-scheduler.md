# Async Conversation Scheduler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace synchronous platform-trigger execution with an async scheduler that limits global concurrency, serializes each conversation, gives queue feedback, reacts when work starts, removes minute rate limits, and stops wall-clock timeout killing.

**Architecture:** Add a focused scheduler state module for admission, queue positions, promotion, and status snapshots. Keep final output delivery in `server.rs` so existing `OutboundPlan` normalization and platform adapters continue to be used. Remove dispatcher rate limiting and make platform ingress submit work to background tasks instead of awaiting Claude completion inline.

**Tech Stack:** Rust 2024, Tokio tasks, Axum, existing `Runner`, `PlatformRegistry`, `OutboundPlan`, and platform adapters.

---

## File Map

- Create `agent-runner/src/scheduler.rs`: pure in-memory scheduler state machine and unit tests.
- Modify `agent-runner/src/lib.rs`: export the scheduler module.
- Modify `agent-runner/src/server.rs`: replace `mpsc` dispatcher/rate limiter with `RunScheduler`; make platform ingress async; expand `/agent-status`.
- Modify `agent-runner/src/exec.rs`: remove `tokio::time::timeout` wrapping and process killing on elapsed timeout.
- Modify `agent-runner/src/models.rs`: keep `timeout_secs` accepted for compatibility, but stop validating and carrying it into `ValidatedRunRequest`.
- Modify `agent-runner/src/config.rs`: stop rejecting default timeout greater than max; keep old env parsing temporarily if needed.
- Modify `claude-prompt/CLAUDE.md`: add long foreground task guidance.
- Modify `deploy/dogbot.env.example` and docs/tests that assert scheduling env vars.
- Modify tests under `agent-runner/tests/`: update synchronous platform delivery expectations, remove minute-limit tests, add async scheduling tests.

---

### Task 1: Pure Scheduler State

**Files:**
- Create: `agent-runner/src/scheduler.rs`
- Modify: `agent-runner/src/lib.rs`

- [ ] **Step 1: Write scheduler tests**

Add tests in `agent-runner/src/scheduler.rs` for:

```rust
#[test]
fn starts_immediately_when_capacity_and_conversation_are_free() {
    let mut scheduler = SchedulerState::new(2, 8);
    let admission = scheduler.admit(TaskSummary::new("t1", "qq:group:1"));
    assert_eq!(admission, Admission::StartNow);
    assert_eq!(scheduler.snapshot().active_count, 1);
}

#[test]
fn queues_behind_running_same_conversation() {
    let mut scheduler = SchedulerState::new(2, 8);
    assert_eq!(scheduler.admit(TaskSummary::new("t1", "qq:group:1")), Admission::StartNow);
    assert_eq!(scheduler.admit(TaskSummary::new("t2", "qq:group:1")), Admission::Queued { tasks_ahead: 1 });
    assert_eq!(scheduler.snapshot().waiting_count, 1);
}

#[test]
fn queues_when_global_capacity_is_full() {
    let mut scheduler = SchedulerState::new(1, 8);
    assert_eq!(scheduler.admit(TaskSummary::new("t1", "qq:group:1")), Admission::StartNow);
    assert_eq!(scheduler.admit(TaskSummary::new("t2", "qq:group:2")), Admission::Queued { tasks_ahead: 1 });
}

#[test]
fn queue_capacity_rejects_waiting_task() {
    let mut scheduler = SchedulerState::new(1, 1);
    assert_eq!(scheduler.admit(TaskSummary::new("t1", "qq:group:1")), Admission::StartNow);
    assert_eq!(scheduler.admit(TaskSummary::new("t2", "qq:group:2")), Admission::Queued { tasks_ahead: 1 });
    assert_eq!(scheduler.admit(TaskSummary::new("t3", "qq:group:3")), Admission::QueueFull);
}

#[test]
fn completion_promotes_same_conversation_then_ready_conversations() {
    let mut scheduler = SchedulerState::new(2, 8);
    scheduler.admit(TaskSummary::new("a1", "qq:group:1"));
    scheduler.admit(TaskSummary::new("a2", "qq:group:1"));
    scheduler.admit(TaskSummary::new("b1", "qq:group:2"));
    let promoted = scheduler.finish("qq:group:1", TerminalState::Completed);
    assert_eq!(promoted, vec!["a2".to_string()]);
}
```

- [ ] **Step 2: Implement `SchedulerState` minimally**

Define these public types:

```rust
pub struct SchedulerState { ... }
pub struct TaskSummary { pub task_id: String, pub conversation_key: String, ... }
pub enum Admission { StartNow, Queued { tasks_ahead: usize }, QueueFull }
pub enum TerminalState { Completed, Failed }
pub struct SchedulerSnapshot { pub active_count: usize, pub max_concurrent: usize, pub waiting_count: usize, ... }
```

Implement:

```rust
impl SchedulerState {
    pub fn new(max_concurrent: usize, max_queue_depth: usize) -> Self;
    pub fn admit(&mut self, task: TaskSummary) -> Admission;
    pub fn finish(&mut self, conversation_key: &str, terminal: TerminalState) -> Vec<String>;
    pub fn take_promoted_payloads<T>(&mut self, ids: &[String], payloads: &mut HashMap<String, T>) -> Vec<T>;
    pub fn snapshot(&self) -> SchedulerSnapshot;
}
```

- [ ] **Step 3: Export module and run unit tests**

Add to `agent-runner/src/lib.rs`:

```rust
pub mod scheduler;
```

Run:

```bash
cargo test --manifest-path agent-runner/Cargo.toml scheduler -- --nocapture
```

Expected: scheduler tests pass.

---

### Task 2: Async Platform Scheduler Integration

**Files:**
- Modify: `agent-runner/src/server.rs`
- Test: `agent-runner/tests/platform_http_delivery_tests.rs`
- Test: `agent-runner/tests/platform_ingress_delivery_tests.rs`

- [ ] **Step 1: Add async scheduler tests**

Add tests proving:

```rust
#[tokio::test]
async fn qq_ingress_returns_before_blocking_runner_completes() { ... }

#[tokio::test]
async fn queued_same_conversation_replies_with_tasks_ahead_without_reaction() { ... }

#[tokio::test]
async fn queued_task_reacts_only_when_promoted() { ... }
```

Use a fake runner backed by `tokio::sync::oneshot` or `Notify` so the test can assert the webhook returns while the runner is still blocked.

- [ ] **Step 2: Replace `queue_tx` with scheduler handle**

Change `AppState` from:

```rust
queue_tx: mpsc::Sender<QueuedRun>,
```

to:

```rust
runner: Arc<dyn Runner>,
scheduler: Arc<RunScheduler>,
```

`RunScheduler` lives in `server.rs` and owns:

```rust
state: Mutex<SchedulerState>,
payloads: Mutex<HashMap<String, ScheduledRun>>,
runner: Arc<dyn Runner>,
platform_registry: PlatformRegistry,
message_override: Option<Arc<dyn Messenger>>,
```

- [ ] **Step 3: Submit platform runs asynchronously**

Change `TriggerDecision::Run` handling to:

```rust
let feedback = state.scheduler.submit(event.clone(), request, validated).await;
match feedback {
    ScheduleFeedback::Started => accepted_response("accepted"),
    ScheduleFeedback::Queued { tasks_ahead } => {
        let plan = queue_wait_plan(tasks_ahead);
        deliver_plan_for_event(state, &event, &plan).await ...;
        accepted_response("queued")
    }
    ScheduleFeedback::QueueFull => {
        let plan = queue_full_plan();
        deliver_plan_for_event(state, &event, &plan).await ...;
        accepted_response("queue_full")
    }
}
```

- [ ] **Step 4: Start task sends reaction and spawns run**

Implement `RunScheduler::start_task` so it:

1. sends `start_reaction_plan(&event)` best-effort for QQ only;
2. `tokio::spawn`s the existing run/normalize/deliver flow;
3. calls scheduler `finish` and starts promoted tasks.

- [ ] **Step 5: Run platform integration tests**

Run:

```bash
cargo test --manifest-path agent-runner/Cargo.toml --test platform_http_delivery_tests -- --nocapture
cargo test --manifest-path agent-runner/Cargo.toml --test platform_ingress_delivery_tests -- --nocapture
```

Expected: async ingress tests pass, existing delivery behavior still passes after waiting for final output where needed.

---

### Task 3: Remove Timeout Kill and Minute Rate Limits

**Files:**
- Modify: `agent-runner/src/exec.rs`
- Modify: `agent-runner/src/models.rs`
- Modify: `agent-runner/src/config.rs`
- Test: `agent-runner/tests/http_api_tests.rs`
- Test: `agent-runner/tests/config_tests.rs`

- [ ] **Step 1: Update model validation tests**

Change timeout tests so `timeout_secs` is accepted but ignored:

```rust
#[test]
fn run_request_ignores_timeout_secs_for_validation() {
    let mut request = sample_run_request();
    request.timeout_secs = Some(999_999);
    let validated = request.validate().expect("validated");
    assert_eq!(validated.cwd, "/workspace");
}
```

- [ ] **Step 2: Remove timeout from `ValidatedRunRequest`**

Change:

```rust
pub struct ValidatedRunRequest {
    pub timeout_secs: u64,
    pub cwd: String,
    pub prompt: String,
    pub system_prompt: String,
}
```

to:

```rust
pub struct ValidatedRunRequest {
    pub cwd: String,
    pub prompt: String,
    pub system_prompt: String,
}
```

Change `RunRequest::validate(...)` to `RunRequest::validate()`.

- [ ] **Step 3: Remove exec timeout kill**

In `agent-runner/src/exec.rs`, replace:

```rust
let result = timeout(Duration::from_secs(validated.timeout_secs), self.runtime.collect_exec_output(&exec.id)).await;
```

with:

```rust
let result = self.runtime.collect_exec_output(&exec.id).await;
```

Remove timeout-specific process killing from this path.

- [ ] **Step 4: Remove rate limiter tests**

Delete or rewrite tests that expect `rate_limited` from `/v1/runs`. Keep queue/full tests only if `/v1/runs` still exposes synchronous local behavior; otherwise move scheduling assertions to platform tests.

- [ ] **Step 5: Run API/config tests**

Run:

```bash
cargo test --manifest-path agent-runner/Cargo.toml --test http_api_tests -- --nocapture
cargo test --manifest-path agent-runner/Cargo.toml --test config_tests -- --nocapture
```

Expected: no timeout bound or minute rate-limit assertions remain.

---

### Task 4: Status and Prompt/Config Docs

**Files:**
- Modify: `agent-runner/src/server.rs`
- Modify: `claude-prompt/CLAUDE.md`
- Modify: `deploy/dogbot.env.example`
- Modify: `deploy/README.md`
- Modify: `README.md`
- Test: `agent-runner/tests/platform_ingress_delivery_tests.rs`
- Test: `agent-runner/tests/prompt_contract_tests.rs`

- [ ] **Step 1: Expand `/agent-status` output**

Change `status_outbound_plan()` into `status_outbound_plan(snapshot: SchedulerSnapshot)` and include:

```text
agent-runner ok
running: <active>/<max>
queued: <waiting>/<max_queue_depth>
recent: <last terminal summary or none>
```

- [ ] **Step 2: Add prompt guidance**

Append to `claude-prompt/CLAUDE.md`:

```markdown
Long-running commands:

- For benchmarks, long tests, training, crawlers, or builds that may run for a long time, prefer background execution.
- Use a log path under /workspace, for example:
  `mkdir -p /workspace/.run/logs && nohup <command> > /workspace/.run/logs/<name>.log 2>&1 &`
- Return early with the command started, the log path, and how the user can ask for status later.
- Do not keep the foreground turn blocked on long-running work unless the user explicitly asks you to wait.
```

- [ ] **Step 3: Simplify env examples**

Remove from `deploy/dogbot.env.example`:

```text
DEFAULT_TIMEOUT_SECS=120
MAX_TIMEOUT_SECS=300
GLOBAL_RATE_LIMIT_PER_MINUTE=10
USER_RATE_LIMIT_PER_MINUTE=3
CONVERSATION_RATE_LIMIT_PER_MINUTE=5
```

Keep:

```text
MAX_CONCURRENT_RUNS=10
MAX_QUEUE_DEPTH=20
```

- [ ] **Step 4: Run prompt/doc tests**

Run:

```bash
cargo test --manifest-path agent-runner/Cargo.toml --test prompt_contract_tests -- --nocapture
bash scripts/tests/test_start_agent_runner.sh
```

Expected: prompt contract and startup env tests pass after updating expected env keys.

---

### Task 5: Full Verification and Commit

**Files:**
- All modified implementation, docs, and tests.

- [ ] **Step 1: Run Rust test suite**

```bash
cargo test --manifest-path agent-runner/Cargo.toml -- --nocapture
```

Expected: all tests pass; live Postgres tests may skip when `DOGBOT_TEST_DATABASE_URL` is unset.

- [ ] **Step 2: Run shell smoke tests**

```bash
bash scripts/tests/test_start_agent_runner.sh
bash scripts/tests/test_deploy_stack_platform_ingress.sh
```

Expected: both pass.

- [ ] **Step 3: Run formatting/checks**

```bash
cargo fmt --manifest-path agent-runner/Cargo.toml
git diff --check
```

Expected: formatting succeeds and `git diff --check` is clean.

- [ ] **Step 4: Commit**

```bash
git add README.md agent-runner claude-prompt deploy docs scripts
git commit -m "Simplify agent scheduling"
```

Expected: one implementation commit containing scheduler, tests, prompt, and config updates.
