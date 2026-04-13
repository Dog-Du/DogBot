#!/usr/bin/env bash
set -euo pipefail

container_name="${CLAUDE_CONTAINER_NAME:-claude-runner}"
api_proxy_port="${API_PROXY_PORT:-9000}"
policy_chain="${POLICY_CHAIN:-MYQQBOT_RUNNER_POLICY}"

require_root() {
  if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
    echo "apply_runner_network_policy.sh must run as root." >&2
    exit 1
  fi
}

inspect_values() {
  docker inspect "$container_name" \
    --format '{{range $name, $net := .NetworkSettings.Networks}}{{println $net.IPAddress $net.Gateway}}{{end}}'
}

host_ipv4s() {
  ip -o -4 addr show scope global | awk '{print $4}' | cut -d/ -f1
}

ensure_chain() {
  iptables -N "$policy_chain" 2>/dev/null || true
  iptables -F "$policy_chain"
  if ! iptables -C INPUT -j "$policy_chain" >/dev/null 2>&1; then
    iptables -I INPUT 1 -j "$policy_chain"
  fi
}

main() {
  require_root
  ensure_chain

  declare -a container_ips=()
  declare -a gateway_ips=()
  while read -r container_ip gateway_ip; do
    [[ -n "${container_ip:-}" ]] && container_ips+=("$container_ip")
    [[ -n "${gateway_ip:-}" ]] && gateway_ips+=("$gateway_ip")
  done < <(inspect_values)

  declare -A host_ip_set=()
  for ip_addr in "${gateway_ips[@]}"; do
    host_ip_set["$ip_addr"]=1
  done
  while read -r ip_addr; do
    [[ -n "${ip_addr:-}" ]] && host_ip_set["$ip_addr"]=1
  done < <(host_ipv4s)

  for container_ip in "${container_ips[@]}"; do
    for host_ip in "${!host_ip_set[@]}"; do
      iptables -A "$policy_chain" -s "$container_ip" -d "$host_ip" -p tcp --dport "$api_proxy_port" -j RETURN
      iptables -A "$policy_chain" -s "$container_ip" -d "$host_ip" -j REJECT
    done
  done

  iptables -A "$policy_chain" -j RETURN

  echo "Applied runner network policy for $container_name allowing host tcp/$api_proxy_port and outbound internet."
}

main "$@"
