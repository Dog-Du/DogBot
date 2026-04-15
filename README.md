# DogBot

`DogBot` 是一个面向个人账号机器人的多平台 Agent 项目。

当前已经落地两条接入链路：

- `QQ -> NapCat -> qq-adapter -> agent-runner -> claude-runner`
- `微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner`

目标：

1. 资源控制：把 CLI Agent 放进 Docker，限制 CPU、内存、进程数和宿主机暴露面，用统一的宿主机控制层管理超时、队列、限流、会话和消息回发。
2. 多平台接入：接入 QQ、微信、飞书等多平台
3. 易用性：开箱即用的体验，运行脚本即可一键部署
4. 上下文管理：为 Agent 提供 memory、skills、system prompt 等内容

## 当前架构

```text
QQ
-> NapCat
-> qq-adapter
-> agent-runner
-> claude-runner 容器
-> agent-runner 内置上游代理
-> Claude 协议模型源

微信
-> WeChatPadPro
-> wechatpadpro-adapter
-> agent-runner
-> claude-runner 容器
-> agent-runner 内置上游代理
-> Claude 协议模型源
```

## 仓库结构

```text
.
├── deploy_stack.sh              # 根目录部署入口
├── stop_stack.sh                # 根目录停止入口
├── README.md
├── agent-runner/                 # Rust 核心服务
├── compose/                      # Docker Compose 编排
│   └── README.md                 # 高级用户的 compose 说明
├── deploy/                       # 配置模板与部署说明
├── docker/claude-runner/         # Claude 容器镜像
├── qq_adapter/                   # 宿主机 QQ 适配器
├── scripts/                      # 启停、配置、诊断脚本
├── wechatpadpro_adapter/         # 宿主机微信适配器
└── docs/                         # 设计文档
```

## 文档入口

完整部署说明见：

- `deploy/README.md`

默认配置模板见：

- `deploy/dogbot.env.example`

## 部署入口

普通用户只需要关心两件事：

- `deploy/dogbot.env`
- `./deploy_stack.sh` / `./stop_stack.sh`

`compose/` 目录默认不需要修改；如果你确实需要自定义容器层行为，请查看 `compose/README.md`。

当前部署脚本支持两种使用方式：

- 无参数运行 `./deploy_stack.sh`
  - 进入交互式平台选择
- 显式传参
  - `./deploy_stack.sh --qq`
  - `./deploy_stack.sh --wechat`
  - `./deploy_stack.sh --qq --wechat`

## 部署依赖

部署 `DogBot` 前，至少需要这些内容：

### 必需软件

- `Linux`
  - 当前部署路径按 Linux 宿主机设计
- `uv`
  - 用来运行 Python 工具和测试
- `Docker Engine`
  - 用来运行 `claude-runner`、`NapCat`、`WeChatPadPro`
- `Docker Compose v2`
  - 用来启动各个容器栈
- `Rust` / `cargo`
  - 用来编译和运行宿主机上的 `agent-runner`
- `curl`
  - 用来做本地接口联调和脚本诊断
- `sudo`
  - 某些 Docker、iptables、系统级操作需要 root 权限

### 必需外部条件

- 一个可用的 `Claude 协议模型源`
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
- [ ] 清理 AstrBot 遗留
  - 删除不再使用的插件和部署残留
  - 彻底收敛到双 adapter 架构
- [ ] Agent 内容管理与记忆管理
  - 对 `skill`、`memory`、`system prompt` 等内容做结构化管理
  - 减少上下文污染，提升长期可维护性
- [ ] 支持 `Codex`、`OpenCode` 等
  - 除 Claude Code 外，扩展更多 CLI Agent 后端
  - 保持统一的运行、会话和资源限制边界
- [ ] 易用性整理
  - 继续收敛部署流程、默认配置、脚本入口和文档
  - 降低新环境搭建和日常使用成本
