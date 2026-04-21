# DogBot Control Plane Design

## Goal

Define a single control-plane architecture for DogBot that supports:

- long-term memory and content management
- skill and policy loading
- strict session and scope isolation across QQ and WeChat
- controlled proactive messaging
- structured multi-platform ingress and reply rendering
- limited image handling with stronger outbound delivery than inbound understanding
- future history ingestion and retrieval

The design keeps the current runtime shape:

```text
QQ/WeChat Adapter -> agent-runner -> claude-runner
```

V1 does not add a new standalone service. All new control-plane logic is embedded inside `agent-runner`.

## Why This Shape

The current repository already has a stable split:

- adapters handle platform ingress
- `agent-runner` handles execution, session mapping, queueing, rate limiting, and message delivery
- `claude-runner` only runs the CLI agent inside Docker

Adding a separate context or scheduler service now would introduce new RPC, auth, consistency, deployment, and debugging overhead before the core model is stable. V1 should instead add a clear internal control-plane layer inside `agent-runner`.

## External Ideas Worth Borrowing

This design does not directly adopt an external framework, but it intentionally borrows useful ideas:

- OpenViking: unified management of `memory`, `resource`, and `skill`; layered context loading; session-end memory extraction
- LangGraph: separate short-lived thread state from long-lived namespace storage
- Letta: separate always-loaded memory blocks from retrieval-oriented archival memory
- Mem0: require structured scope filters rather than relying on natural-language memory writes
- Deep Agents memory guidance: shared memory should default to read-only or explicit approval to reduce prompt-injection risk

References:

- <https://github.com/volcengine/OpenViking>
- <https://docs.langchain.com/oss/python/concepts/memory>
- <https://docs.letta.com/guides/core-concepts/memory/memory-blocks>
- <https://docs.letta.com/guides/core-concepts/memory/archival-memory/>
- <https://docs.mem0.ai/platform/features/v2-memory-filters>
- <https://docs.langchain.com/oss/python/deepagents/memory>

## 与 codex-bridge 对齐的取舍说明

This design intentionally aligns with the parts of `codex-bridge` that improve chat experience directly:

- one private chat or one group maps to one short-lived session binding
- adapters emit structured inbound events instead of collapsing everything into raw text too early
- replies are rendered from a structured outbound model so mentions, quotes, text, and images can be combined cleanly
- image support aims for the same practical level as `codex-bridge`
  - outbound image sending is a first-class path
  - the runner may access images attached to the current message or a recent message window in the same conversation
  - inbound image understanding remains incomplete and is not treated as a guaranteed V1 capability

This design does not copy `codex-bridge` wholesale:

- DogBot still needs cross-platform identity, long-term memory scopes, and conversation-scoped history retrieval
- DogBot therefore keeps a richer control-plane than `codex-bridge`'s minimal runtime sqlite
- DogBot does not keep a separate long-lived image asset namespace just to support historical image resend
- instead, image handling is reduced to recent conversation attachments plus outbound send support

## Architecture

`agent-runner` gains an internal `DogBot control plane` with five subsystems:

- `ingress`
  - normalizes adapter requests
  - applies trigger recognition policy
  - builds canonical inbound event models
- `identity/session`
  - normalizes QQ and WeChat identities
  - resolves conversation identity
  - manages short-lived Claude session bindings
  - resolves admin whitelist membership
- `context`
  - manages `memory`, `resource`, `skill`, `policy`, and `history_index` metadata
  - manages conversation-scoped attachment descriptors needed for recent image access
  - resolves readable scopes
  - validates write permissions
  - assembles the context pack for each run
- `automation/outbox`
  - stores proactive messaging jobs
  - evaluates cron and internal-condition triggers
  - records delivery audit state and retries
- `delivery/rendering`
  - converts output for QQ and WeChat
  - handles mentions, replies, markdown degradation, structured segments, media sends, and platform send adapters

The existing `exec` path remains responsible only for invoking Claude Code CLI with the prepared prompt and context payload.

## Identity Model

Every inbound request is normalized into four core identities:

- `platform_account`
  - the concrete robot account instance on a platform
  - examples:
    - `qq:bot_uin:123456`
    - `wechatpadpro:account:wxid_bot_1`
- `actor`
  - the real human sender
  - examples:
    - `qq:user:9988`
    - `wechat:user:wxid_abcd`
- `conversation`
  - the concrete chat space where the message occurs
  - examples:
    - `qq:private:9988`
    - `qq:group:5566`
    - `wechat:private:wxid_abcd`
    - `wechat:group:123@chatroom`
