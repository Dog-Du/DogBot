# DogBot 部署指南

本文档只描述当前部署方式。历史上的 sqlite、外部 adapter 和旧的多层配置方式已经废弃。

部署原则：

- 用户配置只改 `deploy/dogbot.env`。
- 启动只用 `./deploy_stack.sh`。
- 停止只用 `./stop_stack.sh`。
- `deploy/docker/` 默认不用改。

## 1. 部署链路

```text
QQ -> NapCat HTTP callback
   -> agent-runner
   -> claude-runner container
   -> Bifrost inside container
   -> agent-runner api-proxy on host
   -> upstream model provider

微信 -> WeChatPadPro webhook
    -> agent-runner
    -> claude-runner container
    -> Bifrost inside container
    -> agent-runner api-proxy on host
    -> upstream model provider
```

数据持久化：

- PostgreSQL 保存 session 映射、文本历史消息和 history read grants。
- `claude-runner` 的 Claude Code 状态保存在 `AGENT_STATE_DIR`。
- NapCat / WeChatPadPro 登录态也建议放在同一个 `AGENT_STATE_DIR` 下。

## 2. 依赖

必需：

- Linux 宿主机
- Docker Engine
- Docker Compose v2，或兼容的 `docker-compose`
- Rust / cargo
- uv
- curl
- sudo

快速检查：

```bash
docker --version
docker compose version
cargo --version
uv --version
curl --version
```

如果当前用户不能访问 Docker：

```bash
sudo usermod -aG docker "$USER"
newgrp docker
```

或者直接用 `sudo ./deploy_stack.sh`。

## 3. 快速开始

复制配置：

```bash
cp deploy/dogbot.env.example deploy/dogbot.env
```

编辑配置：

```bash
$EDITOR deploy/dogbot.env
```

启动：

```bash
./deploy_stack.sh
```

显式选择平台：

```bash
./deploy_stack.sh --qq
./deploy_stack.sh --wechat
./deploy_stack.sh --qq --wechat
```

指定 env 文件：

```bash
./deploy_stack.sh --qq --env-file deploy/dogbot.env
```

停止：

```bash
./stop_stack.sh
```

停止核心链路但保留平台容器：

```bash
./stop_stack.sh --keep-qq --keep-wechat
```

## 4. 必须修改的参数

大部分参数可以使用 `deploy/dogbot.env.example` 的默认值。下面这些需要按你的环境修改。

### 4.1 所有部署都要改

| 参数 | 说明 |
| --- | --- |
| `AGENT_WORKSPACE_DIR` | Agent 能读写的工作目录。建议用绝对路径。 |
| `AGENT_STATE_DIR` | 运行态目录，保存日志、会话、Postgres、平台状态。建议用绝对路径。 |
| `DOGBOT_CLAUDE_PROMPT_ROOT` | 运行时 prompt 同步目录。通常放在 `${AGENT_STATE_DIR}/claude-prompt`。 |
| `POSTGRES_DATA_DIR` | Postgres 数据目录。通常放在 `${AGENT_STATE_DIR}/postgres`。 |
| `BIFROST_MODEL` | 容器内 Claude 使用的模型别名，例如 `primary/deepseek-v4-pro`。 |
| `API_PROXY_UPSTREAM_BASE_URL` | 真实上游模型地址。 |
| `API_PROXY_UPSTREAM_TOKEN` | 真实上游 token，只保存在宿主机。 |
| `API_PROXY_UPSTREAM_MODEL` | 可选但常用。把容器内模型别名改写成真实上游模型名。 |

### 4.2 QQ 部署要改

| 参数 | 说明 |
| --- | --- |
| `ENABLE_QQ=1` | 启用 QQ。 |
| `PLATFORM_QQ_BOT_ID` | 登录的 QQ 号。NapCat 配置文件名依赖它。 |
| `PLATFORM_QQ_ACCOUNT_ID` | 平台账号隔离键，推荐 `qq:bot_uin:<你的QQ号>`。 |

如果改了 `AGENT_RUNNER_BIND_ADDR` 的端口，还要同步改：

```env
NAPCAT_HTTP_CLIENT_URL=http://host.docker.internal:<agent-runner-port>/v1/platforms/qq/napcat/events
```

### 4.3 微信部署要改

