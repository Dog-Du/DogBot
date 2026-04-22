# DogBot 部署说明

本文档说明如何部署当前仓库里的 `DogBot`。

当前部署遵循三条原则：

1. 用户只修改 `deploy/dogbot.env`
2. 用户只通过 `./deploy_stack.sh` 启动
3. 用户只通过 `./stop_stack.sh` 停止

当前 `./deploy_stack.sh` 支持两种模式：

- 无参数运行
  - 进入交互式平台选择
- 显式参数运行
  - `--qq`
  - `--wechat`
  - `--qq --wechat`

Claude prompt 内容当前也已经接入部署流程：

- 部署前会把仓库中的 `claude-prompt/` 同步到外部 `DOGBOT_CLAUDE_PROMPT_ROOT`
- `agent-runner` 运行时从该目录为 Claude 提供静态 `CLAUDE.md` 和 `.claude/skills`
- 动态 scope / history / session 约束仍由 `agent-runner` 在每次运行时注入

当前支持两条主要链路：

```text
QQ
-> NapCat
-> qq-adapter
-> agent-runner
-> claude-runner
-> agent-runner 内置上游代理
-> 上游模型服务

微信
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner
-> agent-runner 内置上游代理
-> 上游模型服务
```

## 1. 部署依赖

下面这些是当前仓库部署 `DogBot` 所需的完整前置条件。

### 1.1 必需软件

- `Linux`
  - 当前部署方案默认以 Linux 宿主机为目标
- `uv`
  - 用来运行 Python 相关脚本、适配器和测试
- `Docker Engine`
  - 用来运行 `claude-runner`、`NapCat`、`WeChatPadPro`
- `Docker Compose v2`
  - 用来编排多个容器栈
- `Rust` / `cargo`
  - 用来编译和运行宿主机上的 `agent-runner`
- `curl`
  - 用于接口联调、健康检查和脚本诊断
- `sudo`
  - 某些 Docker、iptables、网络策略和系统级操作需要 root 权限

### 1.2 必需外部条件

- 一个可用的 `Claude 协议模型源`
  - 当前 Docker 内的 Claude Code 只能直接使用 Claude 协议接口
  - 你需要自行提供可用的上游地址和对应 key
- 至少一个可登录的平台账号
  - QQ：个人 QQ 号，供 `NapCat` 登录
  - 微信：个人微信号，供 `WeChatPadPro` 登录

### 1.3 可选但推荐

- `git`
- `rg`

### 1.4 快速检查

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

### 1.5 重要说明

- 不能把 OpenAI 协议地址直接给当前 Docker 内的 Claude Code 使用
- 真实上游 key 只应保留在宿主机，不能直接注入 `claude-runner` 容器
- 当前工程已经把真实 key 隔离在宿主机 `agent-runner` 的内置代理里

## 2. 配置与入口

正常情况下，用户只需要关心下面三个入口：

- `deploy/dogbot.env`
- `./deploy_stack.sh`
- `./stop_stack.sh`

`compose/` 目录默认不需要修改；如果你确实要调整容器层行为，请查看 `compose/README.md`。

## 3. 重要文件

最重要的配置和脚本如下：

- `deploy/dogbot.env`
  - 你自己的实际部署配置
- `deploy/dogbot.env.example`
  - 默认配置模板
- `./deploy_stack.sh`
  - 根目录部署入口
- `./stop_stack.sh`
  - 根目录停止入口
- `compose/docker-compose.yml`
  - `claude-runner` 容器定义
- `compose/platform-stack.yml`
  - `napcat` 容器定义
- `compose/wechatpadpro-stack.yml`
  - `wechatpadpro` / MySQL / Redis 容器定义
- `scripts/start_agent_runner.sh`
  - 启动宿主机 `agent-runner`
- `scripts/start_qq_adapter.sh`
  - 启动宿主机 QQ 适配器
- `scripts/start_wechatpadpro_adapter.sh`
  - 启动宿主机微信适配器

## 4. 快速开始

### 4.1 复制配置模板

```bash
cp deploy/dogbot.env.example deploy/dogbot.env
```

### 4.2 编辑配置文件

至少要改这些项：

