from __future__ import annotations

from typing import Any

from .mapper import build_inbound_payload


def _normalize_group_history_event(event: dict[str, Any], group_id: str) -> dict[str, Any]:
    normalized = dict(event)
    normalized.setdefault("post_type", "message")
    normalized["message_type"] = "group"
    normalized["group_id"] = normalized.get("group_id") or group_id
    return normalized


async def sync_group_history(
    napcat,
    runner,
    *,
    group_id: str,
    platform_account_id: str,
    current_message_id: str | None = None,
    count: int = 50,
) -> None:
    for event in await napcat.get_group_msg_history(group_id, count=count):
        normalized = _normalize_group_history_event(event, group_id)
        message_id = str(normalized.get("message_id") or "").strip()
        if current_message_id and message_id and message_id == current_message_id:
            continue

        if not str(normalized.get("user_id") or "").strip():
            continue
        if not str(normalized.get("raw_message") or "").strip() and not normalized.get("message"):
            continue

        payload = build_inbound_payload(
            normalized,
            platform_account_id=platform_account_id,
        )
        await runner.send_inbound_message(payload)
