#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
source "$script_dir/../lib/common.sh"

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"

  if [[ "$actual" != "$expected" ]]; then
    echo "FAIL: $message" >&2
    echo "expected: $expected" >&2
    echo "actual:   $actual" >&2
    exit 1
  fi
}

assert_eq "$dogbot_default_env_file" "$(dogbot_resolve_env_file)" \
  "no-arg resolution should return the default env file"
assert_eq "$dogbot_default_env_file" "$(dogbot_resolve_env_file "")" \
  "empty env-file override should fall back to the default env file"
assert_eq "/tmp/custom.env" "$(dogbot_resolve_env_file "/tmp/custom.env")" \
  "explicit env-file override should be preserved"

tmp_runtime_root="$(mktemp -d)"
runtime_launch_dir="$tmp_runtime_root/claude-runner"
dogbot_write_claude_runner_runtime "$runtime_launch_dir"

if [[ ! -x "$runtime_launch_dir/launch.sh" ]]; then
  echo "FAIL: dogbot_write_claude_runner_runtime must create an executable launch.sh" >&2
  exit 1
fi

if ! grep -q 'bifrost -host 127.0.0.1 -port "$port" -app-dir "$bifrost_dir"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must start bifrost" >&2
  exit 1
fi

if ! grep -q 'jq -n' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must materialize bifrost config at runtime" >&2
  exit 1
fi

if ! grep -q 'prompt_root="/state/claude-prompt"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must define the Claude prompt source root" >&2
  exit 1
fi

if ! grep -q 'project_root="/workspace"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must define the Claude project root" >&2
  exit 1
fi

if ! grep -q 'ensure_link "$prompt_root/CLAUDE.md" "$project_root/CLAUDE.md"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must project CLAUDE.md into /workspace" >&2
  exit 1
fi

if ! grep -q 'ensure_link "$prompt_root/persona.md" "$project_root/persona.md"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must project persona.md into /workspace" >&2
  exit 1
fi

if ! grep -q 'ensure_link "$prompt_root/.claude" "$project_root/.claude"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must project .claude into /workspace" >&2
  exit 1
fi

if ! grep -q 'default_model="${BIFROST_MODEL:-primary/model-id}"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must derive the default Bifrost model" >&2
  exit 1
fi

if ! grep -Fq 'stripped_model="${default_model#*/}"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must derive the provider-stripped model name" >&2
  exit 1
fi

if ! grep -Fq '[$default_model, $stripped_model]' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must emit an explicit Bifrost model allowlist" >&2
  exit 1
fi

if grep -q '"\*"' "$runtime_launch_dir/launch.sh"; then
  echo "FAIL: generated claude-runner launch.sh must not rely on wildcard Bifrost model matching" >&2
  exit 1
fi

rm -rf "$tmp_runtime_root"

port=$((20000 + $$ % 10000))
delayed_launcher_pid=""
listener_pid=""
delayed_writer_pid=""

cleanup() {
  if [[ -n "$listener_pid" ]] && kill -0 "$listener_pid" >/dev/null 2>&1; then
    kill "$listener_pid" >/dev/null 2>&1 || true
    wait "$listener_pid" 2>/dev/null || true
  fi

  if [[ -n "$delayed_launcher_pid" ]] && kill -0 "$delayed_launcher_pid" >/dev/null 2>&1; then
    kill "$delayed_launcher_pid" >/dev/null 2>&1 || true
    wait "$delayed_launcher_pid" 2>/dev/null || true
  fi

  if [[ -n "$delayed_writer_pid" ]] && kill -0 "$delayed_writer_pid" >/dev/null 2>&1; then
    kill "$delayed_writer_pid" >/dev/null 2>&1 || true
    wait "$delayed_writer_pid" 2>/dev/null || true
  fi
}

trap cleanup EXIT

(
  sleep 3
  exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1
) &
delayed_launcher_pid=$!

if ! dogbot_wait_for_http_ok "http://127.0.0.1:$port" 10; then
  echo "FAIL: delayed HTTP server should become ready before timeout" >&2
  exit 1
fi

listener_pid="$(dogbot_wait_for_listener_pid "$port" 10)"
if [[ -z "$listener_pid" ]]; then
  echo "FAIL: delayed listener should be detected before timeout" >&2
  exit 1
fi

if ! kill -0 "$listener_pid" >/dev/null 2>&1; then
  echo "FAIL: detected delayed listener pid is not alive" >&2
  exit 1
fi

timeout_start="$(date +%s)"
ready_file="$(mktemp)"
rm -f "$ready_file"
(
  sleep 2
  printf 'ready\n' >"$ready_file"
) &
delayed_writer_pid=$!

deadline_epoch="$(dogbot_deadline_in 5)"
if ! dogbot_wait_until_deadline "$deadline_epoch" test -f "$ready_file"; then
  echo "FAIL: dogbot_wait_until_deadline should succeed before the deadline" >&2
  exit 1
fi

elapsed=$(( $(date +%s) - timeout_start ))
if (( elapsed < 2 )); then
  echo "FAIL: dogbot_wait_until_deadline returned before the condition became true" >&2
  exit 1
fi

if dogbot_wait_until_deadline "$(dogbot_deadline_in 1)" false; then
  echo "FAIL: dogbot_wait_until_deadline should fail after deadline expiry" >&2
  exit 1
fi

wait "$delayed_writer_pid"
rm -f "$ready_file"

echo "common.sh env resolution tests passed."
