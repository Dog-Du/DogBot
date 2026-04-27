# DogBot Postgres History Read Skill Design

Date: 2026-04-27

## Summary

DogBot will move history and session persistence to PostgreSQL only. Existing
SQLite history data is not important and does not need compatibility migration.

The history read capability is exposed to Claude as a repository-managed skill.
The skill reads a short-lived run token and PostgreSQL connection settings from
environment variables, then queries a read-only `agent_read.messages` view.
Claude may also use `psql` directly for complex analysis.

Isolation is enforced in PostgreSQL with RLS, not by a history HTTP API. A fixed
read-only database login role is shared by all agent runs. Each run receives a
high-entropy token valid for 30 minutes. The token maps to one or more read
grants in PostgreSQL. RLS checks the token hash for every row.

## Goals

- Replace SQLite-backed history with PostgreSQL.
- Keep the history model small: exactly two physical history tables.
- Let agents read and filter history by time, sender, content, platform, and
  conversation.
- Enforce ordinary conversation isolation at the database layer.
- Allow admin private-chat runs to read across authorized conversations.
- Avoid a custom history API in V1.
- Keep image persistence out of scope.

## Non-Goals

- No migration from existing `history.db` or `runner.db`.
- No image, voice, video, or file storage for historical retrieval.
- No semantic/vector index in V1.
- No per-run database role creation.
- No agent write access to history.

## Architecture

Runtime flow:

```text
platform event
-> agent-runner normalizes canonical message
-> agent-runner writes text history to Postgres
-> agent-runner inserts one or more read grants for the run token
-> agent-runner starts Claude with history env vars
-> Claude uses the history skill or psql
-> PostgreSQL RLS filters agent_read.messages rows
```

The trust boundary is PostgreSQL:

- `agent-runner` uses a writer/admin connection for ingestion and grant inserts.
- Claude uses the fixed `dogbot_agent_reader` login role.
- Claude receives a run token, but not platform/conversation authority.
- RLS derives readable scope from `history_read_grants`, not from agent-supplied
  SQL filters.
- If the agent omits `conversation_id` filters, RLS still restricts rows.
- If the token is missing, expired, or guessed incorrectly, reads return zero
  rows.

## PostgreSQL Deployment

DogBot should ship a PostgreSQL 15+ container in `deploy/docker/`, preferably in
the platform/runtime compose stack rather than requiring a system database.

New configuration should replace SQLite paths:

```env
POSTGRES_HOST=127.0.0.1
POSTGRES_PORT=5432
POSTGRES_DB=dogbot
POSTGRES_ADMIN_USER=dogbot_admin
POSTGRES_ADMIN_PASSWORD=change-me
POSTGRES_AGENT_READER_USER=dogbot_agent_reader
POSTGRES_AGENT_READER_PASSWORD=change-me-reader
HISTORY_RETENTION_DAYS=180
HISTORY_RUN_TOKEN_TTL_SECS=1800
```

`SESSION_DB_PATH` and `HISTORY_DB_PATH` become obsolete.

## Physical Tables

PostgreSQL contains four DogBot-owned physical tables:

- two session tables
- two history tables

### Session Tables

`runner_sessions` keeps the current conversation-to-Claude-session mapping:

```sql
CREATE TABLE runner_sessions (
    session_key text PRIMARY KEY,
    claude_session_id text NOT NULL,
    platform text NOT NULL,
    platform_account text NOT NULL,
    conversation_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX runner_sessions_identity_idx
    ON runner_sessions (platform, platform_account, conversation_id);
```

`runner_session_aliases` preserves the existing external `session_id` alias
contract:

```sql
CREATE TABLE runner_session_aliases (
    external_session_id text PRIMARY KEY,
    session_key text NOT NULL REFERENCES runner_sessions(session_key)
);
```

These tables replace the current SQLite `sessions` and `session_aliases` tables.

### History Tables

`history_messages` is the only message storage table:

