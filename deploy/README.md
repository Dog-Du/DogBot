# Deployment Guide

This document describes the current deployable stack in this repository:

```text
QQ
-> NapCat
-> AstrBot
-> agent-runner
-> claude-runner (Docker)
-> agent-runner built-in Anthropic-compatible proxy
-> upstream provider (Packy / GLM / MiniMax official)
```

Optional WeChat path:

```text
WeChat
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner (Docker)
-> agent-runner built-in Anthropic-compatible proxy
-> upstream provider
```

When AstrBot is not used for WeChat, the repository can run:

```text
WeChat
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner
```

Important boundary:

- real upstream API keys stay on the host
- the Docker Claude container does not receive real provider keys
- the Docker Claude container only receives:
  - `ANTHROPIC_BASE_URL=http://host.docker.internal:<proxy_port>`
  - `ANTHROPIC_AUTH_TOKEN=<local proxy token>`

## 1. Host Requirements

Required on the host:

- Linux
- Docker Engine
- Docker Compose v2
- Rust toolchain with `cargo`
- `uv`
- `curl`
- `sudo`

Recommended:

- `git`
- `rg`

Quick checks:

```bash
docker --version
docker compose version
cargo --version
uv --version
```

If your current user cannot access Docker, either:

```bash
sudo usermod -aG docker "$USER"
newgrp docker
```

or run deploy commands with `sudo`.

## 2. Important Files

The main files you need to know:

- `deploy/myqqbot.env.example`
  - environment template
- `deploy/myqqbot.env`
  - your real local deployment config
- `compose/docker-compose.yml`
  - Claude container definition
- `compose/platform-stack.yml`
  - NapCat and AstrBot containers
- `compose/wechatpadpro-stack.yml`
  - optional WeChatPadPro, MySQL, and Redis containers
- `docker/claude-runner/Dockerfile`
  - Claude image build
- `astrbot/plugins/claude_runner_bridge/main.py`
  - AstrBot bridge plugin
- `scripts/deploy_stack.sh`
  - full start script
- `scripts/stop_stack.sh`
  - full stop script
- `scripts/send_session_message.sh`
  - host-local proactive send helper
- `scripts/start_wechatpadpro_adapter.sh`
  - host-local WeChatPadPro adapter startup script

## 3. Quick Start

1. Copy the env template:

```bash
cp deploy/myqqbot.env.example deploy/myqqbot.env
```

2. Edit `deploy/myqqbot.env`

3. Start:

```bash
./scripts/deploy_stack.sh deploy/myqqbot.env
```

4. Stop:

```bash
./scripts/stop_stack.sh deploy/myqqbot.env
```

If Docker permissions are restricted:

```bash
sudo ./scripts/deploy_stack.sh deploy/myqqbot.env
sudo ./scripts/stop_stack.sh deploy/myqqbot.env
```

If `ENABLE_WECHATPADPRO=1`, the same deploy command also starts:

- `wechatpadpro_mysql`
- `wechatpadpro_redis`
- `wechatpadpro`
- `wechatpadpro-adapter`

## 4. What Starts

The stack starts:

- local Rust `agent-runner`
- local Anthropic-compatible proxy listener inside `agent-runner`
- `claude-runner` Docker container
- `napcat` Docker container
- `astrbot` Docker container
- optional host firewall policy for the Claude container

`agent-runner` is one host process with two listeners:

- `AGENT_RUNNER_BIND_ADDR`
  - `/healthz`
  - `/v1/runs`
  - `/v1/messages`
- `API_PROXY_BIND_ADDR`
  - Anthropic-compatible upstream proxy for Claude

## 5. Core Config File

The most important file is:

```text
deploy/myqqbot.env
```

Everything important is controlled there:

- workspace and state paths
- Rust service ports
- Claude container resources
- upstream provider selection
- provider API key
- NapCat and AstrBot ports and data paths
- optional WeChatPadPro ports, secrets, and state paths

### 5.1 Claude Container

```env
CLAUDE_CONTAINER_NAME=claude-runner
CLAUDE_IMAGE_NAME=myqqbot/claude-runner:local
CLAUDE_CODE_VERSION=2.1.104
```

Meaning:

- container name
- image tag
- Claude Code version baked into the image

### 5.2 Workspace and State

```env
AGENT_WORKSPACE_DIR=/srv/agent-workdir
AGENT_STATE_DIR=/srv/agent-state
SESSION_DB_PATH=/srv/agent-state/runner.db
```

