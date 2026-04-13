#!/usr/bin/env bash
set -euo pipefail

CLAUDE_PROXY_URL="${CLAUDE_PROXY_URL:-http://localhost:8080}"
CLAUDE_MODEL="${CLAUDE_MODEL:-claude-2.1}"

export CLAUDE_PROXY_URL
export CLAUDE_MODEL

exec claude "$@"
