# myQQbot

Personal QQ bot stack built around:

- `NapCat` for personal QQ access
- `AstrBot` for bot platform integration
- `agent-runner` in Rust for Docker-managed Claude CLI execution
- host-local `api-proxy` for PackyAPI or MiniMax access

The top priorities of this repository are:

1. hard CPU and memory limits for the CLI container
2. hard command timeout enforcement in `agent-runner`

## Planned Architecture

```text
QQ -> NapCat -> AstrBot -> agent-runner -> claude-runner container -> local api-proxy -> PackyAPI / MiniMax
```

## Planned Repository Layout

```text
.
├── README.md
├── compose/
│   └── docker-compose.yml
├── docker/
│   └── claude-runner/
│       ├── Dockerfile
│       └── entrypoint.sh
├── agent-runner/
│   ├── Cargo.toml
│   └── src/
├── astrbot/
│   └── plugins/
├── scripts/
└── docs/
```

## v1 Scope

- personal QQ account only
- Claude CLI only
- MiniMax as the primary upstream model path
- host-local API proxy for secret isolation
- one approved writable workspace mount
- strict timeout and resource controls
- platform-neutral core request model, even though v1 only targets QQ

## TODO

- [x] create `compose/docker-compose.yml`
- [x] define `claude-runner` container with `ubuntu:24.04`
- [x] install `node`, `npm`, `claude`, and required shell tools in the image
- [x] set Compose CPU limit for the CLI container
- [x] set Compose memory limit for the CLI container
- [x] set `pids_limit` for the CLI container
- [x] mount only `/workspace` and `/state` as writable paths
- [x] enable `read_only` root filesystem where compatible
- [x] add `tmpfs` mount for `/tmp`
- [x] scaffold Rust `agent-runner`
- [x] implement `POST /v1/runs`
- [x] keep the core `agent-runner` request schema platform-neutral with fields such as `platform`, `conversation_id`, `user_id`, and `session_id`
- [x] implement command timeout enforcement in Rust
- [x] implement forced termination on timeout
- [x] capture and normalize stdout, stderr, exit code, and duration
- [x] add health endpoint for runtime status
- [x] add host-local `api-proxy` integration notes
- [x] document secret handling so upstream keys never enter the CLI container
- [x] add verification steps for CPU, memory, and timeout behavior
- [x] make `agent-runner` auto-create the Claude container when it does not exist, not only start an existing container
- [x] switch from one-shot `claude -p` execution to session-aware resume flow
- [x] persist platform-neutral session metadata in SQLite under the host state directory, separate from the mounted Claude runtime state
- [x] define `session_id` to Claude internal session mapping and resume lifecycle rules
- [x] restrict Claude container access to host `api-proxy` only, while still allowing arbitrary outbound internet access
- [x] document the host-side network assumptions so no other host service is exposed to the Claude container
- [x] add bounded concurrency control for active CLI runs
- [x] add bounded queue length and overflow behavior
- [x] add global per-minute reply rate limiting
- [x] add per-user and per-conversation rate limiting
- [x] add AstrBot plugin that calls `agent-runner`
- [x] define session mapping strategy for private chat
- [ ] define error responses for queue saturation, rate limit, and container creation failure
- [ ] add bootstrap script for local setup
- [ ] add versioned Claude upgrade and rollback workflow

## Notes

- Real upstream API keys must stay on the host, not in the CLI container.
- `agent-runner` is the policy boundary for timeout and execution behavior.
- Default container targets are `4 CPU`, `4GB` memory, and `50GB` writable storage where the Docker storage driver supports service-level size limits.
- First-pass network policy is: allow outbound internet plus host `api-proxy`, but deny reliance on any other host-local service.
- `disk` limits still depend on Docker storage-driver support; if `storage_opt.size` is ignored on the host, host-side quota remains the fallback control.
- Python-based AstrBot integration and smoke-test helpers should be run with `uv run ...`, not bare `python`.
- Host firewall enforcement is implemented via `scripts/apply_runner_network_policy.sh` and `scripts/remove_runner_network_policy.sh`, and the full smoke path is in `scripts/smoke_test_claude_runner.sh`.
