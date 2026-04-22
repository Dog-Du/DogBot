from fastapi.testclient import TestClient

from qq_adapter.app import create_app


def test_healthz():
    app = create_app()
    client = TestClient(app)
    response = client.get("/healthz")
    assert response.status_code == 200
    assert response.json() == {"status": "ok"}


def test_private_plain_text_runs_and_replies(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = payload
        return {"stdout": "hello", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json({"post_type": "message", "message_type": "private", "raw_message": "hello", "user_id": 1})

    assert calls["inbound"]["normalized_text"] == "hello"
    assert calls["run"]["prompt"] == "hello"
    assert calls["private"] == ("1", "hello")


def test_private_agent_text_runs_without_special_stripping(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["payload"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json({"post_type": "message", "message_type": "private", "raw_message": "/agent hello", "user_id": 1})

    assert calls["inbound"]["normalized_text"] == "/agent hello"
    assert calls["payload"]["prompt"] == "/agent hello"
    assert calls["private"] == ("1", "pong")


def test_private_inbound_accepted_without_legacy_command_match_runs_raw_text(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "private",
                "raw_message": "请帮我 /agent 总结一下",
                "user_id": 1,
            }
        )

    assert calls["inbound"]["normalized_text"] == "请帮我 /agent 总结一下"
    assert calls["run"]["prompt"] == "请帮我 /agent 总结一下"
    assert calls["private"] == ("1", "pong")


def test_group_requires_at_but_not_agent_prefix(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["payload"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_group(self, group_id, user_id, message):
        calls["group"] = (group_id, user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_group_msg", fake_send_group)
    monkeypatch.setenv("QQ_ADAPTER_QQ_BOT_ID", "123")

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "group",
                "raw_message": "[CQ:at,qq=123] hello",
                "group_id": 2,
                "user_id": 1,
            }
        )

    assert calls["inbound"]["is_group"] is True
    assert calls["payload"]["prompt"] == "hello"
    assert calls["group"] == ("2", "1", "pong")


def test_group_enablement_triggers_limited_history_backfill(monkeypatch):
    calls = {"history": 0, "forwarded": 0}

    async def fake_history(self, group_id: str, count: int = 50):
        calls["history"] += 1
        assert group_id == "2"
        assert count == 50
        return [
            {
                "message_id": 1,
                "raw_message": "old text",
                "user_id": 7,
                "group_id": 2,
            }
        ]

    async def fake_inbound(self, payload):
        calls["forwarded"] += 1
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["payload"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_group(self, group_id, user_id, message):
        calls["group"] = (group_id, user_id, message)

    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.get_group_msg_history", fake_history)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_group_msg", fake_send_group)
    monkeypatch.setenv("QQ_ADAPTER_QQ_BOT_ID", "123")

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "group",
                "raw_message": "[CQ:at,qq=123] hello",
                "group_id": 2,
                "user_id": 1,
                "message_id": 99,
            }
        )

    assert calls["payload"]["prompt"] == "hello"
    assert calls["group"] == ("2", "1", "pong")
    assert calls["history"] == 1
    assert calls["forwarded"] == 2


def test_group_segment_mention_runs_even_without_cq_prefix_raw_message(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["payload"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_group(self, group_id, user_id, message):
        calls["group"] = (group_id, user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_group_msg", fake_send_group)
    monkeypatch.setenv("QQ_ADAPTER_QQ_BOT_ID", "123")

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "group",
                "raw_message": "@DogDu hello",
                "group_id": 2,
                "user_id": 1,
                "message": [
                    {"type": "at", "data": {"qq": "123", "name": "DogDu"}},
                    {"type": "text", "data": {"text": " hello"}},
                ],
            }
        )

    assert calls["inbound"]["mentions"] == ["qq:bot_uin:123"]
    assert calls["payload"]["prompt"] == "hello"
    assert calls["group"] == ("2", "1", "pong")


def test_group_inbound_defaults_platform_account_id_from_bot_id(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "ignored"}

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setenv("QQ_ADAPTER_QQ_BOT_ID", "123")
    monkeypatch.delenv("QQ_PLATFORM_ACCOUNT_ID", raising=False)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "group",
                "raw_message": "[CQ:at,qq=123] hello",
                "group_id": 2,
                "user_id": 1,
                "message": [
                    {"type": "at", "data": {"qq": "123"}},
                    {"type": "text", "data": {"text": " hello"}},
                ],
            }
        )

    assert calls["inbound"]["platform_account"] == "qq:bot_uin:123"
    assert calls["inbound"]["mentions"] == ["qq:bot_uin:123"]


def test_group_without_at_is_ignored(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "ignored"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = True
        return {"stdout": "pong", "stderr": ""}

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setenv("QQ_ADAPTER_QQ_BOT_ID", "123")

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "group",
                "raw_message": "hello",
                "group_id": 2,
                "user_id": 1,
            }
        )

    assert calls["inbound"]["normalized_text"] == "hello"
    assert "run" not in calls


def test_private_status_from_inbound_replies_without_run(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "status"}

    async def fake_healthz(self):
        calls["healthz"] = True
        return {"status": "ok"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = True
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.healthz", fake_healthz)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "private",
                "raw_message": "/agent-status",
                "user_id": 1,
            }
        )

    assert calls["inbound"]["normalized_text"] == "/agent-status"
    assert calls["healthz"] is True
    assert "run" not in calls
    assert calls["private"] == ("1", "agent-runner ok")


def test_private_status_from_inbound_decision_replies_without_run(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "accepted", "decision": "status"}

    async def fake_healthz(self):
        calls["healthz"] = True
        return {"status": "ok"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = True
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.healthz", fake_healthz)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "private",
                "raw_message": "/agent-status",
                "user_id": 1,
            }
        )

    assert calls["inbound"]["normalized_text"] == "/agent-status"
    assert calls["healthz"] is True
    assert "run" not in calls
    assert calls["private"] == ("1", "agent-runner ok")


def test_websocket_continues_after_inbound_failure(monkeypatch):
    calls = {"inbound": 0}

    async def fake_inbound(self, payload):
        calls["inbound"] += 1
        if calls["inbound"] == 1:
            raise RuntimeError("transient inbound failure")
        return {"status": "accepted"}

    async def fake_run(self, payload, timeout_secs):
        calls["payload"] = payload
        return {"stdout": "pong", "stderr": ""}

    async def fake_send_private(self, user_id, message):
        calls["private"] = (user_id, message)

    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.send_inbound_message", fake_inbound)
    monkeypatch.setattr("qq_adapter.runner_client.AgentRunnerClient.run", fake_run)
    monkeypatch.setattr("qq_adapter.napcat_client.NapCatClient.send_private_msg", fake_send_private)

    app = create_app()
    client = TestClient(app)
    with client.websocket_connect("/napcat/ws") as websocket:
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "private",
                "raw_message": "first",
                "user_id": 1,
            }
        )
        websocket.send_json(
            {
                "post_type": "message",
                "message_type": "private",
                "raw_message": "second",
                "user_id": 1,
            }
        )

    assert calls["inbound"] == 2
    assert calls["payload"]["prompt"] == "second"
    assert calls["private"] == ("1", "pong")
