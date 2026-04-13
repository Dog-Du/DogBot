from __future__ import annotations

from typing import Any

import httpx

from astrbot.api import AstrBotConfig, logger
from astrbot.api.event import AstrMessageEvent, MessageEventResult, filter
from astrbot.api.star import Context, Star, register


@register(
    "claude_runner_bridge",
    "dogdu",
    "Bridge AstrBot command messages to the local agent-runner service.",
    "0.1.0",
)
class ClaudeRunnerBridge(Star):
    def __init__(self, context: Context, config: AstrBotConfig) -> None:
        super().__init__(context)
        self.config = config
        self.agent_runner_base_url = config.get(
            "agent_runner_base_url", "http://127.0.0.1:8787"
        ).rstrip("/")
        self.default_cwd = config.get("default_cwd", "/workspace")
        self.default_timeout_secs = int(config.get("default_timeout_secs", 120))
        self.command_name = config.get("command_name", "agent")
        self.status_command_name = config.get("status_command_name", "agent-status")

    @filter.command("agent")
    async def run_agent(self, event: AstrMessageEvent) -> MessageEventResult:
        raw_message = event.message_str.strip()
        prompt = self._extract_prompt(raw_message, self.command_name)
        if not prompt:
            return event.plain_result(f"Usage: /{self.command_name} <prompt>")

        payload = self._build_payload(event, prompt)
        logger.info(
            "claude_runner_bridge forwarding request: conversation_id=%s session_id=%s",
            payload["conversation_id"],
            payload["session_id"],
        )

        try:
            async with httpx.AsyncClient(
                base_url=self.agent_runner_base_url,
                timeout=self.default_timeout_secs + 10,
            ) as client:
                response = await client.post("/v1/runs", json=payload)
        except httpx.HTTPError as exc:
            logger.exception("agent-runner request failed: %s", exc)
            return event.plain_result("agent-runner unavailable")

        data = response.json()
        if response.is_success:
            stdout = str(data.get("stdout", "")).strip()
            stderr = str(data.get("stderr", "")).strip()
            if stdout:
                return event.plain_result(stdout)
            if stderr:
                return event.plain_result(stderr)
            return event.plain_result("(empty response)")

        return event.plain_result(self._format_error(data))

    @filter.command("agent-status")
    async def agent_status(self, event: AstrMessageEvent) -> MessageEventResult:
        try:
            async with httpx.AsyncClient(
                base_url=self.agent_runner_base_url, timeout=5
            ) as client:
                response = await client.get("/healthz")
        except httpx.HTTPError as exc:
            logger.exception("agent-runner health check failed: %s", exc)
            return event.plain_result("agent-runner unavailable")

        if response.is_success:
            return event.plain_result("agent-runner ok")
        return event.plain_result(f"agent-runner unhealthy: {response.status_code}")

    def _build_payload(self, event: AstrMessageEvent, prompt: str) -> dict[str, Any]:
        platform = self._platform_name(event)
        user_id = str(event.get_sender_id())
        group_id = self._group_id(event)
        if group_id:
            conversation_id = f"{platform}:group:{group_id}"
            session_id = f"{conversation_id}:user:{user_id}"
            chat_type = "group"
        else:
            conversation_id = f"{platform}:private:{user_id}"
            session_id = conversation_id
            chat_type = "private"

        return {
            "platform": platform,
            "conversation_id": conversation_id,
            "session_id": session_id,
            "user_id": user_id,
            "chat_type": chat_type,
            "cwd": self.default_cwd,
            "prompt": prompt,
            "timeout_secs": self.default_timeout_secs,
        }

    def _extract_prompt(self, message: str, command_name: str) -> str:
        normalized = message.lstrip()
        prefixes = (f"/{command_name}", f"！{command_name}", f"!{command_name}")
        for prefix in prefixes:
            if normalized.startswith(prefix):
                return normalized[len(prefix) :].strip()
        return normalized.strip()

    def _format_error(self, data: dict[str, Any]) -> str:
        error_code = str(data.get("error_code", "unknown"))
        message = str(data.get("message", "request failed"))
        if error_code == "timeout":
            return "agent timeout"
        if error_code in {"queue_full", "rate_limited"}:
            return "agent busy, try again later"
        if error_code == "session_conflict":
            return "session conflict detected"
        return f"agent error: {message}"

    def _platform_name(self, event: AstrMessageEvent) -> str:
        try:
            return str(event.get_platform_name())
        except AttributeError:
            origin = getattr(event, "unified_msg_origin", "") or "unknown"
            return origin.split(":", 1)[0]

    def _group_id(self, event: AstrMessageEvent) -> str | None:
        message_obj = getattr(event, "message_obj", None)
        if message_obj is None:
            return None
        group_id = getattr(message_obj, "group_id", None)
        if group_id in (None, "", 0):
            return None
        return str(group_id)