| 参数 | 说明 |
| --- | --- |
| `ENABLE_WECHATPADPRO=1` | 启用微信。 |
| `WECHATPADPRO_ADMIN_KEY` | WeChatPadPro 管理 key。必须换成自己的值。 |
| `WECHATPADPRO_MYSQL_ROOT_PASSWORD` | MySQL root 密码。必须换。 |
| `WECHATPADPRO_MYSQL_PASSWORD` | WeChatPadPro 业务库密码。必须换。 |
| `PLATFORM_WECHATPADPRO_ACCOUNT_ID` | 平台账号隔离键，例如 `wechatpadpro:account:<wxid>`。 |
| `PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES` | 微信群里触发机器人的昵称，多个用逗号分隔。 |

`WECHATPADPRO_ACCOUNT_KEY` 不需要手写。登录脚本会生成或刷新，并写回 `deploy/dogbot.env`。

如果改了 `AGENT_RUNNER_BIND_ADDR` 的端口，还要同步改：

```env
WECHATPADPRO_WEBHOOK_URL=http://host.docker.internal:<agent-runner-port>/v1/platforms/wechatpadpro/events
```

### 4.4 端口冲突时才改

| 参数 | 默认 | 说明 |
| --- | --- | --- |
| `POSTGRES_PORT` | `15432` | 宿主机访问 Postgres 的端口。 |
| `AGENT_RUNNER_BIND_ADDR` | `0.0.0.0:8787` | 平台事件入口。 |
| `API_PROXY_BIND_ADDR` | `0.0.0.0:9000` | 宿主机本地模型代理入口。 |
| `API_PROXY_PORT` | `9000` | 网络策略放行端口，要和 `API_PROXY_BIND_ADDR` 一致。 |
| `NAPCAT_WEBUI_PORT` | `6099` | NapCat WebUI。 |
| `NAPCAT_ONEBOT_PORT` | `3001` | NapCat HTTP API。 |
| `WECHATPADPRO_HOST_PORT` | `38849` | WeChatPadPro API。 |

Postgres 默认使用 `15432`，避免和宿主机已有 `5432` 冲突。

## 5. 模型配置

### 5.1 DeepSeek Anthropic-compatible 示例

```env
BIFROST_PROVIDER_NAME=primary
BIFROST_MODEL=primary/deepseek-v4-pro
BIFROST_UPSTREAM_PROVIDER_TYPE=anthropic
BIFROST_UPSTREAM_BASE_URL=http://host.docker.internal:9000
BIFROST_UPSTREAM_API_KEY=local-proxy-token

API_PROXY_BIND_ADDR=0.0.0.0:9000
API_PROXY_AUTH_TOKEN=local-proxy-token
API_PROXY_UPSTREAM_BASE_URL=https://api.deepseek.com/anthropic
API_PROXY_UPSTREAM_TOKEN=你的真实 token
API_PROXY_UPSTREAM_AUTH_HEADER=x-api-key
API_PROXY_UPSTREAM_AUTH_SCHEME=
API_PROXY_UPSTREAM_MODEL=deepseek-v4-pro[1m]
```

说明：

- `BIFROST_MODEL` 是 Claude Code 在容器内看到的模型名。
- `API_PROXY_UPSTREAM_MODEL` 是真实上游模型名。
- `API_PROXY_AUTH_TOKEN` 必须和 `BIFROST_UPSTREAM_API_KEY` 一致。
- 真实 token 只写在 `API_PROXY_UPSTREAM_TOKEN`。

### 5.2 OpenAI-compatible 示例

如果上游是 OpenAI-compatible 网关，通常需要：

```env
BIFROST_MODEL=primary/gpt-5
BIFROST_UPSTREAM_PROVIDER_TYPE=openai
API_PROXY_UPSTREAM_BASE_URL=https://api.openai.com/v1
API_PROXY_UPSTREAM_TOKEN=你的真实 token
API_PROXY_UPSTREAM_AUTH_HEADER=authorization
API_PROXY_UPSTREAM_AUTH_SCHEME=Bearer
API_PROXY_UPSTREAM_MODEL=gpt-5
```

具体是否可用取决于上游是否完整支持 Claude Code 需要的工具调用语义。

### 5.3 切换模型后的 session

切换模型后，旧 Claude session 可能带有旧模型的 thinking 状态。如果出现：

```text
The content[].thinking in the thinking mode must be passed back to the API.
```

处理方式：

- 更新 PostgreSQL 中对应会话的 `claude_session_id`，让它生成新会话。
- 或删除/重置对应会话映射。

这是 session 状态不兼容，不是 QQ/NapCat 本身的问题。

## 6. 启动流程做了什么

`./deploy_stack.sh` 会按顺序执行：

