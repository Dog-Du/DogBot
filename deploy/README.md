# DogBot 部署说明

本文档说明如何部署当前仓库里的 `DogBot`。

当前支持两条主要链路：

```text
QQ
-> NapCat
-> AstrBot
-> claude_runner_bridge
-> agent-runner
-> claude-runner
-> agent-runner 内置 Anthropic 兼容代理
-> 上游模型服务

微信
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner
-> agent-runner 内置 Anthropic 兼容代理
-> 上游模型服务
```

## 1. 宿主机依赖

必需：

- Linux
- Docker Engine
- Docker Compose v2
- Rust 工具链（`cargo`）
- `uv`
- `curl`
- `sudo`

推荐：

- `git`
- `rg`

快速检查：

```bash
docker --version
docker compose version
cargo --version
uv --version
```

如果当前用户不能直接访问 Docker，可以：

```bash
sudo usermod -aG docker "$USER"
newgrp docker
```

或者直接在部署命令前加 `sudo`。

## 2. 重要文件

最重要的配置和脚本如下：

- [deploy/dogbot.env.example](/home/dogdu/workspace/myQQbot/deploy/dogbot.env.example)
  - 默认配置模板
- `deploy/dogbot.env`
  - 你自己的实际部署配置
- `deploy/myqqbot.env`
  - 旧文件名，仍兼容，但建议迁移到 `dogbot.env`
- [compose/docker-compose.yml](/home/dogdu/workspace/myQQbot/compose/docker-compose.yml)
  - `claude-runner` 容器定义
- [compose/platform-stack.yml](/home/dogdu/workspace/myQQbot/compose/platform-stack.yml)
  - `napcat` / `astrbot` 容器定义
- [compose/wechatpadpro-stack.yml](/home/dogdu/workspace/myQQbot/compose/wechatpadpro-stack.yml)
  - `wechatpadpro` / MySQL / Redis 容器定义
- [scripts/deploy_stack.sh](/home/dogdu/workspace/myQQbot/scripts/deploy_stack.sh)
  - 一键启动
- [scripts/stop_stack.sh](/home/dogdu/workspace/myQQbot/scripts/stop_stack.sh)
  - 一键停止
- [scripts/start_agent_runner.sh](/home/dogdu/workspace/myQQbot/scripts/start_agent_runner.sh)
  - 启动宿主机 `agent-runner`
- [scripts/start_wechatpadpro_adapter.sh](/home/dogdu/workspace/myQQbot/scripts/start_wechatpadpro_adapter.sh)
  - 启动宿主机微信适配器

## 3. 快速开始

### 3.1 复制配置模板

```bash
cp deploy/dogbot.env.example deploy/dogbot.env
```

如果你仍然沿用旧名字，也可以：

```bash
cp deploy/dogbot.env.example deploy/myqqbot.env
```

### 3.2 编辑配置文件

至少要改这些项：

- 工作目录和状态目录
- `AGENT_RUNNER_BIND_ADDR`
- 上游 provider 相关配置
- 上游 key
- QQ / 微信相关目录和端口

### 3.3 启动

```bash
./scripts/deploy_stack.sh deploy/dogbot.env
```

如果 Docker 权限不够：

```bash
sudo ./scripts/deploy_stack.sh deploy/dogbot.env
```

### 3.4 停止

```bash
./scripts/stop_stack.sh deploy/dogbot.env
```

## 4. 配置文件说明

推荐使用：

- [deploy/dogbot.env.example](/home/dogdu/workspace/myQQbot/deploy/dogbot.env.example)

模板里已经为每个字段补了中文注释。下面只强调最重要的几组。

### 4.1 Claude 容器

```env
CLAUDE_CONTAINER_NAME=claude-runner
CLAUDE_IMAGE_NAME=dogbot/claude-runner:local
CLAUDE_CODE_VERSION=2.1.104
```

含义：

