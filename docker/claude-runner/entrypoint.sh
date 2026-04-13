#!/usr/bin/env bash
set -euo pipefail

mkdir -p /workspace /state/claude
chown -R claude:claude /workspace /state
touch /state/claude/.keep

exec gosu claude sleep infinity