Recommended:

- keep `AGENT_WORKSPACE_DIR` for Claude-readable/writable project data
- keep `AGENT_STATE_DIR` for:
  - Claude persistent session state
  - `runner.db`
  - logs
  - NapCat and AstrBot data if you choose to colocate them

If you change these paths later, old state and session continuity may appear lost.

### 5.3 agent-runner Ports

```env
AGENT_RUNNER_BIND_ADDR=127.0.0.1:8787
API_PROXY_BIND_ADDR=127.0.0.1:9000
```

`AGENT_RUNNER_BIND_ADDR` is for AstrBot.

`API_PROXY_BIND_ADDR` is for Claude inside Docker.

Claude should always point to the local proxy:

```env
ANTHROPIC_BASE_URL=http://host.docker.internal:9000
API_PROXY_AUTH_TOKEN=local-proxy-token
```

`API_PROXY_AUTH_TOKEN` is not a real provider key. It is only the local token used between Claude and the host-local proxy.

### 5.4 Timeout and Queue

```env
DEFAULT_TIMEOUT_SECS=120
MAX_TIMEOUT_SECS=300
MAX_CONCURRENT_RUNS=10
MAX_QUEUE_DEPTH=20
GLOBAL_RATE_LIMIT_PER_MINUTE=10
USER_RATE_LIMIT_PER_MINUTE=3
CONVERSATION_RATE_LIMIT_PER_MINUTE=5
```

These values affect:

- command hard timeout
- queue overflow behavior
- anti-abuse rate limits

### 5.5 Claude Container Resources

```env
CLAUDE_CONTAINER_CPU_CORES=4
CLAUDE_CONTAINER_MEMORY_MB=4096
CLAUDE_CONTAINER_DISK_GB=50
CLAUDE_CONTAINER_PIDS_LIMIT=256
```

Current reality:

- CPU, memory, and pids limits are enforced
- disk size is still a target config value
- Docker-layer disk quota is not currently enforced on this host filesystem

### 5.6 NapCat

```env
NAPCAT_IMAGE=mlikiowa/napcat-docker:latest
NAPCAT_CONTAINER_NAME=napcat
NAPCAT_WEBUI_PORT=6099
NAPCAT_ONEBOT_PORT=3001
NAPCAT_UID=1000
NAPCAT_GID=1000
NAPCAT_QQ_DIR=/srv/napcat/qq
NAPCAT_CONFIG_DIR=/srv/napcat/config
```

`NAPCAT_QQ_DIR` stores login/runtime data.  
`NAPCAT_CONFIG_DIR` stores NapCat config.

### 5.7 AstrBot

```env
ASTRBOT_IMAGE=soulter/astrbot:latest
ASTRBOT_CONTAINER_NAME=astrbot
ASTRBOT_WEBUI_PORT=6185
ASTRBOT_QQ_BRIDGE_PORT=6199
ASTRBOT_DATA_DIR=/srv/astrbot/data
ASTRBOT_PLUGIN_DIR=../astrbot/plugins/claude_runner_bridge

AGENT_RUNNER_BASE_URL=http://host.docker.internal:8787
CLAUDE_BRIDGE_DEFAULT_CWD=/workspace
CLAUDE_BRIDGE_TIMEOUT_SECS=120
CLAUDE_BRIDGE_COMMAND_NAME=agent
CLAUDE_BRIDGE_STATUS_COMMAND_NAME=agent-status
```

### 5.8 WeChatPadPro

WeChatPadPro support is optional.

Enable it with:

```env
ENABLE_WECHATPADPRO=1
```

Required settings:

```env
WECHATPADPRO_IMAGE=<set this to a real upstream image tag>
WECHATPADPRO_HOST_PORT=38849
WECHATPADPRO_ADMIN_KEY=<set a strong random value>
WECHATPADPRO_DATA_DIR=/srv/wechatpadpro/data

WECHATPADPRO_MYSQL_ROOT_PASSWORD=<required>
WECHATPADPRO_MYSQL_DATABASE=weixin
WECHATPADPRO_MYSQL_USER=weixin
WECHATPADPRO_MYSQL_PASSWORD=<required>
WECHATPADPRO_MYSQL_DIR=/srv/wechatpadpro/mysql

WECHATPADPRO_REDIS_DIR=/srv/wechatpadpro/redis
```

Important notes:

- this repository intentionally does not hardcode a default `WECHATPADPRO_IMAGE`
- upstream image naming and distribution can change; set the exact image/tag from the current upstream release before enabling
- deploy will fail fast if WeChatPadPro is enabled but required secrets or image are missing

