# DogBot

`DogBot` 是一个面向个人账号机器人的多平台 Agent 工程，当前已经落地两条接入链路：

- `QQ -> NapCat -> AstrBot -> agent-runner -> claude-runner`
- `微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner`

其中：

- `agent-runner` 使用 Rust 编写，负责 Docker 容器管理、超时、限流、会话、消息回发和内置上游代理
- `claude-runner` 在 Docker 中运行 Claude Code CLI
- 真实模型密钥只保留在宿主机，不进入 Claude 容器

## 项目目标

这个仓库优先解决三件事：

1. 把 CLI Agent 放进 Docker，限制 CPU、内存、进程数和宿主机暴露面
2. 用统一的宿主机控制层管理超时、队列、限流和会话
3. 让 QQ / 微信等不同平台都能复用同一套 Agent 执行后端

## 当前架构

```text
QQ
-> NapCat
-> AstrBot
-> claude_runner_bridge
-> agent-runner
-> claude-runner 容器
-> agent-runner 内置 Anthropic 兼容代理
-> Packy / GLM / MiniMax 等上游

微信
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner 容器
-> agent-runner 内置 Anthropic 兼容代理
-> Packy / GLM / MiniMax 等上游
```

## 仓库结构

```text
.
├── README.md
├── agent-runner/                 # Rust 核心服务
├── astrbot/                      # AstrBot 插件
├── compose/                      # Docker Compose 编排
├── deploy/                       # 配置模板与部署说明
├── docker/claude-runner/         # Claude 容器镜像
├── scripts/                      # 启停、配置、诊断脚本
├── wechatpadpro_adapter/         # 宿主机微信适配器
└── docs/                         # 设计文档
```

## 文档入口

完整部署说明见：

- [deploy/README.md](/home/dogdu/workspace/myQQbot/deploy/README.md)

默认配置模板见：

- [deploy/dogbot.env.example](/home/dogdu/workspace/myQQbot/deploy/dogbot.env.example)

## 目前规则

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status` 保留为状态检查命令

## 备注

- 旧的 `deploy/myqqbot.env` 仍然兼容，脚本会优先找 `deploy/dogbot.env`，找不到时回退到旧文件名。
- 旧的 `myqqbot/claude-runner:local` 镜像名已切换为 `dogbot/claude-runner:local`。
- `WeChatPadPro` 当前仍有自身实现层面的不稳定点，尤其是 webhook 私聊推送与消息同步质量，需要继续观察。
