# Unified Platform Protocol Design

Date: 2026-04-24

## Summary

DogBot 现在的 QQ 和 WeChatPadPro 接入存在两个根本问题：

- `qq_adapter/` 和 `wechatpadpro_adapter/` 把平台接入、消息归一化、触发判断、回复发送分散在 Python 中，和 `agent-runner` 的 Rust 逻辑形成双份实现
- 当前 `agent-runner` 只吃扁平文本导向的 `InboundMessage`，出站也只具备有限的平台能力，无法作为统一多平台运行时

本设计将 `agent-runner` 重构为唯一宿主机服务，直接接入 `NapCat` 与 `WeChatPadPro`，并在内部建立一套一次到位的 structured core：

- 删除 `qq_adapter/` 与 `wechatpadpro_adapter/`
- 删除旧的 adapter -> `/v1/inbound-messages` 归一化边界
- 建立统一的 canonical inbound / outbound protocol
- 统一 `trigger`、`history`、`session`、`response normalizer`、`dispatch`
- 把图片、视频、语音、文件等媒体能力纳入同一套引用式协议，而不是继续围绕纯文本补丁式演进

这是一次性重构，不保留对历史版本、旧 schema、旧目录结构或旧部署方式的兼容层。

## Goals

- 让 `agent-runner` 直接承担 QQ 和 WeChatPadPro 的平台接入与回发
- 删除 Python adapter 中重复的 trigger、mapper、history sync、reply 逻辑
- 建立统一的 canonical message / event / outbound action 模型
- 支持首批统一能力：
  - `text`
  - `image`
  - `file`
  - `voice`
  - `video`
  - `sticker`
  - `reply/quote`
  - `mention/at`
  - `reaction`
- 保持当前用户可见触发规则：
  - 私聊任意非空文本
  - 群聊必须显式 `@机器人`
  - `reply` 本身不单独触发
- 让 Agent 在大多数情况下继续输出自然语言纯文本，同时允许用结构化 action block 声明非文本能力
- 媒体内容在协议层只传引用，不传大块二进制，按需解析、按需加载、按需发送
- 收敛部署、配置、测试和运行时边界，使后续新增平台时只需新增平台模块，不需要复制一套业务逻辑

## Non-Goals

- 本轮不保留对当前 Python adapter API、旧 inbound schema 或旧配置命名的兼容
- 本轮不实现历史读取 skill
- 本轮不实现 asset 读取 skill
- 本轮不实现复杂的宿主机任意路径授权模型
- 本轮不引入独立事件总线服务、单独 adapter 进程或新的中间 RPC 服务
- 本轮不把历史数据库迁移到 PostgreSQL

## Chosen Shape

本设计采用“分层式单体重构”：

- `agent-runner` 仍然是一个进程
- 进程内部拆分为 `platform ingress -> canonical protocol -> runtime pipeline -> outbound dispatch`
- 平台模块只负责 native 协议与平台能力差异
- 所有 trigger、history、session、reply normalization、dispatch policy 都只认 canonical model

不采用以下方案：

- 直接把 Python adapter 逻辑原样搬进 `server.rs`
  - 这样只能统一语言，不能统一边界
- 再加一个独立事件总线或新服务
  - 对当前项目过重，收益不足

## Architecture

重构后整体链路为：

```text
QQ/NapCat WS or API webhook
    -> agent-runner platform ingress
    -> canonical inbound event
    -> trigger gate
    -> history ingest
    -> run context build
    -> claude-runner execute
    -> response normalizer
    -> canonical outbound plan
    -> platform dispatch
    -> outbound history ingest
```

`agent-runner` 内部按如下模块组织：

- `platforms/qq`
  - NapCat WebSocket 事件接入
  - NapCat API 调用
  - QQ native message decode / encode
- `platforms/wechatpadpro`
  - WeChatPadPro webhook 接入
  - WeChatPadPro API 调用
  - 微信 native message decode / encode
- `protocol`
  - canonical event、message、message part、asset ref、outbound action 定义
- `pipeline`
  - inbound flow、trigger、history ingest、run context build、normalizer 组装
