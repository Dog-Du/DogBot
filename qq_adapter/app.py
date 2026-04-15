from __future__ import annotations

from fastapi import FastAPI, WebSocket, WebSocketDisconnect

from .config import Settings
from .mapper import build_run_payload, classify_message
from .napcat_client import NapCatClient
from .runner_client import AgentRunnerClient


def create_app() -> FastAPI:
    settings = Settings.from_env()
    runner = AgentRunnerClient(settings.agent_runner_base_url)
    napcat = NapCatClient(settings.napcat_api_base_url, settings.napcat_access_token)

    app = FastAPI()

    @app.get("/healthz")
    async def healthz() -> dict[str, str]:
        return {"status": "ok"}

    @app.websocket("/napcat/ws")
    async def napcat_ws(websocket: WebSocket) -> None:
        await websocket.accept()
        try:
            while True:
                event = await websocket.receive_json()
                if str(event.get("post_type") or "") != "message":
                    continue

                command = classify_message(
                    event,
                    command_name=settings.command_name,
                    status_command_name=settings.status_command_name,
                    bot_id=settings.qq_bot_id,
                )
                if command is None:
                    continue

                if command["mode"] == "status":
                    health = await runner.healthz()
                    text = "agent-runner ok" if health.get("status") == "ok" else "agent-runner unavailable"
                else:
                    payload = build_run_payload(
                        event,
                        prompt=command["prompt"],
                        default_cwd=settings.default_cwd,
                        timeout_secs=settings.default_timeout_secs,
                    )
                    result = await runner.run(payload, settings.default_timeout_secs)
                    text = (str(result.get("stdout") or "").strip() or str(result.get("stderr") or "").strip())

                if not text:
                    continue

                if str(event.get("message_type") or "") == "group":
                    await napcat.send_group_msg(str(event["group_id"]), str(event["user_id"]), text)
                else:
                    await napcat.send_private_msg(str(event["user_id"]), text)
        except WebSocketDisconnect:
            return

    return app