- 工作目录和状态目录
- `AGENT_RUNNER_BIND_ADDR`
- 上游配置
- 上游 key
- `QQ_ADAPTER_QQ_BOT_ID`
- `QQ_PLATFORM_ACCOUNT_ID`
- `WECHATPADPRO_ADMIN_KEY`
- `WECHATPADPRO_MYSQL_ROOT_PASSWORD`
- `WECHATPADPRO_MYSQL_PASSWORD`
- `WECHATPADPRO_PLATFORM_ACCOUNT_ID`
- 如果保留群聊 mention 门禁，需要把 `WECHATPADPRO_BOT_MENTION_NAMES` 改成你的机器人群昵称
- QQ / 微信相关目录和端口

### 4.3 启动

```bash
./deploy_stack.sh
```

默认会进入交互式平台选择：

- 询问是否启用 QQ
- 询问是否启用微信
- 如果选择了对应平台，会自动准备登录二维码并阻塞等待扫码
- 若 100 秒内未完成扫码，部署脚本会退出

也可以显式指定平台：

```bash
./deploy_stack.sh --qq
./deploy_stack.sh --wechat
./deploy_stack.sh --qq --wechat
```

说明：

- 示例配置默认启用了 `WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1`
- 如果你手动改成 `0`，部署脚本不会替你注册 webhook
- 这时必须自行配置 `WECHATPADPRO_ADAPTER_WEBHOOK_URL`
- 部署前会自动把仓库内 `claude-prompt/` 同步到 `DOGBOT_CLAUDE_PROMPT_ROOT`

如果 Docker 权限不够：

```bash
sudo ./deploy_stack.sh
```

如果你希望显式指定配置文件，也可以：

```bash
./deploy_stack.sh deploy/dogbot.env
./deploy_stack.sh --qq --env-file deploy/dogbot.env
```

### 4.4 停止

```bash
./stop_stack.sh
```

如果你希望显式指定配置文件，也可以：

```bash
./stop_stack.sh deploy/dogbot.env
```

## 5. 配置文件说明

推荐使用：

- `deploy/dogbot.env.example`

模板里已经为每个字段补了中文注释。下面只强调最重要的几组。

### 5.1 Claude 容器

```env
CLAUDE_CONTAINER_NAME=claude-runner
CLAUDE_IMAGE_NAME=dogbot/claude-runner:local
CLAUDE_CODE_VERSION=2.1.104
```

含义：

- Claude 容器名
- Claude 镜像名
- 镜像内安装的 Claude Code 版本

### 5.2 工作目录和状态目录

```env
AGENT_WORKSPACE_DIR=/srv/dogbot/runtime/agent-workspace
AGENT_STATE_DIR=/srv/dogbot/runtime/agent-state
SESSION_DB_PATH=/srv/dogbot/runtime/agent-state/runner.db
CONTROL_PLANE_DB_PATH=/srv/dogbot/runtime/agent-state/control.db
HISTORY_DB_PATH=/srv/dogbot/runtime/agent-state/history.db
DOGBOT_CLAUDE_PROMPT_ROOT=/srv/dogbot/runtime/agent-state/claude-prompt
DOGBOT_ADMIN_ACTOR_IDS=qq:user:10001,wechat:user:wxid_admin
```

建议：

- `AGENT_WORKSPACE_DIR` 给 Agent 读写业务工作目录
- 建议把 `AGENT_WORKSPACE_DIR` 和 `AGENT_STATE_DIR` 放到同一个 `runtime/` 根目录下
- `AGENT_STATE_DIR` 用来保存：
  - Claude 会话状态
  - SQLite 数据库
  - 日志
  - NapCat / WeChatPadPro 状态
- `SESSION_DB_PATH` 保存短期 Claude session 映射
- `CONTROL_PLANE_DB_PATH` 保存 memory candidate、authorization 等 control-plane 数据
- `HISTORY_DB_PATH` 保存 history ingest 和 retrieval 基础数据
- `DOGBOT_CLAUDE_PROMPT_ROOT` 推荐使用绝对路径，指向运行时 Claude prompt 根目录
- 部署脚本会把仓库内 `claude-prompt/` 同步到 `DOGBOT_CLAUDE_PROMPT_ROOT`
- `DOGBOT_ADMIN_ACTOR_IDS` 用逗号分隔管理员 actor ID
- `WeChatPadPro` 的 `data/mysql/redis` 目录也建议放到 `AGENT_STATE_DIR/wechatpadpro-data/`

