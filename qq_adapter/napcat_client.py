from __future__ import annotations

import httpx


class NapCatClient:
    def __init__(self, base_url: str, access_token: str | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self.access_token = access_token

    def _headers(self) -> dict[str, str]:
        if not self.access_token:
            return {}
        return {"Authorization": f"Bearer {self.access_token}"}

    async def send_private_msg(self, user_id: str, message: str) -> httpx.Response:
        async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
            response = await client.post(
                "/send_private_msg",
                headers=self._headers(),
                json={"user_id": int(user_id), "message": message},
            )
        response.raise_for_status()
        return response

    async def send_group_msg(self, group_id: str, user_id: str, message: str) -> httpx.Response:
        full_message = f"[CQ:at,qq={user_id}] {message}"
        async with httpx.AsyncClient(base_url=self.base_url, timeout=10) as client:
            response = await client.post(
                "/send_group_msg",
                headers=self._headers(),
                json={"group_id": int(group_id), "message": full_message},
            )
        response.raise_for_status()
        return response
