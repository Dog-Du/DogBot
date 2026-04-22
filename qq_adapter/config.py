from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
    bind_addr: str
    agent_runner_base_url: str
    napcat_api_base_url: str
    napcat_access_token: str | None
    default_cwd: str
    default_timeout_secs: int
    command_name: str
    status_command_name: str
    qq_bot_id: str
    platform_account_id: str

    @classmethod
    def from_env(cls) -> "Settings":
        qq_bot_id = os.getenv("QQ_ADAPTER_QQ_BOT_ID", "").strip()
        platform_account_id = (
            os.getenv("QQ_PLATFORM_ACCOUNT_ID")
            or (f"qq:bot_uin:{qq_bot_id}" if qq_bot_id else "qq:bot_uin:unknown")
        )
        return cls(
            bind_addr=os.getenv("QQ_ADAPTER_BIND_ADDR", "0.0.0.0:19000"),
            agent_runner_base_url=os.getenv("AGENT_RUNNER_BASE_URL", "http://127.0.0.1:11451").rstrip("/"),
            napcat_api_base_url=os.getenv("NAPCAT_API_BASE_URL", "http://127.0.0.1:3001").rstrip("/"),
            napcat_access_token=(os.getenv("NAPCAT_ACCESS_TOKEN") or "").strip() or None,
            default_cwd=os.getenv("QQ_ADAPTER_DEFAULT_CWD", "/workspace"),
            default_timeout_secs=int(os.getenv("QQ_ADAPTER_TIMEOUT_SECS", "120")),
            command_name=os.getenv("QQ_ADAPTER_COMMAND_NAME", "agent").strip(),
            status_command_name=os.getenv("QQ_ADAPTER_STATUS_COMMAND_NAME", "agent-status").strip(),
            platform_account_id=platform_account_id.strip(),
            qq_bot_id=qq_bot_id,
        )
