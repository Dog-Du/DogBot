from __future__ import annotations

import re
from typing import Any


TRANSPORT_PREFIX_RE = re.compile(
    r"^\s*(?P<sender>[A-Za-z0-9_@-]+):\s*\n(?P<body>.+)$",
    re.S,
)

def extract_raw_content(event: dict[str, Any]) -> str:
    return str(
        event.get("content")
        or event.get("Content")
        or event.get("text")
        or event.get("TextContent")
        or ""
    ).strip()


def parse_transport_prefixed_content(event: dict[str, Any]) -> tuple[str | None, str]:
    raw = extract_raw_content(event)
    if not raw:
        return (None, "")
    match = TRANSPORT_PREFIX_RE.match(raw)
    if not match:
        return (None, raw)
    return (match.group("sender").strip(), match.group("body").strip())


def unwrap_event(payload: dict[str, Any]) -> dict[str, Any]:
    if isinstance(payload.get("message"), dict):
        event = dict(payload["message"])
        for key in ("event_type", "type", "uuid", "timestamp", "signature"):
            if key in payload and key not in event:
                event[key] = payload[key]
        return event

    if isinstance(payload.get("data"), dict):
        data = payload["data"]
        if isinstance(data.get("message"), dict):
            event = dict(data["message"])
            for key in ("event_type", "type", "uuid", "timestamp", "signature"):
                if key in payload and key not in event:
                    event[key] = payload[key]
            return event
        return data

    return payload


def extract_content(event: dict[str, Any]) -> str:
    _sender, content = parse_transport_prefixed_content(event)
    return content


def extract_sender(event: dict[str, Any]) -> str:
    sender = str(
        event.get("senderWxid")
        or event.get("senderWxId")
        or event.get("senderId")
        or ""
    ).strip()
    if sender:
        return sender

    prefixed_sender, _content = parse_transport_prefixed_content(event)
    if prefixed_sender:
        return prefixed_sender

    from_user = str(
        event.get("fromUserName")
        or event.get("fromUsername")
        or event.get("FromUserName")
        or ""
    ).strip()
    if from_user.endswith("@chatroom"):
        return ""
    return from_user


def extract_group_id(event: dict[str, Any]) -> str:
    group_id = str(
        event.get("roomId")
        or event.get("chatroomId")
        or event.get("chatRoomName")
        or event.get("fromChatRoom")
        or event.get("fromGroup")
        or ""
    ).strip()
    if group_id:
        return group_id

    from_user = str(
        event.get("fromUserName")
        or event.get("fromUsername")
        or event.get("FromUserName")
        or ""
    ).strip()
    if from_user.endswith("@chatroom"):
        return from_user

    to_user = str(
        event.get("toUserName")
        or event.get("toUsername")
        or event.get("ToUserName")
        or ""
    ).strip()
    if to_user.endswith("@chatroom"):
        return to_user

    return ""


def is_group_event(event: dict[str, Any]) -> bool:
    explicit = event.get("isGroup")
    if explicit is not None:
        if isinstance(explicit, str):
            return explicit.strip().lower() in {"1", "true", "yes", "on"}
        return bool(explicit)

    group_id = extract_group_id(event)
    return group_id.endswith("@chatroom") or bool(group_id)


def build_run_payload(
    event: dict[str, Any],
    *,
    platform_account_id: str,
    default_cwd: str,
    timeout_secs: int,
) -> dict[str, Any]:
    event = unwrap_event(event)
    prompt = extract_content(event)
    sender = extract_sender(event)
    msg_id = str(
        event.get("msgId")
        or event.get("MsgId")
        or event.get("newMsgId")
        or ""
    ).strip() or None
    is_group = is_group_event(event)

    if is_group:
        group_id = extract_group_id(event)
        conversation_id = f"wechatpadpro:group:{group_id}"
        session_id = f"{conversation_id}:user:{sender}"
        chat_type = "group"
    else:
        conversation_id = f"wechatpadpro:private:{sender}"
        session_id = conversation_id
        chat_type = "private"

    payload = {
        "platform": "wechatpadpro",
        "platform_account_id": platform_account_id,
        "conversation_id": conversation_id,
        "session_id": session_id,
        "user_id": sender,
        "chat_type": chat_type,
        "cwd": default_cwd,
        "prompt": prompt,
        "timeout_secs": timeout_secs,
    }
    if msg_id:
        payload["reply_to_message_id"] = msg_id
    return payload