1. 读取 `deploy/dogbot.env`。
2. 创建和修复运行态目录权限。
3. 同步 `claude-prompt/` 到 `DOGBOT_CLAUDE_PROMPT_ROOT`。
4. 生成 `claude-runner` 运行时 launch script。
5. 启动 PostgreSQL。
6. 编译并启动宿主机 `agent-runner`。
7. 启动 `claude-runner`。
8. 按选择启动 NapCat 和/或 WeChatPadPro。
9. 等待扫码登录。
10. 自动配置 NapCat HTTP callback / WeChatPadPro webhook。
11. 如果 `APPLY_NETWORK_POLICY=1`，为 `claude-runner` 应用网络限制。

## 7. 日志和状态

常用位置：

```text
${AGENT_STATE_DIR}/logs/agent-runner.log
${AGENT_STATE_DIR}/bifrost/bifrost.log
${AGENT_STATE_DIR}/bifrost/logs.db
${AGENT_STATE_DIR}/claude/
${AGENT_STATE_DIR}/postgres/
${AGENT_STATE_DIR}/napcat-login/
${AGENT_STATE_DIR}/wechatpadpro-login/
```

查看容器：

```bash
docker ps
docker logs claude-runner
docker logs napcat
docker logs wechatpadpro
docker logs dogbot-postgres
```

健康检查：

```bash
curl http://127.0.0.1:8787/healthz
```

如果你改了 `AGENT_RUNNER_BIND_ADDR` 端口，把 `8787` 替换成新端口。

## 8. 更新 prompt / 人格

修改源文件：

```text
claude-prompt/CLAUDE.md
claude-prompt/persona.md
claude-prompt/skills/
```

重新部署会同步到运行时目录：

```bash
./deploy_stack.sh --qq
```

不要直接修改 `${DOGBOT_CLAUDE_PROMPT_ROOT}` 下的文件；它是部署产物，下次部署会被覆盖。

## 9. 常见问题

### 9.1 `address already in use`

先看冲突端口。常见改法：

- Postgres 冲突：改 `POSTGRES_PORT`。
- agent-runner 冲突：改 `AGENT_RUNNER_BIND_ADDR`，并同步改平台回调 URL。
- api-proxy 冲突：改 `API_PROXY_BIND_ADDR` 和 `API_PROXY_PORT`。
- NapCat 冲突：改 `NAPCAT_WEBUI_PORT` / `NAPCAT_ONEBOT_PORT`。
- WeChatPadPro 冲突：改 `WECHATPADPRO_HOST_PORT`。

### 9.2 `host.docker.internal connection refused`

通常是宿主机服务绑定到了 `127.0.0.1`，容器无法访问。需要绑定 `0.0.0.0`：

```env
API_PROXY_BIND_ADDR=0.0.0.0:9000
AGENT_RUNNER_BIND_ADDR=0.0.0.0:8787
```

还要确认 `APPLY_NETWORK_POLICY=1` 时 `API_PROXY_PORT` 和真实端口一致。

### 9.3 Docker 拉镜像失败

检查 Docker daemon 网络。`HTTP_PROXY` / `HTTPS_PROXY` 只会注入容器，不能替代 Docker daemon 自身代理配置。

### 9.4 QQ 私聊有回复，群聊没回复

群聊必须显式 `@机器人`。同时确认：

```env
PLATFORM_QQ_BOT_ID=<实际登录 QQ 号>
```

### 9.5 微信群聊没回复

确认群聊消息里使用的昵称在：

```env
PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES=DogDu,另一个昵称
```

如果不想要求 mention，可改：

```env
WECHATPADPRO_REQUIRE_MENTION_IN_GROUP=0
```

### 9.6 NapCat reaction 失败

NapCat 有时会返回 reaction 业务失败。DogBot 会记录 warning 并继续执行任务；这通常不影响最终文本回复。

### 9.7 Claude 容器里手动运行正常，平台消息失败

优先看：

- 平台消息是否复用了旧 Claude session。
- `agent-runner.log` 里的 `scheduled runner completed` exit code。
- `bifrost/logs.db` 里的上游错误。
- 是否改了模型但没有重置对应会话。

### 9.8 如何重建 claude-runner 镜像

改了 `CLAUDE_CODE_VERSION` 或 Dockerfile 后：

```bash
docker compose \
  --project-name dogbot \
  --project-directory . \
  --env-file deploy/dogbot.env \
  -f deploy/docker/docker-compose.yml \
  build claude-runner

./stop_stack.sh --keep-qq --keep-wechat
./deploy_stack.sh --qq --wechat
```

### 9.9 如何临时关闭网络限制

调试时可在 `deploy/dogbot.env` 中设置：

```env
APPLY_NETWORK_POLICY=0
```

确认问题后再改回 `1`。