- Claude 容器名
- Claude 镜像名
- 镜像内安装的 Claude Code 版本

### 4.2 工作目录和状态目录

```env
AGENT_WORKSPACE_DIR=/srv/agent-workdir
AGENT_STATE_DIR=/srv/agent-state
SESSION_DB_PATH=/srv/agent-state/runner.db
```

建议：

- `AGENT_WORKSPACE_DIR` 给 Agent 读写业务工作目录
- `AGENT_STATE_DIR` 用来保存：
  - Claude 会话状态
  - SQLite 数据库
  - 日志
  - NapCat / AstrBot / WeChatPadPro 状态

如果你改这些路径，旧会话和旧状态看起来会像“丢了”。

### 4.3 agent-runner 与内置代理

```env
AGENT_RUNNER_BIND_ADDR=127.0.0.1:8787
API_PROXY_BIND_ADDR=0.0.0.0:9000
ANTHROPIC_BASE_URL=http://host.docker.internal:9000
API_PROXY_AUTH_TOKEN=local-proxy-token
```

说明：

- `AGENT_RUNNER_BIND_ADDR` 给 AstrBot / 微信 adapter 调用
- `API_PROXY_BIND_ADDR` 给 Docker 里的 Claude 调用
- `API_PROXY_BIND_ADDR` 不能绑到 `127.0.0.1`，否则 Docker 内访问不到
- `API_PROXY_AUTH_TOKEN` 不是上游真实 key，只是本地代理 token

### 4.4 上游 provider 配置

当前支持的思路是：

- `packy`
- `glm_official`
- `minimax_official`

例如：

```env
API_PROXY_ACTIVE_PROVIDER=packy
API_PROXY_PACKY_BASE_URL=https://www.packyapi.com
API_PROXY_PACKY_UPSTREAM_TOKEN=你的真实token
API_PROXY_PACKY_AUTH_HEADER=x-api-key
```

切换到 GLM 时，改成：

```env
API_PROXY_ACTIVE_PROVIDER=glm_official
API_PROXY_GLM_BASE_URL=https://open.bigmodel.cn/api/anthropic
API_PROXY_GLM_UPSTREAM_TOKEN=你的GLM key
API_PROXY_GLM_AUTH_HEADER=Authorization
API_PROXY_GLM_AUTH_SCHEME=Bearer
```

## 5. NapCat 配置

### 5.1 WebUI

默认端口：

```text
http://127.0.0.1:6099
```

### 5.2 登录 QQ

- 打开 NapCat WebUI
- 扫码登录

### 5.3 反向 WebSocket

当前工程要求 `NapCat` 把 OneBot 事件推给 `AstrBot`。

目标地址：

```text
ws://astrbot:6199/ws
```

这部分现在由脚本自动写入：

- [scripts/configure_napcat_ws.sh](/home/dogdu/workspace/myQQbot/scripts/configure_napcat_ws.sh)

正常情况下不需要你手动改容器内配置。

## 6. AstrBot 配置

### 6.1 WebUI

默认地址：

```text
http://127.0.0.1:6185
```

### 6.2 QQ 机器人

在 AstrBot 里创建 `OneBot v11` 机器人，关键配置：

- 反向 WebSocket 主机：`0.0.0.0`
- 反向 WebSocket 端口：`6199`

### 6.3 触发规则

