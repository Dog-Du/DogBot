from fastapi.testclient import TestClient

from wechatpadpro_adapter.app import create_app


def test_webhook_ignores_empty_payload():
    app = create_app()
    client = TestClient(app)
    response = client.post("/wechatpadpro/events", json={})
    assert response.status_code == 200
    assert response.json()["status"] == "ignored"


def test_webhook_probe_supports_head_and_get():
    app = create_app()
    client = TestClient(app)

    head_response = client.head("/wechatpadpro/events")
    assert head_response.status_code == 200

    get_response = client.get("/wechatpadpro/events")
    assert get_response.status_code == 200
    assert get_response.json()["status"] == "ok"


def test_webhook_accepts_agent_command_payload(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "/agent hi",
                "fromUserName": "wxid_user",
                "isGroup": False,
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "accepted"
    assert calls == {"inbound": "/agent hi", "run": True, "send": True}


def test_webhook_ignores_text_without_agent_command_by_default(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "ignored"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "hi",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "ignored"
    assert calls == {"inbound": "hi"}


def test_webhook_ignores_plain_text_when_prefix_not_required_because_command_is_still_mandatory(
    monkeypatch,
):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setenv("WECHATPADPRO_REQUIRE_COMMAND_PREFIX", "0")
    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "hi",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "ignored"
    assert calls == {"inbound": "hi"}


def test_webhook_strips_agent_prefix_before_runner(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["prompt"] = payload["message"]["content"]
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setenv("WECHATPADPRO_REQUIRE_COMMAND_PREFIX", "1")
    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "/agent hi",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "accepted"
    assert calls == {"inbound": "/agent hi", "prompt": "hi", "send": True}


def test_webhook_accepts_group_mention_before_agent_command(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["prompt"] = payload["message"]["content"]
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setenv("WECHATPADPRO_REQUIRE_COMMAND_PREFIX", "1")
    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "@DogDu /agent hi",
                "fromUserName": "123@chatroom",
                "senderWxid": "wxid_user",
                "msgId": "group-mention-1",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "accepted"
    assert calls == {"inbound": "/agent hi", "prompt": "hi", "send": True}


def test_webhook_ignores_group_agent_command_without_mention(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "ignored"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setenv("WECHATPADPRO_REQUIRE_COMMAND_PREFIX", "1")
    monkeypatch.setenv("WECHATPADPRO_REQUIRE_MENTION_IN_GROUP", "1")
    monkeypatch.setenv("WECHATPADPRO_BOT_MENTION_NAMES", "DogDu,%&*#")
    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "/agent hi",
                "fromUserName": "123@chatroom",
                "senderWxid": "wxid_user",
                "msgId": "group-no-mention-1",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "ignored"
    assert calls == {"inbound": "/agent hi"}


def test_webhook_status_command_replies_without_runner_execution(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = payload["normalized_text"]
        return {"status": "accepted"}

    async def fake_healthz(self):
        calls["healthz"] = True
        return {"status": "ok"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "should not run", "stderr": ""}

    async def fake_send(self, event, result):
        calls["reply"] = result["stdout"]

    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.runner_client.AgentRunnerClient.healthz", fake_healthz)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "/agent-status",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "accepted"
    assert calls == {"inbound": "/agent-status", "healthz": True, "reply": "agent-runner ok"}


def test_webhook_rejects_invalid_signature(monkeypatch):
    monkeypatch.setenv("WECHATPADPRO_WEBHOOK_SECRET", "top-secret")
    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        headers={"X-Signature": "invalid"},
        json={
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "/agent hi",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 401


def test_webhook_ignores_non_message_event():
    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={"event_type": "login_status_change", "data": {"state": 1}},
    )
    assert response.status_code == 200
    assert response.json()["status"] == "ignored"


def test_webhook_ignores_control_message_type_51(monkeypatch):
    calls = {}

    async def fake_inbound(self, payload):
        calls["inbound"] = True
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] = True

    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={
            "event_type": "message",
            "message": {
                "msgType": 51,
                "content": "<msg><op id='1'><name>lastMessage</name></op></msg>",
                "fromUserName": "wxid_user",
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "ignored"
    assert calls == {}


def test_webhook_deduplicates_same_message_id(monkeypatch):
    calls = {"inbound": 0, "run": 0, "send": 0}

    async def fake_inbound(self, payload):
        calls["inbound"] += 1
        return {"status": "accepted"}

    async def fake_run(self, payload):
        calls["run"] += 1
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(self, event, result):
        calls["send"] += 1

    monkeypatch.setattr(
        "wechatpadpro_adapter.runner_client.AgentRunnerClient.send_inbound_message",
        fake_inbound,
    )
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.processor.EventProcessor.send_reply", fake_send)

    app = create_app()
    client = TestClient(app)
    payload = {
        "event_type": "message",
        "message": {
            "msgType": 1,
            "msgId": "dup-1",
            "content": "/agent hello",
            "fromUserName": "wxid_user",
        },
    }

    response1 = client.post("/wechatpadpro/events", json=payload)
    response2 = client.post("/wechatpadpro/events", json=payload)

    assert response1.status_code == 200
    assert response1.json()["status"] == "accepted"
    assert response2.status_code == 200
    assert response2.json()["status"] == "ignored"
    assert calls == {"inbound": 1, "run": 1, "send": 1}
