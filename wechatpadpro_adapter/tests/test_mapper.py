import json

from wechatpadpro_adapter.mapper import (
    build_inbound_payload,
    build_run_payload,
    extract_content,
    extract_sender,
    unwrap_event,
)


def test_private_text_event_maps_to_runner_payload():
    event = {
        "msgType": "Text",
        "content": "hello",
        "fromUserName": "wxid_user",
        "toUserName": "wxid_bot",
        "isGroup": False,
        "msgId": "m-1",
    }

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=120,
    )

    assert payload["platform"] == "wechatpadpro"
    assert payload["platform_account_id"] == "wechatpadpro:account:wxid_bot_1"
    assert payload["conversation_id"] == "wechatpadpro:private:wxid_user"
    assert payload["session_id"] == "wechatpadpro:private:wxid_user"
    assert payload["user_id"] == "wxid_user"
    assert payload["chat_type"] == "private"
    assert payload["prompt"] == "hello"
    assert payload["reply_to_message_id"] == "m-1"


def test_private_text_without_is_group_stays_private():
    event = {
        "msgType": 1,
        "content": "hello",
        "fromUserName": "wxid_user",
        "toUserName": "wxid_bot",
    }

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=120,
    )

    assert payload["conversation_id"] == "wechatpadpro:private:wxid_user"
    assert payload["session_id"] == "wechatpadpro:private:wxid_user"
    assert payload["chat_type"] == "private"


def test_group_text_event_maps_to_runner_payload():
    event = {
        "msgType": "Text",
        "content": "hi group",
        "fromUserName": "wxid_user",
        "roomId": "room-123",
        "isGroup": True,
    }

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=90,
    )

    assert payload["conversation_id"] == "wechatpadpro:group:room-123"
    assert payload["session_id"] == "wechatpadpro:group:room-123:user:wxid_user"
    assert payload["chat_type"] == "group"
    assert payload["timeout_secs"] == 90


def test_group_message_uses_chatroom_sender_and_room_id():
    event = {
        "msgType": 1,
        "content": "hi group",
        "fromUserName": "123@chatroom",
        "senderWxid": "wxid_user",
    }

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=90,
    )

    assert payload["conversation_id"] == "wechatpadpro:group:123@chatroom"
    assert payload["session_id"] == "wechatpadpro:group:123@chatroom:user:wxid_user"
    assert payload["user_id"] == "wxid_user"
    assert payload["chat_type"] == "group"


def test_group_message_sent_by_bot_uses_chatroom_target_as_group_id():
    event = {
        "msgType": 1,
        "content": "/agent hello",
        "fromUserName": "wxid_bot",
        "toUserName": "123@chatroom",
    }

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=90,
    )

    assert payload["conversation_id"] == "wechatpadpro:group:123@chatroom"
    assert payload["session_id"] == "wechatpadpro:group:123@chatroom:user:wxid_bot"
    assert payload["user_id"] == "wxid_bot"
    assert payload["chat_type"] == "group"


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
        platform_account_id="wechatpadpro:account:wxid_bot_1",
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


def test_group_transport_prefix_is_normalized_from_content():
    event = {
        "msgType": 1,
        "content": "wxid_user:\n/agent hello",
        "fromUserName": "123@chatroom",
    }

    assert extract_content(event) == "/agent hello"
    assert extract_sender(event) == "wxid_user"

    payload = build_run_payload(
        event,
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        default_cwd="/workspace",
        timeout_secs=60,
    )
    assert payload["conversation_id"] == "wechatpadpro:group:123@chatroom"
    assert payload["session_id"] == "wechatpadpro:group:123@chatroom:user:wxid_user"
    assert payload["prompt"] == "/agent hello"


def test_build_inbound_payload_emits_canonical_group_fields():
    payload = build_inbound_payload(
        {
            "event_type": "message",
            "message": {
                "msgType": 1,
                "content": "wxid_user:\n@DogDu /agent hello",
                "fromUserName": "123@chatroom",
                "senderWxid": "wxid_user",
                "msgId": "g-1",
                "quoteMsgId": "bot-99",
                "createTime": 1_700_000_123,
            },
        },
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        mention_names=("DogDu",),
    )

    assert payload["platform"] == "wechatpadpro"
    assert payload["platform_account"] == "wechatpadpro:account:wxid_bot_1"
    assert payload["conversation_id"] == "wechatpadpro:group:123@chatroom"
    assert payload["actor_id"] == "wechatpadpro:user:wxid_user"
    assert payload["message_id"] == "g-1"
    assert payload["reply_to_message_id"] == "bot-99"
    assert payload["normalized_text"] == "/agent hello"
    assert payload["mentions"] == ["wechatpadpro:account:wxid_bot_1"]
    assert payload["is_group"] is True
    assert payload["is_private"] is False
    assert payload["timestamp_epoch_secs"] == 1_700_000_123
    assert json.loads(payload["raw_segments_json"]) == [
        {"type": "text", "text": "/agent hello"}
    ]


def test_build_inbound_payload_emits_canonical_private_fields():
    payload = build_inbound_payload(
        {
            "msgType": 1,
            "content": "hello",
            "fromUserName": "wxid_user",
            "msgId": "p-1",
        },
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        mention_names=(),
    )

    assert payload["conversation_id"] == "wechatpadpro:private:wxid_user"
    assert payload["actor_id"] == "wechatpadpro:user:wxid_user"
    assert payload["message_id"] == "p-1"
    assert payload["reply_to_message_id"] is None
    assert payload["normalized_text"] == "hello"
    assert payload["mentions"] == []
    assert payload["is_group"] is False
    assert payload["is_private"] is True


def test_build_inbound_payload_preserves_plain_at_text_without_mention_names():
    payload = build_inbound_payload(
        {
            "msgType": 1,
            "content": "@Alice 你好",
            "fromUserName": "wxid_user",
            "msgId": "p-2",
        },
        platform_account_id="wechatpadpro:account:wxid_bot_1",
        mention_names=(),
    )

    assert payload["normalized_text"] == "@Alice 你好"
    assert payload["mentions"] == []