- `session_binding`
  - the short-lived Claude session mapping used for conversational continuity

Important boundary:

- `session_binding` is not a memory scope
- `session_binding` is not a permission boundary
- `session_binding` only preserves short-term Claude conversation state

## Scope Model

V1 uses four long-term scopes:

- `user-private`
  - bound to `actor`
  - stores personal preferences and private long-term memory
- `conversation-shared`
  - bound to `conversation`
  - stores facts or conventions shared inside one chat space
- `platform-account-shared`
  - bound to `platform_account`
  - stores content shared by one bot account on one platform
- `bot-global-admin`
  - bound to the DogBot deployment
  - stores global administrator-owned content

### Read Order

The load order for one run is:

`short-term session state -> user-private -> conversation-shared -> platform-account-shared -> bot-global-admin -> history retrieval`

This order allows local context to override broader defaults without collapsing all content into one shared memory pool.

### Write Rules

Read scope and write scope are separate decisions. A run may read multiple scopes but only write to scopes allowed by policy.

## Permission Model

Administrator identity comes from a static configuration whitelist. Group admin or owner roles are not authority sources in V1.

Default write rules:

- `user-private`
  - only the owning actor may cause writes
- `conversation-shared`
  - only explicitly authorized users for that conversation may write
  - administrators may always write
- `platform-account-shared`
  - only administrators may write
- `bot-global-admin`
  - only administrators may write

For `conversation-shared`, normal users do not gain write access merely by saying "remember this". V1 treats shared memory as controlled shared state, not an open graffiti wall.

## Agent Write Policy

V1 uses a conservative write model:

- the agent may propose write intents
- `memory` may be auto-committed only when a specific policy rule allows it
- every committed `memory` record must retain source metadata
- `resource`, `skill`, and `policy` are not modified by the agent at runtime
- `resource` and `skill` remain repository-managed and deployment-published

This keeps agent autonomy useful for personal memory without allowing runtime mutation of global behavior or capabilities.

## Content Object Model

The control plane manages five persistent object kinds:

- `memory`
  - short, structured, durable facts
- `resource`
  - larger reusable content blocks such as guides, templates, and knowledge cards
- `skill`
  - deployable capability definitions loaded from the repository
- `policy`
  - behavior, permission, trigger, and formatting rules
- `history_index`
  - indexed message-history fragments for retrieval, not automatically promoted to memory

All persistent objects share common metadata:

- `id`
- `kind`
- `scope_kind`
- `scope_id`
- `created_at`
- `updated_at`
- `created_by_actor`
- `source_platform`
- `source_conversation`
- `source_message_id`
- `status`

`memory` additionally carries:

- `summary`
- `raw_evidence`
- `confidence`
- `extraction_rule`

`history_index` additionally carries:

- `time_range`
- `message_count`
- `retrieval_terms`

## Core Data Flows

### Inbound Run Flow

1. Adapter sends normalized platform message to `agent-runner`.
2. `ingress` produces a canonical event.
3. `identity/session` resolves `platform_account`, `actor`, `conversation`, and `session_binding`.
4. `context` resolves readable scopes and assembles the context pack.
5. `exec` invokes Claude Code with short-term session state plus the resolved long-term context.
6. `delivery/rendering` converts output into platform-safe reply text and delivery metadata.

### Memory Write Flow

1. Agent output or explicit user instruction produces a structured memory write intent.
2. `context` resolves target scope from `actor`, `conversation`, `platform_account`, and policy.
3. Permission checks run before any write.
4. If the write is allowed by policy, the memory record is stored with source metadata.
5. If not allowed, the write becomes a candidate record or is rejected.

### Proactive Automation Flow

1. Agent or user action creates a structured automation intent.
2. `automation/outbox` validates authorization and target conversation.
3. A persistent job is stored with trigger, action, ownership, and audit fields.
4. An internal scheduler loop evaluates cron and internal-condition triggers.
5. When triggered, the runner either:
   - sends a templated message directly, or
   - launches a fresh Claude run to generate the final outbound text
6. `delivery/rendering` sends the final message and stores delivery outcome.

### History Ingestion Flow

1. Adapter receives a message event for a conversation with history capture enabled.
2. `ingress` normalizes the message into a canonical inbound model.
3. `context` persists the raw message shell, normalized text, reply linkage, and attachment metadata.
4. A background indexer updates `history_index` entries for retrieval.
5. The next `/agent` run may request history evidence scoped to the same conversation.

### Image Send Flow

