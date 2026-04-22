from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .config import Settings
from .mapper import extract_content, is_group_event, unwrap_event


@dataclass(frozen=True, slots=True)
class CommandMatch:
    mode: str
    prompt: str


def normalize_command_content(content: str) -> str:
    return content.strip()


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
    is_group = is_group_event(event)
    if is_group and settings.require_group_mention:
        content, mentioned = strip_group_mention_prefix(content, settings.bot_mention_names)
        if not mentioned:
            return None
    content = normalize_command_content(content)

    status_command = f"/{settings.status_command_name}".strip()
    if content == status_command:
        return CommandMatch(mode="status", prompt="")

    if is_group:
        return CommandMatch(mode="run", prompt=content)

    if not content:
        return None
    return CommandMatch(mode="run", prompt=content)
