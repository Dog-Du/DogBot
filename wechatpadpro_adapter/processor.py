from __future__ import annotations

import hashlib
import hmac
import logging
from typing import Any

from fastapi import HTTPException

from .command_policy import CommandMatch, resolve_command
from .config import Settings
from .events import RecentMessageCache, build_message_dedupe_key, is_text_event
from .mapper import build_run_payload, extract_content, unwrap_event
from .runner_client import AgentRunnerClient
from .wechat_client import WeChatPadProClient

logger = logging.getLogger(__name__)


class EventProcessor:
    def __init__(
        self,
        settings: Settings,
        runner: AgentRunnerClient | None = None,
        wechat: WeChatPadProClient | None = None,
        recent_messages: RecentMessageCache | None = None,
    ) -> None:
        self.settings = settings
        self.runner = runner or AgentRunnerClient(settings.agent_runner_base_url)
        self.wechat = wechat or WeChatPadProClient(
            settings.wechatpadpro_base_url, settings.account_key
        )
        self.recent_messages = recent_messages or RecentMessageCache()

    def verify_signature(self, body: bytes, signature: str) -> bool:
        secret = self.settings.webhook_secret
        if not secret:
            return True
        if not signature:
            return False
        digest = hmac.new(secret.encode("utf-8"), body, hashlib.sha256).hexdigest()
        return hmac.compare_digest(digest, signature)

    async def run_agent(self, payload: dict[str, Any]) -> dict[str, Any]:
        request = build_run_payload(
            payload,
            default_cwd=self.settings.default_cwd,
            timeout_secs=self.settings.default_timeout_secs,
        )
        return await self.runner.run(request, self.settings.default_timeout_secs)

    async def send_reply(self, event: dict[str, Any], result: dict[str, Any]) -> None:
        stdout = str(result.get("stdout", "")).strip()
        stderr = str(result.get("stderr", "")).strip()
        text = stdout or stderr
        if not text:
            logger.warning("wechatpadpro send_reply skipped: empty stdout/stderr")
            return
        response = await self.wechat.send_text(event, text)
        logger.warning(
            "wechatpadpro send_reply success: status=%s body=%s",
            response.status_code,
            response.text[:500],
        )

    async def handle_payload(
        self, payload: dict[str, Any], raw_body: bytes, signature: str
    ) -> dict[str, str]:
        if not self.verify_signature(raw_body, signature):
            raise HTTPException(status_code=401, detail="invalid webhook signature")

        event_type = str(payload.get("event_type") or payload.get("type") or "").strip().lower()
        if event_type and event_type not in {"message", "message_received"}:
            logger.warning("wechatpadpro event ignored: event_type=%r", event_type)
            return {"status": "ignored"}

        if not is_text_event(payload):
            event = unwrap_event(payload)
            logger.warning(
                "wechatpadpro event ignored: non-text msg_type=%r content=%r keys=%s",
                event.get("msgType") or event.get("MsgType"),
                extract_content(event),
                sorted(event.keys()),
            )
            return {"status": "ignored"}

        event = unwrap_event(payload)
        dedupe_key = build_message_dedupe_key(payload)
        logger.warning(
            "wechatpadpro inbound text: msg_type=%r msg_id=%r new_msg_id=%r dedupe_key=%r content=%r from=%r to=%r",
            event.get("msgType") or event.get("MsgType"),
            event.get("msgId") or event.get("MsgId"),
            event.get("newMsgId") or event.get("NewMsgId"),
            dedupe_key,
            extract_content(event),
            event.get("fromUserName") or event.get("FromUserName") or event.get("senderWxid"),
            event.get("toUserName") or event.get("ToUserName"),
        )

        if self.recent_messages.check_and_mark(dedupe_key):
            logger.warning("wechatpadpro event ignored: duplicate message")
            return {"status": "ignored"}

        command = resolve_command(payload, self.settings)
        if command is None:
            logger.warning("wechatpadpro event ignored: no command match")
            return {"status": "ignored"}

        await self._dispatch(payload, command)
        logger.warning("wechatpadpro event accepted: mode=%s", command.mode)
        return {"status": "accepted"}

    async def _dispatch(self, payload: dict[str, Any], command: CommandMatch) -> None:
        try:
            if command.mode == "status":
                health = await self.runner.healthz()
                result = {
                    "stdout": (
                        "agent-runner ok"
                        if health.get("status") == "ok"
                        else "agent-runner unavailable"
                    ),
                    "stderr": "",
                }
            else:
                result_payload = payload
                if command.prompt:
                    event = unwrap_event(payload)
                    event["content"] = command.prompt
                    result_payload = {
                        "event_type": payload.get("event_type")
                        or payload.get("type")
                        or "message",
                        "message": event,
                    }
                result = await self.run_agent(result_payload)

            await self.send_reply(payload, result)
        except Exception as exc:
            event = unwrap_event(payload)
            logger.exception(
                "wechatpadpro event handling failed: mode=%s content=%r keys=%s sender=%r room=%r msg_type=%r",
                command.mode,
                extract_content(event),
                sorted(event.keys()),
                event.get("senderWxid") or event.get("fromUserName") or event.get("fromUsername"),
                event.get("roomId") or event.get("chatroomId") or event.get("fromChatRoom"),
                event.get("msgType") or event.get("MsgType"),
            )
            raise HTTPException(status_code=502, detail=str(exc)) from exc
