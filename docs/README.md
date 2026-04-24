# 文档索引

本目录用于整理 `DogBot` 的项目文档。

## 1. 先读哪些文档

如果你第一次接触这个仓库，建议按下面顺序阅读：

1. `../README.md`
2. `../AGENTS.md`
3. `../deploy/README.md`
4. `../deploy/dogbot.env.example`

## 2. 文档分类

### 入口文档

- `../README.md`
  - 项目简介、架构、依赖、TODO
- `../AGENTS.md`
  - 给 AI Agent 的仓库上下文说明
- `../deploy/README.md`
  - 实际部署说明
- `../deploy/dogbot.env.example`
  - 配置模板
- `control-plane-integration.md`
  - A/B/C 控制面改造后的联调与验收说明

### 设计文档

位于：

- `superpowers/specs/`

当前已存在的设计文档包括：

- `2026-04-13-qq-agent-design.md`
- `2026-04-14-wechatpadpro-integration-design.md`
- `2026-04-14-wechatpadpro-adapter-design.md`
- `2026-04-15-usability-cleanup-design.md`
- `2026-04-15-code-cleanup-design.md`
- `2026-04-19-dogbot-control-plane-design.md`
- `2026-04-22-dogbot-claude-prompt-design.md`
- `2026-04-24-unified-platform-protocol-design.md`

### 实施计划

位于：

- `superpowers/plans/`

这些文档主要记录阶段性实现计划和拆解过程。

控制面相关计划：

- `2026-04-19-dogbot-control-plane-phase-a.md`
- `2026-04-19-dogbot-control-plane-phase-b.md`
- `2026-04-19-dogbot-control-plane-phase-c.md`
- `2026-04-22-dogbot-claude-prompt-migration.md`

## 3. 当前建议

如果后续继续整理文档，建议把内容逐步收敛成三类：

- 面向使用者的文档
- 面向开发者的文档
- 面向 AI Agent 的文档
