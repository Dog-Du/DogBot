#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="${DOGBOT_ENV_FILE:-}"
if [[ -z "$env_file" ]]; then
  env_file="$repo_root/deploy/dogbot.env"
fi

usage() {
  cat <<'EOF'
Usage:
  send_session_message.sh --session-id <id> --text <message> [--reply-to <message_id>] [--mention-user <user_id>] [--env-file <path>]

This is the first-pass proactive messaging entrypoint.
It sends a message into an existing session through agent-runner.
EOF
}

session_id=""
text=""
reply_to=""
mention_user=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session-id)
      session_id="${2:-}"
      shift 2
      ;;
    --text)
      text="${2:-}"
      shift 2
      ;;
    --reply-to)
      reply_to="${2:-}"
      shift 2
      ;;
    --mention-user)
      mention_user="${2:-}"
      shift 2
      ;;
    --env-file)
      env_file="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$session_id" || -z "$text" ]]; then
  usage >&2
  exit 1
fi

if [[ -f "$env_file" ]]; then
  set -a
  source "$env_file"
  set +a
fi

agent_runner_base_url="${AGENT_RUNNER_BASE_URL_LOCAL:-http://127.0.0.1:8787}"
payload="$(mktemp)"
trap 'rm -f "$payload"' EXIT

uv run python - "$payload" "$session_id" "$text" "$reply_to" "$mention_user" <<'PY'
import json
import pathlib
import sys

payload_path, session_id, text, reply_to, mention_user = sys.argv[1:]
payload = {
    "session_id": session_id,
    "text": text,
    "reply_to_message_id": reply_to or None,
    "mention_user_id": mention_user or None,
}
pathlib.Path(payload_path).write_text(json.dumps(payload), encoding="utf-8")
PY

curl --fail-with-body \
  --silent \
  --show-error \
  -H 'content-type: application/json' \
  -X POST \
  "$agent_runner_base_url/v1/messages" \
  --data @"$payload"