如果你改这些路径，旧会话和旧状态看起来会像“丢了”。

### 5.3 平台账号隔离键

建议显式设置：

```env
QQ_PLATFORM_ACCOUNT_ID=qq:bot_uin:123456
WECHATPADPRO_PLATFORM_ACCOUNT_ID=wechatpadpro:account:wxid_bot_1
```

作用：

- 作为 `platform-account-shared` scope 的隔离键
- 避免多个机器人账号共用同一套 platform 级上下文

### 5.4 agent-runner 与内置代理

```env
AGENT_RUNNER_BIND_ADDR=127.0.0.1:8787
API_PROXY_BIND_ADDR=0.0.0.0:9000
ANTHROPIC_BASE_URL=http://host.docker.internal:9000
API_PROXY_AUTH_TOKEN=local-proxy-token
```

说明：

- `AGENT_RUNNER_BIND_ADDR` 给 QQ / 微信 adapter 调用
- `API_PROXY_BIND_ADDR` 给 Docker 里的 Claude 调用
- `API_PROXY_BIND_ADDR` 不能绑到 `127.0.0.1`，否则 Docker 内访问不到
- `API_PROXY_AUTH_TOKEN` 不是上游真实 key，只是本地代理 token

### 5.5 上游配置

当前只保留一套 Claude 协议上游配置：

```env
API_PROXY_UPSTREAM_BASE_URL=https://example.com
API_PROXY_UPSTREAM_TOKEN=你的真实 token
API_PROXY_UPSTREAM_AUTH_HEADER=x-api-key
# API_PROXY_UPSTREAM_AUTH_SCHEME=
# API_PROXY_UPSTREAM_MODEL=
```

说明：

- `API_PROXY_UPSTREAM_BASE_URL` 是 Claude 容器最终访问的模型源地址
- `API_PROXY_UPSTREAM_TOKEN` 是真实上游 key，只保留在宿主机
- `API_PROXY_UPSTREAM_AUTH_HEADER` 和 `API_PROXY_UPSTREAM_AUTH_SCHEME` 用来适配上游鉴权方式
- `API_PROXY_UPSTREAM_MODEL` 可选，填写后会覆盖请求体里的模型名

## 6. NapCat 配置

### 6.1 WebUI

默认端口：

```text
http://127.0.0.1:6099
```

### 6.2 登录 QQ

- 打开 NapCat WebUI
- 扫码登录
- 部署脚本也会自动准备 NapCat 登录二维码：
  - 如果本机安装了 `qrencode`，会直接在终端打印二维码
  - 同时保留二维码图片和原始登录链接
  - 脚本会阻塞等待扫码；若 100 秒内未完成扫码会退出

### 6.3 反向 WebSocket

当前工程要求 `NapCat` 把 OneBot 事件推给宿主机上的 `qq-adapter`。

目标地址：

```text
ws://host.docker.internal:19000/napcat/ws
```

这部分现在由脚本自动写入：

- `scripts/configure_napcat_ws.sh`

正常情况下不需要你手动改容器内配置。

## 7. QQ adapter 配置

QQ 链路为：

```text
NapCat -> qq-adapter -> agent-runner
```

适配器启动脚本：

- `scripts/start_qq_adapter.sh`

关键配置：

```env
QQ_ADAPTER_BIND_ADDR=0.0.0.0:19000
QQ_ADAPTER_QQ_BOT_ID=你的QQ号
QQ_PLATFORM_ACCOUNT_ID=qq:bot_uin:你的机器人QQ号
QQ_ADAPTER_COMMAND_NAME=agent
QQ_ADAPTER_STATUS_COMMAND_NAME=agent-status
```

## 8. 触发规则

