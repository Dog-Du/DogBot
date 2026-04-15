from wechatpadpro_adapter.command_policy import resolve_command
from wechatpadpro_adapter.config import Settings


def make_settings(**overrides) -> Settings:
    base = Settings.from_env()
    for key, value in overrides.items():
        setattr(base, key, value)
    return base


def test_resolve_command_accepts_prefixed_private_command():
    settings = make_settings()
    match = resolve_command(
        {"message": {"content": "/agent hi", "fromUserName": "wxid_user"}},
        settings,
    )
    assert match is not None
    assert match.mode == "run"
    assert match.prompt == "hi"


def test_resolve_command_requires_group_mention_when_enabled():
    settings = make_settings(
        require_group_mention=True,
        bot_mention_names=("DogDu",),
    )
    no_match = resolve_command(
        {"message": {"content": "/agent hi", "fromUserName": "123@chatroom"}},
        settings,
    )
    yes_match = resolve_command(
        {"message": {"content": "@DogDu /agent hi", "fromUserName": "123@chatroom"}},
        settings,
    )
    assert no_match is None
    assert yes_match is not None
    assert yes_match.prompt == "hi"


def test_resolve_command_accepts_status_command():
    settings = make_settings()
    match = resolve_command(
        {"message": {"content": "/agent-status", "fromUserName": "wxid_user"}},
        settings,
    )
    assert match is not None
    assert match.mode == "status"