1. Agent output or an automation job produces a structured media-send intent.
2. `delivery/rendering` validates that the intent targets an authorized conversation.
3. If the source is remote, the runner downloads and validates the file into controlled storage.
4. If the source points at a recent inbound attachment, the runner resolves it only inside the same conversation and within the configured retention window.
5. The platform delivery client uploads or sends the image using the appropriate QQ or WeChat API.

## Automation and Outbox Model

V1 supports:

- scheduled reminders
- internal-condition-triggered notifications

V1 does not support:

- direct agent-held platform send permissions
- external webhook-triggered jobs

Each automation job includes:

- `job_id`
- `job_owner_actor`
- `target_conversation`
- `target_platform_account`
- `authorization_scope`
- `trigger_type`
- `trigger_definition`
- `action_type`
- `action_payload`
- `status`
- `last_run_at`
- `next_run_at`
- `audit_log`

Authorization rules:

- normal users may only create jobs for their current conversation
- normal users may not create jobs targeting unrelated conversations
- normal users may not create cross-platform jobs
- administrators may create platform-account-level jobs
- only administrators may create deployment-global jobs

## Relationship to Current Session Mapping

The current adapters map group chats into per-user sub-sessions. This design treats that as a mistake and removes it.

The corrected rule is:

- private chat: one conversation maps to one `session_id`
- group chat: one group conversation maps to one `session_id`
- in practice, `session_id` should equal the stable `conversation_id`
- short-term continuity still uses `session_id -> claude_session_id`
- sender identity remains explicit through `actor_id`, reply metadata, queueing, and permission policy
- long-term memory never uses Claude session identity as its scope key
- all long-term writes and reads go through `scope resolver + permission policy`

This matches the conversation-level session model used by `codex-bridge` and avoids the current split where one group is artificially fragmented into many unrelated short-term sessions.

## Implementation Phases

### Phase A: Memory, Content, Skill, Permission, Session Isolation

Deliver:

- canonical identity model
- scope resolver
- permission policy engine
- persistent object store for `memory`, `resource`, `skill`, `policy`
- repository-driven skill and resource loading
- conservative memory write pipeline

This phase solves the core isolation and memory-mixing problem.

### Phase B: Trigger Recognition, Reply Rendering, Platform Interaction Details

Deliver:

- unified trigger resolver for QQ and WeChat
- structured adapter contract for QQ, WeChatPadPro, and future third-party platforms
- non-starting-position `/agent` recognition after message normalization
- reply-aware and mention-aware trigger gating in group chats
- reply rendering pipeline for markdown degradation and structured outbound segments
- richer platform reply metadata such as mention, reply, stylistic wrappers, and outbound image actions
- ingress policies that distinguish normal chat, slash commands, and protected actions

This phase improves usability and readability without changing the scope model.

### Phase C: History Ingestion, Storage, and Retrieval Injection

Deliver:

- message-history ingestion pipeline
- persistent message store, attachment metadata, and `history_index`
- per-conversation enablement and retention policy
- QQ limited backfill plus realtime mirror
- WeChat realtime mirror after enablement
- retrieval APIs for the `context` subsystem
- policy-controlled history injection into runs

This phase gives the agent access to prior conversation material without treating all history as long-term memory.

## Phase B Detailed Design

### Trigger Resolver

Phase B introduces a unified `trigger resolver` inside `agent-runner`.

Adapters still parse platform-native payloads, but they must emit a canonical `InboundMessage` model with:

- `platform`
- `platform_account`
- `conversation_id`
- `actor_id`
- `message_id`
- `reply_to_message_id`
- `raw_segments`
- `normalized_text`
- `mentions`
- `is_group`
- `is_private`
- `timestamp`

The resolver makes the final trigger decision. This removes the current split where QQ and WeChat implement separate command-matching behavior with inconsistent capabilities.

### Structured Outbound Model

`delivery/rendering` should no longer accept plain text as its only output contract.

The canonical outbound shape is one `OutboundMessage` containing:

- `platform`
- `conversation_id`
- `reply_to_message_id`
- `mention_targets`
- `segments`

`segments` may contain:

- `text`
- `reply_ref`
- `mention_actor`
- `image`

Platform adapters remain responsible for degrading unsupported segment types safely, but QQ/NapCat, WeChatPadPro, and future adapters must all target the same canonical structure first.

### Message Normalization

Trigger resolution is based on `normalized_text`, not the raw platform string.

QQ normalization rules:

- strip CQ wrappers from `at`, `reply`, and other control segments
- preserve `reply_to_message_id` separately
- preserve `mentions` separately
- keep only human-readable text in `normalized_text`

