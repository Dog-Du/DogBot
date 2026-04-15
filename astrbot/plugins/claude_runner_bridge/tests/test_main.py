from __future__ import annotations

import importlib.util
import sys
import types
from pathlib import Path

import pytest


def load_plugin_module():
    module_name = "claude_runner_bridge_under_test"
    sys.modules.pop(module_name, None)

    astrbot = types.ModuleType("astrbot")
    api = types.ModuleType("astrbot.api")
    event = types.ModuleType("astrbot.api.event")
    star = types.ModuleType("astrbot.api.star")
    message_components = types.ModuleType("astrbot.api.message_components")
    httpx = types.ModuleType("httpx")

    class DummyLogger:
        def info(self, *args, **kwargs):
            return None

        def exception(self, *args, **kwargs):
            return None

    class DummyConfig(dict):
        def get(self, key, default=None):
            return super().get(key, default)

    class DummyContext:
        pass

    class DummyStar:
        def __init__(self, context):
            self.context = context

    class DummyFilter:
        class EventMessageType:
            ALL = "all"

        @staticmethod
        def command(_name):
            def decorator(func):
                return func

            return decorator

        @staticmethod
        def event_message_type(_kind):
            def decorator(func):
                return func

            return decorator

    def register(*_args, **_kwargs):
        def decorator(cls):
            return cls

        return decorator

    class At:
        def __init__(self, qq=None):
            self.qq = qq

    class Plain:
        def __init__(self, text):
            self.text = text

    event.AstrMessageEvent = object
    event.MessageEventResult = object
    event.filter = DummyFilter
    star.Context = DummyContext
    star.Star = DummyStar
    star.register = register
    api.AstrBotConfig = DummyConfig
    api.logger = DummyLogger()
    message_components.At = At
    message_components.Plain = Plain

    sys.modules["astrbot"] = astrbot
    sys.modules["astrbot.api"] = api
    sys.modules["astrbot.api.event"] = event
    sys.modules["astrbot.api.star"] = star
    sys.modules["astrbot.api.message_components"] = message_components
    sys.modules["httpx"] = httpx

    path = (
        Path(__file__).resolve().parent.parent / "main.py"
    )
    spec = importlib.util.spec_from_file_location(module_name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec is not None and spec.loader is not None
    spec.loader.exec_module(module)
    return module


class DummyMessage:
    def __init__(
        self, *, group_id=None, session_id=None, message_id=None, message=None
    ):
        self.group_id = group_id
        self.session_id = session_id
        self.message_id = message_id
        self.message = message or []


class DummyEvent:
    def __init__(
        self,
        *,
        message,
        sender_id="123",
        platform="qq",
        unified_msg_origin="qq:FriendMessage:abc",
        group_id=None,
        session_id=None,
        message_id="42",
        components=None,
    ):
        self.message_str = message
        self._sender_id = sender_id
        self._platform = platform
        self.unified_msg_origin = unified_msg_origin
        self.message_obj = DummyMessage(
            group_id=group_id,
            session_id=session_id,
            message_id=message_id,
            message=components,
        )

    def get_sender_id(self):
        return self._sender_id

    def get_platform_name(self):
        return self._platform

    def get_messages(self):
        return self.message_obj.message

    def plain_result(self, text):
        return ("plain", text)

    def chain_result(self, chain):
        return ("chain", chain)

    def set_result(self, result):
        self.result = result

    def stop_event(self):
        self.stopped = True


@pytest.fixture
def plugin():
    module = load_plugin_module()
    return module.ClaudeRunnerBridge(module.Context(), module.AstrBotConfig())


@pytest.fixture
def qq_plugin():
    module = load_plugin_module()
    plugin = module.ClaudeRunnerBridge(module.Context(), module.AstrBotConfig())
    plugin.qq_bot_id = "3472283357"
    return plugin


def test_should_not_route_private_messages_without_agent_command(plugin):
    event = DummyEvent(message="你好")

    assert not plugin._should_route_to_agent(event, "你好")
    assert plugin._should_route_to_agent(event, "/agent 你好")


def test_should_not_route_private_messages_with_plain_agent_text(plugin):
    plain = type("Plain", (), {"text": "agent hello"})()
    event = DummyEvent(message="agent hello", components=[plain])

    assert not plugin._should_route_to_agent(event, "agent hello")


def test_should_route_private_messages_when_raw_chain_keeps_slash(plugin):
    plain = type("Plain", (), {"text": "/agent hello"})()
    event = DummyEvent(message="agent hello", components=[plain])

    assert plugin._should_route_to_agent(event, "agent hello")


def test_should_not_route_special_slash_commands(plugin):
    event = DummyEvent(message="/help")

    assert not plugin._should_route_to_agent(event, "/agent-status")
    assert not plugin._should_route_to_agent(event, "/help")


def test_should_not_route_plain_qq_group_messages_without_at(qq_plugin):
    event = DummyEvent(
        message="大家好",
        platform="DogBot",
        unified_msg_origin="DogBot:GroupMessage:965104090",
        group_id="965104090",
    )

    assert not qq_plugin._should_route_to_agent(event, "大家好")


def test_should_not_route_qq_group_messages_with_at_only(qq_plugin):
    event = DummyEvent(
        message="[At:3472283357] hello",
        platform="DogBot",
        unified_msg_origin="DogBot:GroupMessage:965104090",
        group_id="965104090",
    )

    assert not qq_plugin._should_route_to_agent(event, "[At:3472283357] hello")


def test_should_route_qq_group_messages_with_agent_command(qq_plugin):
    event = DummyEvent(
        message="/agent hello",
        platform="DogBot",
        unified_msg_origin="DogBot:GroupMessage:965104090",
        group_id="965104090",
    )

    assert not qq_plugin._should_route_to_agent(event, "/agent hello")


def test_should_route_qq_group_messages_with_at_and_agent_command(qq_plugin):
    at = type("At", (), {"qq": "3472283357"})()
    plain = type("Plain", (), {"text": " /agent hello"})()
    event = DummyEvent(
        message="agent hello",
        platform="DogBot",
        unified_msg_origin="DogBot:GroupMessage:965104090",
        group_id="965104090",
        components=[at, plain],
    )

    assert qq_plugin._should_route_to_agent(event, "agent hello")


def test_success_result_mentions_group_sender_for_aiocqhttp_origin(plugin):
    event = DummyEvent(
        message="你好",
        sender_id="u-9",
        platform="DogBot",
        unified_msg_origin="DogBot:GroupMessage:g-1",
        group_id="g-1",
    )

    kind, chain = plugin._success_result(event, "已收到")

    assert kind == "chain"
    assert chain[0].qq == "u-9"
    assert chain[1].text == "已收到"


def test_build_payload_uses_unified_origin_for_private_sessions(plugin):
    event = DummyEvent(
        message="你好",
        sender_id="u-1",
        unified_msg_origin="qq:FriendMessage:session-A",
        session_id="session-A",
    )

    payload = plugin._build_payload(event, "你好")

    assert payload["conversation_id"] == "qq:FriendMessage:session-A"
    assert payload["session_id"] == "qq:FriendMessage:session-A"
    assert payload["chat_type"] == "private"


def test_build_payload_scopes_group_sessions_by_sender(plugin):
    event = DummyEvent(
        message="你好",
        sender_id="u-9",
        unified_msg_origin="qq:GroupMessage:g-1",
        group_id="g-1",
        session_id="g-1",
    )

    payload = plugin._build_payload(event, "你好")

    assert payload["conversation_id"] == "qq:GroupMessage:g-1"
    assert payload["session_id"] == "qq:GroupMessage:g-1:user:u-9"
    assert payload["chat_type"] == "group"


def test_success_result_mentions_group_sender(plugin):
    event = DummyEvent(
        message="你好",
        sender_id="u-9",
        unified_msg_origin="qq:GroupMessage:g-1",
        group_id="g-1",
    )

    kind, chain = plugin._success_result(event, "已收到")

    assert kind == "chain"
    assert chain[0].qq == "u-9"
    assert chain[1].text == "已收到"
