# DogBot 易用性整理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把部署体验收敛为“一个配置文件、一个启动脚本、一个停止脚本”，并移除 AstrBot 依赖，让 QQ 与微信都通过宿主机薄 adapter 接入 `agent-runner`。

**Architecture:** 保留 `agent-runner + claude-runner` 作为唯一执行核心，把 QQ 接入从 `NapCat -> AstrBot -> claude_runner_bridge` 改成 `NapCat -> qq-adapter -> agent-runner`，并把部署脚本升级为统一的交互式入口。`compose/` 保留为高级配置层，默认用户只接触 `deploy/dogbot.env` 与两个脚本。

**Tech Stack:** Python adapter、Rust `agent-runner`、Docker Compose、shell scripts、NapCat、WeChatPadPro、pytest、cargo test。

---

## 文件结构收敛

本次实施后，文件边界应当是：

- `deploy/dogbot.env.example`
  - 唯一配置模板
- `deploy/README.md`
  - 面向用户的部署文档
- `compose/README.md`
  - 面向高级用户的 compose 说明
- `compose/docker-compose.yml`
  - `claude-runner`
- `compose/platform-stack.yml`
  - `napcat`
- `compose/wechatpadpro-stack.yml`
  - `wechatpadpro` / MySQL / Redis
- `qq_adapter/`
  - 新的 QQ 宿主机适配器
- `wechatpadpro_adapter/`
  - 微信宿主机适配器
- `scripts/deploy_stack.sh`
  - 唯一部署入口
- `scripts/stop_stack.sh`
  - 唯一停止入口
- `scripts/start_agent_runner.sh`
  - 宿主机 runner 启动脚本
- `scripts/start_qq_adapter.sh`
  - 新增，宿主机 QQ adapter 启动脚本
- `scripts/start_wechatpadpro_adapter.sh`
  - 微信 adapter 启动脚本
- `scripts/prepare_napcat_login.sh`
  - 新增，QQ 登录二维码准备和终端打印
- `scripts/prepare_wechatpadpro_login.sh`
  - 微信登录二维码准备和终端打印

## Task 1: 文档与配置入口收敛

**Files:**
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `deploy/dogbot.env.example`
- Create: `compose/README.md`

- [ ] **Step 1: 更新 README 的部署入口说明**

将 `README.md` 中涉及部署入口的部分收敛为：

```md
## 部署入口

普通用户只需要关心两件事：

- `deploy/dogbot.env`
- `./scripts/deploy_stack.sh` / `./scripts/stop_stack.sh`

`compose/` 目录默认不需要修改；如果你确实需要自定义容器层行为，请查看 `compose/README.md`。
```

- [ ] **Step 2: 更新部署文档的核心原则**

在 `deploy/README.md` 中明确写出：

```md
当前部署遵循三条原则：

1. 用户只修改 `deploy/dogbot.env`
2. 用户只通过 `./scripts/deploy_stack.sh` 启动
3. 用户只通过 `./scripts/stop_stack.sh` 停止
```

- [ ] **Step 3: 精简 env 模板头部说明**

在 `deploy/dogbot.env.example` 顶部增加统一说明：

```env
# DogBot 唯一用户配置文件
# 正常情况下你不需要修改 compose/ 目录中的文件
# 部署入口：
#   ./scripts/deploy_stack.sh
# 停止入口：
#   ./scripts/stop_stack.sh
```

- [ ] **Step 4: 新增 compose/README.md**

写入：

```md
# compose 目录说明

一般情况下不需要直接修改本目录。

本目录中的文件用于定义容器层运行方式：

- `docker-compose.yml`
  - 定义 `claude-runner`
- `platform-stack.yml`
  - 定义 `napcat`
- `wechatpadpro-stack.yml`
  - 定义 `wechatpadpro` / `mysql` / `redis`

只有在以下场景才建议直接修改：

- 自定义镜像名
- 自定义端口映射
- 自定义 volume 挂载
- 自定义资源限制

普通用户应优先通过 `deploy/dogbot.env` 调整配置。
```

- [ ] **Step 5: 验证文档引用**

Run:

