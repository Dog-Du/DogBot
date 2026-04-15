from __future__ import annotations

import os
from typing import Any

import httpx
import astrbot.api.message_components as Comp

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
        self.agent_runner_base_url = os.getenv(
            "AGENT_RUNNER_BASE_URL",
            config.get("agent_runner_base_url", "http://127.0.0.1:8787"),
        ).rstrip("/")
        self.default_cwd = os.getenv(
            "CLAUDE_BRIDGE_DEFAULT_CWD", config.get("default_cwd", "/workspace")
        )
        self.default_timeout_secs = int(
            os.getenv(
                "CLAUDE_BRIDGE_TIMEOUT_SECS",
                str(config.get("default_timeout_secs", 120)),
            )
        )
        self.command_name = os.getenv(
            "CLAUDE_BRIDGE_COMMAND_NAME", config.get("command_name", "agent")
        )
        self.status_command_name = os.getenv(
            "CLAUDE_BRIDGE_STATUS_COMMAND_NAME",
            config.get("status_command_name", "agent-status"),
        )
        self.qq_bot_id = os.getenv(
            "CLAUDE_BRIDGE_QQ_BOT_ID", str(config.get("qq_bot_id", ""))
        ).strip()

    @filter.event_message_type(filter.EventMessageType.ALL)
    async def on_all_message(
        self, event: AstrMessageEvent
    ) -> MessageEventResult | None:
        raw_message = event.message_str.strip()
        logger.info(
            "claude_runner_bridge saw message: origin=%s platform=%s text=%s",
            getattr(event, "unified_msg_origin", ""),
            self._platform_name(event),
            raw_message,
        )
        if self._is_status_command(raw_message):
            return await self.agent_status(event)
        if not self._should_route_to_agent(event, raw_message):
            return None

        result = await self._forward_request(event, raw_message)
        result.stop_event()
        event.set_result(result)
        event.stop_event()
        return None

    async def _forward_request(
        self, event: AstrMessageEvent, raw_message: str
    ) -> MessageEventResult:
        prompt = self._extract_prompt(event, raw_message, self.command_name)
        if not prompt:
            return self._text_result(event, f"Usage: /{self.command_name} <prompt>")

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
            return self._text_result(event, "agent-runner unavailable")

        data = response.json()
        if response.is_success:
            stdout = str(data.get("stdout", "")).strip()
            stderr = str(data.get("stderr", "")).strip()
            if stdout:
                return self._success_result(event, stdout)
            if stderr:
                return self._text_result(event, stderr)
            return self._text_result(event, "(empty response)")

        return self._text_result(event, self._format_error(data))

    async def agent_status(self, event: AstrMessageEvent) -> MessageEventResult | None:
        try:
            async with httpx.AsyncClient(
                base_url=self.agent_runner_base_url, timeout=5
            ) as client:
                response = await client.get("/healthz")
        except httpx.HTTPError as exc:
            logger.exception("agent-runner health check failed: %s", exc)
            result = self._text_result(event, "agent-runner unavailable")
            result.stop_event()
            event.set_result(result)
            event.stop_event()
            return None

        if response.is_success:
            result = self._text_result(event, "agent-runner ok")
            result.stop_event()
            event.set_result(result)
            event.stop_event()
            return None
        result = self._text_result(
            event, f"agent-runner unhealthy: {response.status_code}"
        )
        result.stop_event()
        event.set_result(result)
        event.stop_event()
        return None

    def _build_payload(self, event: AstrMessageEvent, prompt: str) -> dict[str, Any]:
        platform = self._platform_name(event)
        user_id = str(event.get_sender_id())
        group_id = self._group_id(event)
        message_id = self._message_id(event)
        conversation_identity = self._conversation_identity(event, platform, group_id)
        if group_id:
            conversation_id = conversation_identity or f"{platform}:group:{group_id}"
            session_id = f"{conversation_id}:user:{user_id}"
            chat_type = "group"
        else:
            conversation_id = conversation_identity or f"{platform}:private:{user_id}"
            session_id = conversation_id
            chat_type = "private"

        payload = {
            "platform": platform,
            "conversation_id": conversation_id,
            "session_id": session_id,
            "user_id": user_id,
            "chat_type": chat_type,
            "cwd": self.default_cwd,
            "prompt": prompt,
            "timeout_secs": self.default_timeout_secs,
        }

        if message_id:
            payload["reply_to_message_id"] = message_id
        if group_id:
            payload["mention_user_id"] = user_id

        return payload

    def _should_route_to_agent(self, event: AstrMessageEvent, message: str) -> bool:
        normalized = self._normalize_message_for_routing(event, message)
        if not normalized:
            return False
        if self._is_status_command(normalized):
            return False
        if self._is_qq_group_message(event):
            return self._is_addressed_to_qq_bot(event, message) and self._matches_agent_command(
                event, normalized
            )
        return self._matches_agent_command(event, normalized)

    def _is_qq_group_message(self, event: AstrMessageEvent) -> bool:
        if self._group_id(event) is None:
            return False
        origin = str(getattr(event, "unified_msg_origin", "") or "")
        if ":GroupMessage:" in origin:
            return True
        return self._platform_name(event) == "qq"

    def _is_addressed_to_qq_bot(self, event: AstrMessageEvent, message: str) -> bool:
        if not self.qq_bot_id:
            return False
        if f"[At:{self.qq_bot_id}]" in message:
            return True
        return f"[At:{self.qq_bot_id}]" in self._raw_chain_text(event)

    def _is_status_command(self, message: str) -> bool:
        prefixes = (
            f"/{self.status_command_name}",
            f"!{self.status_command_name}",
            f"！{self.status_command_name}",
        )
        return any(message == prefix or message.startswith(f"{prefix} ") for prefix in prefixes)

    def _is_agent_alias(self, message: str) -> bool:
        prefixes = (
            f"/{self.command_name}",
            f"!{self.command_name}",
            f"！{self.command_name}",
        )
        return any(message == prefix or message.startswith(f"{prefix} ") for prefix in prefixes)

    def _matches_agent_command(self, event: AstrMessageEvent, message: str) -> bool:
        if self._is_agent_alias(message):
            return True

        raw_chain_text = self._raw_chain_text(event)
        if not raw_chain_text:
            return False

        raw_without_mentions = self._strip_leading_qq_mentions(raw_chain_text)
        return raw_without_mentions.startswith(f"/{self.command_name}") and (
            message == self.command_name or message.startswith(f"{self.command_name} ")
        )

    def _extract_prompt(
        self, event: AstrMessageEvent, message: str, command_name: str
    ) -> str:
        normalized = self._strip_leading_qq_mentions(message)
        prefixes = (
            f"/{command_name}",
            f"！{command_name}",
            f"!{command_name}",
        )
        for prefix in prefixes:
            if normalized.startswith(prefix):
                return normalized[len(prefix) :].strip()
        if normalized.startswith(f"{command_name} "):
            raw_without_mentions = self._strip_leading_qq_mentions(
                self._raw_chain_text(event)
            )
            if raw_without_mentions.startswith(f"/{command_name}"):
                return normalized[len(command_name) :].strip()
        return normalized.strip()

    def _raw_chain_text(self, event: AstrMessageEvent) -> str:
        parts: list[str] = []
        try:
            messages = event.get_messages()
        except AttributeError:
            return ""

        for component in messages:
            qq = getattr(component, "qq", None)
            text = getattr(component, "text", None)
            if qq is not None:
                parts.append(f"[At:{qq}]")
            elif isinstance(text, str):
                parts.append(text)
        return "".join(parts).strip()

    def _normalize_message_for_routing(
        self, event: AstrMessageEvent, message: str
    ) -> str:
        if self._is_qq_group_message(event):
            return self._strip_leading_qq_mentions(message)
        return message.lstrip()

    def _strip_leading_qq_mentions(self, message: str) -> str:
        normalized = message.lstrip()
        while normalized.startswith("[At:"):
            closing_index = normalized.find("]")
            if closing_index == -1:
                break
            normalized = normalized[closing_index + 1 :].lstrip()
        return normalized

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

    def _success_result(
        self, event: AstrMessageEvent, message: str
    ) -> MessageEventResult:
        return self._text_result(event, message)

    def _text_result(self, event: AstrMessageEvent, message: str) -> MessageEventResult:
        group_id = self._group_id(event)
        if group_id and self._is_qq_group_message(event):
            return event.chain_result(
                [Comp.At(qq=str(event.get_sender_id())), Comp.Plain(message)]
            )
        return event.plain_result(message)

    def _platform_name(self, event: AstrMessageEvent) -> str:
        try:
            return str(event.get_platform_name())
        except AttributeError:
            origin = getattr(event, "unified_msg_origin", "") or "unknown"
            return origin.split(":", 1)[0]

    def _conversation_identity(
        self, event: AstrMessageEvent, platform: str, group_id: str | None
    ) -> str | None:
        origin = getattr(event, "unified_msg_origin", None)
        if origin:
            return str(origin)

        message_obj = getattr(event, "message_obj", None)
        if message_obj is None:
            return None

        session_id = getattr(message_obj, "session_id", None)
        if session_id in (None, "", 0):
            return None

        if group_id:
            return f"{platform}:group:{session_id}"
        return f"{platform}:private:{session_id}"

    def _group_id(self, event: AstrMessageEvent) -> str | None:
        message_obj = getattr(event, "message_obj", None)
        if message_obj is None:
            return None
        group_id = getattr(message_obj, "group_id", None)
        if group_id in (None, "", 0):
            return None
        return str(group_id)

    def _message_id(self, event: AstrMessageEvent) -> str | None:
        message_obj = getattr(event, "message_obj", None)
        if message_obj is None:
            return None
        message_id = getattr(message_obj, "message_id", None)
        if message_id in (None, "", 0):
            return None
        return str(message_id)
