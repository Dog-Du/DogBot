# DogBot Content Bootstrap Design

Date: 2026-04-19

## Summary

DogBot 已经落地了 control-plane 主干，但仓库托管内容仍然几乎为空。当前只有一个很小的 policy 文件：

- [content/policies/defaults.json](/home/dogdu/workspace/dogbot/content/policies/defaults.json)

这意味着：

- `agent-runner` 已经具备加载 `policy / resource / skill / memory-candidate` 的基本能力
- 但 DogBot 还没有真正“开箱即用”的 starter content
- 现有 runtime 中由 Claude 自行积累的 memory 既不稳定，也不可审计，不应继续作为正式内容源

本设计引入一套轻量内容引导系统：

- 选择 3 个 upstream 作为 starter content 来源
- 用锁版本的 `sources.lock` 管理来源
- 用离线同步脚本执行浅归一化
- 运行时只读取 DogBot 自己的 pack，不直接兼容 upstream 原始格式

## Goals

- 为 DogBot 提供第一批可直接启用的 starter skills、memory taxonomy、prompt/resource 模板
- 避免手写大量 skill 和 memory 正文
- 保持 runtime 内容稳定、可审计、可版本化
- 把 normalize 控制在“补元数据和路径映射”层面，而不是重写上游内容
- 为后续追加更多 upstream 保留清晰扩展点

## Non-Goals

- V1 不把 `agent-runner` 变成多上游格式兼容器
- V1 不让 runtime 直接依赖 GitHub、submodule 或远端仓库
- V1 不支持聊天中直接创建或修改 `resource / skill`
- V1 不把旧 runtime Claude memory 继续当正式长期记忆源
- V1 不做复杂的自动内容合并或冲突解决

## Chosen Upstreams

V1 只引入 3 个 upstream，并为它们分配不同职责：

1. `OpenViking`
   - 作用：提供内容模型、目录结构、少量 examples 级模板
   - 导入范围：优先使用 `examples` 和概念结构
   - 不直接导入：主项目核心代码和大段 AGPL 主体内容

2. `OpenHands/extensions`
   - 作用：作为 starter skills 主内容源
   - 导入范围：`skills/{name}/SKILL.md`、可选 `README.md`、`references/`、`scripts/`
   - 不直接导入：`plugins/`、需要额外执行 hook 或外部 token 的扩展

3. `Mem0`
   - 作用：提供 memory taxonomy、group attribution、category 设计思路
   - 导入范围：结构化规则和分类，不导入整篇文档正文
   - 不直接导入：服务端代码、API client、文档 prose 复制

这个分工明确对应 DogBot 的需求：

- `OpenViking` 负责“怎么组织内容”
- `OpenHands/extensions` 负责“有哪些 starter skills”
- `Mem0` 负责“memory 怎么分类、怎么按群聊/用户归因”

## Source Of Truth

DogBot 的正式内容源只有两类：

- 仓库内的 `content/packs/` 和 `content/policies/`
- `control.db` 中经权限和来源约束后的结构化对象

以下内容不是正式内容源：

- upstream 仓库原始目录
- runtime 中 Claude CLI 自行积累的 memory 文件
- 任意未带来源与作用域元数据的文本片段

其中旧 runtime Claude memory 仅作为“可审计导入输入源”，不是正式长期存储。

## Content Topology

`content/` 调整为：

```text
content/
├── local/
│   ├── packs/
│   └── overrides/
├── packs/
│   ├── base/
│   ├── qq/
│   ├── wechat/
│   ├── starter-skills/
│   ├── ov-examples/
│   └── memory-baseline/
├── policies/
│   └── defaults.json
├── upstream/
│   ├── openviking_examples/
│   ├── openhands_extensions/
│   └── mem0_taxonomy/
└── sources.lock.json
```

职责划分：

- `upstream/`
  - 保存锁版本后的原始内容快照
  - 只给同步工具、审计和 diff 使用
- `packs/`
  - 给 runtime 正式加载
  - 只包含浅归一化后的 manifest 和必要副本