```bash
rg -n "compose/README|deploy/dogbot.env|deploy_stack.sh|stop_stack.sh" README.md deploy/README.md deploy/dogbot.env.example compose/README.md
```

Expected:
- 四个文件都能检索到统一入口说明

- [ ] **Step 6: Commit**

```bash
git add README.md deploy/README.md deploy/dogbot.env.example compose/README.md
git commit -m "docs: align deployment entrypoints"
```

## Task 2: 新增 QQ 宿主机 adapter

**Files:**
- Create: `qq_adapter/__init__.py`
- Create: `qq_adapter/config.py`
- Create: `qq_adapter/mapper.py`
- Create: `qq_adapter/runner_client.py`
- Create: `qq_adapter/napcat_client.py`
- Create: `qq_adapter/app.py`
- Create: `qq_adapter/tests/test_app.py`
- Create: `qq_adapter/tests/test_mapper.py`

- [ ] **Step 1: 复制微信 adapter 的基础结构**

以 `wechatpadpro_adapter/` 为参考，新建 `qq_adapter/` 包结构：

```python
"""QQ host-local adapter package."""
```

`qq_adapter/config.py` 定义：

```python
from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
    agent_runner_base_url: str
    napcat_api_base_url: str
    napcat_access_token: str | None
    adapter_bind_addr: str
    default_cwd: str
    default_timeout_secs: int
    command_name: str
    status_command_name: str
    qq_bot_id: str

    @classmethod
    def from_env(cls) -> "Settings":
        return cls(
            agent_runner_base_url=os.getenv("AGENT_RUNNER_BASE_URL", "http://127.0.0.1:8787").rstrip("/"),
            napcat_api_base_url=os.getenv("NAPCAT_API_BASE_URL", "http://127.0.0.1:3001").rstrip("/"),
            napcat_access_token=os.getenv("NAPCAT_ACCESS_TOKEN") or None,
            adapter_bind_addr=os.getenv("QQ_ADAPTER_BIND_ADDR", "127.0.0.1:19000"),
            default_cwd=os.getenv("QQ_ADAPTER_DEFAULT_CWD", "/workspace"),
            default_timeout_secs=int(os.getenv("QQ_ADAPTER_TIMEOUT_SECS", "120")),
            command_name=os.getenv("QQ_ADAPTER_COMMAND_NAME", "agent"),
            status_command_name=os.getenv("QQ_ADAPTER_STATUS_COMMAND_NAME", "agent-status"),
            qq_bot_id=os.getenv("QQ_ADAPTER_QQ_BOT_ID", "").strip(),
        )
```

- [ ] **Step 2: 先写消息映射测试**

`qq_adapter/tests/test_mapper.py` 先覆盖 4 条规则：

```python
from qq_adapter.mapper import classify_message


def test_private_requires_agent_prefix():
    event = {"message_type": "private", "raw_message": "/agent hello", "user_id": 1}
    result = classify_message(event, command_name="agent", bot_id="123")
    assert result["mode"] == "run"
    assert result["prompt"] == "hello"


def test_private_plain_text_is_ignored():
    event = {"message_type": "private", "raw_message": "hello", "user_id": 1}
    assert classify_message(event, command_name="agent", bot_id="123") is None


def test_group_requires_at_and_agent_prefix():
    event = {
        "message_type": "group",
        "raw_message": "[CQ:at,qq=123] /agent hello",
        "group_id": 2,
        "user_id": 1,
    }
    result = classify_message(event, command_name="agent", bot_id="123")
    assert result["mode"] == "run"
    assert result["prompt"] == "hello"


def test_group_plain_agent_without_at_is_ignored():
    event = {
        "message_type": "group",
        "raw_message": "/agent hello",
        "group_id": 2,
        "user_id": 1,
    }
    assert classify_message(event, command_name="agent", bot_id="123") is None
```

- [ ] **Step 3: 实现 QQ 消息分类和 payload 构建**

`qq_adapter/mapper.py` 最小实现：

