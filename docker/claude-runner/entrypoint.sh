#!/usr/bin/env bash
set -euo pipefail

sudo mkdir -p /workspace /state/claude
sudo chown claude:claude /workspace /state/claude
sudo chown claude:claude /state
sudo touch /state/claude/.keep

exec gosu claude sleep infinity
