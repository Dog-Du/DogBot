# DogBot Claude Prompt Design

Date: 2026-04-22

## 2026-04-26 Corrections

以下内容覆盖本 spec 中较早版本的过时描述：

- 静态 skill 源目录现在是 `claude-prompt/skills/**`，不再使用 `claude-prompt/.claude/skills/**`
- `claude-runner` 启动时不再把 `CLAUDE.md`、`persona.md` 或 `.claude` 投影到 `/workspace`
- 当前运行方式是：
  - Claude 通过 `--add-dir /state/claude-prompt` 发现静态 prompt 目录
  - `agent-runner` 的 system prompt 明确要求先读 `/state/claude-prompt/CLAUDE.md`
  - 涉及回复协议时，必须再读 `/state/claude-prompt/skills/reply-format/SKILL.md`

## Summary

DogBot 当前的 `content/` + pack manifest + upstream sync 方案已经证明过重：

- 运行时并没有把真实 `SKILL.md` 正文交给 Claude Code
- `agent-runner` 只把 pack item 摘要注入 prompt
- 同步脚本、manifest、deploy 开关和 cleanup 脚本带来了额外维护成本
- 这套格式不是 Claude Code 原生格式，价值有限

新方案收敛为 Claude Code 原生内容模型：

- 仓库内保留一个轻量 source 目录：`claude-prompt/`
- 目录内直接存放 Claude 原生文件：
  - `CLAUDE.md`
  - `.claude/skills/.../SKILL.md`
- 部署时把 `claude-prompt/` 同步到运行时目录
- `agent-runner` 只负责：
  - 动态上下文注入
  - 把 Claude 原生目录暴露给容器内 Claude Code

目标是让内容层回到“文件即真相来源”，避免 DogBot 继续维护一套额外 content plane。

## Goals

- 删除 repository-managed content bootstrap 方案及其 deploy/runtime 依赖
- 让 DogBot 的静态提示词和 skills 直接采用 Claude Code 原生目录结构
- 保留 `agent-runner` 的 session / history / trigger / memory-candidate 主干能力
- 让部署过程只做本地文件同步，不依赖外部仓库或 upstream refresh
- 保持仓库内内容轻量、可审计、可直接编辑

## Non-Goals

- 本轮不实现大量新 skills
- 本轮不引入新的外部内容源
- 本轮不把动态上下文完全迁移到 `--append-system-prompt`
- 本轮不重做 history / scope / memory candidate 数据模型
- 本轮不支持聊天中动态安装、更新或删除 skills

## Source Of Truth

新的正式静态内容源只有：

- `claude-prompt/CLAUDE.md`
- `claude-prompt/.claude/skills/**`

以下内容不再作为正式静态内容源：

- `content/`
- `content/packs/`
- `content/policies/`
- `content/upstream/`
- `content/sources.lock.json`
- 任何外部 upstream 仓库

旧的 runtime Claude content cleanup 逻辑也一并删除，不再区分 “legacy content” 与 “new content”。

## Directory Layout

仓库内新增轻量目录：

```text
claude-prompt/
├── CLAUDE.md
└── .claude/
    └── skills/
        └── emit-memory-candidate/
            └── SKILL.md
```

说明：

- `CLAUDE.md`
  - 承载长期稳定的 bot 行为说明
  - 例如平台边界、回复约束、memory block 约定
- `.claude/skills/`
  - 承载少量 first-party DogBot skills
  - V1 只保留和现有 runner 行为直接对接的轻量 skill

V1 不再保留独立 manifest、pack id、source metadata。

## Runtime Materialization

部署时将 `claude-prompt/` 同步到运行时目录，例如：

```text
/srv/dogbot/runtime/agent-state/claude-prompt/
```

容器内路径对应为：

```text
/state/claude-prompt/
```

`agent-runner` 启动 Claude Code 时：

- 继续允许 `/workspace` 和 `/state`
- 额外允许 `/state/claude-prompt`
- 打开 additional directories 的 `CLAUDE.md` 发现能力

因此静态内容通过 Claude Code 原生文件发现机制生效，而不是由 DogBot 自己解析。

## Runner Responsibilities

`agent-runner` 在新方案下只保留两类上下文职责：

1. 静态内容装配
   - 不再解析 pack manifest
   - 只读取 `DOGBOT_CLAUDE_PROMPT_ROOT`
   - 只负责把该目录暴露给 Claude 容器

2. 动态上下文注入
   - scope readable context
   - history evidence pack
   - 未来如需 platform runtime hints，继续在 run-time 注入

这意味着：

- 删除 `repo_loader.rs`
- 删除 enabled pack item 摘要注入
- 保留 `context_pack.rs` 中的动态上下文渲染，但只渲染 runtime 信息

## Initial Prompt Content

V1 的静态 prompt 结构应拆成两个轻量文件：

- `claude-prompt/CLAUDE.md`
  - 只保留运行边界与最小长期指令
  - 通过 `@persona.md` 导入人格文件
