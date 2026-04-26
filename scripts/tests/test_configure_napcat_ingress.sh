#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

env_example="$repo_root/deploy/dogbot.env.example"
env_file="$tmpdir/dogbot.env"
config_dir="$tmpdir/napcat-config"

if ! grep -q '^NAPCAT_HTTP_CLIENT_URL=http://host.docker.internal:8787/v1/platforms/qq/napcat/events$' "$env_example"; then
  echo "FAIL: dogbot.env.example must define the default NapCat HTTP ingress URL" >&2
  exit 1
fi

mkdir -p "$config_dir" "$tmpdir/bin"

cat >"$env_file" <<EOF
NAPCAT_CONTAINER_NAME=napcat
NAPCAT_CONFIG_DIR=$config_dir
NAPCAT_ONEBOT_PORT=3456
NAPCAT_ACCESS_TOKEN=test-access-token
PLATFORM_QQ_BOT_ID=3472283357
EOF

cat >"$tmpdir/bin/uv" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "run" ]]; then
  shift
fi

if [[ "${1:-}" == "python" ]]; then
  shift
fi

exec python3 "$@"
EOF

chmod +x "$tmpdir/bin/uv"

output="$(
  PATH="$tmpdir/bin:$PATH" \
    "$repo_root/scripts/configure_napcat_ingress.sh" "$env_file" 2>&1
)"

config_file="$config_dir/onebot11_3472283357.json"

python3 - <<'PY' "$config_file"
import json
import sys
from pathlib import Path

config_path = Path(sys.argv[1])
data = json.loads(config_path.read_text(encoding="utf-8"))

http_servers = data["network"]["httpServers"]
assert len(http_servers) == 1, http_servers
server = http_servers[0]
assert server["enable"] is True, server
assert server["host"] == "0.0.0.0", server
assert server["port"] == 3456, server
assert server["token"] == "test-access-token", server
assert server["messagePostFormat"] == "array", server
assert server["enableCors"] is True, server
assert server["enableWebsocket"] is False, server

http_clients = data["network"]["httpClients"]
assert len(http_clients) == 1, http_clients
assert http_clients[0]["url"] == "http://host.docker.internal:8787/v1/platforms/qq/napcat/events", http_clients[0]
PY

if [[ "$output" != *"configured NapCat HTTP client in $config_file"* ]]; then
  echo "FAIL: expected configure_napcat_ingress.sh success message" >&2
  echo "$output" >&2
  exit 1
fi

if [[ "$output" == *"restart"* ]]; then
  echo "FAIL: configure_napcat_ingress.sh must not restart napcat anymore" >&2
  echo "$output" >&2
  exit 1
fi

echo "configure_napcat_ingress test passed."
