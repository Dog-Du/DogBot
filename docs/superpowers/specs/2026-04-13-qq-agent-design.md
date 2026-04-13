# QQ Personal Bot with Dockerized Claude CLI

## Goal

Build a personal QQ bot stack with these components:

- `NapCat` for personal QQ account access
- `AstrBot` for bot platform, session routing, and plugin orchestration
- a Rust `agent-runner` service for Docker lifecycle management and command execution
- a local `api-proxy` on the host for upstream model access
- a dedicated Docker container running `Claude Code` CLI

The design prioritizes two controls above all others:

1. strict CPU and memory limits for the CLI container
2. strict command timeout and forced termination on timeout

## Non-Goals

- supporting multiple CLI agents in v1
- exposing Docker control to the CLI container
- storing real upstream API keys inside the CLI container
- implementing advanced long-term memory, group permissions, or web admin UI in v1

## Recommended Architecture

```text
QQ Personal Account
  -> NapCat
  -> AstrBot
  -> AstrBot plugin
  -> agent-runner (Rust, host service)
  -> claude-runner container
  -> local api-proxy on host
  -> PackyAPI / MiniMax
```

## Component Responsibilities

### NapCat

- logs in with the personal QQ account
- translates QQ events into OneBot-compatible events
- sends outbound messages back to QQ

### AstrBot

- receives events from NapCat
- maps chat messages to bot commands and sessions
- forwards requests to `agent-runner`
- returns final text responses to NapCat

### AstrBot Plugin

- extracts the effective user prompt
- maps QQ user or group context to a stable `session_id`
- calls `agent-runner` over HTTP
- handles timeout and error replies in a user-friendly way

### agent-runner

- ensures the Claude container image exists
- ensures the Claude container is created and running
- sends execution requests into the container
- enforces command timeout
- captures stdout and stderr
- applies concurrency rules
- normalizes errors for AstrBot

This service is the policy boundary for execution safety.

### claude-runner container

- runs `ubuntu:24.04`
- includes `node`, `npm`, `claude` CLI, and basic shell tools
- mounts only the approved workspace and state directories
- does not contain the real upstream API key
- uses the host `api-proxy` as its only model endpoint

### api-proxy

- runs on the host, not in the CLI container
- stores `PACKYAPI_KEY` or direct `MINIMAX_API_KEY`
- exposes an Anthropic-compatible endpoint for Claude CLI
- can later expose an OpenAI-compatible endpoint for other clients

This prevents prompt injection from directly reading the real key from the CLI runtime.

## Execution Flow

1. QQ user sends a message.
2. NapCat forwards the event to AstrBot.
3. AstrBot plugin extracts the message and builds a request:
   - `session_id`
   - `user_id`
   - `chat_type`
   - `text`
   - `cwd`
   - timeout hints if needed
4. Plugin sends the request to `agent-runner`.
5. `agent-runner` ensures the Claude container is ready.
6. `agent-runner` executes `claude` in the container with the configured workspace.
7. Claude CLI calls the host `api-proxy`, which forwards to PackyAPI or MiniMax.
8. `agent-runner` returns normalized output to AstrBot.
9. AstrBot formats the result and replies through NapCat.

## Runtime Safety Requirements

### Resource Limits

The Claude container must define these controls in Compose from day one:

- `cpus`
- `mem_limit`
- `memswap_limit` or equivalent memory ceiling strategy
- `pids_limit`
- bounded writable mounts
- `read_only: true` for the container root filesystem where practical
- `tmpfs` for `/tmp`

Recommended initial defaults:

- CPU: `2.0`
- memory: `4g`
- pids: `256`

These values should live in configuration and not be hard-coded in Rust.

### Timeout Enforcement

Timeout enforcement belongs in `agent-runner`, not in AstrBot.

Required behavior:

- each run request has a timeout value
- `agent-runner` starts a timer when the command begins
- on timeout, `agent-runner` terminates the exec process
- if the process does not stop cleanly, `agent-runner` escalates to container-side forced termination logic for the execution
- the final response must clearly indicate timeout

Recommended initial defaults:

- default timeout: `120s`
- max timeout: `300s`

### Workspace Isolation

Only these writable mounts should exist for the CLI container:

- `/workspace`
- `/state`

Do not mount:

- `/var/run/docker.sock`
- host SSH directories
- host home directory
- secrets directories

## Secrets Design

The real key must not be accessible from the Claude container.

Allowed pattern:

- host `api-proxy` reads key from host environment or a host-only secret file
- Claude container only sees:
  - proxy base URL
  - optional non-sensitive routing config

Disallowed pattern:

- putting `PACKYAPI_KEY` or `MINIMAX_API_KEY` in the Claude container environment
- storing keys in `/workspace`
- passing keys through prompts

## Code Structure

```text
.
├── README.md
├── docker/
│   └── claude-runner/
│       ├── Dockerfile
│       └── entrypoint.sh
├── compose/
│   └── docker-compose.yml
├── agent-runner/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── docker_client.rs
│       ├── exec.rs
│       ├── timeout.rs
│       ├── models.rs
│       └── server.rs
├── astrbot/
│   └── plugins/
│       └── claude_runner_bridge/
├── scripts/
│   ├── bootstrap.sh
│   └── healthcheck.sh
└── docs/
    └── superpowers/
        └── specs/
            └── 2026-04-13-qq-agent-design.md
```

## Interface Contract

### agent-runner HTTP API

`POST /v1/runs`

Request body:

```json
{
  "session_id": "qq-user-123",
  "user_id": "123",
  "chat_type": "private",
  "cwd": "/workspace",
  "prompt": "hello",
  "timeout_secs": 120
}
```

Response body:

```json
{
  "status": "ok",
  "stdout": "...",
  "stderr": "",
  "exit_code": 0,
  "timed_out": false,
  "duration_ms": 1820
}
```

Error shape:

```json
{
  "status": "error",
  "error_code": "timeout",
  "message": "command exceeded timeout",
  "timed_out": true
}
```

## Implementation Phases

### Phase 1

- define project layout
- add Claude container Dockerfile
- add Compose file with strict CPU and memory limits
- implement Rust `agent-runner`
- implement timeout enforcement
- implement AstrBot bridge plugin
- run the full path for private QQ chat

### Phase 2

- better retry and restart handling
- session persistence in `/state`
- response truncation and formatting
- host proxy hardening and audit logging

### Phase 3

- optional Codex support
- per-user policy controls
- richer observability and metrics

## Risks and Mitigations

### Risk: Claude CLI startup depends on external setup

Mitigation:

- bake all stable dependencies into the image
- keep only credentials and workspace outside the image

### Risk: timed out commands leave orphan processes

Mitigation:

- centralize timeout enforcement in Rust
- ensure the container exec lifecycle is tracked
- add explicit kill escalation logic

### Risk: resource limits are configured but not verified

Mitigation:

- add startup checks in `agent-runner`
- expose a health endpoint that reports effective runtime settings

## Recommendation

This design should proceed with:

- Rust as the primary implementation language
- one dedicated Claude container
- host-local API proxy
- Compose-defined resource ceilings
- `agent-runner`-enforced command timeout

This is the smallest design that keeps the key out of the CLI container while making timeout and resource controls first-class.