- `claude-prompt/persona.md`
  - 放默认 conversational persona
  - 不放 deploy、架构、pack 之类的控制面说明

其中 `claude-prompt/CLAUDE.md` 只应包含 DogBot 现态需要的最小内容：

- DogBot 是 QQ / 微信个人号机器人
- 用户可见触发规则仍然是显式 `/agent`
- 图片出站不是当前验收项
- 若要提交 memory candidate，使用 `dogbot-memory` fenced block
- 不要假设自己可以直接修改运行时 skills 或 prompt 目录

不要把大量历史设计、上游来源或 deploy 细节塞进 `CLAUDE.md`。人格表达也不应继续堆在 `CLAUDE.md` 中，而应收敛到 `persona.md`。

## Initial Skill Content

V1 只保留一个轻量 first-party skill：

- `emit-memory-candidate`
  - 说明什么时候应该输出 `dogbot-memory` fenced block
  - 明确 JSON 结构：
    - `scope`
    - `summary`
    - `raw_evidence`

该 skill 直接对接当前 `agent-runner` 已有的 memory candidate 解析逻辑，因此不是概念占位符，而是立即可用的 Claude-native 内容。

## Deployment Changes

删除旧部署开关：

- `DOGBOT_CONTENT_ROOT`
- `DOGBOT_SYNC_CONTENT_ON_DEPLOY`
- `DOGBOT_REFRESH_CONTENT_ON_DEPLOY`
- `DOGBOT_PRUNE_LEGACY_CLAUDE_CONTENT_ON_DEPLOY`

新增单一配置：

- `DOGBOT_CLAUDE_PROMPT_ROOT`
  - 运行时 Claude prompt 根目录
  - 推荐指向 `AGENT_STATE_DIR` 下的持久化目录

部署脚本行为收敛为：

1. 确保 runtime 目录存在
2. 把仓库内 `claude-prompt/` 同步到 `DOGBOT_CLAUDE_PROMPT_ROOT`
3. 启动 `agent-runner`
4. 启动 `claude-runner` 和平台侧组件

不再联网拉取 upstream，也不再清理所谓 legacy Claude content。

## Docs Cleanup

以下文档应删除：

- `docs/superpowers/specs/2026-04-19-dogbot-content-bootstrap-design.md`
- `docs/superpowers/plans/2026-04-19-dogbot-content-bootstrap.md`
- `docs/superpowers/plans/2026-04-19-deploy-content-bootstrap.md`

以下文档应更新：

- `README.md`
- `deploy/README.md`
- `deploy/dogbot.env.example`
- `docs/control-plane-integration.md`
- `docs/README.md`

文档中所有关于 `content bootstrap`、`sources.lock`、`sync_content_sources.py`、`legacy Claude content cleanup` 的表述都应被移除。

## Migration Strategy

迁移按一次性 repo cleanup 执行：

1. 删除 `content/` 与旧脚本、测试、deploy 开关
2. 删除 `agent-runner` 中的 pack loader 逻辑和相关测试
3. 新增 `claude-prompt/` 与最小内容
4. 在 deploy / runner / container 路径中接入 `DOGBOT_CLAUDE_PROMPT_ROOT`
5. 更新说明文档和结构检查

迁移后仓库中不应同时保留旧 `content/` 与新 `claude-prompt/` 两套静态内容体系。

## Testing Strategy

至少覆盖：

- `agent-runner` 配置解析：
  - 新增 `DOGBOT_CLAUDE_PROMPT_ROOT`
  - 删除 `DOGBOT_CONTENT_ROOT`
- Claude 命令构造：
  - 允许访问 `/state/claude-prompt`
- deploy / start 脚本：
  - 会同步 `claude-prompt/`
  - 不再引用旧 content/bootstrap 变量和脚本
- 结构检查：
  - 存在 `claude-prompt/CLAUDE.md`
  - 存在 `claude-prompt/persona.md`
  - 存在 `claude-prompt/.claude/skills/.../SKILL.md`
  - 不再要求 `content/` 或 sync/cleanup 脚本

## Risks

- Claude Code 对 additional directory `CLAUDE.md` 的发现依赖运行时配置，必须明确打开
- 如果删除旧 cleanup 脚本但部署目录中仍留存旧内容，可能产生人为混淆
- 如果 `CLAUDE.md` 写得过重，仍会回到“新 content plane 只是换皮”的问题

## Acceptance Criteria

满足以下条件视为迁移完成：

- 仓库中不再存在 `content/` 目录
- 不再存在 `sync_content_sources.py` / `cleanup_legacy_claude_content.py`
- `agent-runner` 不再读取 pack manifest
- 仓库中存在 `claude-prompt/CLAUDE.md`
- 仓库中存在至少一个 first-party Claude-native skill
- 部署会把 `claude-prompt/` 同步到运行时目录
- 文档中不再把 content bootstrap 描述为现态
