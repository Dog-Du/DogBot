#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/common.sh
source "$repo_root/scripts/lib/common.sh"
scenario="${1:-success}"
tmp_root="$(mktemp -d)"
workspace_dir="$tmp_root/workspace"
state_dir="$tmp_root/state"
prompt_root="$state_dir/claude-prompt"
mock_ctx="$tmp_root/mock-claude-image"
runner_log="$tmp_root/agent-runner.log"
ingress_response="$tmp_root/ingress-response.json"
napcat_log="$tmp_root/napcat-requests.ndjson"
mock_claude_log="$state_dir/mock-claude-last.json"
container_name="claude-runner-e2e-$$"
base_image="${CLAUDE_IMAGE_NAME_BASE:-dogbot/claude-runner:local}"
mock_image="dogbot/claude-runner:e2e-$$"
mock_server_pid=""
runner_pid=""

reserve_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

runner_port="$(reserve_port)"
napcat_port="$(reserve_port)"

cleanup() {
  set +e
  if [[ -n "$runner_pid" ]] && kill -0 "$runner_pid" >/dev/null 2>&1; then
    kill "$runner_pid" >/dev/null 2>&1 || true
    wait "$runner_pid" 2>/dev/null || true
  fi
  if [[ -n "$mock_server_pid" ]] && kill -0 "$mock_server_pid" >/dev/null 2>&1; then
    kill "$mock_server_pid" >/dev/null 2>&1 || true
    wait "$mock_server_pid" 2>/dev/null || true
  fi
  docker rm -f "$container_name" >/dev/null 2>&1 || true
  docker rmi "$mock_image" >/dev/null 2>&1 || true
  rm -rf "$tmp_root"
}
trap cleanup EXIT

mkdir -p "$workspace_dir" "$state_dir" "$mock_ctx"
rsync -a "$repo_root/claude-prompt/" "$prompt_root/"
dogbot_write_claude_runner_runtime "$state_dir/claude-runner"

cat >"$tmp_root/mock_napcat_server.py" <<'PY'
#!/usr/bin/env python3
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--scenario", required=True)
    args = parser.parse_args()

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            length = int(self.headers.get("content-length", "0"))
            raw = self.rfile.read(length)
            body = json.loads(raw.decode("utf-8"))
            with open(args.log, "a", encoding="utf-8") as fh:
                fh.write(
                    json.dumps(
                        {"path": self.path, "body": body},
                        ensure_ascii=False,
                    )
                    + "\n"
                )

            if self.path == "/set_msg_emoji_like":
                response = {"status": "ok", "data": {}}
                status = 200
            elif args.scenario == "delivery_fail" and self.path == "/send_group_msg":
                response = {"status": "failed", "message": "mock send failure"}
                status = 500
            else:
                response = {"status": "ok", "data": {"message_id": 991}}
                status = 200

            encoded = json.dumps(response).encode("utf-8")
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def log_message(self, format, *args):
            return

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
PY
chmod +x "$tmp_root/mock_napcat_server.py"

cat >"$mock_ctx/mock-claude.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

python3 - "$@" <<'PY'
import json
import os
import pathlib
import sys

args = sys.argv[1:]
system_prompt = ""
add_dirs = []
session_mode = None
session_value = None
i = 0

while i < len(args):
    arg = args[i]
    if arg == "--append-system-prompt" and i + 1 < len(args):
        system_prompt = args[i + 1]
        i += 2
        continue
    if arg == "--add-dir":
        i += 1
        while i < len(args) and not args[i].startswith("--"):
            add_dirs.append(args[i])
            i += 1
        continue
    if arg in ("--session-id", "--resume") and i + 1 < len(args):
        session_mode = arg
        session_value = args[i + 1]
        i += 2
        continue
    i += 1

prompt = args[-1] if args else ""
payload = {
    "args": args,
    "system_prompt": system_prompt,
    "add_dirs": add_dirs,
    "session_mode": session_mode,
    "session_value": session_value,
    "prompt": prompt,
}
pathlib.Path("/state/mock-claude-last.json").write_text(
    json.dumps(payload, ensure_ascii=False, indent=2),
    encoding="utf-8",
)

errors = []

for needle in (
    "/state/claude-prompt/CLAUDE.md",
    "/state/claude-prompt/skills/reply-format/SKILL.md",
    "Do not use Markdown",
):
    if needle not in system_prompt:
        errors.append(f"missing system prompt guidance: {needle}")

