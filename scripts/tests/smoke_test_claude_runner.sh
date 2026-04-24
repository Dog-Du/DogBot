#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
# shellcheck source=../lib/common.sh
source "$repo_root/scripts/lib/common.sh"

resolve_uv_bin() {
  if [[ -n "${UV_BIN:-}" ]]; then
    printf '%s\n' "$UV_BIN"
    return 0
  fi

  if command -v uv >/dev/null 2>&1; then
    command -v uv
    return 0
  fi

  if [[ -n "${SUDO_USER:-}" ]]; then
    su -l "$SUDO_USER" -c 'command -v uv' 2>/dev/null || true
    return 0
  fi

  return 1
}

container_name="${CLAUDE_CONTAINER_NAME:-claude-runner-smoke}"
image_name="${CLAUDE_IMAGE_NAME:-dogbot/claude-runner:local}"
upstream_port="${UPSTREAM_PORT:-19090}"
blocked_host_port="${BLOCKED_HOST_PORT:-19091}"
apply_network_policy="${APPLY_NETWORK_POLICY:-1}"
tmp_root="$(mktemp -d)"
workspace_dir="$tmp_root/workspace"
state_dir="$tmp_root/state"
upstream_log="$tmp_root/upstream.log"
blocked_log="$tmp_root/blocked.log"
docker_build_log="$tmp_root/docker-build.log"
docker_run_log="$tmp_root/docker-run.log"
docker_run_err="$tmp_root/docker-run.err"
claude_version_log="$tmp_root/claude-version.log"
upstream_pid=""
blocked_pid=""
uv_bin="$(resolve_uv_bin)"

if [[ -z "${uv_bin:-}" ]]; then
  echo "uv not found; set UV_BIN or ensure uv is in PATH." >&2
  exit 1
fi

cleanup() {
  set +e
  if [[ "$apply_network_policy" == "1" && ${EUID:-$(id -u)} -eq 0 ]]; then
    POLICY_CHAIN="${POLICY_CHAIN:-DOGBOT_RUNNER_POLICY}" \
      "$repo_root/scripts/remove_runner_network_policy.sh" >/dev/null 2>&1 || true
  fi
  if [[ -n "$upstream_pid" ]]; then kill "$upstream_pid" >/dev/null 2>&1 || true; fi
  if [[ -n "$blocked_pid" ]]; then kill "$blocked_pid" >/dev/null 2>&1 || true; fi
  docker rm -f "$container_name" >/dev/null 2>&1 || true
  rm -rf "$tmp_root"
}
trap cleanup EXIT

mkdir -p "$workspace_dir" "$state_dir" "$tmp_root/upstream" "$tmp_root/blocked"
dogbot_write_claude_runner_runtime "$state_dir/claude-runner"
mkdir -p "$state_dir/claude-prompt"
cat >"$state_dir/claude-prompt/CLAUDE.md" <<'EOF'
# Smoke Prompt

@persona.md
EOF
cat >"$state_dir/claude-prompt/persona.md" <<'EOF'
# Smoke Persona
EOF
printf 'ok\n' > "$tmp_root/upstream/index.html"
printf 'blocked\n' > "$tmp_root/blocked/index.html"

"$uv_bin" run python -m http.server "$upstream_port" --bind 0.0.0.0 --directory "$tmp_root/upstream" >"$upstream_log" 2>&1 &
upstream_pid=$!
"$uv_bin" run python -m http.server "$blocked_host_port" --bind 0.0.0.0 --directory "$tmp_root/blocked" >"$blocked_log" 2>&1 &
blocked_pid=$!
sleep 1

docker build -t "$image_name" -f "$repo_root/docker/claude-runner/Dockerfile" "$repo_root" >"$docker_build_log"
run_args=(
  -d
  --name "$container_name"
  --read-only
  --tmpfs /tmp:size=256m,mode=1777
  --tmpfs /run:size=64m
  --cpus 4
  --memory 4g
  --memory-swap 4g
  --pids-limit 256
  --add-host host.docker.internal:host-gateway
  -e "ANTHROPIC_BASE_URL=http://127.0.0.1:8080/anthropic"
  -e "ANTHROPIC_API_KEY=dummy"
  -e "BIFROST_MODEL=primary/model-id"
  -e "BIFROST_UPSTREAM_PROVIDER_TYPE=openai"
  -e "BIFROST_UPSTREAM_BASE_URL=http://host.docker.internal:$upstream_port"
  -e "BIFROST_UPSTREAM_API_KEY=replace-me"
  -e "CLAUDE_CONFIG_DIR=/state/claude"
  -e "CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1"
  -e "CLAUDE_CODE_DISABLE_AUTO_MEMORY=1"
  -v "$workspace_dir:/workspace"
  -v "$state_dir:/state"
  -w /workspace
)

if ! docker run "${run_args[@]}" --storage-opt size=50G "$image_name" >"$docker_run_log" 2>"$docker_run_err"; then
  if grep -q "storage-opt is supported only" "$docker_run_err"; then
    echo "storage-opt size=50G unsupported on this host; retrying smoke test without disk quota enforcement."
    docker run "${run_args[@]}" "$image_name" >"$docker_run_log"
  else
    cat "$docker_run_err" >&2
    exit 1
  fi
fi

docker exec "$container_name" claude --version >"$claude_version_log"
docker exec "$container_name" sh -lc 'test -L /workspace/CLAUDE.md && test "$(readlink /workspace/CLAUDE.md)" = "/state/claude-prompt/CLAUDE.md"'
docker exec "$container_name" sh -lc 'test -L /workspace/persona.md && test "$(readlink /workspace/persona.md)" = "/state/claude-prompt/persona.md"'
docker exec "$container_name" sh -lc "curl -fsS http://host.docker.internal:$upstream_port >/dev/null"
docker exec "$container_name" sh -lc "curl -fsS https://example.com >/dev/null"

if [[ "$apply_network_policy" == "1" ]]; then
  if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
    echo "Smoke precheck passed, but APPLY_NETWORK_POLICY=1 requires root." >&2
    exit 1
  fi

  API_PROXY_PORT="$upstream_port" CLAUDE_CONTAINER_NAME="$container_name" \
    "$repo_root/scripts/apply_runner_network_policy.sh"

  if docker exec "$container_name" sh -lc "curl -fsS --max-time 3 http://host.docker.internal:$blocked_host_port >/dev/null"; then
    echo "Expected blocked host port $blocked_host_port to fail, but it succeeded." >&2
    exit 1
  fi
fi

echo "Smoke test passed for $container_name."