Adapter settings:

```env
WECHATPADPRO_BASE_URL=http://127.0.0.1:38849
WECHATPADPRO_ACCOUNT_KEY=
WECHATPADPRO_ADAPTER_HOST=127.0.0.1
WECHATPADPRO_ADAPTER_PORT=18999
WECHATPADPRO_ADAPTER_BIND_ADDR=127.0.0.1:18999
WECHATPADPRO_ADAPTER_WEBHOOK_URL=http://host.docker.internal:18999/wechatpadpro/events
WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=0
WECHATPADPRO_WEBHOOK_INCLUDE_SELF_MESSAGE=false
# WECHATPADPRO_WEBHOOK_SECRET=
WECHATPADPRO_DEFAULT_CWD=/workspace
WECHATPADPRO_DEFAULT_TIMEOUT_SECS=120
```

### 5.9 Built-in Proxy and Upstream Provider

The built-in proxy chooses one active provider:

```env
API_PROXY_ACTIVE_PROVIDER=packy
```

Supported provider config groups right now:

- `API_PROXY_PACKY_*`
- `API_PROXY_GLM_*`
- `API_PROXY_MINIMAX_*`

#### Packy Example

```env
API_PROXY_ACTIVE_PROVIDER=packy
API_PROXY_PACKY_BASE_URL=https://www.packyapi.com
API_PROXY_PACKY_UPSTREAM_TOKEN=your_real_packy_token
API_PROXY_PACKY_AUTH_HEADER=x-api-key
# API_PROXY_PACKY_AUTH_SCHEME=
# API_PROXY_PACKY_MODEL=
```

#### GLM Example

```env
API_PROXY_ACTIVE_PROVIDER=glm_official
API_PROXY_GLM_BASE_URL=https://open.bigmodel.cn/api/anthropic
API_PROXY_GLM_UPSTREAM_TOKEN=your_real_glm_token
API_PROXY_GLM_AUTH_HEADER=Authorization
API_PROXY_GLM_AUTH_SCHEME=Bearer
# API_PROXY_GLM_MODEL=
```

#### MiniMax Example

```env
API_PROXY_ACTIVE_PROVIDER=minimax_official
API_PROXY_MINIMAX_BASE_URL=<your_minimax_anthropic_compatible_base_url>
API_PROXY_MINIMAX_UPSTREAM_TOKEN=your_real_minimax_token
API_PROXY_MINIMAX_AUTH_HEADER=Authorization
API_PROXY_MINIMAX_AUTH_SCHEME=Bearer
# API_PROXY_MINIMAX_MODEL=
```

## 6. Very Important Compatibility Rule

The Claude container must talk to an Anthropic-compatible upstream path.

That means:

- the configured upstream `BASE_URL` must be compatible with Claude / Anthropic Messages API
- not every OpenAI-compatible endpoint can be used here

Examples that fit:

- Packy's Claude-compatible route
- GLM Anthropic-compatible route
- MiniMax Anthropic-compatible route

Examples that do not automatically fit:

- a pure OpenAI-compatible `/v1/chat/completions` endpoint
- a provider route intended only for Codex or OpenAI SDKs

In short:

- `Claude Code` requires a Claude/Anthropic-compatible `BASE_URL`
- if you want OpenAI-compatible providers only, that is a different integration path

## 7. NapCat Configuration

### 7.1 Open NapCat WebUI

Default:

```text
http://127.0.0.1:6099
```

If you are on a remote server, use SSH port forwarding from your local machine.

### 7.2 Get WebUI Token if Needed

NapCat often prints the WebUI token in container logs:

```bash
sudo docker logs --tail 200 napcat | rg -i 'token|webui|login'
```

### 7.3 Log in to QQ

Use the NapCat WebUI to scan the QR code and log in with your QQ account.

### 7.4 Configure Reverse WebSocket to AstrBot

In NapCat WebUI, add a WebSocket client config:

- type: `WebSockets客户端`
- URL:

```text
ws://astrbot:6199/ws
```

- Token: leave empty unless you intentionally configured one
- enable it
- heartbeat: `1000`
- reconnect: `1000`

Important:

- do not use `127.0.0.1` here
- NapCat and AstrBot are Docker containers, so they should communicate by container name

## 8. AstrBot Configuration

### 8.1 Open AstrBot WebUI

Default:

```text
http://127.0.0.1:6185
```

