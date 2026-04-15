from __future__ import annotations

import json

from fastapi import FastAPI, HTTPException, Request

from .config import Settings
from .processor import EventProcessor


def create_app() -> FastAPI:
    app = FastAPI()
    settings = Settings.from_env()
    processor = EventProcessor(settings)

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

        signature = str(
            payload.get("signature") or request.headers.get("X-Signature") or ""
        ).strip()
        return await processor.handle_payload(payload, raw_body, signature)

    return app
