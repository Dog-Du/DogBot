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

## 部署依赖

部署 `DogBot` 前，至少需要这些内容：

### 必需软件

- `Linux`
  - 当前部署路径按 Linux 宿主机设计
- `uv`
  - 用来运行 Python 工具和测试
- `Docker Engine`
  - 用来运行 `claude-runner`、`NapCat`、`AstrBot`、`WeChatPadPro`
- `Docker Compose v2`
  - 用来启动各个容器栈
- `Rust` / `cargo`
  - 用来编译和运行宿主机上的 `agent-runner`
- `curl`
  - 用来做本地接口联调和脚本诊断
- `sudo`
  - 某些 Docker、iptables、系统级操作需要 root 权限

### 必需外部条件

- 一个可用的 `Claude / Anthropic 协议兼容模型源`
  - 例如：
    - `Claude`
    - `GLM` 官方 Anthropic 兼容入口
    - `MiniMax` 官方 Anthropic 兼容入口
- 至少一个机器人接入平台
  - QQ：需要 `NapCat` 可登录的个人 QQ 号
  - 微信：需要 `WeChatPadPro` 可登录的个人微信号

### 建议安装

- `git`
- `rg`

## 目前规则

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status` 保留为状态检查命令

## 后续 TODO

- [ ] 历史消息持久化
  - 对 QQ、微信等平台的消息做统一入库
  - 为后续上下文补全、长期记忆、检索和审计提供基础
- [ ] 去除 AstrBot，改为轻量 Python adapter
  - 让 QQ 也走与微信类似的薄适配层
  - 收敛平台接入逻辑，减少额外抽象层
- [ ] Agent 内容管理与记忆管理
  - 对 `skill`、`memory`、`system prompt` 等内容做结构化管理
  - 减少上下文污染，提升长期可维护性
- [ ] 支持 `Codex`、`OpenCode`
  - 除 Claude Code 外，扩展更多 CLI Agent 后端
  - 保持统一的运行、会话和资源限制边界
- [ ] 易用性整理
  - 继续收敛部署流程、默认配置、脚本入口和文档
  - 降低新环境搭建和日常使用成本