- `dispatch`
  - capability validation、降级决策、平台编译与发送
- `runtime`
  - queue、rate limit、timeout、session store、Claude container execution

平台模块不再承担业务判断。它们不决定“是否触发执行”“是否写入 history”“是否默认 @ 某人”。这些统一收敛到 canonical pipeline。

## Canonical Protocol

### Inbound Event

旧的 `InboundMessage` 过于扁平，无法承载 structured media 和 reaction。新的 canonical inbound 顶层应至少包含：

- `platform`
- `platform_account`
- `conversation`
- `actor`
- `event_id`
- `timestamp`
- `kind`
- `raw_native_payload`

`kind` 首批包括：

- `message`
- `reaction_added`
- `reaction_removed`

未来如需支持编辑、撤回、系统事件，可以继续扩展，不再把一切硬塞进文本字段。

### Canonical Message

`message` 事件承载一个 `CanonicalMessage`：

- `message_id`
- `reply_to`
- `parts: Vec<MessagePart>`
- `plain_text`
- `mentions`
- `native_metadata`

`plain_text` 是结构化消息的文本投影，用于 trigger、日志、基础摘要，不再是唯一消息真相。

### Message Parts

首批 canonical `MessagePart` 类型：

- `text`
- `mention`
- `image`
- `file`
- `voice`
- `video`
- `sticker`
- `quote`

其中：

- `mention` 是结构化目标，不是平台字符串拼接
- `quote` 是消息关系，不等价于平台文本前缀
- 媒体 part 只携带 `AssetRef`，不内嵌大块内容

### Outbound Plan

出站不再使用“`text + reply_to_message_id + mention_user_id`”模型，而是：

- `OutboundPlan`
  - `messages: Vec<OutboundMessage>`
  - `actions: Vec<OutboundAction>`
  - `delivery_report_policy`
- `OutboundMessage`
  - `parts: Vec<MessagePart>`
  - `reply_to`
  - `delivery_policy`
- `OutboundAction`
  - `reaction_add`
  - `reaction_remove`

这样一来：

- 文本消息和 reaction 不再混在一起
- reply、mention、image、sticker、file 等能力可以组合
- 后续新增平台时只需新增 canonical -> native compiler

## Agent Output Normalization

Agent 的原始输出大多数情况下仍然是纯文本。这一点不改。

但是 `agent-runner` 必须新增 `response normalizer`，负责把 Agent 原始输出转成 `OutboundPlan`。否则结构化协议会退化回“文本中心 + 平台特有语法”。

采用混合模式：

- 普通情况
  - Agent 输出自然语言纯文本
  - normalizer 将其转成默认 `OutboundMessage{text}`
- 需要非文本能力时
  - Agent 输出结构化 action block
  - normalizer 解析后生成 `reply/image/file/voice/video/sticker/reaction` 等 canonical 动作

明确禁止：

- Agent 直接输出平台私有语法，例如 QQ CQ 码
- Agent 直接拼平台 `@` 前缀或 quote 语法
- Agent 在消息体中嵌入大块 base64 媒体

运行时自动补足平台无关的默认外壳，例如：

- 群聊默认是否 `@` 当前触发者
- 是否默认引用触发消息
- markdown 降级
- 平台长度切分

这些规则由 runner 决定，不交给 Agent 自己拼装。

## Trigger Policy

trigger 统一作用于 canonical inbound，不再由各平台各自判断。

当前规则保持不变：

- 私聊：任意非空文本触发
- 群聊：必须显式 `@机器人` 才触发
- `reply` 本身不单独触发
- `/agent-status` 保留

补充规则：

- `reaction` 事件可以入 history，但默认不触发执行
- 非文本媒体消息是否触发执行，默认依赖其是否带有可触发文本内容
- 群聊触发判断只看 canonical mention，而不是平台原始字符串

## Session Model

session 模型改为：

`one conversation -> one agent session`

也就是：

- 一个私聊对应一个 Claude session
- 一个群聊对应一个 Claude session

不再按 `conversation + user` 细分群聊子 session。

原因：