```sql
CREATE TABLE history_messages (
    id bigserial PRIMARY KEY,
    platform text NOT NULL,
    platform_account text NOT NULL,
    conversation_id text NOT NULL,
    chat_type text NOT NULL,
    message_id text NOT NULL,
    actor_id text NOT NULL,
    actor_display text,
    plain_text text NOT NULL,
    reply_to_message_id text,
    created_at timestamptz NOT NULL,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    raw jsonb
);

CREATE UNIQUE INDEX history_messages_unique_msg_idx
    ON history_messages (platform_account, conversation_id, message_id);

CREATE INDEX history_messages_conversation_time_idx
    ON history_messages (platform_account, conversation_id, created_at DESC);

CREATE INDEX history_messages_actor_time_idx
    ON history_messages (platform_account, actor_id, created_at DESC);

CREATE INDEX history_messages_platform_account_idx
    ON history_messages (platform_account);
```

`history_read_grants` stores short-lived token scopes:

```sql
CREATE TABLE history_read_grants (
    id bigserial PRIMARY KEY,
    token_hash bytea NOT NULL,
    platform_account text NOT NULL,
    conversation_id text,
    actor_id text NOT NULL,
    is_admin boolean NOT NULL DEFAULT false,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX history_read_grants_token_idx
    ON history_read_grants (token_hash);

CREATE INDEX history_read_grants_expiry_idx
    ON history_read_grants (expires_at);

CREATE INDEX history_read_grants_platform_account_idx
    ON history_read_grants (platform_account);
```

Normal runs insert one grant row with `conversation_id` set. Admin private-chat
runs insert one admin grant row per authorized platform account, with
`conversation_id = NULL`.

## Read Surface

`agent_read` is a schema of views/functions, not a physical history table.

The primary view is:

```sql
CREATE VIEW agent_read.messages
WITH (security_barrier = true)
AS
SELECT
    id,
    platform,
    platform_account,
    conversation_id,
    chat_type,
    message_id,
    actor_id,
    actor_display,
    plain_text,
    reply_to_message_id,
    created_at
FROM history_messages;
```

The agent reader role gets:

- `USAGE` on schema `agent_read`
- `SELECT` on `agent_read.messages`
- no privileges on `history_messages`
- no privileges on `history_read_grants`
- no privileges on session tables

To avoid giving the agent base-table privileges, `agent_read.messages` should be
owned by a dedicated view-owner role that:

- can select from `history_messages`
- does not own `history_messages`
- does not have `BYPASSRLS`
- is covered by the same RLS policy

This matters because PostgreSQL view permission checks normally use the view
owner for underlying tables unless a `security_invoker` view is used. The V1
design chooses a non-bypass view owner plus RLS, because the agent should only
hold view privileges.

## RLS Design

`history_messages` enables and forces RLS:

```sql
ALTER TABLE history_messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE history_messages FORCE ROW LEVEL SECURITY;
```

RLS calls a security-definer function owned by the database owner. The function
checks `history_read_grants` without granting the agent or view owner direct
access to that table.

Sketch:

```sql
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE FUNCTION dogbot_can_read_history_row(
    row_platform_account text,
    row_conversation_id text
)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM history_read_grants g
        WHERE g.token_hash = digest(
            current_setting('dogbot.run_token', true),
            'sha256'
        )
          AND g.expires_at > now()
          AND g.platform_account = row_platform_account
          AND (
              g.is_admin
              OR g.conversation_id = row_conversation_id
          )
    );
$$;

CREATE POLICY history_messages_agent_read
ON history_messages
FOR SELECT
USING (dogbot_can_read_history_row(platform_account, conversation_id));
```

The token is a bearer credential for database reads. It is high entropy, stored
only as a hash in PostgreSQL, and valid for 30 minutes. The agent can reuse its
own token during the run, but cannot derive another run's token.

## Admin Scope

Admin authority comes from static DogBot configuration, not from platform group
roles.

V1 issues broad history grants only when:

- the actor is in the configured admin whitelist
- the current chat is private

An admin run can query across authorized conversations by adding SQL filters
such as `platform_account`, `conversation_id`, `actor_id`, or time windows. RLS
still limits rows to platform accounts for which `agent-runner` inserted admin
grant rows.