```python
from __future__ import annotations

from typing import Any


def strip_qq_at_prefix(raw_message: str, bot_id: str) -> tuple[str, bool]:
    prefix = f"[CQ:at,qq={bot_id}]"
    text = raw_message.strip()
    if text.startswith(prefix):
        return text[len(prefix):].strip(), True
    return text, False


def classify_message(event: dict[str, Any], command_name: str, bot_id: str) -> dict[str, str] | None:
    raw_message = str(event.get("raw_message") or "").strip()
    if not raw_message:
        return None

    if event.get("message_type") == "group":
        normalized, mentioned = strip_qq_at_prefix(raw_message, bot_id)
        if not mentioned:
            return None
    else:
        normalized = raw_message

    if normalized == f"/{command_name}":
        return {"mode": "run", "prompt": ""}
    if normalized.startswith(f"/{command_name} "):
        return {"mode": "run", "prompt": normalized[len(command_name) + 2:].strip()}
    if normalized == f"/{command_name}-status":
        return {"mode": "status", "prompt": ""}
    return None


def build_run_payload(event: dict[str, Any], prompt: str, default_cwd: str, timeout_secs: int) -> dict[str, Any]:
    user_id = str(event["user_id"])
    if event.get("message_type") == "group":
        group_id = str(event["group_id"])
        conversation_id = f"qq:group:{group_id}"
        session_id = f"{conversation_id}:user:{user_id}"
        chat_type = "group"
    else:
        conversation_id = f"qq:private:{user_id}"
        session_id = conversation_id
        chat_type = "private"

    return {
        "platform": "qq",
        "conversation_id": conversation_id,
        "session_id": session_id,
        "user_id": user_id,
        "chat_type": chat_type,
        "cwd": default_cwd,
        "prompt": prompt,
        "timeout_secs": timeout_secs,
        "reply_to_message_id": str(event.get("message_id") or ""),
        "mention_user_id": user_id if chat_type == "group" else "",
    }
```

- [ ] **Step 4: 写入 NapCat 入站和出站客户端**

`qq_adapter/napcat_client.py` 最小实现：

```python
from __future__ import annotations

import httpx


class NapCatClient:
    def __init__(self, base_url: str, access_token: str | None) -> None:
        self.base_url = base_url.rstrip("/")
        self.access_token = access_token

    def _headers(self) -> dict[str, str]:
        if not self.access_token:
            return {}
        return {"Authorization": f"Bearer {self.access_token}"}

    async def send_private_msg(self, user_id: str, message: str) -> None:
        async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
            response = await client.post(
                "/send_private_msg",
                headers=self._headers(),
                json={"user_id": int(user_id), "message": message},
            )
            response.raise_for_status()

    async def send_group_msg(self, group_id: str, user_id: str, message: str) -> None:
        full_message = f"[CQ:at,qq={user_id}] {message}"
        async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
            response = await client.post(
                "/send_group_msg",
                headers=self._headers(),
                json={"group_id": int(group_id), "message": full_message},
            )
            response.raise_for_status()
```

- [ ] **Step 5: 写入 adapter FastAPI 入口**

`qq_adapter/app.py` 应提供：

- `GET /healthz`
- `POST /napcat/events`

核心流程：

```python
command = classify_message(event, settings.command_name, settings.qq_bot_id)
if command is None:
    return {"status": "ignored"}
if command["mode"] == "status":
    health = await runner.healthz()
    text = "agent-runner ok" if health.get("status") == "ok" else "agent-runner unavailable"
else:
    payload = build_run_payload(event, command["prompt"], settings.default_cwd, settings.default_timeout_secs)
    result = await runner.run(payload, settings.default_timeout_secs)
    text = (result.get("stdout") or result.get("stderr") or "").strip()
if text:
    if event.get("message_type") == "group":
        await napcat.send_group_msg(str(event["group_id"]), str(event["user_id"]), text)
    else:
        await napcat.send_private_msg(str(event["user_id"]), text)
return {"status": "accepted"}
```

- [ ] **Step 6: 跑 QQ adapter 测试**

Run:

```bash
uv run --with pytest --with fastapi --with httpx python -m pytest qq_adapter/tests -q
```

Expected:
- `PASS`

- [ ] **Step 7: Commit**

