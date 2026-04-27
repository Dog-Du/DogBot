# AGENTS

本文件给进入仓库的 AI Agent 快速建立上下文。优先相信本文件、`README.md`、`deploy/README.md` 和当前代码，不要依赖旧对话记忆。

## 项目定位

DogBot 是个人账号机器人项目，当前主线是：

- QQ 个人号机器人
- 微信个人号机器人
- 统一复用同一套 Agent 执行后端
- 用 Docker 隔离 Claude Code 的运行环境
- 用 PostgreSQL 保存 session 映射和文本历史消息

当前链路：

```text
QQ -> NapCat -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> agent-runner -> claude-runner
```

模型链路：

```text
Claude Code -> Bifrost -> agent-runner api-proxy -> upstream model provider
```

## 核心组件

### `agent-runner`

- Rust 服务，运行在宿主机。
- 提供平台 HTTP ingress。
- 管理同会话队列和全局并发。
- 执行 Docker 内 Claude Code。
- 保存和读取 PostgreSQL session/history/grant。
- 负责平台消息回发。
- 内置 api-proxy，真实上游模型 key 只保存在宿主机。

### `claude-runner`

- Docker 容器。
- 运行 Claude Code CLI 和 Bifrost。
- 只挂载 `/workspace` 和 `/state`。
- 不直接持有真实上游 key。
- Claude Code 只访问容器内 `127.0.0.1:${BIFROST_PORT}/anthropic`。

### 平台层

- QQ 使用 NapCat。
- 微信使用 WeChatPadPro。
- 两个平台都直接把事件推给 `agent-runner`，不再经过旧 adapter。

## 触发规则

- QQ 私聊：任意非空文本。
- QQ 群聊：必须 `@机器人 + 正文`。
- 微信私聊：任意非空文本。
- 微信群聊：必须 `@机器人名 + 正文`。
- `/agent-status` 保留。

群聊 reply 本身不会单独触发执行。

## 历史消息

- PostgreSQL only。
- 旧 sqlite `runner.db` / `history.db` 已废弃，不做迁移。
- 普通 Agent 只能读取当前会话历史。
- `DOGBOT_ADMIN_ACTOR_IDS` 中的 admin 在私聊中可获得跨会话读取授权。
- Agent 通过 `claude-prompt/skills/history-read/` 查询历史。
- 当前不保存图片历史。

## 调度语义

- 全局并发由 `MAX_CONCURRENT_RUNS` 控制。
- 同一会话同时只运行一个任务，后续任务 FIFO 排队。
- 等待中的任务只回复前面还有几个任务。
- 任务真正开始执行时才发送 reaction。
- 不再使用分钟级限流。
- 不再用固定 wall-clock timeout kill Claude Code。

## 重要目录

- `agent-runner/`：Rust 核心服务。
- `claude-prompt/`：`CLAUDE.md`、`persona.md`、skills。
- `deploy/`：部署文档、env 模板、Docker Compose。
- `scripts/`：启停、配置、登录、诊断脚本。
- `docs/`：设计文档。
- `runtime/`：本地运行态目录，默认不提交。

## 配置入口

用户只应修改：

```text
deploy/dogbot.env
```

模板：

```text
deploy/dogbot.env.example
```

启动和停止：

```bash
./deploy_stack.sh
./stop_stack.sh
```

## 已知注意事项

- 切换模型后，旧 Claude session 可能因 thinking 状态不兼容报错；需要重置或更新 PostgreSQL 中对应 session。
- WeChatPadPro 自身仍有稳定性风险，尤其是私聊推送、同步流、DNS 和登录状态。
- QQ reaction 失败通常不影响最终文本回复。
- 图片出站链路尚未完整收敛。
- 网络策略开启时，Claude 容器只应访问必要的宿主机代理端口。

## 阅读顺序

1. `README.md`
2. `deploy/README.md`
3. `deploy/dogbot.env.example`
4. `agent-runner/src/`
5. `claude-prompt/CLAUDE.md`
6. `scripts/`
7. `deploy/docker/`
