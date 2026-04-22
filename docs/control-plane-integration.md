# DogBot Control Plane 联调说明

本文档对应当前控制面现态，覆盖：

- A. 记忆 / 内容 / skill / 权限 / 会话隔离
- B. 触发识别 / 回复表现 / QQ微信交互细节
- C. 历史消息采集 / 存储

目标不是重复设计文档，而是给部署和联调提供一个可执行的检查清单。

## 1. 当前落地范围

本轮已经落地的主干能力：

- `agent-runner`
  - `session`
  - `history`
  - `trigger resolver`
- inbound-first 链路
  - QQ / WeChat adapter 会先把规范化后的消息发送给 `agent-runner /v1/inbound-messages`
- 历史消息基础版
  - 首次有效触发会启用当前会话的 history ingest
  - QQ 群聊首次启用后会做一次有界 backfill
  - WeChat 目前只做启用后的 realtime mirror
- retention cleanup
  - 请求路径会按节流周期 opportunistic 清理过期 history

## 2. 当前边界

为了避免把设计态和现态混淆，当前联调以这几条为准：

- 用户可见触发规则
  - QQ 私聊：任意非空文本
  - QQ 群聊：`@机器人 + 正文`
  - 微信私聊：任意非空文本
  - 微信群聊：`@机器人名 + 正文`
  - `/agent-status` 保留
- `agent-runner` 与两个 adapter 当前已经按上面的规则对齐
- 群聊仍保留显式 mention gate
  - reply 本身不会单独触发执行
  - 所以联调时不要把“reply 中带 `/agent` 就能直接执行”当成现态
- markdown 降级和 media action 校验已经有基础实现
  - 但图片出站仍未完成整条生产链路
  - 本轮不要把“Agent 已经可以端到端发图”当成验收项

## 3. 运行态文件

本轮新增或需要重点关注的运行态对象：

- `runner.db`
  - Claude session 映射和消息回发 session 依赖
- `history.db`
  - `message_store`
  - `message_attachment`
  - `asset_store`
  - `conversation_ingest_state`
- `claude-prompt/`
  - 仓库管理的静态 `CLAUDE.md`、`persona.md` 与 `.claude/skills`

建议路径：

```text
/srv/dogbot/runtime/agent-state/runner.db
/srv/dogbot/runtime/agent-state/history.db
/srv/dogbot/runtime/agent-state/claude-prompt/
```

## 4. 关键配置

至少确认下面这些配置已经显式设置或理解其默认值：

```env
AGENT_RUNNER_BIND_ADDR=127.0.0.1:8787
SESSION_DB_PATH=/srv/dogbot/runtime/agent-state/runner.db
HISTORY_DB_PATH=/srv/dogbot/runtime/agent-state/history.db
DOGBOT_CLAUDE_PROMPT_ROOT=/srv/dogbot/runtime/agent-state/claude-prompt

QQ_PLATFORM_ACCOUNT_ID=qq:bot_uin:123456
WECHATPADPRO_PLATFORM_ACCOUNT_ID=wechatpadpro:account:wxid_bot_1
WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK=1
WECHATPADPRO_BOT_MENTION_NAMES=DogDu
```

说明：

- `QQ_PLATFORM_ACCOUNT_ID` / `WECHATPADPRO_PLATFORM_ACCOUNT_ID` 决定 platform-account scope 的隔离键
- `DOGBOT_CLAUDE_PROMPT_ROOT` 推荐使用绝对路径，部署时会把仓库中的 `claude-prompt/` 同步到这里
- 示例配置默认启用 `WECHATPADPRO_REQUIRE_MENTION_IN_GROUP=1`
- 启用群聊 mention 门禁时，必须保证 `WECHATPADPRO_BOT_MENTION_NAMES` 非空并与群昵称一致
- 如果关闭 `WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK`，必须手动配置 webhook，否则不会收到微信消息

## 5. 联调前准备

