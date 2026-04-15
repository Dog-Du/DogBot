#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <env-file>" >&2
  exit 1
fi

ENV_FILE="$1"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "env file not found: $ENV_FILE" >&2
  exit 1
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

if ! command -v uv >/dev/null 2>&1; then
  echo "uv not found. Please install uv first." >&2
  exit 1
fi

NAPCAT_CONTAINER_NAME="${NAPCAT_CONTAINER_NAME:-napcat}"
NAPCAT_CONFIG_DIR="${NAPCAT_CONFIG_DIR:-$ROOT_DIR/agent-state/napcat-config}"
NAPCAT_WS_CLIENT_URL="${NAPCAT_WS_CLIENT_URL:-ws://host.docker.internal:19000/napcat/ws}"
NAPCAT_WS_CLIENT_TOKEN="${NAPCAT_WS_CLIENT_TOKEN:-}"
NAPCAT_WS_CLIENT_RECONNECT_MS="${NAPCAT_WS_CLIENT_RECONNECT_MS:-1000}"
NAPCAT_WS_CLIENT_HEART_MS="${NAPCAT_WS_CLIENT_HEART_MS:-1000}"
QQ_ADAPTER_QQ_BOT_ID="${QQ_ADAPTER_QQ_BOT_ID:-}"

if [[ -z "$QQ_ADAPTER_QQ_BOT_ID" ]]; then
  echo "QQ_ADAPTER_QQ_BOT_ID is required to configure NapCat websocket client" >&2
  exit 1
fi

CONFIG_FILE="$NAPCAT_CONFIG_DIR/onebot11_${QQ_ADAPTER_QQ_BOT_ID}.json"
mkdir -p "$NAPCAT_CONFIG_DIR"

uv run python - <<PY
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

network["websocketClients"] = [{
    "name": "qq-adapter",
    "enable": True,
    "url": ${NAPCAT_WS_CLIENT_URL@Q},
    "reportSelfMessage": False,
    "messagePostFormat": "array",
    "token": ${NAPCAT_WS_CLIENT_TOKEN@Q},
    "debug": False,
    "heartInterval": int(${NAPCAT_WS_CLIENT_HEART_MS@Q}),
    "reconnectInterval": int(${NAPCAT_WS_CLIENT_RECONNECT_MS@Q}),
}]

config_path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

docker restart "$NAPCAT_CONTAINER_NAME" >/dev/null
echo "configured NapCat websocket client in $CONFIG_FILE"
