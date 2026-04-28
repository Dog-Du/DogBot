# DogBot

DogBot 是一个个人账号机器人项目。当前主要支持 QQ 和微信个人号接入，并把平台消息统一交给同一套 CLI Agent 后端处理。

当前链路：

```text
QQ -> NapCat -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> agent-runner -> claude-runner
```

`agent-runner` 运行在宿主机上，负责平台入口、调度、会话映射、历史消息、消息回发和上游模型代理。`claude-runner` 运行在 Docker 里，只执行 Claude Code，并通过容器内 Bifrost 访问宿主机上的本地模型代理。

## 现在能做什么

- QQ 私聊：任意非空文本触发 Agent。
- QQ 群聊：必须显式 `@机器人 + 正文`。
- 微信私聊：任意非空文本触发 Agent。
- 微信群聊：必须显式 `@机器人名 + 正文`。
- `/agent-status`：查询运行状态。
- 同一会话串行执行，多个会话受全局并发上限控制。
- 文本历史消息保存到 PostgreSQL，Agent 通过 `history-read` skill 读取。
- 普通 Agent 只能读取当前会话历史；白名单 admin 在私聊里可以读取更大范围历史。

## 架构

```text
平台事件
  -> agent-runner HTTP ingress
  -> trigger resolver
  -> per-conversation scheduler
  -> Claude Code exec in claude-runner
  -> Bifrost inside claude-runner
  -> agent-runner api-proxy on host
  -> upstream model provider
  -> outbound normalizer
  -> platform reply / mention / reaction
```

核心边界：

- 真实上游模型 key 只放在宿主机的 `deploy/dogbot.env`。
- Docker 内的 Claude Code 只看到 `ANTHROPIC_BASE_URL=http://127.0.0.1:8080/anthropic` 和 dummy key。
- 容器内 Bifrost 使用本地代理 token 访问宿主机 `agent-runner` 的 api-proxy。
- PostgreSQL 是唯一持久化数据库；旧的 sqlite `runner.db` / `history.db` 不再使用。

## 快速开始

准备依赖：

- Linux
- Docker Engine
- Docker Compose v2
- Rust / cargo
- curl
- uv

复制配置：

```bash
cp deploy/dogbot.env.example deploy/dogbot.env
```

只需要先改少量参数。完整说明见 [deploy/README.md](deploy/README.md)。

必须关注的参数：

| 场景 | 参数 |
| --- | --- |
| 所有部署 | `AGENT_WORKSPACE_DIR`, `AGENT_STATE_DIR` |
| 所有部署 | `BIFROST_MODEL`, `API_PROXY_UPSTREAM_BASE_URL`, `API_PROXY_UPSTREAM_TOKEN`, `API_PROXY_UPSTREAM_MODEL` |
| QQ | `ENABLE_QQ=1`, `PLATFORM_QQ_BOT_ID`, `PLATFORM_QQ_ACCOUNT_ID` |
| 微信 | `ENABLE_WECHATPADPRO=1`, `WECHATPADPRO_ADMIN_KEY`, `WECHATPADPRO_MYSQL_ROOT_PASSWORD`, `WECHATPADPRO_MYSQL_PASSWORD`, `PLATFORM_WECHATPADPRO_ACCOUNT_ID`, `PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES` |
| 端口冲突 | `POSTGRES_PORT`, `AGENT_RUNNER_BIND_ADDR`, `API_PROXY_BIND_ADDR`, `NAPCAT_WEBUI_PORT`, `NAPCAT_ONEBOT_PORT`, `WECHATPADPRO_HOST_PORT` |

启动：

```bash
./deploy_stack.sh
```

或显式指定平台：

```bash
./deploy_stack.sh --qq
./deploy_stack.sh --wechat
./deploy_stack.sh --qq --wechat
```

停止：

```bash
./stop_stack.sh
```

保留平台容器只重启核心链路：

```bash
./stop_stack.sh --keep-qq --keep-wechat
./deploy_stack.sh --qq --wechat
```

## 目录

```text
agent-runner/       Rust 核心服务
claude-prompt/      Claude Code 的静态 CLAUDE.md、persona 和 skills
deploy/             部署文档、env 模板、Docker Compose 定义
docs/               设计文档
scripts/            启停、登录、配置、诊断脚本
runtime/            推荐的本地运行态目录，默认不提交
```

## Prompt 和人格

仓库里的静态 prompt 在 `claude-prompt/`：

