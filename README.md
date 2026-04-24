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
- Claude prompt 轻量内容加载
- inbound trigger 解析
- 历史消息采集和 retention cleanup

## Claude Prompt

DogBot 现在改为轻量的 Claude 原生内容方案：

- `claude-prompt/CLAUDE.md`
  - 只放运行边界与最小长期指令
- `claude-prompt/persona.md`
  - 放人格/语气源文件，由 `CLAUDE.md` 用 `@persona.md` 导入
- `claude-prompt/.claude/skills/**`
  - 放仓库自带的轻量 skills
- `deploy_stack.sh`
  - 部署时把仓库里的 `claude-prompt/` 同步到运行时 `DOGBOT_CLAUDE_PROMPT_ROOT`
  - `claude-runner` 启动时会把 `CLAUDE.md`、`persona.md` 和 `.claude/` 投影到 `/workspace` 标准路径，确保 Claude Code 自动发现项目级 prompt
- `claude-runner`
  - 显式关闭 Claude Code auto memory，只保留 DogBot 自己的可审计 memory 流
  - 容器内置 `Bifrost`，Claude Code 默认通过本地 `Anthropic` 入口走 `Bifrost -> agent-runner 内置上游代理 -> 真实模型源`

这套方案的边界是：

- 静态 prompt / skill 直接保存在仓库里
- 不再依赖外部内容仓库或 pack manifest 归一化
- 不再把 Claude runtime 自行积累的 memory 当成正式静态内容源

## 仓库结构

```text
.
├── deploy_stack.sh              # 根目录部署入口
├── stop_stack.sh                # 根目录停止入口
├── README.md
├── agent-runner/                 # Rust 核心服务
├── claude-prompt/                # Claude 原生静态 prompt / skills 源目录
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

当前用户可见触发规则：

- QQ 私聊：任意非空文本
- QQ 群聊：必须 `@机器人 + 正文`
- 微信私聊：任意非空文本
- 微信群聊：必须 `@机器人名 + 正文`
- `/agent-status` 保留为状态检查命令

补充说明：

- `agent-runner` 与两个 adapter 当前已经按上述规则对齐
- 群聊仍保留显式 mention gate，reply 本身不会单独触发执行
- 联调和验收应以当前规则为准
- WeChat 示例配置启用了群聊 mention 门禁，部署前需要把 `WECHATPADPRO_BOT_MENTION_NAMES` 改成真实群昵称

## 当前已落地

- [x] Agent 运行主干
  - `runner.db` 保存 Claude session 映射
  - `history.db` 保存 history ingest / retrieval 基础数据
  - `claude-prompt/` 承载仓库管理的静态 `CLAUDE.md` 与轻量 skills
  - `claude-runner` 运行时会把 `claude-prompt/` 投影为 `/workspace` 下的 Claude Code 标准项目文件
- [x] 触发识别与基础回复链路
  - QQ / WeChat 统一先走 `/v1/inbound-messages`
  - 规范化 inbound message、mention/reply 元数据和 runner-side trigger resolver 已落地
  - QQ / WeChat 的 reply / mention 回发链路已整理
- [x] 历史消息基础版
  - 首次有效触发会启用当前会话的 history ingest
  - QQ 群聊支持有限 backfill
  - WeChat 支持启用后的 realtime mirror

## 近期已收敛

- [x] 基础部署与联调体验整理
  - 统一 `dogbot.env` 命名和 `runtime/` 运行态目录布局
  - `deploy_stack.sh` / `stop_stack.sh` 跑通并补齐脚本级回归检查
  - `QQ/Wechat` 登录流程支持二维码刷新、阻塞等待、超时退出和回发链路修复
  - 移除 `astrbot` 依赖及相关历史运行链路
- [x] `claude-runner` 内置 `Bifrost`
  - 运行链路已经切到 `Claude Code -> 同容器 Bifrost -> agent-runner 内置上游代理 -> 真实模型源`
  - 真实上游 token 与 base URL 继续只保留在宿主机，不进入 `claude-runner` 容器

## 后续 TODO

- [ ] 主动消息 / automation / outbox
- [ ] 更完整的记忆审批与共享写权限治理
- [ ] 历史记录读取 skill 与管理权限
  - 为 Agent 提供明确的历史读取 skill，说明如何按平台、群聊/私聊、消息时间读取历史记录
  - 对静态白名单配置中的 admin 开放特殊命令，可查询更大范围或全部历史记录
- [ ] 长任务超时与同会话并发治理
  - 支持长时间运行的任务，例如周期性汇报、长耗时整理，不应被当前严格超时机制直接 kill 掉
  - 会话模型应统一为“一个私聊/一个群聊对应一个 `session_id`”，不再按群成员拆分子 session
  - 同一会话在长任务进行时再次发消息，需要会话级队列、状态查询、取消和重试机制，避免 turn 串扰或冲突
- [ ] 统一结构化平台接入与回复协议
  - QQ/NapCat、WeChatPadPro 和后续第三方平台接入应先归一化为同一套结构化 inbound event，而不是尽早压扁成纯文本
  - 出站回复也应统一为结构化 `reply / mention / text / image` 能力，再由各平台 adapter 做降级和发送
  - 这项工作应合并当前零散的 trigger、reply、mention、图片发送适配逻辑
- [ ] 删除 Python adapter，统一改为 Rust 适配层
  - 当前 `qq_adapter/` 和 `wechatpadpro_adapter/` 存在大量重复的 trigger、reply、mention、结构化消息映射和回发逻辑，维护成本偏高
  - 后续如果继续补齐结构化消息与媒体能力，Python 侧和 Rust 侧会出现重复实现，增加演进成本和行为漂移风险
  - 目标是把平台适配主逻辑统一收敛到 Rust，只保留必要的平台协议差异层，避免双份实现
- [ ] 图片链路做到与 `codex-bridge` 同等程度
  - 重点是图片发送和结构化回复中的图片 segment，而不是完整视觉链路
  - 支持读取当前消息或最近一小段会话窗口里的图片附件，并在同会话内继续发送
  - 不承诺完整入站图片理解、OCR、captioning、跨会话历史图片库或长期图片资产复用
  - 失败降级要明确：图片不可用时回退为文本说明，而不是静默失败
- [ ] 精简数据模型，去掉不必要的长期图片资产设计
  - 当前数据库设计对“历史图片复用/独立 asset 平面”的考虑偏重，可以收缩为更轻量的会话、任务、消息、附件模型
  - 保留 memory / policy / history 等确有边界价值的对象，去掉只为旧图片复用服务的冗余表和授权路径
  - 目标不是照抄 `codex-bridge` 的最小 sqlite，而是在保留多平台和长期上下文能力的前提下明显减重
- [ ] 支持 `Codex`、`OpenCode` 等更多 CLI Agent 后端
