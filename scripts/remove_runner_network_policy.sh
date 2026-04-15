#!/usr/bin/env bash
set -euo pipefail

policy_chain="${POLICY_CHAIN:-DOGBOT_RUNNER_POLICY}"

if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
  echo "remove_runner_network_policy.sh must run as root." >&2
  exit 1
fi

iptables -D INPUT -j "$policy_chain" >/dev/null 2>&1 || true
iptables -F "$policy_chain" >/dev/null 2>&1 || true
iptables -X "$policy_chain" >/dev/null 2>&1 || true

echo "Removed runner network policy chain $policy_chain."
