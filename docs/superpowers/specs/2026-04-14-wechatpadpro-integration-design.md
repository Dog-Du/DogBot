# WeChatPadPro Integration Design

## Goal

Add WeChat personal-account support through `WeChatPadPro` while preserving the existing architecture:

```text
QQ -> NapCat -> AstrBot -> agent-runner -> claude-runner
WeChat -> WeChatPadPro -> AstrBot -> agent-runner -> claude-runner
```

The new integration must reuse AstrBot as the single message orchestration layer and must not introduce a second direct-to-runner platform adapter.

## Why This Approach

`agent-runner` is already platform-neutral. The existing AstrBot bridge plugin already builds neutral request envelopes with `platform`, `conversation_id`, `session_id`, and `user_id`. `WeChatPadPro` should therefore live at the same architectural layer as `NapCat`: a platform ingress service that AstrBot consumes.

This keeps:

- one execution engine
- one Claude container
- one upstream proxy path
- one session store
- one queue and rate-limit implementation

## Constraints

- Continue using AstrBot as the unified message orchestrator.
- Do not expose real upstream API keys to Docker containers.
- Preserve the current QQ deployment path.
- Make WeChat support optional and switchable through deployment config.
- Prefer repository-owned deployment files over manual one-off host setup.
- Keep platform-specific behavior in deployment/docs unless code changes are required.

## Confirmed External Facts

- AstrBot `v3.5.10+` supports `WeChatPadPro` as a personal WeChat adapter.
- AstrBot’s public docs describe WeChatPadPro login, adapter configuration, credential persistence, and an optional active polling mode.
- WeChatPadPro exposes an HTTP API, webhook capabilities, and requires MySQL + Redis for the standard Docker deployment path.
- AstrBot docs note that `Gewechat` is deprecated/unavailable and recommend switching to `WeChatPadPro`.

## Architecture

### Existing Components

- `napcat`: QQ ingress
- `astrbot`: platform orchestration and plugin execution
- `claude_runner_bridge`: AstrBot plugin that forwards normalized messages to `agent-runner`
- `agent-runner`: host-local Rust service providing run/message APIs and built-in Anthropic-compatible upstream proxy
- `claude-runner`: Dockerized Claude Code CLI runtime

### New Components

- `wechatpadpro`: personal WeChat ingress service
- `wechatpadpro_mysql`: MySQL backing store required by WeChatPadPro
- `wechatpadpro_redis`: Redis backing store required by WeChatPadPro

### Final Topology

```text
QQ private/group
  -> NapCat
  -> AstrBot
  -> claude_runner_bridge
  -> agent-runner
  -> claude-runner

WeChat private/group
  -> WeChatPadPro
  -> AstrBot
  -> claude_runner_bridge
  -> agent-runner
  -> claude-runner
```

## Deployment Model

WeChat support is added as an optional stack controlled by environment variables.

Two deployment modes remain possible:

1. QQ only
2. QQ + WeChatPadPro

The repository should not force WeChatPadPro on users who only want QQ. The deploy scripts should only start the WeChatPadPro-related services when explicitly enabled.

## Data and State

The following new data roots are required:

- `WECHATPADPRO_DATA_DIR`
- `WECHATPADPRO_MYSQL_DIR`
- `WECHATPADPRO_REDIS_DIR`

The following secrets/config inputs are required:

- `WECHATPADPRO_ADMIN_KEY`
- MySQL credentials
- Redis password
- exposed host/API port for WeChatPadPro

AstrBot will continue to persist its own platform credentials under `ASTRBOT_DATA_DIR`, including WeChatPadPro credential files managed by AstrBot.

## AstrBot Integration

AstrBot remains the only platform-aware application layer. The repository does not add a separate custom WeChat adapter. Instead, the deployment and docs will make it easy to:

1. start `WeChatPadPro`
2. open AstrBot WebUI
3. create a `wechatpadpro` adapter
4. set:
   - `admin_key`
   - `host`
   - `port`
5. let AstrBot handle QR-code login and credential persistence

The existing `claude_runner_bridge` plugin already supports platform-neutral routing and should continue to work without WeChat-specific logic unless testing proves metadata gaps.

## Session and Message Routing

The current bridge uses platform-neutral identities and should naturally isolate WeChat from QQ as long as AstrBot provides distinct `platform`/origin values.

Expected session identity rules:

- private:
  - `wechat:<private-conversation-id>`
- group:
  - `wechat:<group-conversation-id>:user:<sender-id>`

Exact origin strings should continue to come from AstrBot when available. The bridge should continue preferring `unified_msg_origin` over fragile local reconstruction.

## Error Handling

### Deployment-Time Errors

- Missing WeChatPadPro enable flag: QQ stack still deploys successfully.
- Missing WeChatPadPro secrets when enabled: deploy script should fail fast with a clear message.
- WeChatPadPro DB services unhealthy: deploy docs should instruct users to wait for health before configuring AstrBot.

### Runtime Errors

- WeChatPadPro offline: AstrBot adapter logs should surface connection/login failures.
- Login expired: operator re-enters AstrBot platform adapter flow and rescans QR code.
- No messages arriving: docs should point to the AstrBot setting for active message polling.

## Security

- Real model-provider API keys stay on the host in `deploy/dogbot.env`.
- `WeChatPadPro` credentials are separate from model-provider keys.
- `claude-runner` continues to receive only the local proxy token.
- No new host-sensitive mounts are exposed to the Claude container.
- WeChatPadPro is non-official and carries account-risk; docs must state this explicitly.

## Testing Strategy

Implementation should verify:

- deployment files parse cleanly
- deploy scripts create WeChatPadPro directories when enabled
- `dogbot.env.example` contains all required WeChatPadPro fields
- structure checks still pass
- Rust tests still pass unchanged

Manual validation steps should be documented:

1. enable WeChatPadPro in env
2. start stack
3. confirm MySQL and Redis are healthy
4. confirm WeChatPadPro UI/API page is reachable
5. create `wechatpadpro` adapter in AstrBot
6. scan login QR code
7. send a private WeChat message
8. send a WeChat group message

## Scope of This Change

This design includes:

- deployment scaffolding for `WeChatPadPro`
- env/config surface for optional WeChat enablement
- deploy/stop script support
- deployment documentation

This design does not include:

- a new custom WeChat message adapter written in this repository
- long-term history storage redesign
- platform capability abstraction for stickers/emoji/reactions
- changes to Claude session semantics