```bash
git add qq_adapter
git commit -m "feat: add qq host adapter"
```

## Task 3: 去掉 AstrBot 依赖

**Files:**
- Modify: `compose/platform-stack.yml`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/stop_stack.sh`
- Modify: `scripts/configure_napcat_ws.sh`
- Delete: `astrbot/plugins/claude_runner_bridge/README.md`
- Keep unused source temporarily or schedule removal after tests pass

- [ ] **Step 1: 从 platform-stack.yml 移除 AstrBot 服务**

目标结果：

```yaml
services:
  napcat:
    image: ${NAPCAT_IMAGE:-mlikiowa/napcat-docker:latest}
    container_name: ${NAPCAT_CONTAINER_NAME:-napcat}
    restart: unless-stopped
    ports:
      - "${NAPCAT_WEBUI_PORT:-6099}:6099"
      - "${NAPCAT_ONEBOT_PORT:-3001}:3001"
    environment:
      NAPCAT_UID: ${NAPCAT_UID:-1000}
      NAPCAT_GID: ${NAPCAT_GID:-1000}
    volumes:
      - "${NAPCAT_QQ_DIR:-/srv/napcat/qq}:/app/.config/QQ"
      - "${NAPCAT_CONFIG_DIR:-/srv/napcat/config}:/app/napcat/config"
```

- [ ] **Step 2: 把 NapCat WebSocket 目标切到 qq-adapter**

`scripts/configure_napcat_ws.sh` 中：

```bash
NAPCAT_WS_CLIENT_URL="${NAPCAT_WS_CLIENT_URL:-ws://host.docker.internal:19000/napcat/ws}"
```

并移除 `CLAUDE_BRIDGE_QQ_BOT_ID` 的强依赖，改为：

```bash
QQ_ADAPTER_QQ_BOT_ID="${QQ_ADAPTER_QQ_BOT_ID:-}"
if [[ -z "$QQ_ADAPTER_QQ_BOT_ID" ]]; then
  echo "QQ_ADAPTER_QQ_BOT_ID is required to configure NapCat websocket client" >&2
  exit 1
fi
CONFIG_FILE="$NAPCAT_CONFIG_DIR/onebot11_${QQ_ADAPTER_QQ_BOT_ID}.json"
```

- [ ] **Step 3: 新增 QQ adapter 启动脚本**

创建 `scripts/start_qq_adapter.sh`：

```bash
#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
env_file="${1:-$repo_root/deploy/dogbot.env}"

set -a
source "$env_file"
set +a

log_dir="${AGENT_STATE_DIR:-/srv/agent-state}/logs"
pid_file="${QQ_ADAPTER_PID_FILE:-${AGENT_STATE_DIR:-/srv/agent-state}/qq-adapter.pid}"
mkdir -p "$log_dir"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" >/dev/null 2>&1; then
  echo "qq-adapter already running with pid $(cat "$pid_file")"
  exit 0
fi

nohup uv run --with fastapi --with uvicorn --with httpx \
  uvicorn qq_adapter.app:create_app \
  --factory \
  --host "${QQ_ADAPTER_HOST:-0.0.0.0}" \
  --port "${QQ_ADAPTER_PORT:-19000}" \
  >>"$log_dir/qq-adapter.log" 2>&1 < /dev/null &

