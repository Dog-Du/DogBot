#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

env_file="$tmpdir/dogbot.env"
cat >"$env_file" <<'EOF'
ENABLE_WECHATPADPRO=1
WECHATPADPRO_ACCOUNT_KEY=test-account-key
WECHATPADPRO_ADAPTER_WEBHOOK_URL=http://127.0.0.1:18999/wechatpadpro/events
WECHATPADPRO_BASE_URL=http://127.0.0.1:38849
WECHATPADPRO_MYSQL_ROOT_PASSWORD=test-root-password
WECHATPADPRO_MYSQL_CONTAINER_NAME=wechatpadpro_mysql
WECHATPADPRO_MYSQL_DATABASE=weixin
EOF

mkdir -p "$tmpdir/bin"
curl_log="$tmpdir/curl.log"
docker_log="$tmpdir/docker.log"

cat >"$tmpdir/bin/curl" <<EOF
#!/usr/bin/env bash
echo "curl: (28) Operation timed out after 15002 milliseconds with 0 bytes received" >&2
echo curl "\$@" >>"$curl_log"
exit 28
EOF

cat >"$tmpdir/bin/docker" <<EOF
#!/usr/bin/env bash
echo docker "\$@" >>"$docker_log"
exit 0
EOF

chmod +x "$tmpdir/bin/curl" "$tmpdir/bin/docker"

output="$(
  PATH="$tmpdir/bin:$PATH" \
    "$repo_root/scripts/configure_wechatpadpro_webhook.sh" "$env_file" 2>&1
)"

if [[ "$output" != *"WeChatPadPro webhook configuration synced."* ]]; then
  echo "FAIL: expected webhook sync completion message" >&2
  echo "$output" >&2
  exit 1
fi

if ! grep -q 'docker exec wechatpadpro_mysql mysql' "$docker_log"; then
  echo "FAIL: expected direct MySQL sync fallback to run" >&2
  cat "$docker_log" >&2 || true
  exit 1
fi

echo "configure_wechatpadpro_webhook fallback test passed."
