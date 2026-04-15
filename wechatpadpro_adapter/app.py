from __future__ import annotations

import hashlib
import hmac
import json
import logging
import time
from typing import Any

from fastapi import FastAPI, HTTPException, Request

from .config import Settings
from .mapper import build_run_payload, extract_content, unwrap_event
from .runner_client import AgentRunnerClient
from .wechat_client import WeChatPadProClient

logger = logging.getLogger(__name__)


class RecentMessageCache:
    def __init__(self, ttl_seconds: float = 30.0) -> None:
        self.ttl_seconds = ttl_seconds
        self._seen: dict[str, float] = {}

    def check_and_mark(self, key: str | None) -> bool:
        if not key:
            return False
        now = time.monotonic()
        expired = [
            existing_key
            for existing_key, timestamp in self._seen.items()
            if now - timestamp > self.ttl_seconds
        ]
        for existing_key in expired:
            self._seen.pop(existing_key, None)

        if key in self._seen:
            return True

        self._seen[key] = now
        return False


def normalize_command_content(content: str) -> str:
    normalized = content.strip()
    if not normalized:
        return normalized

    parts = normalized.split(maxsplit=1)
    if len(parts) == 2 and parts[0].startswith("@"):
        return parts[1].strip()

    return normalized


def strip_group_mention_prefix(
    content: str, mention_names: tuple[str, ...]
) -> tuple[str, bool]:
    normalized = content.strip()
    if not normalized:
        return ("", False)

    for name in mention_names:
        prefix = f"@{name}"
        if normalized.startswith(prefix):
            return (normalized[len(prefix) :].strip(), True)

    return (normalized, False)


async def run_agent(
    runner: AgentRunnerClient, event: dict[str, Any], settings: Settings
) -> dict[str, Any]:
    payload = build_run_payload(
        event, default_cwd=settings.default_cwd, timeout_secs=settings.default_timeout_secs
    )
    return await runner.run(payload, settings.default_timeout_secs)


async def send_reply(
    wechat: WeChatPadProClient, event: dict[str, Any], result: dict[str, Any]
) -> None:
    stdout = str(result.get("stdout", "")).strip()
    stderr = str(result.get("stderr", "")).strip()
    text = stdout or stderr
    if not text:
        logger.warning("wechatpadpro send_reply skipped: empty stdout/stderr")
        return
    response = await wechat.send_text(event, text)
    logger.warning(
        "wechatpadpro send_reply success: status=%s body=%s",
        response.status_code,
        response.text[:500],
    )


def normalize_prompt(
    payload: dict[str, Any], settings: Settings
) -> tuple[str, str] | None:
    event = unwrap_event(payload)
    content = normalize_command_content(extract_content(event))
    is_group = str(event.get("fromUserName") or event.get("FromUserName") or "").strip().endswith(
        "@chatroom"
    )
    mentioned = False
    if is_group and settings.require_group_mention:
        content, mentioned = strip_group_mention_prefix(content, settings.bot_mention_names)
        if not mentioned:
            return None
    if not content:
        return None

    status_command = f"/{settings.status_command_name}".strip()
    if content == status_command:
        return ("status", "")

    if not settings.require_command_prefix:
        return ("run", content)

    command_prefix = f"/{settings.command_name}".strip()
    if content == command_prefix:
        return ("run", "")
    if content.startswith(f"{command_prefix} "):
        return ("run", content[len(command_prefix) :].strip())

    return None


def verify_signature(body: bytes, signature: str, secret: str | None) -> bool:
    if not secret:
        return True
    if not signature:
        return False
    digest = hmac.new(secret.encode("utf-8"), body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(digest, signature)


def is_text_event(payload: dict[str, Any]) -> bool:
    event = unwrap_event(payload)
    msg_type = str(event.get("msgType") or event.get("MsgType") or "").strip().lower()
    if msg_type:
        return msg_type in {"1", "text"}
    return bool(extract_content(event))


def build_message_dedupe_key(payload: dict[str, Any]) -> str | None:
    event = unwrap_event(payload)
    msg_id = str(
        event.get("msgId")
        or event.get("MsgId")
        or event.get("newMsgId")
        or event.get("NewMsgId")
        or ""
    ).strip()
    if msg_id:
        return msg_id

    content = extract_content(event)
    if not content:
        return None

    from_user = str(
        event.get("fromUserName")
        or event.get("FromUserName")
        or event.get("senderWxid")
        or ""
    ).strip()
    create_time = str(event.get("CreateTime") or event.get("createTime") or "").strip()
    if not from_user and not create_time:
        return None
    return f"{from_user}:{create_time}:{content}"


def create_app() -> FastAPI:
    app = FastAPI()
    settings = Settings.from_env()
    runner = AgentRunnerClient(settings.agent_runner_base_url)
    wechat = WeChatPadProClient(
        settings.wechatpadpro_base_url, settings.account_key
    )
    recent_messages = RecentMessageCache()

    @app.get("/healthz")
    async def healthz() -> dict[str, str]:
        return {"status": "ok"}

    @app.api_route("/wechatpadpro/events", methods=["GET", "HEAD"])
    async def webhook_probe() -> dict[str, str]:
        return {"status": "ok"}

    @app.post("/wechatpadpro/events")
    async def receive_event(request: Request) -> dict[str, str]:
        raw_body = await request.body()
        try:
            payload = json.loads(raw_body.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise HTTPException(status_code=400, detail="invalid json payload") from exc

        signature = str(payload.get("signature") or request.headers.get("X-Signature") or "").strip()
        if not verify_signature(raw_body, signature, settings.webhook_secret):
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

        if recent_messages.check_and_mark(dedupe_key):
            logger.warning("wechatpadpro event ignored: duplicate message")
            return {"status": "ignored"}

        command = normalize_prompt(payload, settings)
        if command is None:
            logger.warning("wechatpadpro event ignored: no command match")
            return {"status": "ignored"}

        try:
            mode, prompt = command
            if mode == "status":
                health = await runner.healthz()
                result = {"stdout": "agent-runner ok" if health.get("status") == "ok" else "agent-runner unavailable", "stderr": ""}
            else:
                if prompt:
                    event = unwrap_event(payload)
                    event["content"] = prompt
                    payload = {
                        "event_type": payload.get("event_type") or payload.get("type") or "message",
                        "message": event,
                    }
                result = await run_agent(runner, payload, settings)
            await send_reply(wechat, payload, result)
        except Exception as exc:
            event = unwrap_event(payload)
            logger.exception(
                "wechatpadpro event handling failed: mode=%s content=%r keys=%s sender=%r room=%r msg_type=%r",
                command[0] if command else None,
                extract_content(event),
                sorted(event.keys()),
                event.get("senderWxid") or event.get("fromUserName") or event.get("fromUsername"),
                event.get("roomId") or event.get("chatroomId") or event.get("fromChatRoom"),
                event.get("msgType") or event.get("MsgType"),
            )
            raise HTTPException(status_code=502, detail=str(exc)) from exc

        logger.warning("wechatpadpro event accepted: mode=%s", command[0])
        return {"status": "accepted"}

    return app
