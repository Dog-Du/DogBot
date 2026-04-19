# DogBot Control Plane Design

## Goal

Define a single control-plane architecture for DogBot that supports:

- long-term memory and content management
- skill and policy loading
- strict session and scope isolation across QQ and WeChat
- controlled proactive messaging
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
  - manages `memory`, `resource`, `skill`, `policy`, and `history_index`
  - resolves readable scopes
  - validates write permissions
  - assembles the context pack for each run
- `automation/outbox`
  - stores proactive messaging jobs
  - evaluates cron and internal-condition triggers
  - records delivery audit state and retries
- `delivery/rendering`
  - converts output for QQ and WeChat
  - handles mentions, replies, markdown degradation, and platform send adapters

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

The current adapters map group chats into per-user sub-sessions so short-term Claude context does not leak between senders. That behavior can remain.

The key design correction is:

- short-term continuity still uses `session_id -> claude_session_id`
- long-term memory never uses Claude session identity as its scope key
- all long-term writes and reads go through `scope resolver + permission policy`

This removes the current failure mode where Claude session isolation exists but explicit long-term memory writes still mix across users or groups.

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

- richer trigger recognition for QQ and WeChat
- non-prefix-only invocation rules where policy allows
- reply rendering pipeline for markdown degradation
- richer platform reply metadata such as mention, reply, and stylistic wrappers
- ingress policies that distinguish normal chat, slash commands, and protected actions

This phase improves usability and readability without changing the scope model.

### Phase C: History Ingestion, Storage, and Retrieval Injection

Deliver:

- message-history ingestion pipeline
- persistent message store and `history_index`
- retrieval APIs for the `context` subsystem
- policy-controlled history injection into runs

This phase gives the agent access to prior conversation material without treating all history as long-term memory.

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
- integration tests
  - end-to-end adapter -> runner -> context -> exec flow
  - memory write approval and rejection cases
  - scheduled job execution and delivery audit behavior
- contract tests
  - persistent object schemas
  - repository-loaded skill/resource manifests
  - history-index retrieval inputs and outputs

The most important regression targets are:

- one group cannot contaminate another group's shared memory
- one user cannot write another user's private memory
- Claude session reuse cannot change long-term scope selection
- proactive jobs cannot escape their authorized conversation