当前项目统一规则如下：

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status` 保留

## 7. WeChatPadPro 配置

### 7.1 启用

```env
ENABLE_WECHATPADPRO=1
```

### 7.2 容器

会额外启动：

- `wechatpadpro`
- `wechatpadpro_mysql`
- `wechatpadpro_redis`

### 7.3 登录

部署脚本会自动：

- 生成 `WECHATPADPRO_ACCOUNT_KEY`
- 拉取登录二维码
- 把二维码图片和元信息写到：
  - `WECHATPADPRO_LOGIN_OUTPUT_DIR`

### 7.4 微信适配器

微信不经过 AstrBot，而是：

```text
WeChatPadPro -> wechatpadpro-adapter -> agent-runner
```

适配器启动脚本：

- [scripts/start_wechatpadpro_adapter.sh](/home/dogdu/workspace/myQQbot/scripts/start_wechatpadpro_adapter.sh)

## 8. Docker 资源限制

关键字段：

```env
CLAUDE_CONTAINER_CPU_CORES=4
CLAUDE_CONTAINER_MEMORY_MB=4096
CLAUDE_CONTAINER_DISK_GB=50
CLAUDE_CONTAINER_PIDS_LIMIT=256
```

当前实际情况：

- CPU / memory / pids 限制生效
- `disk=50` 只是目标值，不一定是宿主机上的硬限制
- 如果宿主机文件系统不支持 Docker 层磁盘配额，仍需要宿主机级别的限额方案

## 9. 如何修改 API key 和模型

### 9.1 改 key

直接修改你的真实环境文件，例如：

```env
API_PROXY_PACKY_UPSTREAM_TOKEN=新的token
```

或：

```env
API_PROXY_GLM_UPSTREAM_TOKEN=新的token
```

### 9.2 改 provider

例如切到 GLM：

```env
API_PROXY_ACTIVE_PROVIDER=glm_official
```

### 9.3 改模型

如果上游支持模型字段：

```env
API_PROXY_PACKY_MODEL=xxx
API_PROXY_GLM_MODEL=xxx
API_PROXY_MINIMAX_MODEL=xxx
```

### 9.4 重新生效

改完后重启：

```bash
./scripts/stop_stack.sh deploy/dogbot.env
sudo ./scripts/deploy_stack.sh deploy/dogbot.env
```

## 10. 需要特别注意的事情

### 10.1 只能使用 Claude / Anthropic 兼容的 Base URL

现在 Docker 里的 Claude Code 调用的是：

- Anthropic Messages 兼容接口

所以你必须给它配置：

- `Anthropic-compatible`
- 或 `Claude-compatible`

的上游地址。

不能把只兼容 OpenAI 的地址直接塞给 Claude Code。

### 10.2 真实 key 不进 Docker

真实 provider key 只放在宿主机环境变量里。  
Docker 里的 `claude-runner` 只拿到：

- `ANTHROPIC_BASE_URL=http://host.docker.internal:9000`
- `ANTHROPIC_AUTH_TOKEN=local-proxy-token`

### 10.3 QQ 登录态容易受重建影响

避免无意义地重建 `napcat`。  
只要 `napcat` 容器和数据目录不乱动，就不应该频繁要求重新扫码。

### 10.4 WeChatPadPro 仍有自身稳定性问题

当前已经确认过：

- webhook 群聊链路可用
- 某些场景下 DNS 和长连接稳定性会影响消息同步

如果后续继续遇到微信私聊推送异常，优先排查：

- `wechatpadpro` 容器 DNS
- `GetSyncMsg / HttpSyncMsg` 是否能拿到消息

## 11. 常用命令

### 11.1 启动

```bash
./scripts/deploy_stack.sh deploy/dogbot.env
```

### 11.2 停止

```bash
./scripts/stop_stack.sh deploy/dogbot.env
```

### 11.3 检查 runner

```bash
curl http://127.0.0.1:8787/healthz
```

### 11.4 主动向已有会话发消息

```bash
./scripts/send_session_message.sh \
  --env-file deploy/dogbot.env \
  --session-id qq:private:123456 \
  --text "hello from cron"
```

## 12. 兼容说明

当前仓库已经把项目名统一成 `DogBot`，但为了不把现有环境直接改坏：

- 脚本优先读取 `deploy/dogbot.env`
- 若不存在，会自动回退到 `deploy/myqqbot.env`

建议你后续逐步把本地配置迁移到：

- `deploy/dogbot.env`