if add_dirs != ["/workspace", "/state/claude-prompt"]:
    errors.append(f"unexpected add_dirs: {add_dirs!r}")

for required_path in (
    "/state/claude-prompt/CLAUDE.md",
    "/state/claude-prompt/persona.md",
    "/state/claude-prompt/skills/reply-format/SKILL.md",
):
    if not os.path.exists(required_path):
        errors.append(f"missing prompt file: {required_path}")

claude_md = pathlib.Path("/state/claude-prompt/CLAUDE.md").read_text(encoding="utf-8")
if "skills/reply-format/SKILL.md" not in claude_md:
    errors.append("CLAUDE.md does not reference reply-format skill")

for unexpected_path in (
    "/workspace/CLAUDE.md",
    "/workspace/persona.md",
    "/workspace/.claude",
):
    if os.path.exists(unexpected_path):
        errors.append(f"unexpected workspace prompt artifact: {unexpected_path}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(64)
PY

printf '%s\n' '容器链路正常'
printf '%s\n' '```dogbot-action'
printf '%s\n' '{"type":"reaction_add","target_message_id":"99","emoji":"👍"}'
printf '%s\n' '```'
EOF
chmod +x "$mock_ctx/mock-claude.sh"

cat >"$mock_ctx/Dockerfile" <<EOF
FROM ${base_image}
USER root
COPY mock-claude.sh /usr/local/bin/dogbot-mock-claude
RUN chmod +x /usr/local/bin/dogbot-mock-claude \\
 && ln -sfn /usr/local/bin/dogbot-mock-claude /usr/local/bin/claude \\
 && ln -sfn /usr/local/bin/dogbot-mock-claude /usr/bin/claude \\
 && chown claude:claude /usr/local/bin/dogbot-mock-claude
USER claude
EOF

if ! docker image inspect "$base_image" >/dev/null 2>&1; then
  docker build -t "$base_image" -f "$repo_root/deploy/docker/Dockerfile" "$repo_root" >/dev/null
fi

docker build -t "$mock_image" "$mock_ctx" >/dev/null

python3 "$tmp_root/mock_napcat_server.py" --port "$napcat_port" --log "$napcat_log" --scenario "$scenario" &
mock_server_pid=$!

if [[ -n "${SUDO_USER:-}" ]]; then
  su -l "$SUDO_USER" -c "export PATH=\"\$HOME/.cargo/bin:\$PATH\"; cd '$repo_root' && cargo build --release --manifest-path '$repo_root/agent-runner/Cargo.toml'" >/dev/null
else
  cargo build --release --manifest-path "$repo_root/agent-runner/Cargo.toml" >/dev/null
fi

env \
  RUST_LOG=warn \
  BIND_ADDR="127.0.0.1:$runner_port" \
  DEFAULT_TIMEOUT_SECS=30 \
  MAX_TIMEOUT_SECS=30 \
  CLAUDE_CONTAINER_NAME="$container_name" \
  CLAUDE_IMAGE_NAME="$mock_image" \
  AGENT_WORKSPACE_DIR="$workspace_dir" \
  AGENT_STATE_DIR="$state_dir" \
  DOGBOT_CLAUDE_PROMPT_ROOT="$prompt_root" \
  DOGBOT_CLAUDE_RUNNER_RUNTIME_DIR="$state_dir/claude-runner" \
  SESSION_DB_PATH="$state_dir/runner.db" \
  HISTORY_DB_PATH="$state_dir/history.db" \
  BIFROST_PORT=8080 \
  BIFROST_PROVIDER_NAME=primary \
  BIFROST_MODEL=primary/model-id \
  BIFROST_UPSTREAM_PROVIDER_TYPE=openai \
  BIFROST_UPSTREAM_BASE_URL=https://example.com \
  BIFROST_UPSTREAM_API_KEY=replace-me \
  ANTHROPIC_BASE_URL=http://127.0.0.1:8080/anthropic \
  ANTHROPIC_API_KEY=dummy \
  NAPCAT_API_BASE_URL="http://127.0.0.1:$napcat_port" \
  PLATFORM_QQ_ACCOUNT_ID=qq:bot_uin:123 \
  PLATFORM_QQ_BOT_ID=123 \
  PLATFORM_WECHATPADPRO_ACCOUNT_ID=wechatpadpro:account:bot \
  PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES=DogDu \
  MAX_CONCURRENT_RUNS=1 \
  MAX_QUEUE_DEPTH=1 \
  GLOBAL_RATE_LIMIT_PER_MINUTE=10 \
  USER_RATE_LIMIT_PER_MINUTE=10 \
  CONVERSATION_RATE_LIMIT_PER_MINUTE=10 \
  "$repo_root/agent-runner/target/release/agent-runner" \
  >"$runner_log" 2>&1 &
runner_pid=$!

for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:$runner_port/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

if ! curl -fsS "http://127.0.0.1:$runner_port/healthz" >/dev/null; then
  echo "FAIL: agent-runner did not become healthy" >&2
  cat "$runner_log" >&2 || true
  exit 1
fi

status_code="$(curl -sS -o "$ingress_response" -w '%{http_code}' \
  -X POST "http://127.0.0.1:$runner_port/v1/platforms/qq/napcat/events" \
  -H 'content-type: application/json' \
  --data-binary @- <<'JSON'
{
  "time": 1710000000,
  "post_type": "message",
  "message_type": "group",
  "group_id": 5566,
  "user_id": 42,
  "message_id": 99,
  "raw_message": "[CQ:at,qq=123] hello",
  "message": [
    {"type": "at", "data": {"qq": "123"}},
    {"type": "text", "data": {"text": " hello"}}
  ]
}
JSON
)"

