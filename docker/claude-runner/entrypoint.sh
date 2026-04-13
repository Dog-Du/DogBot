#!/usr/bin/env bash
set -euo pipefail

sudo mkdir -p /workspace /state/claude
sudo chown claude:claude /state/claude
sudo chown claude:claude /state
sudo touch /state/claude/.keep
sudo rm -f /etc/sudoers.d/claude

exec sleep infinity
