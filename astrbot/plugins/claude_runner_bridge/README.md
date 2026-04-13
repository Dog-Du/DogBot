# claude_runner_bridge

AstrBot plugin that forwards normal chat messages to the local Rust `agent-runner`.

## Commands

- Plain messages: routed to `agent-runner` by default
- `/agent <prompt>`: optional compatibility alias for explicit routing
- `/agent-status`: check the runner health endpoint

Other slash-prefixed commands are ignored so they can be handled elsewhere.

## Config

Configured through AstrBot plugin settings:

- `agent_runner_base_url`
- `default_cwd`
- `default_timeout_secs`
- `command_name`
- `status_command_name`

The plugin also supports these environment variable overrides, which are useful for Docker deployment without manual WebUI edits:

- `AGENT_RUNNER_BASE_URL`
- `CLAUDE_BRIDGE_DEFAULT_CWD`
- `CLAUDE_BRIDGE_TIMEOUT_SECS`
- `CLAUDE_BRIDGE_COMMAND_NAME`
- `CLAUDE_BRIDGE_STATUS_COMMAND_NAME`

## Session Mapping

- private chat conversation: prefer AstrBot's native `unified_msg_origin`
- private chat fallback: `<platform>:private:<session_id or user_id>`
- group chat conversation: prefer AstrBot's native `unified_msg_origin`
- group chat fallback: `<platform>:group:<group_id or session_id>`
- group chat session: `<conversation_id>:user:<user_id>`

This keeps the Rust runner platform-neutral while letting AstrBot choose the chat-specific session model. QQ group replies also prepend `@sender` automatically so the response is explicitly addressed to the triggering user.