Typical default credentials are:

- username: `astrbot`
- password: `astrbot`

### 8.2 Create the QQ Bot

Inside AstrBot WebUI:

1. Go to `机器人`
2. Create a new bot
3. Choose `OneBot v11`
4. Fill:
   - ID: any name, such as `DogBot`
   - enable: checked
   - reverse WebSocket host: `0.0.0.0`
   - reverse WebSocket port: `6199`
   - token: empty unless you want auth

### 8.3 Bridge Plugin Behavior

The current bridge plugin behavior is:

- normal messages are forwarded to the agent by default
- `/agent-status` is a special command
- `/agent <prompt>` still works as a compatibility alias
- QQ group replies automatically prepend `@sender`

### 8.4 Optional WeChatPadPro Adapter

The current public AstrBot image does not expose a usable `WeChatPadPro` platform adapter, so this repository uses a host-local `wechatpadpro-adapter` instead.

WeChat flow is:

```text
WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner
```

The adapter:

- receives webhook POST events from WeChatPadPro
- forwards text messages to `agent-runner`
- sends text replies back through WeChatPadPro `/message/SendTextMessage`

The adapter needs `WECHATPADPRO_ACCOUNT_KEY`. This is not the same thing as `WECHATPADPRO_ADMIN_KEY`.

Recommended setup:

1. Start the stack with `ENABLE_WECHATPADPRO=1`
2. Log in to WeChatPadPro with the target WeChat account
3. Obtain the account key from WeChatPadPro after login
4. Set `WECHATPADPRO_ACCOUNT_KEY` in `deploy/myqqbot.env`
5. Optionally set `WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1`
6. Redeploy

## 9. Docker Configuration

The main Claude container config lives in:

- `compose/docker-compose.yml`

The platform services live in:

- `compose/platform-stack.yml`
- `compose/wechatpadpro-stack.yml`

### Claude Container Properties

The Claude container currently has:

- `cpus: "4.0"`
- `mem_limit: 4g`
- `memswap_limit: 4g`
- `pids_limit: 256`
- `read_only: true`
- `tmpfs` for `/tmp` and `/run`
- writable mounts only for:
  - `/workspace`
  - `/state`

Container env intentionally includes only:

- `ANTHROPIC_BASE_URL=http://host.docker.internal:<proxy_port>`
- `ANTHROPIC_AUTH_TOKEN=<local proxy token>`

It does not contain real Packy, GLM, or MiniMax tokens.

## 10. WeChatPadPro Configuration

WeChatPadPro is a non-official personal WeChat ingress layer. It is optional and separate from NapCat.

The repository starts it as three services:

- `wechatpadpro_mysql`
- `wechatpadpro_redis`
- `wechatpadpro`

After deployment:

1. Confirm the containers are healthy:

```bash
docker ps --filter name=wechatpadpro --filter name=wechatpadpro_mysql --filter name=wechatpadpro_redis
```

2. Check WeChatPadPro logs:

```bash
docker logs --tail 200 wechatpadpro
```

3. Confirm its API port is reachable:

```bash
curl http://127.0.0.1:38849
```

Expected result:

- an HTTP response from WeChatPadPro
- or at least a non-empty server response proving the port is bound

This repository does not store QR codes or WeChat auth files in git.

## 10.1 WeChatPadPro Adapter

The host-local adapter receives webhook events and forwards text messages to `agent-runner`.

Health check:

```bash
curl http://127.0.0.1:18999/healthz
```

Webhook endpoint:

```text
POST /wechatpadpro/events
```

The first pass only supports text private/group messages. Unsupported message types are ignored.

If `WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1` and `WECHATPADPRO_ACCOUNT_KEY` is set, deploy also pushes this webhook config to WeChatPadPro:

- `URL`: `WECHATPADPRO_ADAPTER_WEBHOOK_URL`
- `Enabled`: `true`
- `IncludeSelfMessage`: `WECHATPADPRO_WEBHOOK_INCLUDE_SELF_MESSAGE`
- `MessageTypes`: `["Text"]`
- `Secret`: `WECHATPADPRO_WEBHOOK_SECRET`

## 11. How to Start and Stop

### Start

```bash
./scripts/deploy_stack.sh deploy/myqqbot.env
```

If Docker access requires privilege:

```bash
sudo ./scripts/deploy_stack.sh deploy/myqqbot.env
```

### Stop

```bash
./scripts/stop_stack.sh deploy/myqqbot.env
```

### What the Start Script Does

It will:

