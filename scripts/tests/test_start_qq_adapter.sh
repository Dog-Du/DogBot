#!/usr/bin/env bash
set -euo pipefail

script_path="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/start_qq_adapter.sh"

if ! grep -Eq -- '--with[[:space:]]+(websockets|wsproto)|--with[[:space:]]+uvicorn\\[standard\\]' "$script_path"; then
  echo "FAIL: start_qq_adapter.sh must install a WebSocket-capable uvicorn runtime" >&2
  exit 1
fi

echo "start_qq_adapter.sh websocket runtime dependency check passed."
