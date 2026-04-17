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
QQ -> NapCat -> qq-adapter -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner
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
- `qq_adapter`
  - 宿主机 Python 服务
  - 负责把 QQ 事件转给 `agent-runner`

### 微信接入层

- `WeChatPadPro`
  - 负责微信登录和消息入口
- `wechatpadpro-adapter`
  - 宿主机 Python 服务
  - 负责把微信事件转给 `agent-runner`

## 3. 当前触发规则

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status`：保留

## 4. 重要目录

- `agent-runner/`
  - Rust 核心服务
- `qq_adapter/`
  - QQ 适配器
- `wechatpadpro_adapter/`
  - 微信适配器
- `compose/`
  - Docker Compose 配置
- `deploy/`
  - 部署文档与配置模板
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
- 历史消息持久化还没真正落地

## 7. 后续方向

- 历史消息持久化
- Agent 内容管理与记忆管理
- 支持 `Codex`、`OpenCode`
- 整理部署与使用体验

## 8. 阅读顺序建议

新 Agent 进入仓库后，建议优先阅读：

1. `README.md`
2. `deploy/README.md`
3. `deploy/dogbot.env.example`
4. `agent-runner/`
5. `qq_adapter/`
6. `wechatpadpro_adapter/`