WeChat normalization rules:

- unwrap transport-prefixed payloads
- strip textual group-mention prefixes
- preserve reply linkage where available
- keep only the normalized message body in `normalized_text`

This enables `/agent` to be recognized after normalization even when it does not appear at the raw string start.

### Trigger Policy

V1 keeps explicit invocation protection.

Private chat trigger:

- `/agent` must appear in `normalized_text`
- `/agent-status` remains a separate control path
- natural conversation without `/agent` never triggers execution

Group chat trigger:

- `/agent` must appear in `normalized_text`
- and one of the following must also be true:
  - the bot is mentioned
  - the message replies to a bot-authored message

Messages that must not trigger:

- messages without `/agent`
- group messages with `/agent` but without mention or bot-reply linkage
- obvious control or system messages
- bot self-echo messages

### Minimal Provenance Store

Phase B adds a minimal provenance store so reply-based triggers can reliably detect whether a referenced message came from the bot.

Each entry stores:

- `message_id`
- `platform`
- `platform_account`
- `conversation_id`
- `sender_actor`
- `sender_role`
- `created_at`

This provenance store is intentionally small and becomes the seed for Phase C history storage.
It may be implemented as a dedicated lightweight table first and then folded into `message_store` once Phase C lands.

### Rendering Pipeline

Claude output no longer flows directly to the platform.

The rendering pipeline performs:

1. output classification
2. markdown degradation
3. reply and mention metadata injection
4. media action extraction
5. final platform payload assembly

### Markdown Degradation

V1 targets readability, not full markdown fidelity.

Degradation rules:

- headings become plain paragraphs with spacing
- bullet and numbered lists retain list markers
- emphasis markers are removed while preserving text
- code blocks retain body text but drop language labels
- inline code remains plain or lightly wrapped
- links degrade to text plus URL
- tables degrade to key-value style line groups
- quotes degrade to plain prefixed text

If degradation fails, the renderer falls back to plain text instead of blocking delivery.

### Platform Reply Metadata

QQ defaults:

- group replies keep reply metadata when available
- group replies mention the requesting actor by default
- private replies are plain text unless explicit reply metadata exists

WeChat defaults:

- group replies keep `AtWxIDList` when available
- group replies may prefix the sender nickname in text
- private replies remain plain text

### Outbound Image Actions

V1 supports outbound image sending but not full visual understanding.

Agent or automation output may produce:

- `send_image`
- `send_text_with_image`

Each action must include:

- `source_type`
  - `remote_url`
  - `recent_attachment`
  - `local_generated_file`
- `source_value`
- `caption_text`
- `target_conversation`

Runner responsibilities:

- download and validate remote files into controlled storage
- resolve recent conversation attachments without promoting them into a long-lived asset library
- enforce conversation-target authorization
- send the image through the platform delivery client

V1 does not allow the agent to call platform media APIs directly.
V1 also does not guarantee OCR, captioning, or general-purpose image understanding for inbound images.

### Phase B Failure and Degradation Rules

- if reply metadata is missing, delivery falls back to plain reply without quote linkage
- if mention metadata is missing, delivery falls back to plain text
- if image download fails, the user receives a text error instead of silent failure
- if a platform image send path is unavailable, delivery falls back to text plus a safe link when possible

## Phase C Detailed Design

### History Capture Scope

History is not global by default. V1 captures full message history only for conversations that are enabled.

An enabled conversation is:

- a private chat that has seen at least one valid `/agent` request, or
- a group chat that has seen at least one valid `/agent` request, or
- a conversation explicitly enabled by an administrator

Default retention is `180 days`, with per-conversation overrides for administrators.

### Ingestion Strategy

QQ strategy:

- enable realtime mirror for all messages in enabled conversations
- support limited backfill when a conversation becomes enabled
- backfill writes history records only and does not trigger agent execution

WeChat strategy:

- enable realtime mirror for all webhook-delivered messages in enabled conversations
- V1 does not depend on a WeChat history backfill API

### Storage Model

Phase C extends the local state store with four primary tables or collections:

- `message_store`
- `message_attachment`
- `history_index`
- `conversation_ingest_state`

`message_store` includes:

- `message_id`
- `platform`
- `platform_account`
- `conversation_id`
- `actor_id`
- `sender_role`
- `reply_to_message_id`
- `normalized_text`
- `raw_text`
- `message_type`
- `created_at`
- `ingested_at`
- `deleted_at`
- `retention_expires_at`

`message_attachment` includes:

