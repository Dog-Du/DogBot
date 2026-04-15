from __future__ import annotations

import time
from typing import Any

from .mapper import extract_content, unwrap_event


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
