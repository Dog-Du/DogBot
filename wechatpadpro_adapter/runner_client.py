from __future__ import annotations

from typing import Any

import httpx


class AgentRunnerClient:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    async def healthz(self) -> dict[str, Any]:
        async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
            response = await client.get("/healthz")
            response.raise_for_status()
            return response.json()

    async def run(self, payload: dict[str, Any], timeout_secs: int) -> dict[str, Any]:
        async with httpx.AsyncClient(
            base_url=self.base_url, timeout=timeout_secs + 10
        ) as client:
            response = await client.post("/v1/runs", json=payload)
            response.raise_for_status()
            return response.json()

    async def send_inbound_message(self, payload: dict[str, Any]) -> dict[str, Any]:
        async with httpx.AsyncClient(base_url=self.base_url, timeout=15) as client:
            response = await client.post("/v1/inbound-messages", json=payload)
            response.raise_for_status()
            return response.json()
