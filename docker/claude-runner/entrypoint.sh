#!/usr/bin/env bash
set -euo pipefail

mkdir -p /state/claude /workspace
touch /state/claude/.keep

exec sleep infinity
