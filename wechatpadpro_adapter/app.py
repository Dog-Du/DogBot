from __future__ import annotations

import hashlib
import hmac
import json
from typing import Any

from fastapi import FastAPI, HTTPException, Request

from .config import Settings
from .mapper import build_run_payload, extract_content, unwrap_event
from .runner_client import AgentRunnerClient
from .wechat_client import WeChatPadProClient


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
        return
    response = await wechat.send_text(event, text)
    response.raise_for_status()


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
    if msg_type in {"1", "text"}:
        return True
    return bool(extract_content(event))


def create_app() -> FastAPI:
    app = FastAPI()
    settings = Settings.from_env()
    runner = AgentRunnerClient(settings.agent_runner_base_url)
    wechat = WeChatPadProClient(
        settings.wechatpadpro_base_url, settings.account_key
    )

    @app.get("/healthz")
    async def healthz() -> dict[str, str]:
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
            return {"status": "ignored"}

        if not is_text_event(payload):
            return {"status": "ignored"}

        try:
            result = await run_agent(runner, payload, settings)
            await send_reply(wechat, payload, result)
        except Exception as exc:
            raise HTTPException(status_code=502, detail=str(exc)) from exc

        return {"status": "accepted"}

    return app
