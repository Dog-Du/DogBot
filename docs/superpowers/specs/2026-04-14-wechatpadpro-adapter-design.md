# WeChatPadPro Adapter Design

## Goal

Add a minimal host-local Python adapter that connects `WeChatPadPro` to the existing `agent-runner` without going through AstrBot.

The resulting topology becomes:

```text
QQ -> NapCat -> AstrBot -> agent-runner
WeChat -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner
```

This adapter exists because the current public AstrBot image does not expose a usable `WeChatPadPro` platform adapter even though historical changelogs mention it.

## Why This Approach

The current system already has a stable execution backend:

- `agent-runner`
- Claude container
- upstream proxy
- session store
- queue/rate-limit logic

The missing piece for WeChat is only the ingress/egress layer. A thin adapter is therefore the smallest viable change:

- receive messages from `WeChatPadPro`
- normalize them into the existing platform-neutral `agent-runner` request schema
- send the reply back through `WeChatPadPro`

This avoids redesigning the execution core and keeps WeChat-specific behavior isolated.

## Constraints

- First version should be host-local and simple.
- First version should use Python for speed.
- Do not require changes to the Claude container or `agent-runner` protocol.
- Reuse the existing platform-neutral request model.
- Do not solve all media/platform-capability problems in v1.
- Prioritize text message routing for private chat and group chat.

## Confirmed Inputs

From live verification on this host:

- `WeChatPadPro` is running successfully on `127.0.0.1:38849`
- it exposes an HTTP service on container port `1238`
- it initializes a webhook subsystem on boot
- it is able to start without a repository-local `.env` file when env vars are injected by Compose

From existing repository architecture:

- `agent-runner` accepts platform-neutral `/v1/runs` requests
- `agent-runner` persists external `session_id -> Claude session` mappings
- `agent-runner` already supports proactive `/v1/messages` delivery into known sessions

## Scope

### Included

- a host-local Python adapter process
- adapter config file/env support
- webhook receive endpoint
- forwarding text messages to `agent-runner`
- sending text replies back through `WeChatPadPro`
- private and group session routing
- deployment/startup documentation

### Excluded

- sticker/image/video/file handling
- emoji/reaction capability abstraction
- long-term history persistence redesign
- WeChat-specific moderation/workflow logic
- containerizing the adapter

## Architecture

### Components

1. `wechatpadpro`
   - receives/sends WeChat traffic
   - pushes inbound events through webhook

2. `wechatpadpro-adapter`
   - new host-local Python service
   - receives webhook events from `WeChatPadPro`
   - extracts sender/conversation/message metadata
   - calls `agent-runner /v1/runs`
   - calls `WeChatPadPro` send API for replies

3. `agent-runner`
   - unchanged execution backend
   - owns session mapping, queueing, rate limiting, timeout enforcement

### Message Flow

#### Inbound

```text
WeChat message
-> WeChatPadPro webhook
-> wechatpadpro-adapter
-> POST /v1/runs to agent-runner
```

#### Outbound

```text
agent-runner stdout
-> wechatpadpro-adapter
-> WeChatPadPro send-message HTTP API
-> WeChat private chat or group
```

## Adapter Interface

### Inbound endpoint

The adapter exposes a local HTTP endpoint, for example:

```text
POST /wechatpadpro/events
```

This endpoint will:

- validate a local shared token if configured
- log the event type
- extract text messages only in v1
- ignore unsupported event types with `200 OK`

### Outbound WeChat API client

The adapter will call `WeChatPadPro` HTTP APIs using:

- `WECHATPADPRO_BASE_URL`
- `WECHATPADPRO_ADMIN_KEY`

The exact endpoint paths and payload fields should be confirmed against the live `WeChatPadPro` instance during implementation, but the adapter boundary stays the same regardless of endpoint naming.

## Session Mapping

Session identity must remain platform-neutral and stable.

This design follows the corrected DogBot rule:

- one private chat maps to one session
- one group chat maps to one session
- group chats must not be split into per-sender sub-sessions

### Private chat

Use:

```text
wechatpadpro:private:<conversation-or-user-id>
```

### Group chat

Use:

```text
wechatpadpro:group:<group-id>
```

## Reply Semantics

### Private chat

- send plain text reply

### Group chat

- prefix reply text with a textual `@sender` form if WeChatPadPro's send API supports mentioning
- if true mention semantics are unavailable in the first pass, degrade to plain text prefix:

```text
@昵称 回复内容
```

This preserves addressability even before full capability mapping is implemented.

## Message Filtering

First version should only forward:

- plain text private messages
- plain text group messages

It should ignore:

- images
- files
- stickers
- unsupported system events

This keeps the first pass narrow and testable.

## Configuration

Add a new env/config surface for the adapter:

- `WECHATPADPRO_ADAPTER_BIND_ADDR`
- `WECHATPADPRO_ADAPTER_BASE_URL`
- `WECHATPADPRO_ADAPTER_SHARED_TOKEN`
- `WECHATPADPRO_BASE_URL`
- `WECHATPADPRO_ADMIN_KEY`
- `WECHATPADPRO_DEFAULT_CWD`
- `WECHATPADPRO_DEFAULT_TIMEOUT_SECS`
- `AGENT_RUNNER_BASE_URL`

## Error Handling

### Webhook receive errors

- malformed payload: `400`
- unsupported message type: `200` with ignored log
- missing sender/conversation metadata: `200` with ignored log

### `agent-runner` errors

- timeout -> send a short timeout message back to WeChat
- rate-limited / queue-full -> send busy message
- transport failure -> log error and optionally send generic failure notice

### WeChat send errors

- log the response body and HTTP status
- do not retry in v1

## Security

- adapter runs on host, not in the Claude container
- it does not hold model-provider keys
- it only holds WeChatPadPro admin credentials and runner base URL
- webhook endpoint should support a local shared token to avoid accidental unauthenticated posting

## Testing Strategy

### Automated

- unit tests for session-id derivation
- unit tests for text-message filtering
- unit tests for payload mapping into `/v1/runs`
- unit tests for outbound send payload shaping

### Manual

1. ensure `WeChatPadPro` is running
2. configure webhook to point at adapter
3. send a private WeChat text message
4. confirm adapter logs webhook receipt
5. confirm `agent-runner` receives `/v1/runs`
6. confirm text reply is sent back through `WeChatPadPro`
7. repeat in a group chat

## Migration Path

If later needed:

1. keep this adapter protocol stable
2. containerize the adapter
3. add media support
4. add capability negotiation
5. optionally unify proactive outbound flows with the same adapter
