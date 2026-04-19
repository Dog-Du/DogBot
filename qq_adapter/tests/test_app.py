from fastapi.testclient import TestClient

from qq_adapter.app import create_app


def test_healthz():
    app = create_app()
    client = TestClient(app)
    response = client.get("/healthz")
    assert response.status_code == 200
    assert response.json() == {"status": "ok"}


def test_private_plain_text_is_ignored(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload
        return {"status": "ignored"}

    async def fake_run(self, payload, timeout_secs):
        calls["run"] = True
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
    assert "run" not in calls
    assert "private" not in calls


def test_private_agent_command_runs_and_replies(monkeypatch):
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
    assert calls["payload"]["prompt"] == "hello"
    assert calls["private"] == ("1", "pong")


def test_group_requires_at_and_agent(monkeypatch):
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
                "raw_message": "[CQ:at,qq=123] /agent hello",
                "group_id": 2,
                "user_id": 1,
            }
        )

    assert calls["inbound"]["is_group"] is True
    assert calls["payload"]["prompt"] == "hello"
    assert calls["group"] == ("2", "1", "pong")


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
                "raw_message": "/agent hello",
                "group_id": 2,
                "user_id": 1,
            }
        )

    assert calls["inbound"]["normalized_text"] == "/agent hello"
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