- `claude-prompt/CLAUDE.md`：运行边界、回复格式、history skill 指引。
- `claude-prompt/persona.md`：默认人格。
- `claude-prompt/skills/`：DogBot 自带 skills。

部署时脚本会把 `claude-prompt/` 同步到 `DOGBOT_CLAUDE_PROMPT_ROOT`。要修改人格，优先改 `claude-prompt/persona.md`，然后重新运行：

```bash
./deploy_stack.sh --qq
```

如果只想同步 prompt 并重启 runner，可以先停止再启动；平台容器可用 `--keep-qq` / `--keep-wechat` 保留。

## 模型链路

DogBot 默认使用：

```text
Claude Code -> Bifrost -> agent-runner api-proxy -> upstream
```

推荐把容器内模型名写成 Bifrost provider alias，把真实上游模型名写在 api-proxy：

```env
BIFROST_PROVIDER_NAME=primary
BIFROST_MODEL=primary/deepseek-v4-pro
API_PROXY_UPSTREAM_BASE_URL=https://api.deepseek.com/anthropic
API_PROXY_UPSTREAM_TOKEN=...
API_PROXY_UPSTREAM_AUTH_HEADER=x-api-key
API_PROXY_UPSTREAM_AUTH_SCHEME=
API_PROXY_UPSTREAM_MODEL=deepseek-v4-pro[1m]
```

切换模型后，如果复用旧 Claude session 报 thinking 相关错误，更新 PostgreSQL 中对应 session，或直接重置该会话，让 Agent 使用新的 Claude session id。

## 当前 TODO

- [ ] 主动消息 / automation / outbox。
- [ ] 长任务完成通知、主动状态推送、取消任务。
- [ ] 更完整的图片链路：结构化图片发送、近期会话图片附件读取、失败降级。
- [ ] 支持 Codex、OpenCode 等更多 CLI Agent 后端。
- [ ] WeChatPadPro 稳定性收敛，尤其是私聊推送、同步流和网络/DNS 问题。

## FAQ

### 怎么切换模型？

修改 `deploy/dogbot.env` 里的 `BIFROST_MODEL` 和 `API_PROXY_UPSTREAM_*`。`BIFROST_MODEL` 是容器内 Claude 看到的模型别名，`API_PROXY_UPSTREAM_MODEL` 是真实上游模型名。修改后重启：

```bash
./stop_stack.sh --keep-qq --keep-wechat
./deploy_stack.sh --qq --wechat
```

### 端口 5432 被占用怎么办？

本项目默认把 PostgreSQL 映射到宿主机 `15432`，不是 `5432`。如果仍冲突，改 `POSTGRES_PORT`。如果 `agent-runner` 端口冲突，改 `AGENT_RUNNER_BIND_ADDR`，并同步修改 `NAPCAT_HTTP_CLIENT_URL` 和 `WECHATPADPRO_WEBHOOK_URL` 里的端口。

### 容器里访问 `host.docker.internal` 失败怎么办？

先确认服务在宿主机监听的地址不是 `127.0.0.1`。需要被容器访问的服务应绑定 `0.0.0.0`，例如 `API_PROXY_BIND_ADDR=0.0.0.0:9000`。如果启用了 `APPLY_NETWORK_POLICY=1`，还要确认 `API_PROXY_PORT` 和真实端口一致。

### Docker 拉镜像失败怎么办？

检查 Docker daemon 的代理和网络。`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 会注入容器，但 Docker 拉镜像阶段通常需要配置 Docker daemon 自己的代理。

### 如何修改人格？

编辑 `claude-prompt/persona.md`，然后重新部署或重启 `agent-runner`。不要直接改运行时同步目录里的文件，否则下次部署会被仓库内容覆盖。

### QQ reaction 报错是否影响回复？

通常不影响。NapCat 有时会返回 reaction 业务失败，`agent-runner` 会记录 warning，但任务仍会继续执行并回复文本。

### 为什么群聊不回复？

群聊必须显式 `@机器人`。普通 reply 不会单独触发。QQ 还需要 `PLATFORM_QQ_BOT_ID` 与实际登录账号一致。

### 为什么切换模型后 QQ 报 thinking 相关错误？

这是 Claude session 与新模型的 thinking 协议状态不匹配。更新 PostgreSQL 的 session 映射，或重置对应会话，让它生成新的 Claude session id。

### 如何查看日志？

默认日志在：

```text
${AGENT_STATE_DIR}/logs/agent-runner.log
${AGENT_STATE_DIR}/bifrost/bifrost.log
```

平台容器日志使用：

```bash
docker logs napcat
docker logs wechatpadpro
```