- 群聊里的短期上下文应共享于同一个 conversation
- 群成员维度的拆分会让长任务、队列、取消、状态查询更加混乱
- `actor`、`reply_to`、`mention target` 属于 turn metadata，不属于 session identity

因此 session store 应只绑定：

- `platform`
- `platform_account`
- `conversation`
- `claude_session_id`

不再把 `user_id` 作为 session identity 的一部分。

## Context Injection

上下文注入分两层：

- system prompt / runtime instruction
  - `当前平台`
  - `当前平台账号`
- current turn prompt
  - `conversation`
  - `actor`
  - 当前触发消息的 canonical 投影
  - 当前消息的最小必要 reply 摘要

本轮不自动把 conversation history 批量注入 prompt。history 会被采集和结构化存储，但是否读取它交给未来的 history skill 设计；当前运行默认只注入当前 turn 的最小必要上下文。

不在 prompt 中额外注入：

- skill 说明
- skill 可见范围说明

这些继续交给现有 skill 管理机制。

## History Model

历史存储从“文本导向”升级为“canonical event 导向”。

当前 `history.db` 主要以 `normalized_text` 为核心，这不够。

新模型至少应包含：

- `event_store`
  - event 级别元数据
- `message_store`
  - canonical message 级别元数据
- `message_part_store`
  - 每个 part 的类型、顺序、文本或 asset 引用
- `message_relation_store`
  - reply / mention / reaction target
- `asset_store`
  - 资产元数据和来源
- `conversation_ingest_state`
  - 会话级 ingest 策略与 retention

设计原则：

- history 存 canonical event，不存平台私有拼接文本作为正式模型
- 平台原始 payload 可以保留 debug snapshot，但不是主真相源
- 给 Agent 的上下文投影可以从 canonical history 再生成，而不是反过来把存储层压成文本

平台侧历史策略允许不同：

- QQ 群聊首触发后的有限 backfill
- WeChat 启用后的 realtime mirror

但它们都必须产出同一种 canonical history event。

## Asset Reference And Resource Boundary

图片、视频、语音、文件、贴纸等内容不能在模块间反复复制，也不应在 canonical protocol 中直接携带大块内容。

统一采用引用式模型：

- `AssetRef`
  - `asset_id`
  - `kind`
  - `mime`
  - `size`
  - `source`

`source` 首批可表示：

- `workspace_path`
- `managed_store`
- `external_url`
- `platform_native_handle`
- `bridge_handle`

协议级原则：

`canonical protocol 只传引用，不传大块媒体内容；媒体内容按需解析、按需加载、按需发送。`

### `/workspace` Boundary

为简化权限模型，这轮资源访问采用强约束：

- Agent 只允许读取 `/workspace` 下的资源
- 需要给 Agent 读取的图片、视频、文件，必须先落到 `/workspace`
- Agent 要发送的图片、视频、文件，也必须先存在 `/workspace`
- `/workspace` 之外的路径一律拒绝

因此：

- `AssetRef` 如需 materialize 成本地路径，只能 materialize 到 `/workspace`
- Agent 在结构化动作中引用本地媒体时，也只能引用 `/workspace` 路径
- 平台接收到的入站媒体，如果未来要给 Agent 使用，也必须先转存到 `/workspace`

本轮不实现历史读取或 asset 读取 skill，但该资源边界仍然先落入设计约束中。

## Capability Dispatch And Degrade Policy

统一出站发送入口应变为：

`dispatch(OutboundPlan, CapabilityProfile)`

每个平台模块声明自己的能力矩阵，例如：

- 是否支持原生 `reply/quote`
- 是否支持 `reaction`
- 是否支持 `sticker`
- 各媒体类型是支持路径、句柄、URL 还是必须上传
- mention 是结构化 at，还是只能退化成文本样式

dispatcher 执行三步：

1. 验证 `OutboundPlan`
2. 按平台能力编译 canonical 动作
3. 按降级策略执行发送

### Validation

统一验证至少包括：

- `reply_to` 是否属于当前 conversation
- reaction target 是否存在
- asset 引用是否合法且可访问
- 一个 plan 中是否包含互相冲突的动作