echo $! > "$pid_file"
echo "Started qq-adapter with pid $(cat "$pid_file")"
```

- [ ] **Step 4: 更新 deploy/stop 脚本**

`deploy_stack.sh` 需要：

- 不再启动 `astrbot`
- 启动 `qq-adapter`
- 不再输出 AstrBot WebUI

`stop_stack.sh` 需要：

- 停止 `qq-adapter`
- 不再处理 `astrbot`

- [ ] **Step 5: 验证 compose 和脚本**

Run:

```bash
bash -n scripts/deploy_stack.sh scripts/stop_stack.sh scripts/start_qq_adapter.sh scripts/configure_napcat_ws.sh
./scripts/check_structure.sh
```

Expected:
- 脚本语法通过
- 结构检查通过

- [ ] **Step 6: Commit**

```bash
git add compose/platform-stack.yml scripts/deploy_stack.sh scripts/stop_stack.sh scripts/start_qq_adapter.sh scripts/configure_napcat_ws.sh
git commit -m "refactor: remove astrbot from qq path"
```

## Task 4: 交互式部署流程

**Files:**
- Modify: `scripts/deploy_stack.sh`
- Create: `scripts/lib/platform_selection.sh`
- Create: `scripts/lib/qr_output.sh`

- [ ] **Step 1: 提取平台选择逻辑**

创建 `scripts/lib/platform_selection.sh`：

```bash
parse_platform_args() {
  ENABLE_QQ_SELECTED=""
  ENABLE_WECHAT_SELECTED=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --qq) ENABLE_QQ_SELECTED=1 ;;
      --no-qq) ENABLE_QQ_SELECTED=0 ;;
      --wechat) ENABLE_WECHAT_SELECTED=1 ;;
      --no-wechat) ENABLE_WECHAT_SELECTED=0 ;;
    esac
    shift
  done
}

prompt_platform_enable() {
  local label="$1"
  local answer
  read -r -p "是否启用${label}？[y/N] " answer
  [[ "$answer" =~ ^[Yy]$ ]] && echo 1 || echo 0
}
```

- [ ] **Step 2: 在 deploy_stack.sh 中接入默认交互模式**

逻辑要求：

- 如果没有传平台参数：
  - 先问 QQ
  - 再问微信
- 如果传了平台参数：
  - 按参数执行，不再询问

实现片段：

```bash
parse_platform_args "$@"
if [[ -z "${ENABLE_QQ_SELECTED:-}" ]]; then
  ENABLE_QQ_SELECTED="$(prompt_platform_enable "QQ")"
fi
if [[ -z "${ENABLE_WECHAT_SELECTED:-}" ]]; then
  ENABLE_WECHAT_SELECTED="$(prompt_platform_enable "微信")"
fi
```

- [ ] **Step 3: 如果没有选择任何平台则清理退出**

```bash
if [[ "$ENABLE_QQ_SELECTED" != "1" && "$ENABLE_WECHAT_SELECTED" != "1" ]]; then
  echo "未选择任何平台，开始清理并退出。"
  "$repo_root/scripts/stop_stack.sh" "$env_file" || true
  exit 0
fi
```

- [ ] **Step 4: 新增终端二维码输出工具**

创建 `scripts/lib/qr_output.sh`：

```bash
print_qr_code() {
  local label="$1"
  local link="$2"
  local image_path="$3"
  echo "==== ${label} 登录二维码 ===="
  if command -v qrencode >/dev/null 2>&1; then
    qrencode -t ANSIUTF8 "$link" || true
  fi
  echo "二维码图片: $image_path"
  echo "登录链接: $link"
}
```

- [ ] **Step 5: 把 QQ / 微信二维码都接入终端输出**

`deploy_stack.sh` 在准备登录后：

- QQ：
  - 调 `prepare_napcat_login.sh`
  - 取回图片路径和链接
  - 调 `print_qr_code`
- 微信：
  - 调 `prepare_wechatpadpro_login.sh`
  - 取回图片路径和链接
  - 调 `print_qr_code`

- [ ] **Step 6: 验证交互流程**

Run:

```bash
bash -n scripts/deploy_stack.sh scripts/lib/platform_selection.sh scripts/lib/qr_output.sh
```

Expected:
- 语法通过

- [ ] **Step 7: Commit**

```bash
git add scripts/deploy_stack.sh scripts/lib/platform_selection.sh scripts/lib/qr_output.sh
git commit -m "feat: add interactive platform selection"
```

## Task 5: 调试体验增强

**Files:**
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/stop_stack.sh`
- Modify: `scripts/start_agent_runner.sh`
- Modify: `scripts/start_qq_adapter.sh`
- Modify: `scripts/start_wechatpadpro_adapter.sh`

- [ ] **Step 1: 给启动脚本增加统一输出**

要求每个脚本至少输出：

- 当前使用的 env 文件
- 当前启动的组件
- 当前日志目录
- 当前 PID 文件

