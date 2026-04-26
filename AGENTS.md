# AGENTS

本文件用于让进入仓库的 AI Agent 迅速理解当前项目，而不必先通读全部历史对话。

## 1. 项目定位

`DogBot` 是一个个人账号机器人项目，当前目标是：

- QQ 个人号机器人
- 微信个人号机器人
- 统一复用同一套 Agent 执行后端
- 用 Docker 约束 CLI Agent 的资源和宿主机暴露面

当前已经落地两条链路：

```text
QQ -> NapCat -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> agent-runner -> claude-runner
```

## 2. 核心组件

### `agent-runner`

- 语言：Rust
- 作用：
  - 管理 Claude 容器生命周期
  - 执行 CLI Agent
  - 控制超时、并发、队列、限流
  - 维护会话与 session 映射
  - 提供消息回发接口
  - 提供宿主机内置上游代理

### `claude-runner`

- 运行在 Docker 中
- 作用：
  - 运行 Claude Code CLI
- 约束：
  - 只允许访问工作目录和状态目录
  - 不能直接持有真实上游 key

### QQ 接入层

- `NapCat`
  - 负责 QQ 登录和 OneBot
  - 通过 HTTP 回调把 QQ 事件直接推给 `agent-runner`

### 微信接入层

- `WeChatPadPro`
  - 负责微信登录和消息入口
  - 通过 webhook 直接把微信事件推给 `agent-runner`

## 3. 当前触发规则

- 当前用户可见规则：
  - QQ 私聊：任意非空文本
  - QQ 群聊：必须 `@机器人 + 正文`
  - 微信私聊：任意非空文本
  - 微信群聊：必须 `@机器人名 + 正文`
- `/agent-status`：保留

补充说明：

- `agent-runner` 当前直接执行平台侧 trigger gate
- 群聊 reply 本身不会单独触发执行
- 不要把“reply 中带 `/agent` 就已经全量开放”当成当前现态

## 4. 重要目录

- `agent-runner/`
  - Rust 核心服务
- `deploy/`
  - 部署文档、配置模板与容器定义
- `deploy/docker/`
  - `claude-runner` 镜像、compose 栈与平台容器定义
- `scripts/`
  - 启停、配置、诊断脚本
- `docs/`
  - 项目文档和设计文档

## 5. 当前约束

- 真实模型 key 只保留在宿主机
- Docker 内的 Claude 只连接宿主机上的本地代理
- Docker 容器应能访问外网
- Docker 容器不应访问宿主机除本地代理外的其他服务

## 6. 当前已知问题

- `WeChatPadPro` 仍然存在自身不稳定点
  - 尤其是私聊推送、同步流和 DNS 稳定性
- 历史消息持久化已经落地基础版
  - QQ 仅支持首次启用后的有限 backfill
  - WeChat 目前仅支持启用后的 realtime mirror
- 图片链路尚未完成端到端出站发送
- 群聊仍保留显式 mention gate，reply 单独触发还未对外开放

## 7. 后续方向

- 主动消息 / automation / outbox
- 更完整的 Agent 内容管理与记忆审批
- 支持 `Codex`、`OpenCode`
- 完整图片链路和更丰富的回复渲染

## 8. 阅读顺序建议

新 Agent 进入仓库后，建议优先阅读：

1. `README.md`
2. `deploy/README.md`
3. `deploy/dogbot.env.example`
4. `docs/control-plane-integration.md`
5. `agent-runner/`
6. `deploy/docker/`
7. `scripts/`