当前项目统一规则如下：

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status` 保留

补充说明：

- `agent-runner` 内部的 normalized trigger resolver 已经支持更宽松的识别
- 当前两个 adapter 仍保留兼容性的本地 command gate
- 部署和联调请仍按上面的显式命令规则验收

## 9. WeChatPadPro 配置

### 9.1 启用

```env
ENABLE_WECHATPADPRO=1
```

### 9.2 容器

会额外启动：

- `wechatpadpro`
- `wechatpadpro_mysql`
- `wechatpadpro_redis`

### 9.3 登录

部署脚本会自动：

- 生成 `WECHATPADPRO_ACCOUNT_KEY`
- 拉取登录二维码
- 如果二维码过期，脚本会刷新本地二维码文件
- 如果本机安装了 `qrencode`，会直接在终端打印二维码
- 把二维码图片和元信息写到：
  - `WECHATPADPRO_LOGIN_OUTPUT_DIR`
- 阻塞等待扫码；若 100 秒内未完成扫码会退出

### 9.4 微信适配器

微信不经过任何额外编排层，而是：

```text
WeChatPadPro -> wechatpadpro-adapter -> agent-runner
```

适配器启动脚本：

- `scripts/start_wechatpadpro_adapter.sh`

关键配置：

```env
WECHATPADPRO_AGENT_RUNNER_BASE_URL=http://127.0.0.1:8787
WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1
WECHATPADPRO_ADAPTER_WEBHOOK_URL=http://host.docker.internal:18999/wechatpadpro/events
WECHATPADPRO_REQUIRE_MENTION_IN_GROUP=1
WECHATPADPRO_BOT_MENTION_NAMES=DogDu
WECHATPADPRO_PLATFORM_ACCOUNT_ID=wechatpadpro:account:你的机器人账号
```

说明：

- 示例配置默认会自动向 WeChatPadPro 注册 webhook
- 如果关闭 `WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK`，必须手动配置 webhook，否则 adapter 不会收到消息
- 如果启用了 `WECHATPADPRO_REQUIRE_MENTION_IN_GROUP=1`，`WECHATPADPRO_BOT_MENTION_NAMES` 不能为空

## 10. Docker 资源限制

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

## 11. 如何修改 API key 和模型

### 11.1 改 key

直接修改你的真实环境文件：

```env
API_PROXY_UPSTREAM_TOKEN=新的 token
```

### 11.2 改模型源地址

如果要切到另一个模型源，改这里：

```env
API_PROXY_UPSTREAM_BASE_URL=https://example.com
```

### 11.3 改模型

如果上游支持模型字段：

```env
API_PROXY_UPSTREAM_MODEL=xxx
```

### 11.4 重新生效

改完后重启：

```bash
./stop_stack.sh
sudo ./deploy_stack.sh
```

## 12. 需要特别注意的事情

### 12.1 只能使用 Claude 协议的 Base URL

现在 Docker 里的 Claude Code 调用的是 Claude 协议接口。

所以你必须给它配置：

- `Anthropic-compatible`
- 或 `Claude-compatible`

的上游地址。

不能把 OpenAI 协议地址直接塞给 Claude Code。

### 12.2 真实 key 不进 Docker

真实 provider key 只放在宿主机环境变量里。  
Docker 里的 `claude-runner` 只拿到：

- `ANTHROPIC_BASE_URL=http://host.docker.internal:9000`
- `ANTHROPIC_AUTH_TOKEN=local-proxy-token`

### 12.3 QQ 登录态容易受重建影响

避免无意义地重建 `napcat`。  
只要 `napcat` 容器和数据目录不乱动，就不应该频繁要求重新扫码。

### 12.4 WeChatPadPro 仍有自身稳定性问题

当前已经确认过：

- webhook 群聊链路可用
- 某些场景下 DNS 和长连接稳定性会影响消息同步

如果后续继续遇到微信私聊推送异常，优先排查：

- `wechatpadpro` 容器 DNS
- `GetSyncMsg / HttpSyncMsg` 是否能拿到消息

## 13. 常用命令

### 13.1 启动

```bash
./deploy_stack.sh
```

### 13.2 停止

```bash
./stop_stack.sh
```

### 13.3 检查 runner

```bash
curl http://127.0.0.1:8787/healthz
```

### 13.4 主动向已有会话发消息

```bash
./scripts/send_session_message.sh \
  --env-file deploy/dogbot.env \
  --session-id qq:private:123456 \
  --text "hello from cron"
```

## 14. 环境文件

当前仓库统一使用：

- `deploy/dogbot.env`

## 15. Control Plane 联调

本轮控制面 A/B/C 改造的联调和验收说明单独整理在：

- `docs/control-plane-integration.md`

建议先按本文完成部署，再按该文档做：

- 健康检查
- QQ / WeChat 平台侧手工回归
- `control.db` / `history.db` 核对
- Rust / Python 回归命令
