# Agent Runner Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the agent-runner foundation needed for Tasks 2 and 3 by adding the compose contract guard, configuration parsing with validated timeouts, and platform-neutral request/response models along with TDD evidence.

**Architecture:** The agent-runner crate is a lightweight Rust service that validates the existing compose resource limits via tests, loads runtime defaults from environment variables, and exposes serializable request/response models for higher layers. Each responsibility is isolated into its own module (`tests`, `config`, and `models`) so we can expand to HTTP and Docker logic later.

**Tech Stack:** Rust 2024 edition with `serde` for data shapes, `thiserror` for config errors, and standard library tests to prove the contractual guarantees.

---

### Task 1: Compose Resource Contract Test

**Files:**
- Create: `agent-runner/tests/compose_contract_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
use std::fs;

#[test]
fn compose_defines_required_claude_runner_limits() {
    let compose = fs::read_to_string("../compose/docker-compose.yml").unwrap();
    let required = [
        "cpus: \"2.0\"",
        "mem_limit: 4g",
        "memswap_limit: 4g",
        "pids_limit: 256",
        "read_only: true",
        "- /tmp:size=256m,mode=1777",
        "/workspace",
        "/state",
    ];
    for item in required {
        assert!(
            compose.contains(item),
            "missing compose contract entry: {item}"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd agent-runner && cargo test compose_defines_required_claude_runner_limits --test compose_contract_tests`

Expected: fail because the test file does not exist yet.

- [ ] **Step 3: Write the minimal implementation**

```rust
// The implementation is the same test function; no additional production code is required.
// Create `agent-runner/tests/compose_contract_tests.rs` with the code from Step 1.
```

- [ ] **Step 4: Run the test again**

Run: `cd agent-runner && cargo test compose_defines_required_claude_runner_limits --test compose_contract_tests`

Expected: pass once the test file exists and the compose file already contains the required strings (if not, coordinate with the Task 1 owner to add them).

- [ ] **Step 5: Commit**

```bash
git add agent-runner/tests/compose_contract_tests.rs
git commit -m "test: add compose resource contract"
```

### Task 2: Configuration Parsing with Timeout Validation

**Files:**
- Create: `agent-runner/src/config.rs`
- Modify/Create: `agent-runner/src/lib.rs`
- Create: `agent-runner/tests/config_tests.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use agent_runner::config::Settings;

#[test]
fn settings_use_expected_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();

    assert_eq!(settings.bind_addr, "127.0.0.1:8787");
    assert_eq!(settings.default_timeout_secs, 120);
    assert_eq!(settings.max_timeout_secs, 300);
    assert_eq!(settings.container_name, "claude-runner");
}

#[test]
fn settings_reject_default_timeout_above_max() {
    let env = std::collections::HashMap::from([
        ("DEFAULT_TIMEOUT_SECS".to_string(), "400".to_string()),
        ("MAX_TIMEOUT_SECS".to_string(), "300".to_string()),
    ]);

    let err = Settings::from_env_map(env).unwrap_err();
    assert!(err.to_string().contains("DEFAULT_TIMEOUT_SECS must be <= MAX_TIMEOUT_SECS"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd agent-runner && cargo test --test config_tests`

Expected: fail because `Settings` and the config module do not exist yet.

- [ ] **Step 3: Write the minimal implementation**

```rust
use std::collections::HashMap;
use std::env;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub bind_addr: String,
    pub default_timeout_secs: u64,
    pub max_timeout_secs: u64,
    pub container_name: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid integer for {0}")]
    InvalidInt(&'static str),
    #[error("DEFAULT_TIMEOUT_SECS must be <= MAX_TIMEOUT_SECS")]
    InvalidTimeoutBounds,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let env_map = env::vars().collect::<HashMap<_, _>>();
        Self::from_env_map(env_map)
    }

    pub fn from_env_map(env_map: HashMap<String, String>) -> Result<Self, ConfigError> {
        let bind_addr = env_map
            .get("BIND_ADDR")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1:8787".to_string());
        let default_timeout_secs = parse_u64(&env_map, "DEFAULT_TIMEOUT_SECS", 120)?;
        let max_timeout_secs = parse_u64(&env_map, "MAX_TIMEOUT_SECS", 300)?;
        let container_name = env_map
            .get("CONTAINER_NAME")
            .cloned()
            .unwrap_or_else(|| "claude-runner".to_string());

        if default_timeout_secs > max_timeout_secs {
            return Err(ConfigError::InvalidTimeoutBounds);
        }

        Ok(Self {
            bind_addr,
            default_timeout_secs,
            max_timeout_secs,
            container_name,
        })
    }
}

fn parse_u64(env_map: &HashMap<String, String>, key: &'static str, default: u64) -> Result<u64, ConfigError> {
    match env_map.get(key) {
        Some(raw) => raw.parse().map_err(|_| ConfigError::InvalidInt(key)),
        None => Ok(default),
    }
}
```

- [ ] **Step 4: Run the tests again**

Run: `cd agent-runner && cargo test --test config_tests`

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/Cargo.toml agent-runner/src/lib.rs agent-runner/src/config.rs agent-runner/tests/config_tests.rs
git commit -m "feat: add agent-runner configuration"
```

### Task 3: Platform-Neutral Request/Response Models

**Files:**
- Create: `agent-runner/src/models.rs`
- Modify: `agent-runner/src/lib.rs`
- Create: `agent-runner/tests/http_api_tests.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use agent_runner::models::RunRequest;

#[test]
fn run_request_uses_default_timeout_when_missing() {
    let request = RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: None,
    };

    assert_eq!(request.effective_timeout(120, 300).unwrap(), 120);
}

#[test]
fn run_request_rejects_timeout_over_max() {
    let request = RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: Some(500),
    };

    let err = request.effective_timeout(120, 300).unwrap_err();
    assert!(err.contains("timeout exceeds configured max"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd agent-runner && cargo test --test http_api_tests`

Expected: fail because `models` does not exist yet.

- [ ] **Step 3: Write the minimal implementation**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub platform: String,
    pub conversation_id: String,
    pub session_id: String,
    pub user_id: String,
    pub chat_type: String,
    pub cwd: String,
    pub prompt: String,
    pub timeout_secs: Option<u64>,
}

impl RunRequest {
    pub fn effective_timeout(&self, default_timeout: u64, max_timeout: u64) -> Result<u64, String> {
        let timeout = self.timeout_secs.unwrap_or(default_timeout);
        if timeout > max_timeout {
            return Err("timeout exceeds configured max".to_string());
        }
        Ok(timeout)
    }
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub timed_out: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error_code: String,
    pub message: String,
    pub timed_out: bool,
}
```

- [ ] **Step 4: Run the tests again**

Run: `cd agent-runner && cargo test --test http_api_tests`

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add agent-runner/Cargo.toml agent-runner/src/lib.rs agent-runner/src/models.rs agent-runner/tests/http_api_tests.rs
git commit -m "feat: add API request and response models"
```