expected_status="200"
if [[ "$scenario" == "delivery_fail" ]]; then
  expected_status="502"
fi

if [[ "$status_code" != "$expected_status" ]]; then
  echo "FAIL: ingress request returned HTTP $status_code, expected $expected_status" >&2
  cat "$ingress_response" >&2 || true
  cat "$runner_log" >&2 || true
  docker logs "$container_name" >&2 || true
  exit 1
fi

for _ in $(seq 1 50); do
  if [[ -f "$napcat_log" ]] && [[ "$(wc -l < "$napcat_log")" -ge 2 ]]; then
    break
  fi
  sleep 0.2
done

python3 - "$scenario" "$ingress_response" "$napcat_log" "$mock_claude_log" <<'PY'
import json
import pathlib
import sys

scenario = sys.argv[1]
ingress_response = pathlib.Path(sys.argv[2])
napcat_log = pathlib.Path(sys.argv[3])
mock_claude_log = pathlib.Path(sys.argv[4])

if not napcat_log.exists():
    raise SystemExit("missing NapCat request log")
if not mock_claude_log.exists():
    raise SystemExit("missing mock claude invocation log")

requests = [json.loads(line) for line in napcat_log.read_text(encoding="utf-8").splitlines() if line.strip()]
if len(requests) != 2:
    raise SystemExit(f"expected 2 platform requests, got {len(requests)}: {requests!r}")

first, second = requests
if first["path"] != "/set_msg_emoji_like":
    raise SystemExit(f"first request path mismatch: {first['path']!r}")
if first["body"].get("message_id") != 99 or first["body"].get("emoji_id") != "👍":
    raise SystemExit(f"unexpected reaction payload: {first['body']!r}")

if second["path"] != "/send_group_msg":
    raise SystemExit(f"second request path mismatch: {second['path']!r}")
expected_message = "[CQ:reply,id=99][CQ:at,qq=42] 容器链路正常"
if second["body"].get("message") != expected_message:
    raise SystemExit(f"unexpected outbound message: {second['body']!r}")

response_body = json.loads(ingress_response.read_text(encoding="utf-8"))
if scenario == "success":
    if response_body.get("status") != "ok":
        raise SystemExit(f"unexpected success response: {response_body!r}")
else:
    if response_body.get("error_code") != "delivery_failed":
        raise SystemExit(f"unexpected failure response: {response_body!r}")

invocation = json.loads(mock_claude_log.read_text(encoding="utf-8"))
if invocation.get("session_mode") not in ("--session-id", "--resume"):
    raise SystemExit(f"unexpected session mode: {invocation.get('session_mode')!r}")
if "Turn context (JSON):" not in invocation.get("prompt", ""):
    raise SystemExit("missing turn context in claude prompt")
if "User prompt:" not in invocation.get("prompt", ""):
    raise SystemExit("missing user prompt marker in claude prompt")
PY

echo "agent-runner docker e2e test passed for scenario: $scenario."
