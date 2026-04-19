from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .config import Settings
from .mapper import extract_content, unwrap_event


@dataclass(frozen=True, slots=True)
class CommandMatch:
    mode: str
    prompt: str


def normalize_command_content(content: str) -> str:
    normalized = content.strip()
    if not normalized:
        return normalized

    parts = normalized.split(maxsplit=1)
    if len(parts) == 2 and parts[0].startswith("@"):
        return parts[1].strip()

    return normalized


def strip_group_mention_prefix(
    content: str, mention_names: tuple[str, ...]
) -> tuple[str, bool]:
    normalized = content.strip()
    if not normalized:
        return ("", False)

    for name in mention_names:
        prefix = f"@{name}"
        if normalized.startswith(prefix):
            return (normalized[len(prefix) :].strip(), True)

    return (normalized, False)


def resolve_command(payload: dict[str, Any], settings: Settings) -> CommandMatch | None:
    event = unwrap_event(payload)
    content = extract_content(event).strip()
    command_prefix = f"/{settings.command_name}".strip()
    is_group = str(event.get("fromUserName") or event.get("FromUserName") or "").strip().endswith(
        "@chatroom"
    )
    if is_group and settings.require_group_mention:
        content, mentioned = strip_group_mention_prefix(content, settings.bot_mention_names)
        if not mentioned:
            return None
    content = normalize_command_content(content)
    if not content:
        return None

    status_command = f"/{settings.status_command_name}".strip()
    if content == status_command:
        return CommandMatch(mode="status", prompt="")

    if not settings.require_command_prefix:
        if content == command_prefix:
            return CommandMatch(mode="run", prompt="")
        if content.startswith(f"{command_prefix} "):
            return CommandMatch(mode="run", prompt=content[len(command_prefix) :].strip())
        return None

    if content == command_prefix:
        return CommandMatch(mode="run", prompt="")
    if content.startswith(f"{command_prefix} "):
        return CommandMatch(mode="run", prompt=content[len(command_prefix) :].strip())

    return None
