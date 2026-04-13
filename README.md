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

## TODO

- [ ] create `compose/docker-compose.yml`
- [ ] define `claude-runner` container with `ubuntu:24.04`
- [ ] install `node`, `npm`, `claude`, and required shell tools in the image
- [ ] set Compose CPU limit for the CLI container
- [ ] set Compose memory limit for the CLI container
- [ ] set `pids_limit` for the CLI container
- [ ] mount only `/workspace` and `/state` as writable paths
- [ ] enable `read_only` root filesystem where compatible
- [ ] add `tmpfs` mount for `/tmp`
- [ ] scaffold Rust `agent-runner`
- [ ] implement container existence and startup checks
- [ ] implement `POST /v1/runs`
- [ ] implement command timeout enforcement in Rust
- [ ] implement forced termination on timeout
- [ ] capture and normalize stdout, stderr, exit code, and duration
- [ ] add health endpoint for runtime status
- [ ] add AstrBot plugin that calls `agent-runner`
- [ ] define session mapping strategy for private chat
- [ ] define error responses for timeout and container failure
- [ ] add host-local `api-proxy` integration notes
- [ ] document secret handling so upstream keys never enter the CLI container
- [ ] add bootstrap script for local setup
- [ ] add verification steps for CPU, memory, and timeout behavior

## Notes

- Real upstream API keys must stay on the host, not in the CLI container.
- `agent-runner` is the policy boundary for timeout and execution behavior.
- `disk` limits are not treated as a first-class Compose guarantee; writable mount boundaries and host-side quota remain the practical control.
