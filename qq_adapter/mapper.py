from __future__ import annotations

import json
import re
import time
from typing import Any

_CQ_REPLY_PREFIX_RE = re.compile(r"^(?:\s*\[CQ:reply,[^\]]+\]\s*)*")
_CQ_AT_PREFIX_TEMPLATE = r"^\[CQ:at,qq={bot_id}(?:,[^\]]*)?\]\s*"


def strip_qq_at_prefix(raw_message: str, bot_id: str) -> tuple[str, bool]:
    text = raw_message.strip()
    if bot_id:
        without_reply = _CQ_REPLY_PREFIX_RE.sub("", text)
        at_prefix_re = re.compile(_CQ_AT_PREFIX_TEMPLATE.format(bot_id=re.escape(bot_id)))
        match = at_prefix_re.match(without_reply)
        if match:
            return without_reply[match.end() :].strip(), True
    return text, False


def classify_message(
    event: dict[str, Any],
    *,
    status_command_name: str,
    bot_id: str,
) -> dict[str, str] | None:
    raw_message = str(event.get("raw_message") or "").strip()
    if not raw_message:
        return None

    if str(event.get("message_type") or "") == "group":
        normalized, mentioned = strip_qq_at_prefix(raw_message, bot_id)
        if not mentioned:
            segments = _extract_segments(event)
            bot_mention_id = _normalize_mention_id(bot_id) if bot_id else ""
            mentions = _extract_mentions(raw_message, segments)
            if not bot_mention_id or bot_mention_id not in mentions:
                return None
            normalized = _extract_normalized_text(raw_message, segments)
    else:
        normalized = raw_message

    normalized = normalized.strip()

    if normalized == f"/{status_command_name}":
        return {"mode": "status", "prompt": ""}

    if str(event.get("message_type") or "") == "group":
        if not normalized:
            return {"mode": "run", "prompt": ""}
        return {"mode": "run", "prompt": normalized}

    if normalized:
        return {"mode": "run", "prompt": normalized}
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


_CQ_TOKEN_RE = re.compile(r"\[CQ:[^\]]+\]")
_CQ_AT_RE = re.compile(r"\[CQ:at,qq=(?P<qq>[^\],]+)(?:,[^\]]*)?\]")
_CQ_REPLY_RE = re.compile(r"\[CQ:reply,id=(?P<id>[^\],]+)(?:,[^\]]*)?\]")


def _extract_segments(event: dict[str, Any]) -> list[dict[str, Any]]:
    raw_segments = event.get("message")
    if not isinstance(raw_segments, list):
        return []
    segments: list[dict[str, Any]] = []
    for item in raw_segments:
        if isinstance(item, dict):
            segments.append(item)
    return segments


def _normalize_mention_id(qq: str) -> str:
    qq = qq.strip()
    if not qq:
        return ""
    if qq.startswith("qq:"):
        return qq
    return f"qq:bot_uin:{qq}"


def _extract_mentions(raw_message: str, segments: list[dict[str, Any]]) -> list[str]:
    mentions: list[str] = []
    for segment in segments:
        segment_type = str(segment.get("type") or "").strip().lower()
        if segment_type != "at":
            continue
        data = segment.get("data")
        if not isinstance(data, dict):
            continue
        qq = str(data.get("qq") or "").strip()
        if not qq or qq == "all":
            continue
        mention_id = _normalize_mention_id(qq)
        if mention_id and mention_id not in mentions:
            mentions.append(mention_id)

    for match in _CQ_AT_RE.finditer(raw_message):
        qq = match.group("qq").strip()
        if not qq or qq == "all":
            continue
        mention_id = _normalize_mention_id(qq)
        if mention_id and mention_id not in mentions:
            mentions.append(mention_id)
    return mentions


def _extract_reply_to_message_id(raw_message: str, segments: list[dict[str, Any]]) -> str | None:
    for segment in segments:
        segment_type = str(segment.get("type") or "").strip().lower()
        if segment_type != "reply":
            continue
        data = segment.get("data")
        if not isinstance(data, dict):
            continue
        reply_id = str(data.get("id") or data.get("message_id") or "").strip()
        if reply_id:
            return reply_id

    match = _CQ_REPLY_RE.search(raw_message)
    if match:
        reply_id = match.group("id").strip()
        if reply_id:
            return reply_id
    return None


def _extract_normalized_text(raw_message: str, segments: list[dict[str, Any]]) -> str:
    text_parts: list[str] = []
    for segment in segments:
        segment_type = str(segment.get("type") or "").strip().lower()
        if segment_type != "text":
            continue
        data = segment.get("data")
        if not isinstance(data, dict):
            continue
        text_parts.append(str(data.get("text") or ""))

    if text_parts:
        text = "".join(text_parts)
    elif segments:
        text = ""
    else:
        text = raw_message

    without_cq = _CQ_TOKEN_RE.sub(" ", text)
    return " ".join(without_cq.split()).strip()
def _extract_timestamp_epoch_secs(event: dict[str, Any]) -> int:
    for key in ("time", "timestamp"):
        value = event.get(key)
        if value is None:
            continue
        try:
            return int(value)
        except (TypeError, ValueError):
            continue
    return int(time.time())


def build_inbound_payload(
    event: dict[str, Any],
    *,
    platform_account_id: str,
) -> dict[str, Any]:
    user_id = str(event.get("user_id") or "").strip()
    message_type = str(event.get("message_type") or "").strip().lower()
    message_id = str(event.get("message_id") or "").strip()
    raw_message = str(event.get("raw_message") or "").strip()
    segments = _extract_segments(event)
    is_group = message_type == "group"

    if is_group:
        group_id = str(event.get("group_id") or "").strip()
        conversation_id = f"qq:group:{group_id}"
    else:
        conversation_id = f"qq:private:{user_id}"

    return {
        "platform": "qq",
        "platform_account": platform_account_id,
        "conversation_id": conversation_id,
        "actor_id": f"qq:user:{user_id}",
        "message_id": message_id,
        "reply_to_message_id": _extract_reply_to_message_id(raw_message, segments),
        "raw_segments_json": json.dumps(segments, ensure_ascii=False),
        "normalized_text": _extract_normalized_text(raw_message, segments),
        "mentions": _extract_mentions(raw_message, segments),
        "is_group": is_group,
        "is_private": not is_group,
        "timestamp_epoch_secs": _extract_timestamp_epoch_secs(event),
    }