如果 NapCat 和 WeChatPadPro 已经在线并保持登录，本轮联调通常不需要重新扫码。

建议只确认三类进程：

- `agent-runner`
- `qq-adapter`
- `wechatpadpro-adapter`

健康检查：

```bash
curl -fsS http://127.0.0.1:8787/healthz
curl -fsS http://127.0.0.1:19000/healthz
curl -fsS http://127.0.0.1:18999/healthz
```

预期都返回：

```json
{"status":"ok"}
```

## 6. 平台侧联调步骤

### 6.1 QQ 私聊

发送：

```text
/agent-status
说一句 hello
```

预期：

- `/agent-status` 返回 `agent-runner ok`
- 普通文本能正常触发执行并回消息

### 6.2 QQ 群聊

发送：

```text
@DogDu 说一句 hello
```

预期：

- 当前消息先进入 `/v1/inbound-messages`
- 首次有效触发时会启用该群的 history ingest
- QQ adapter 会对该群做一次最多 `50` 条的有界 history backfill
- 机器人回复时会 `@` 当前发言人

### 6.3 WeChat 私聊

发送：

```text
/agent-status
说一句 hello
```

预期：

- webhook 命中后先做 inbound 归一化
- `/agent-status` 返回 `agent-runner ok`
- 普通文本能正常回消息

### 6.4 WeChat 群聊

发送：

```text
@DogDu 说一句 hello
```

预期：

- 需要 mention 门禁
- 命中后会写入当前 group conversation 的 history ingest
- 当前只做启用后的 realtime mirror，不做历史回填

## 7. 数据面核对

### 7.1 核对 history ingest 是否启用

```bash
sqlite3 /srv/dogbot/runtime/agent-state/history.db \
  "select conversation_id, enabled, retention_days from conversation_ingest_state order by conversation_id;"
```

### 7.2 核对消息是否入库

```bash
sqlite3 /srv/dogbot/runtime/agent-state/history.db \
  "select conversation_id, count(*) as message_count from message_store group by conversation_id order by conversation_id;"
```

## 8. 当前已知限制

本轮合并后仍然保留这些限制：

- WeChatPadPro 历史消息没有 backfill，只支持启用后的 realtime mirror
- 历史只做当前 conversation 注入，不跨会话检索
- 静态 `CLAUDE.md / skills` 仍然是仓库管理，不支持在聊天里直接生效修改
- 图片链路目前只完成了 schema / action validation 基础，不作为本轮联调验收项
- adapter 仍保留群聊显式 mention gate，所以 reply 本身还不会单独触发执行

## 9. 回归命令

本轮对应的最小回归命令：

```bash
cargo test --test history_cleanup_tests --test history_ingest_tests --test inbound_api_tests --test context_run_tests --test http_api_tests --manifest-path agent-runner/Cargo.toml
uv run --with pytest --with fastapi --with httpx python -m pytest qq_adapter/tests -q
uv run --with pytest --with fastapi --with httpx python -m pytest wechatpadpro_adapter/tests -q
```

## 10. Content Bootstrap Checks
## 10. Claude Prompt Checks

静态内容联调前，建议先确认仓库里的 Claude 原生目录已经就位：

```bash
test -f claude-prompt/CLAUDE.md
test -f claude-prompt/persona.md
test -f claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md
```

部署后，建议再确认运行时目录已经同步完成：

```bash
test -f /srv/dogbot/runtime/agent-state/claude-prompt/CLAUDE.md
test -f /srv/dogbot/runtime/agent-state/claude-prompt/persona.md
test -f /srv/dogbot/runtime/agent-state/claude-prompt/.claude/skills/emit-memory-candidate/SKILL.md
```

这一步的目标是确认：

- 仓库中的静态 prompt / skill 会进入运行时目录
- Claude Code 可以在额外目录下读取 `CLAUDE.md`
- 轻量 skills 的 source of truth 就是仓库里的 `claude-prompt/`
