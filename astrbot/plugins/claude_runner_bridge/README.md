# claude_runner_bridge

AstrBot plugin that forwards chat commands to the local Rust `agent-runner`.

## Commands

- `/agent <prompt>`: send a prompt to `agent-runner`
- `/agent-status`: check the runner health endpoint

## Config

Configured through AstrBot plugin settings:

- `agent_runner_base_url`
- `default_cwd`
- `default_timeout_secs`
- `command_name`
- `status_command_name`

## Session Mapping

- private chat: `<platform>:private:<user_id>`
- group chat conversation: `<platform>:group:<group_id>`
- group chat session: `<platform>:group:<group_id>:user:<user_id>`

This keeps the Rust runner platform-neutral while letting AstrBot choose the chat-specific session model.
