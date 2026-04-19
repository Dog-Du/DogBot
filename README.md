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

当前 `agent-runner` 内部已经收敛出一层控制面，负责：

- identity / session 归一化
- scope 与权限判定
- memory candidate 与内容加载
- inbound trigger 解析
- 历史消息采集、检索和 retention cleanup

## Content Bootstrap

DogBot 现在使用仓库托管的内容引导流程，而不是继续把 runtime 中 Claude 自行积累的 memory 当正式内容源：

- `content/sources.lock.json`
  - 锁定 upstream 内容源、版本、导入模式和目标 pack
- `scripts/sync_content_sources.py`
  - 将选中的 upstream 内容同步到 `content/upstream/`，并生成 `content/packs/`
- `agent-runner`
  - 运行时只读取 `content/packs/` 和 `content/policies/`
- `scripts/audit_legacy_runtime_memory.py`
  - 审计旧 runtime memory，输出 `ignore / candidate / manual_review`

当前第一批 upstream 是：

- `OpenViking`
- `OpenHands/extensions`
- `Mem0`

## 仓库结构

```text
.
├── deploy_stack.sh              # 根目录部署入口
├── stop_stack.sh                # 根目录停止入口
├── README.md
├── agent-runner/                 # Rust 核心服务
├── content/                      # policy / resource / skill 仓库内容
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

控制面联调说明见：

- `docs/control-plane-integration.md`

## 部署入口

普通用户只需要关心两件事：

- `deploy/dogbot.env`
- `./deploy_stack.sh` / `./stop_stack.sh`

如果你想把运行态产物收敛到一个目录下，推荐使用同一个 `runtime/` 根目录：

- `runtime/agent-workspace`
- `runtime/agent-state`
- `runtime/agent-state/wechatpadpro-data/`

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

## 当前规则

当前用户可见触发规则仍然保持显式命令：

- QQ 私聊：必须 `/agent ...`
- QQ 群聊：必须 `@机器人 + /agent ...`
- 微信私聊：必须 `/agent ...`
- 微信群聊：必须 `@机器人名 + /agent ...`
- `/agent-status` 保留为状态检查命令

补充说明：

- `agent-runner` 内部的 normalized trigger resolver 已经支持更宽松的识别
- 但两个 adapter 当前仍保留兼容性的本地 command gate
- 联调和验收应以当前显式命令规则为准
- WeChat 示例配置启用了群聊 mention 门禁，部署前需要把 `WECHATPADPRO_BOT_MENTION_NAMES` 改成真实群昵称

## 当前已落地

- [x] Agent 内容管理与记忆管理主干
  - 四层 scope：`user-private` / `conversation-shared` / `platform-account-shared` / `bot-global-admin`
  - `control.db` 落地 memory candidate、authorization 和 control-plane 对象
  - `content/` 目录承载仓库管理的 policy / resource / skill
- [x] 触发识别与基础回复链路
  - QQ / WeChat 统一先走 `/v1/inbound-messages`
  - 规范化 inbound message、mention/reply 元数据和 runner-side trigger resolver 已落地
  - QQ / WeChat 的 reply / mention 回发链路已整理
- [x] 历史消息基础版
  - 首次有效触发会启用当前会话的 history ingest
  - QQ 群聊支持有限 backfill
  - WeChat 支持启用后的 realtime mirror
  - 当前 conversation 的 history evidence pack 会注入 `/v1/runs`

## 近期已收敛

- [x] 基础部署与联调体验整理
  - 统一 `dogbot.env` 命名和 `runtime/` 运行态目录布局
  - `deploy_stack.sh` / `stop_stack.sh` 跑通并补齐脚本级回归检查
  - `QQ/Wechat` 登录流程支持二维码刷新、阻塞等待、超时退出和回发链路修复
  - 移除 `astrbot` 依赖及相关历史运行链路

## 后续 TODO

- [ ] 主动消息 / automation / outbox
- [ ] 更完整的记忆审批与共享写权限治理
- [ ] WeChat 历史回填
- [ ] 图片链路端到端发送
- [ ] 支持 `Codex`、`OpenCode` 等更多 CLI Agent 后端