### Degrade Policy

采用分级策略：

- 核心能力失败，整个 turn 失败
- 非核心能力失败，允许跳过并记录 degraded result

明确分级：

- `required`
  - `text`
  - 显式请求的 `image/file/voice/video`
  - 显式请求的 `reply`
- `best_effort`
  - `reaction`
  - `sticker`
  - mention 呈现样式差异
  - 平台排版和表现层差异

## Error Model

统一错误模型建议分四类：

- `normalize_error`
  - Agent 输出中的结构化 block 不合法
  - 特殊动作失败，但纯文本 body 仍可继续发送
- `validation_error`
  - reply target 不存在、asset 未授权、路径越界等
  - 属于请求错误，不重试
- `delivery_error`
  - 平台 API 失败、上传失败、超时
  - 按动作粒度记录
- `platform_capability_mismatch`
  - 当前平台不支持某能力
  - 按 `required / best_effort` 决定失败还是降级

## Platform-Specific Boundaries

### QQ / NapCat

QQ 模块负责：

- NapCat WebSocket message ingress
- QQ 消息 decode 为 canonical inbound
- QQ API send / upload / reaction compiler
- QQ 群聊回填策略

QQ 模块不负责：

- trigger policy
- status command policy
- session identity policy
- history storage shape

### WeChatPadPro

WeChatPadPro 模块负责：

- webhook ingress
- 微信消息 decode 为 canonical inbound
- WeChatPadPro API send / upload / reaction compiler
- 微信 realtime mirror 策略

WeChatPadPro 模块同样不负责业务判断，只负责 native 协议差异。

## Deployment And Config Changes

部署与配置应一起收口。

删除：

- `qq_adapter/`
- `wechatpadpro_adapter/`
- 对应 Python 测试
- `scripts/start_qq_adapter.sh`
- `scripts/start_wechatpadpro_adapter.sh`
- 与 adapter 进程相关的 deploy 文档和依赖说明

新增或重组配置命名：

- `platform.qq.*`
- `platform.wechatpadpro.*`
- `platform.qq.ingest.*`
- `platform.wechatpadpro.ingest.*`

原则：

- 平台配置按平台分组
- 不再将同一平台能力拆散到 runner 和 adapter 两边
- `agent-runner` 自己暴露 NapCat WS / webhook 接口

## Testing Strategy

至少覆盖五层测试。

### Protocol Tests

验证：

- canonical inbound / outbound schema
- response normalizer
- action block parser
- trigger policy

### History And Session Tests

验证：

- 一会话一 conversation
- canonical history 落库
- reply / reaction relation
- retention cleanup

### Platform Mapper Tests

使用固定 QQ / WeChat fixture，验证：

- native -> canonical
- canonical -> native

### Dispatch Contract Tests

验证同一个 `OutboundPlan` 在 QQ / 微信上的：

- 编译结果
- 必要上传动作
- 降级结果
- capability mismatch 处理

### End-To-End Tests

直接起 `agent-runner`，喂原生平台事件，验证整条链路：

- ingress
- history ingest
- trigger
- Claude execute
- normalizer
- dispatch

## Migration Strategy

迁移按一次性 cleanup 执行，不做兼容层：

1. 在 `agent-runner` 内引入新的 platform modules、protocol、dispatch、normalizer
2. 重做 session model 和 history schema
3. 接入 QQ / WeChat native ingress
4. 把当前运行流切到 canonical pipeline
5. 删除 Python adapter 目录、脚本、测试和文档
6. 更新 deploy、README、control-plane 文档和环境变量模板

迁移完成后，仓库中不应同时保留“Python adapter 逻辑”和“Rust structured platform layer”两套正式实现。

## Open Deferrals

以下内容明确延后，不在本轮实现范围：

- history read skill
- asset read skill
- SQLite -> PostgreSQL 迁移
- 更复杂的资产生命周期治理
- 更精细的跨目录资源授权

这些延后项不影响本轮协议边界。本轮先把 canonical protocol、platform ingress、runtime pipeline、dispatch 和资源边界拉正。