- `attachment_id`
- `message_id`
- `attachment_type`
- `platform_file_id`
- `mime_type`
- `file_name`
- `size_bytes`
- `storage_path`
- `source_url`
- `sha256`
- `width`
- `height`
- `download_status`
- `availability_status`
- `created_at`

`conversation_ingest_state` includes:

- `conversation_id`
- `enabled`
- `enabled_at`
- `retention_days`
- `last_backfill_cursor`
- `last_realtime_message_at`
- `sync_status`

### Retrieval Model

History retrieval is evidence-oriented and conversation-scoped.

Each `/agent` run may assemble a `history evidence pack` from four layers:

1. `anchor layer`
   - current message
   - replied-to message
   - referenced bot message when relevant
2. `recent window`
   - recent messages from the same conversation
3. `targeted retrieval`
   - keyword or FTS retrieval inside the same conversation
4. `attachment stubs`
   - lightweight attachment presence notes, not binary payloads

V1 should start with SQLite indexing plus FTS5, time filters, and reply anchoring. Embedding-based retrieval is deferred.

### History and Memory Boundary

History remains evidence, not truth.

Hard rules:

- repeated appearance in history does not auto-promote content into memory
- group history summaries are not auto-written into `conversation-shared` memory
- one conversation's history is never injected into another conversation
- private chat history is never injected into a group run

Allowed rule:

- a user may explicitly request promotion of a conclusion into memory, and the resulting memory record must reference source message identifiers

### Attachment Handling

V1 stores enough image attachment metadata to support current-message or recent-window image access and outbound sending, but does not build a long-lived image asset library and does not perform OCR, captioning, or complete image understanding.

Inbound image behavior:

- persist message-to-attachment linkage
- optionally download to controlled storage when policy allows
- keep the attachment out of text retrieval content

Retrieval behavior for messages with images:

- inject only a stub such as "message contains image attachment"
- expose a recent attachment handle only for the same conversation and within the allowed retention window

### Retention and Cleanup

Default retention:

- enabled conversation history is retained for `180 days`
- administrators may override retention per conversation

Cleanup requirements:

- TTL-based deletion for expired messages and index rows
- per-conversation manual purge
- per-private-chat purge
- delete expired attachment downloads alongside their message records

### Phase C Failure Handling

- history ingestion failure must not block the live reply path
- history retrieval failure may degrade to a no-history run
- backfill failure must not disable realtime mirror
- attachment download failure must preserve the message shell with an attachment error state

### Phase C Test Priorities

- one group's history is never retrievable from another group
- private history is never injected into group runs
- reply-anchor retrieval returns the referenced message
- QQ backfill and realtime mirror do not duplicate records
- WeChat dedupe does not drop legitimate messages
- attachment-bearing messages are stored without polluting text retrieval
- `send_image(recent_attachment=...)` enforces same-conversation and retention-window boundaries

## Error Handling

Control-plane failures are explicit and typed. V1 must surface at least:

- `permission_denied`
- `invalid_scope`
- `session_conflict`
- `unsupported_target`
- `job_not_authorized`
- `delivery_failed`
- `rendering_failed`
- `context_pack_failed`
- `history_ingest_failed`
- `history_retrieval_failed`
- `attachment_download_failed`
- `attachment_not_available`

Error handling rules:

- permission failures never silently fall back to broader shared scopes
- write failures never retry into a different scope
- delivery failures update job audit state and may trigger bounded retries
- malformed automation definitions are rejected at creation time
- rendering failures degrade to plain text where possible

## Testing Strategy

V1 testing is split by boundary:

- unit tests
  - identity normalization
  - scope resolution
  - permission decisions
  - automation authorization
  - markdown-to-plain-text degradation
  - trigger resolution
  - history evidence-pack assembly
- integration tests
  - end-to-end adapter -> runner -> context -> exec flow
  - memory write approval and rejection cases
  - scheduled job execution and delivery audit behavior
  - reply-trigger recognition across QQ and WeChat
  - enabled-conversation history mirror behavior
  - outbound image send action execution
- contract tests
  - persistent object schemas
  - repository-loaded skill/resource manifests
  - history-index retrieval inputs and outputs
  - structured reply and image action payload schemas

The most important regression targets are:

- one group cannot contaminate another group's shared memory
- one user cannot write another user's private memory
- Claude session reuse cannot change long-term scope selection
- proactive jobs cannot escape their authorized conversation
- group `/agent` invocation cannot bypass mention-or-reply gating
- history retrieval cannot cross conversation boundaries
- recent inbound images cannot be sent outside the authorized target conversation or retention window
