# Deployment

## Files

- `myqqbot.env.example`: copy to `myqqbot.env` and edit values

## Quick Start

1. Copy the env template:

```bash
cp deploy/myqqbot.env.example deploy/myqqbot.env
```

2. Edit:

- `ANTHROPIC_BASE_URL`
- `NAPCAT_API_BASE_URL`
- host directories under `/srv/...`
- NapCat and AstrBot ports if needed

3. Start the full stack:

```bash
./scripts/deploy_stack.sh deploy/myqqbot.env
```

4. Stop the full stack:

```bash
./scripts/stop_stack.sh deploy/myqqbot.env
```

## What Gets Started

- local Rust `agent-runner`
- `claude-runner` Docker container
- `napcat` Docker container
- `astrbot` Docker container
- optional host firewall policy for the Claude container

## Proactive Message MVP

The first-pass proactive message path is host-local and session-based:

```bash
./scripts/send_session_message.sh \
  --env-file deploy/myqqbot.env \
  --session-id qq:private:123456 \
  --text "hello from cron"
```

Optional flags:

- `--reply-to <message_id>`
- `--mention-user <user_id>`

This path assumes the target `session_id` already exists in `agent-runner` SQLite state.
