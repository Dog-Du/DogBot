from __future__ import annotations

from typing import Any


def strip_qq_at_prefix(raw_message: str, bot_id: str) -> tuple[str, bool]:
    text = raw_message.strip()
    prefix = f"[CQ:at,qq={bot_id}]"
    if bot_id and text.startswith(prefix):
        return text[len(prefix):].strip(), True
    return text, False


def classify_message(
    event: dict[str, Any],
    *,
    command_name: str,
    status_command_name: str,
    bot_id: str,
) -> dict[str, str] | None:
    raw_message = str(event.get("raw_message") or "").strip()
    if not raw_message:
        return None

    if str(event.get("message_type") or "") == "group":
        normalized, mentioned = strip_qq_at_prefix(raw_message, bot_id)
        if not mentioned:
            return None
    else:
        normalized = raw_message

    if normalized == f"/{status_command_name}":
        return {"mode": "status", "prompt": ""}
    if normalized == f"/{command_name}":
        return {"mode": "run", "prompt": ""}
    if normalized.startswith(f"/{command_name} "):
        return {"mode": "run", "prompt": normalized[len(command_name) + 2 :].strip()}
    return None


def build_run_payload(
    event: dict[str, Any],
    *,
    platform_account_id: str,
    prompt: str,
    default_cwd: str,
    timeout_secs: int,
) -> dict[str, Any]:
    user_id = str(event["user_id"])
    message_type = str(event.get("message_type") or "")
    payload = {
        "platform": "qq",
        "platform_account_id": platform_account_id,
        "user_id": user_id,
        "cwd": default_cwd,
        "prompt": prompt,
        "timeout_secs": timeout_secs,
    }
    message_id = str(event.get("message_id") or "").strip()

    if message_type == "group":
        group_id = str(event["group_id"])
        conversation_id = f"qq:group:{group_id}"
        payload.update(
            {
                "conversation_id": conversation_id,
                "session_id": f"{conversation_id}:user:{user_id}",
                "chat_type": "group",
                "mention_user_id": user_id,
            }
        )
    else:
        conversation_id = f"qq:private:{user_id}"
        payload.update(
            {
                "conversation_id": conversation_id,
                "session_id": conversation_id,
                "chat_type": "private",
            }
        )

    if message_id:
        payload["reply_to_message_id"] = message_id
    return payload