- [ ] **Step 2: 给常见错误增加可操作提示**

至少覆盖：

- env 文件缺失
- Docker Compose 缺失
- 上游地址或 token 缺失
- 平台未选择
- 二维码准备失败
- 平台登录尚未完成

- [ ] **Step 3: 给 stop_stack.sh 增加停止结果摘要**

输出：

- 停掉了哪些 compose stack
- 停掉了哪些宿主机进程
- 是否清理了网络策略

- [ ] **Step 4: 验证 shell 语法**

Run:

```bash
bash -n scripts/deploy_stack.sh scripts/stop_stack.sh scripts/start_agent_runner.sh scripts/start_qq_adapter.sh scripts/start_wechatpadpro_adapter.sh
```

Expected:
- 全部通过

- [ ] **Step 5: Commit**

```bash
git add scripts/deploy_stack.sh scripts/stop_stack.sh scripts/start_agent_runner.sh scripts/start_qq_adapter.sh scripts/start_wechatpadpro_adapter.sh
git commit -m "chore: improve deployment diagnostics"
```

## Task 6: 最终联调与文档回填

**Files:**
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `compose/README.md`
- Modify: `deploy/dogbot.env.example`

- [ ] **Step 1: 更新 README 中的架构图**

改成：

```text
QQ -> NapCat -> qq-adapter -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner
```

- [ ] **Step 2: 更新 deploy/README 中的一键部署说明**

明确写出：

- 用户只修改 `deploy/dogbot.env`
- 默认运行 `./scripts/deploy_stack.sh`
- 默认运行 `./scripts/stop_stack.sh`
- 默认通过终端二维码完成登录

- [ ] **Step 3: 更新 env 示例**

新增 QQ adapter 相关字段：

```env
QQ_ADAPTER_HOST=0.0.0.0
QQ_ADAPTER_PORT=19000
QQ_ADAPTER_QQ_BOT_ID=
QQ_ADAPTER_PID_FILE=/srv/agent-state/qq-adapter.pid
QQ_ADAPTER_DEFAULT_CWD=/workspace
QQ_ADAPTER_TIMEOUT_SECS=120
QQ_ADAPTER_COMMAND_NAME=agent
QQ_ADAPTER_STATUS_COMMAND_NAME=agent-status
```

- [ ] **Step 4: 跑最终验证**

Run:

```bash
uv run --with pytest --with fastapi --with httpx python -m pytest qq_adapter/tests wechatpadpro_adapter/tests astrbot/plugins/claude_runner_bridge/tests/test_main.py -q
bash -n scripts/deploy_stack.sh scripts/stop_stack.sh scripts/start_agent_runner.sh scripts/start_qq_adapter.sh scripts/start_wechatpadpro_adapter.sh scripts/configure_napcat_ws.sh scripts/prepare_napcat_login.sh scripts/prepare_wechatpadpro_login.sh
./scripts/check_structure.sh
```

Expected:
- 测试通过
- 脚本语法通过
- 结构检查通过

- [ ] **Step 5: Commit**

```bash
git add README.md deploy/README.md compose/README.md deploy/dogbot.env.example
git commit -m "docs: finalize one-command deployment flow"
```

## 自检

### 设计覆盖检查

本计划已覆盖设计中的 5 个结果：

- 唯一配置入口：Task 1, Task 6
- 唯一部署入口：Task 4, Task 5
- 唯一停止入口：Task 3, Task 5
- `compose/` 只作为高级配置层：Task 1
- 去掉 AstrBot 的最终形态：Task 2, Task 3

本计划也覆盖了交互式部署流程：

- 参数模式
- 默认交互模式
- 终端二维码打印
- 未选择平台时清理退出

### 占位符检查

本计划没有保留 `TODO` / `TBD` / “后续补充” 这类占位步骤。每个任务都给出了具体文件、代码片段、命令和验证方式。

### 作用域检查

本计划是一个完整的结构调整计划，范围较大，但仍围绕同一个目标：把部署体验和接入架构一起收敛为“一配置、一启动、一停止、无 AstrBot WebUI”的最终形态。不是多个无关子项目的拼接。