- `local/overrides/`
  - 覆盖 upstream 导入内容
  - 用于平台约束、中文化和 DogBot 专属行为
- `policies/`
  - 承载全局 policy，例如 memory auto-commit 基线

## `sources.lock` Schema

新增文件：

- [content/sources.lock.json](/home/dogdu/workspace/dogbot/content/sources.lock.json)

V1 结构：

```json
{
  "version": 1,
  "sources": [
    {
      "source_id": "openviking_examples",
      "repo_url": "https://github.com/volcengine/OpenViking.git",
      "ref": "git-ref",
      "license": "Apache-2.0",
      "selected_paths": ["examples/..."],
      "import_mode": "copy_examples",
      "target_pack": "ov-examples"
    },
    {
      "source_id": "openhands_extensions",
      "repo_url": "https://github.com/OpenHands/extensions.git",
      "ref": "git-ref",
      "license": "MIT",
      "selected_paths": ["skills/..."],
      "import_mode": "skill_pack",
      "target_pack": "starter-skills"
    },
    {
      "source_id": "mem0_taxonomy",
      "repo_url": "https://github.com/mem0ai/mem0.git",
      "ref": "git-ref",
      "license": "Apache-2.0",
      "selected_paths": ["docs/memory-types", "docs/group-chat"],
      "import_mode": "taxonomy_only",
      "target_pack": "memory-baseline"
    }
  ]
}
```

约束：

- `ref` 必须是 tag 或 commit，禁止浮动分支
- `selected_paths` 只能白名单导入
- `import_mode` 必须显式指定，导入器不做猜测
- `target_pack` 必须唯一映射到一个 pack 目录

## Import Modes

V1 只支持 3 个导入模式：

### `copy_examples`

用于 `OpenViking`：

- 复制选中的 examples 到 `content/upstream/openviking_examples/`
- 生成 `content/packs/ov-examples/manifest.json`
- 对 example 模板补最薄元数据

### `skill_pack`

用于 `OpenHands/extensions`：

- 扫描 `SKILL.md`
- 复制允许的 `README.md`、`references/`、`scripts/`
- 为每个 skill 生成条目，补 `id / title / source / license / tags / enabled_by_default`
- 默认不执行脚本，只把脚本作为可审计附带资源

### `taxonomy_only`

用于 `Mem0`：

- 不复制 docs 正文到 runtime pack
- 只生成 DogBot 自己的 taxonomy manifest
- 将 memory scope、category、attribution 规则显式固化到本地 pack

## Pack Manifest

每个 `content/packs/{pack-id}/` 必须包含一个 `manifest.json`。

V1 字段：

```json
{
  "pack_id": "starter-skills",
  "version": 1,
  "title": "DogBot Starter Skills",
  "kind": "skill-pack",
  "source": {
    "source_id": "openhands_extensions",
    "repo_url": "https://github.com/OpenHands/extensions.git",
    "ref": "git-ref",
    "license": "MIT"
  },
  "items": [
    {
      "id": "starter.summary",
      "kind": "skill",
      "path": "skills/summary/SKILL.md",
      "title": "Conversation Summary",
      "summary": "Summarize discussion into concise output.",
      "tags": ["summary", "conversation"],
      "enabled_by_default": true,
      "platform_overrides": [],
      "upstream_path": "skills/summary/SKILL.md"
    }
  ]
}
```

V1 只支持 4 种 `kind`：

- `skill`
- `resource`
- `prompt`
- `memory-taxonomy`

不支持在 manifest 中直接定义可执行 platform action 或自动生效权限变更。

## Sync Pipeline

增加一个离线同步入口，例如：

- `scripts/sync_content_sources.py`

执行流程：

1. 读取 `content/sources.lock.json`
2. 为每个 source clone 到临时目录
3. checkout 到锁定 `ref`
4. 只复制 `selected_paths` 到 `content/upstream/{source_id}/`
5. 生成 `content/upstream/{source_id}/SOURCE.json`
6. 按 `import_mode` 生成 `content/packs/{target_pack}/`
7. 运行 pack 校验
8. 输出 `content/import-report.json`

`SOURCE.json` 至少记录：

