#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

docker_log="$tmpdir/docker.log"
env_file="$tmpdir/dogbot.env"
runtime_state_dir="$tmpdir/agent-state"
mkdir -p "$tmpdir/bin" "$runtime_state_dir"

cat >"$env_file" <<EOF
ENABLE_QQ=1
ENABLE_WECHATPADPRO=1
APPLY_NETWORK_POLICY=0
AGENT_STATE_DIR=$runtime_state_dir
DOGBOT_COMPOSE_PROJECT_NAME=dogbot
EOF

cat >"$tmpdir/bin/docker" <<EOF
#!/usr/bin/env bash
set -euo pipefail

echo "docker \$*" >>"$docker_log"

if [[ "\${1:-}" == "compose" && "\${2:-}" == "version" ]]; then
  exit 0
fi

exit 0
EOF

chmod +x "$tmpdir/bin/docker"

run_case() {
  local case_name="$1"
  shift
  : >"$docker_log"

  PATH="$tmpdir/bin:$PATH" \
    "$repo_root/scripts/stop_stack.sh" "$@" "$env_file" >/dev/null

  local down_lines
  down_lines="$(grep ' down$' "$docker_log" || true)"

  case "$case_name" in
    keep-both)
      if grep -q 'deploy/docker/platform-stack.yml' <<<"$down_lines"; then
        echo "FAIL: --keep-qq must skip NapCat compose down" >&2
        cat "$docker_log" >&2
        exit 1
      fi
      if grep -q 'deploy/docker/wechatpadpro-stack.yml' <<<"$down_lines"; then
        echo "FAIL: --keep-wechat must skip WeChatPadPro compose down" >&2
        cat "$docker_log" >&2
        exit 1
      fi
      if ! grep -q 'deploy/docker/docker-compose.yml' <<<"$down_lines"; then
        echo "FAIL: stop_stack.sh must still stop claude-runner compose services" >&2
        cat "$docker_log" >&2
        exit 1
      fi
      ;;
    keep-qq)
      if grep -q 'deploy/docker/platform-stack.yml' <<<"$down_lines"; then
        echo "FAIL: --keep-qq must skip NapCat compose down" >&2
        cat "$docker_log" >&2
        exit 1
      fi
      if ! grep -q 'deploy/docker/wechatpadpro-stack.yml' <<<"$down_lines"; then
        echo "FAIL: stop_stack.sh must still stop WeChatPadPro when only --keep-qq is set" >&2
        cat "$docker_log" >&2
        exit 1
      fi
      ;;
  esac
}

run_case keep-both --keep-qq --keep-wechat
run_case keep-qq --keep-qq

echo "stop_stack keep-platform tests passed."
