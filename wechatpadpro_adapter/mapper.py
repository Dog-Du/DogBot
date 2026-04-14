from __future__ import annotations

from typing import Any


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
    return str(
        event.get("content")
        or event.get("Content")
        or event.get("text")
        or event.get("TextContent")
        or ""
    ).strip()


def extract_sender(event: dict[str, Any]) -> str:
    return str(
        event.get("senderWxid")
        or event.get("senderWxId")
        or event.get("fromUserName")
        or event.get("fromUsername")
        or event.get("FromUserName")
        or ""
    ).strip()


def extract_group_id(event: dict[str, Any]) -> str:
    return str(
        event.get("roomId")
        or event.get("chatroomId")
        or event.get("chatRoomName")
        or event.get("fromChatRoom")
        or event.get("fromGroup")
        or event.get("toUserName")
        or ""
    ).strip()


def is_group_event(event: dict[str, Any]) -> bool:
    explicit = event.get("isGroup")
    if explicit is not None:
        return bool(explicit)

    group_id = extract_group_id(event)
    return group_id.endswith("@chatroom") or bool(group_id)


def build_run_payload(
    event: dict[str, Any], *, default_cwd: str, timeout_secs: int
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
