from __future__ import annotations

from typing import Any

import httpx

from .mapper import extract_group_id, extract_sender, is_group_event, unwrap_event


def build_text_reply(event: dict[str, Any], text: str) -> dict[str, Any]:
    event = unwrap_event(event)
    is_group = is_group_event(event)
    target = extract_group_id(event) if is_group else extract_sender(event)
    sender_id = extract_sender(event)
    sender_name = str(
        event.get("senderNickName")
        or event.get("senderName")
        or event.get("fromNickname")
        or event.get("fromUserNickName")
        or ""
    ).strip()
    at_wxids: list[str] = []
    if is_group and sender_name:
        text = f"@{sender_name} {text}"
    if is_group and sender_id:
        at_wxids.append(sender_id)
    msg_item: dict[str, Any] = {
        "MsgType": 1,
        "ToUserName": target,
        "TextContent": text,
    }
    if at_wxids:
        msg_item["AtWxIDList"] = at_wxids
    return {"MsgItem": [msg_item]}


class WeChatPadProClient:
    def __init__(self, base_url: str, account_key: str | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self.account_key = account_key

    async def send_text(self, event: dict[str, Any], text: str) -> httpx.Response:
        if not self.account_key:
            raise RuntimeError("WECHATPADPRO_ACCOUNT_KEY is not configured")

        payload = build_text_reply(event, text)
        async with httpx.AsyncClient(base_url=self.base_url, timeout=15) as client:
            return await client.post(
                "/message/SendTextMessage",
                params={"key": self.account_key},
                json=payload,
            )

    async def configure_webhook(
        self,
        callback_url: str,
        *,
        include_self_message: bool,
        secret: str | None,
        message_types: list[str] | None = None,
    ) -> httpx.Response:
        if not self.account_key:
            raise RuntimeError("WECHATPADPRO_ACCOUNT_KEY is not configured")

        payload: dict[str, Any] = {
            "URL": callback_url,
            "Enabled": True,
            "IncludeSelfMessage": include_self_message,
            "MessageTypes": message_types or ["Text"],
            "RetryCount": 3,
            "Timeout": 5,
            "Secret": secret or "",
        }
        async with httpx.AsyncClient(base_url=self.base_url, timeout=15) as client:
            return await client.post(
                "/webhook/Config",
                params={"key": self.account_key},
                json=payload,
            )