Admin group-chat runs receive ordinary current-conversation grants. This avoids
accidentally exposing unrelated chats into a group context.

## Agent Skill

Add a first-party Claude skill:

```text
claude-prompt/skills/history-read/
  SKILL.md
  history_query.py
```

The skill teaches two usage paths.

Common search path:

```bash
python3 /state/claude-prompt/skills/history-read/history_query.py search \
  --since "2026-04-26 00:00" \
  --sender "qq:user:123" \
  --contains "部署" \
  --limit 20
```

SQL path:

```bash
python3 /state/claude-prompt/skills/history-read/history_query.py sql \
  "select created_at, actor_display, plain_text
   from agent_read.messages
   where plain_text ilike '%部署%'
   order by created_at desc
   limit 20"
```

The helper script reads environment variables and sets PostgreSQL options. It
does not ask the agent to pass platform or conversation identity.

Expected environment:

```env
DOGBOT_HISTORY_DATABASE_URL=postgres://dogbot_agent_reader:...@host:5432/dogbot
DOGBOT_HISTORY_RUN_TOKEN=<high-entropy-token>
PGOPTIONS=-c dogbot.run_token=<token> -c statement_timeout=5000
```

Direct `psql` is also supported for complex work:

```bash
psql "$DOGBOT_HISTORY_DATABASE_URL" \
  -v ON_ERROR_STOP=1 \
  -c "select * from agent_read.messages order by created_at desc limit 10"
```

The database role itself should be read-only and have a default
`statement_timeout`, so safety does not depend entirely on the helper script.

## Runtime Injection

`agent-runner` changes the run preparation flow:

1. Build normal `RunRequest` from the canonical event.
2. Generate a random run token.
3. Hash it with SHA-256.
4. Insert grant rows into `history_read_grants`.
5. Start Claude with history env vars injected into the Docker exec.
6. Do not delete grants synchronously when the run finishes.

Grant insertion rules:

- ordinary run: one row for current `platform_account + conversation_id`
- admin private run: one row per configured admin-readable platform account
- token TTL: 30 minutes

The Docker exec path needs per-exec environment support. The container-level
environment should not contain a long-lived history token.

## Ingestion Rules

History ingestion writes text messages into `history_messages`.

V1 behavior:

- private chats: store non-empty text messages once the platform event reaches
  `agent-runner`
- group chats: store non-empty text messages that `agent-runner` receives
- images and other media are not saved
- message IDs are idempotent per `platform_account + conversation_id`
- raw native payload is optional and should be omitted or redacted if it grows
  too large

Backfill remains platform-specific:

- QQ may continue to do limited backfill later, but it writes the same table.
- WeChat remains realtime-only unless a reliable backfill source appears.

## Cleanup

No per-run role cleanup is needed because V1 does not create per-run roles.

Required cleanup tasks:

- delete expired rows from `history_read_grants`
- delete old rows from `history_messages` using global `HISTORY_RETENTION_DAYS`

Cleanup can run:

- on `agent-runner` startup
- periodically inside `agent-runner`
- or via a small admin script

Expired grants do not authorize reads even before cleanup because RLS checks
`expires_at > now()`.

## Testing

Unit and integration tests should cover:

- session creation and alias lookup using Postgres tables
- history message insert idempotency
- ordinary token can read current conversation rows
- ordinary token cannot read another conversation in the same table
- ordinary token cannot read another platform account
- admin private token can read multiple authorized conversations
- admin group token stays current-conversation scoped
- expired token returns zero rows
- missing token returns zero rows
- `agent_read.messages` exposes no `raw` or grant fields
- fixed reader role cannot insert/update/delete
- fixed reader role cannot select session tables or grant table

## References

- PostgreSQL 15 `CREATE VIEW` documentation: `security_invoker` and view
  permission behavior.
- PostgreSQL `CREATE POLICY` and row security documentation: RLS policies
  restrict visible rows and default to deny when no policy permits access.
