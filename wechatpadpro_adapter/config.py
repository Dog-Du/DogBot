from __future__ import annotations

from dataclasses import dataclass
import os


@dataclass(slots=True)
class Settings:
    bind_addr: str
    wechatpadpro_base_url: str
    agent_runner_base_url: str
    account_key: str | None
    shared_token: str | None
    webhook_secret: str | None
    adapter_webhook_url: str | None
    auto_configure_webhook: bool
    default_cwd: str
    default_timeout_secs: int

    @classmethod
    def from_env(cls) -> "Settings":
        return cls(
            bind_addr=os.getenv("WECHATPADPRO_ADAPTER_BIND_ADDR", "0.0.0.0:18999"),
            wechatpadpro_base_url=os.getenv(
                "WECHATPADPRO_BASE_URL", "http://127.0.0.1:38849"
            ).rstrip("/"),
            agent_runner_base_url=os.getenv(
                "AGENT_RUNNER_BASE_URL", "http://127.0.0.1:11451"
            ).rstrip("/"),
            account_key=os.getenv("WECHATPADPRO_ACCOUNT_KEY") or None,
            shared_token=os.getenv("WECHATPADPRO_ADAPTER_SHARED_TOKEN") or None,
            webhook_secret=os.getenv("WECHATPADPRO_WEBHOOK_SECRET") or None,
            adapter_webhook_url=os.getenv("WECHATPADPRO_ADAPTER_WEBHOOK_URL") or None,
            auto_configure_webhook=os.getenv(
                "WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK", "0"
            ).lower()
            in {"1", "true", "yes", "on"},
            default_cwd=os.getenv("WECHATPADPRO_DEFAULT_CWD", "/workspace"),
            default_timeout_secs=int(
                os.getenv("WECHATPADPRO_DEFAULT_TIMEOUT_SECS", "120")
            ),
        )
