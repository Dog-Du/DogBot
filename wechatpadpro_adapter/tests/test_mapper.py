from wechatpadpro_adapter.mapper import build_run_payload, unwrap_event


def test_private_text_event_maps_to_runner_payload():
    event = {
        "msgType": "Text",
        "content": "hello",
        "fromUserName": "wxid_user",
        "toUserName": "wxid_bot",
        "isGroup": False,
        "msgId": "m-1",
    }

    payload = build_run_payload(event, default_cwd="/workspace", timeout_secs=120)

    assert payload["platform"] == "wechatpadpro"
    assert payload["conversation_id"] == "wechatpadpro:private:wxid_user"
    assert payload["session_id"] == "wechatpadpro:private:wxid_user"
    assert payload["user_id"] == "wxid_user"
    assert payload["chat_type"] == "private"
    assert payload["prompt"] == "hello"
    assert payload["reply_to_message_id"] == "m-1"


def test_group_text_event_maps_to_runner_payload():
    event = {
        "msgType": "Text",
        "content": "hi group",
        "fromUserName": "wxid_user",
        "roomId": "room-123",
        "isGroup": True,
    }

    payload = build_run_payload(event, default_cwd="/workspace", timeout_secs=90)

    assert payload["conversation_id"] == "wechatpadpro:group:room-123"
    assert payload["session_id"] == "wechatpadpro:group:room-123:user:wxid_user"
    assert payload["chat_type"] == "group"
    assert payload["timeout_secs"] == 90


def test_wrapped_message_event_unwraps_before_mapping():
    payload = build_run_payload(
        {
            "event_type": "message",
            "message": {
                "content": "wrapped",
                "fromUserName": "wxid_user",
                "roomId": "123@chatroom",
                "msgType": 1,
            },
        },
        default_cwd="/workspace",
        timeout_secs=60,
    )

    assert payload["conversation_id"] == "wechatpadpro:group:123@chatroom"
    assert payload["session_id"] == "wechatpadpro:group:123@chatroom:user:wxid_user"


def test_unwrap_event_prefers_message_payload():
    event = unwrap_event(
        {
            "type": "message_received",
            "uuid": "abc",
            "message": {"content": "hello", "fromUserName": "wxid_user"},
        }
    )
    assert event["content"] == "hello"
    assert event["uuid"] == "abc"