- create required directories
- start `agent-runner`
- start `claude-runner`
- start `napcat`
- start `astrbot`
- optionally apply host firewall rules for the Claude container

## 12. How to Change API Key or Model

### 11.1 Change API Key

Edit the real host-side provider key in `deploy/myqqbot.env`.

For example, Packy:

```env
API_PROXY_PACKY_UPSTREAM_TOKEN=your_new_packy_token
```

For GLM:

```env
API_PROXY_GLM_UPSTREAM_TOKEN=your_new_glm_token
```

For MiniMax:

```env
API_PROXY_MINIMAX_UPSTREAM_TOKEN=your_new_minimax_token
```

### 11.2 Change Model

If your upstream supports a model field, edit the corresponding model entry:

```env
API_PROXY_PACKY_MODEL=...
API_PROXY_GLM_MODEL=...
API_PROXY_MINIMAX_MODEL=...
```

### 11.3 Change Active Provider

Switch:

```env
API_PROXY_ACTIVE_PROVIDER=glm_official
```

or:

```env
API_PROXY_ACTIVE_PROVIDER=minimax_official
```

### 11.4 Apply Changes

After changing provider config, restart the stack:

```bash
./scripts/stop_stack.sh deploy/myqqbot.env
sudo ./scripts/deploy_stack.sh deploy/myqqbot.env
```

This is the safest way to ensure:

- `agent-runner` reloads provider config
- the local proxy uses the new upstream
- Claude container receives the correct local proxy auth env

## 13. Verification Commands

### Check runner health

```bash
curl http://127.0.0.1:8787/healthz
```

Adjust the port if you changed `AGENT_RUNNER_BIND_ADDR`.

### Check local proxy listener

This should return a non-200 response like `400` if the body is incomplete, but the response itself confirms the listener is alive:

```bash
curl -H 'x-api-key: local-proxy-token' \
  -H 'content-type: application/json' \
  -d '{"messages":[]}' \
  http://127.0.0.1:9000/v1/messages
```

### Check container env does not include the real provider key

```bash
sudo docker exec claude-runner env | rg 'ANTHROPIC_(BASE_URL|AUTH_TOKEN|MODEL)|API_PROXY'
```

Expected:

- `ANTHROPIC_BASE_URL=http://host.docker.internal:9000`
- `ANTHROPIC_AUTH_TOKEN=local-proxy-token`

Not expected:

- your real Packy token
- your real GLM token
- your real MiniMax token

### Check WeChatPadPro when enabled

```bash
docker logs --tail 100 wechatpadpro
docker logs --tail 100 astrbot
```

Expected:

- WeChatPadPro shows a healthy boot
- AstrBot shows the WeChatPadPro adapter and QR/login prompts when configured

## 14. Things to Watch Out For

### 13.1 Provider Compatibility

Only use a Claude/Anthropic-compatible upstream base URL for this stack.

Do not assume that:

- an OpenAI-compatible endpoint
- a Codex-oriented endpoint
- a provider-specific SDK endpoint

will work with Claude Code.

### 13.2 Packy Token Groups

Packy token groups matter.

Examples:

- some token groups work with Claude Code
- some token groups are intended for other routing or model groups and may not work correctly with Claude Code

If Packy works with one token and fails with another, check the token group first.

### 13.3 Real Key Placement

The real upstream token belongs only in host env config.

It should not be:

- placed in Docker workspace files
- committed into git
- injected into prompts

### 13.4 Disk Limit

Current disk limit behavior is incomplete:

- CPU, memory, pids, timeout, and writable path boundaries are active
- service-level Docker disk quota is not currently enforced on this host filesystem

### 13.5 Python Invocation

For Python-based helpers, prefer:

```bash
uv run ...
```

not plain `python`.

### 14.6 WeChatPadPro Risk and Host Constraints

- WeChatPadPro is non-official and may trigger account risk, login churn, or protocol breakage.
- Upstream AstrBot documentation notes the Docker route is Linux-oriented and not arm64-friendly.
- Treat WeChatPadPro as experimental infrastructure, not a stable official API.

## 15. Proactive Message MVP

The current first-pass proactive send path is host-local and session-based:

```bash
./scripts/send_session_message.sh \
  --env-file deploy/myqqbot.env \
  --session-id qq:private:123456 \
  --text "hello from cron"
```

Optional flags:

- `--reply-to <message_id>`
- `--mention-user <user_id>`

This assumes the target `session_id` already exists in `agent-runner` state.
