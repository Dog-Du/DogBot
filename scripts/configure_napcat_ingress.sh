#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib/common.sh
source "$script_dir/lib/common.sh"

env_file="$(dogbot_resolve_env_file "${1:-}")"
dogbot_load_env_file "$env_file"
if ! uv_bin="$(dogbot_resolve_uv_bin)"; then
  exit 1
fi

NAPCAT_CONTAINER_NAME="${NAPCAT_CONTAINER_NAME:-napcat}"
NAPCAT_CONFIG_DIR="${NAPCAT_CONFIG_DIR:-$dogbot_repo_root/agent-state/napcat-config}"
NAPCAT_ONEBOT_PORT="${NAPCAT_ONEBOT_PORT:-3001}"
NAPCAT_HTTP_TOKEN="${NAPCAT_ACCESS_TOKEN:-}"
NAPCAT_HTTP_CLIENT_URL="${NAPCAT_HTTP_CLIENT_URL:-http://host.docker.internal:8787/v1/platforms/qq/napcat/events}"
NAPCAT_HTTP_CLIENT_TOKEN="${NAPCAT_HTTP_CLIENT_TOKEN:-}"
PLATFORM_QQ_BOT_ID="${PLATFORM_QQ_BOT_ID:-}"

if [[ -z "$PLATFORM_QQ_BOT_ID" ]]; then
  echo "PLATFORM_QQ_BOT_ID is required to configure NapCat HTTP client" >&2
  exit 1
fi

CONFIG_FILE="$NAPCAT_CONFIG_DIR/onebot11_${PLATFORM_QQ_BOT_ID}.json"
mkdir -p "$NAPCAT_CONFIG_DIR"

"$uv_bin" run python - <<PY
import json
from pathlib import Path

config_path = Path(${CONFIG_FILE@Q})
if config_path.exists():
    data = json.loads(config_path.read_text(encoding="utf-8"))
else:
    data = {}

network = data.setdefault("network", {})
for key in ("httpServers", "httpSseServers", "httpClients", "websocketServers", "plugins"):
    network.setdefault(key, [])

network["httpServers"] = [{
    "name": "dogbot-http",
    "enable": True,
    "port": int(${NAPCAT_ONEBOT_PORT@Q}),
    "host": "0.0.0.0",
    "enableCors": True,
    "enableWebsocket": False,
    "messagePostFormat": "array",
    "token": ${NAPCAT_HTTP_TOKEN@Q},
    "debug": False,
}]

network["httpClients"] = [{
    "name": "agent-runner-platform-ingress",
    "enable": True,
    "url": ${NAPCAT_HTTP_CLIENT_URL@Q},
    "reportSelfMessage": False,
    "messagePostFormat": "array",
    "token": ${NAPCAT_HTTP_CLIENT_TOKEN@Q},
    "debug": False,
}]
network["websocketClients"] = []

config_path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

docker restart "$NAPCAT_CONTAINER_NAME" >/dev/null
echo "configured NapCat HTTP client in $CONFIG_FILE"