- `source_id`
- `repo_url`
- `requested_ref`
- `resolved_commit`
- `license`
- `imported_at`

## Runtime Loading

`agent-runner` 的 [repo_loader.rs](/home/dogdu/workspace/dogbot/agent-runner/src/context/repo_loader.rs) 不直接兼容 3 个 upstream 的原始格式。

它在 V1 里只做两件事：

- 扫描 `content/packs/` 下的 `manifest.json`
- 扫描 `content/policies/`

这样 runtime 和 upstream 格式彻底解耦。normalize 的复杂度只停留在同步阶段，而不是运行时。

## DogBot Local Overrides

本地覆盖只放在：

- `content/local/packs/`
- `content/local/overrides/`

允许覆盖的内容：

- 中文化标题、摘要、说明
- QQ / WeChat 平台差异
- 是否默认启用
- DogBot 专属 safety/policy 文本

不允许的覆盖方式：

- 直接修改 `content/upstream/` 中的原始文件
- 在 runtime 动态编辑导入内容

## Legacy Runtime Memory

旧 runtime Claude memory 的处理策略：

- 不再作为正式内容源加载
- 新增一个审计入口，例如 `scripts/audit_legacy_runtime_memory.py`
- 审计结果分成：
  - `ignore`
  - `candidate`
  - `manual_review`
- 只允许把 `candidate` 导入 `control.db` 的候选表，并带上：
  - `source = legacy_runtime_memory`
  - `import_batch_id`
  - `scope`
  - `source_path`

V1 不做：

- 自动把旧 runtime memory 直接升级为 shared memory
- 跨会话复用未审计的 runtime memory

## Initial Starter Content

V1 第一批 starter content 只追求“够用”，不追求全面：

- `starter-skills`
  - 5 到 10 个通用 skill
  - 例如总结、待办提炼、结构化回复、提醒草稿、知识卡整理
- `memory-baseline`
  - user / conversation / platform / admin 对应的 category 基线
  - group attribution 规则
- `ov-examples`
  - 少量可参考 resource/prompt 模板
- `base / qq / wechat`
  - DogBot 自己维护的系统提示词和平台差异说明

## Testing Strategy

V1 至少覆盖：

- unit tests
  - `sources.lock` 解析
  - pack manifest 解析
  - import mode 分发
  - local override 叠加规则
- integration tests
  - sync 命令对 3 个 upstream 的最小样例导入
  - `repo_loader` 只读取 pack 而不读取 upstream
  - legacy runtime memory audit 分类结果稳定
- contract tests
  - manifest 必填字段
  - `SOURCE.json` 元数据完整
  - pack item path 存在

## Rollout

按 3 步推进：

1. 定义 schema 和目录
   - `sources.lock`
   - `pack manifest`
   - `SOURCE.json`
2. 落同步器与 loader
   - sync script
   - repo loader pack scan
   - starter packs 最小加载
3. 导入第一批内容
   - OpenHands starter skills
   - Mem0 memory baseline
   - OpenViking examples pack

## Risks

- upstream 结构变化会导致同步脚本失效
  - 通过锁 ref 和白名单路径降低风险
- 许可证边界不清会污染内容仓
  - 通过 `license` 字段和 `SOURCE.json` 明确来源
- starter skills 过多会增加默认 prompt 噪音
  - 通过 `enabled_by_default` 和 pack 粒度控制
- legacy runtime memory 导入质量不稳定
  - 通过 `candidate / manual_review` 分类和来源标签约束

## References

- [OpenViking GitHub](https://github.com/volcengine/OpenViking)
- [OpenViking Context Types](https://github.com/volcengine/OpenViking/blob/main/docs/en/concepts/02-context-types.md)
- [OpenHands/extensions](https://github.com/OpenHands/extensions)
- [OpenHands Skills Overview](https://docs.openhands.dev/overview/skills)
- [Mem0 GitHub](https://github.com/mem0ai/mem0)
- [Mem0 Memory Types](https://docs.mem0.ai/core-concepts/memory-types)
- [Mem0 Group Chat](https://docs.mem0.ai/platform/features/group-chat)
