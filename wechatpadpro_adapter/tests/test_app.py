from fastapi.testclient import TestClient

from wechatpadpro_adapter.app import create_app


def test_webhook_ignores_empty_payload():
    app = create_app()
    client = TestClient(app)
    response = client.post("/wechatpadpro/events", json={})
    assert response.status_code == 200
    assert response.json()["status"] == "ignored"


def test_webhook_accepts_text_payload(monkeypatch):
    calls = {}

    async def fake_run(*args, **kwargs):
        calls["run"] = True
        return {"stdout": "hello from runner", "stderr": ""}

    async def fake_send(*args, **kwargs):
        calls["send"] = True

    monkeypatch.setattr("wechatpadpro_adapter.app.run_agent", fake_run)
    monkeypatch.setattr("wechatpadpro_adapter.app.send_reply", fake_send)

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
                "isGroup": False,
            },
        },
    )

    assert response.status_code == 200
    assert response.json()["status"] == "accepted"
    assert calls == {"run": True, "send": True}


def test_webhook_ignores_non_message_event():
    app = create_app()
    client = TestClient(app)
    response = client.post(
        "/wechatpadpro/events",
        json={"event_type": "login_status_change", "data": {"state": 1}},
    )
    assert response.status_code == 200
    assert response.json()["status"] == "ignored"
