---
name: history-read
description: Use when the agent needs prior DogBot text messages, chat history search, or answers that depend on earlier conversation context.
---

# History Read

Use this skill when the current answer depends on prior text messages. Do not guess or invent history; if no rows are returned, say that no matching history was found.

DogBot injects these environment variables when history is available:

- `DOGBOT_HISTORY_DATABASE_URL`
- `DOGBOT_HISTORY_RUN_TOKEN`

Use the helper:

```bash
python3 /state/claude-prompt/skills/history-read/history_query.py search --limit 20
python3 /state/claude-prompt/skills/history-read/history_query.py search --since "2026-04-27 00:00+08" --sender "qq:user:42"
python3 /state/claude-prompt/skills/history-read/history_query.py search --contains "关键词" --limit 20
python3 /state/claude-prompt/skills/history-read/history_query.py sql "select created_at, actor_id, plain_text from agent_read.messages order by created_at desc limit 20"
```

Search filters:

- `--since` and `--until`: PostgreSQL timestamp text.
- `--sender`: matches `actor_id` exactly or `actor_display` partially.
- `--contains`: case-insensitive substring search in `plain_text`.
- `--platform-account` and `--conversation`: useful only when the run has admin visibility.
- `--limit`: defaults to 20, maximum 200.

Access is enforced by PostgreSQL RLS. Normal runs can only see the current conversation. Admin private-chat runs may see the configured broader platform-account scope.
