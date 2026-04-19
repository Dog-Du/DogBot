from qq_adapter.mapper import build_run_payload, classify_message


def test_private_requires_agent_prefix():
    event = {"message_type": "private", "raw_message": "/agent hello", "user_id": 1}
    result = classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123")
    assert result is not None
    assert result["mode"] == "run"
    assert result["prompt"] == "hello"


def test_private_plain_text_is_ignored():
    event = {"message_type": "private", "raw_message": "hello", "user_id": 1}
    assert classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123") is None


def test_private_plain_agent_text_is_ignored():
    event = {"message_type": "private", "raw_message": "agent hello", "user_id": 1}
    assert classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123") is None


def test_group_requires_at_and_agent_prefix():
    event = {
        "message_type": "group",
        "raw_message": "[CQ:at,qq=123] /agent hello",
        "group_id": 2,
        "user_id": 1,
    }
    result = classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123")
    assert result is not None
    assert result["mode"] == "run"
    assert result["prompt"] == "hello"


def test_group_plain_agent_without_at_is_ignored():
    event = {
        "message_type": "group",
        "raw_message": "/agent hello",
        "group_id": 2,
        "user_id": 1,
    }
    assert classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123") is None


def test_group_at_without_agent_is_ignored():
    event = {
        "message_type": "group",
        "raw_message": "[CQ:at,qq=123] hello",
        "group_id": 2,
        "user_id": 1,
    }
    assert classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123") is None


def test_private_status_command_is_supported():
    event = {"message_type": "private", "raw_message": "/agent-status", "user_id": 1}
    result = classify_message(event, command_name="agent", status_command_name="agent-status", bot_id="123")
    assert result is not None
    assert result["mode"] == "status"


def test_build_run_payload_for_group():
    event = {
        "message_type": "group",
        "raw_message": "[CQ:at,qq=123] /agent hello",
        "group_id": 2,
        "user_id": 1,
        "message_id": 99,
    }
    payload = build_run_payload(
        event,
        platform_account_id="qq:bot_uin:123",
        prompt="hello",
        default_cwd="/workspace",
        timeout_secs=120,
    )
    assert payload["platform"] == "qq"
    assert payload["conversation_id"] == "qq:group:2"
    assert payload["session_id"] == "qq:group:2:user:1"
    assert payload["reply_to_message_id"] == "99"
    assert payload["mention_user_id"] == "1"
    assert payload["platform_account_id"] == "qq:bot_uin:123"


def test_build_run_payload_for_private():
    event = {"message_type": "private", "raw_message": "/agent hello", "user_id": 1}
    payload = build_run_payload(
        event,
        platform_account_id="qq:bot_uin:123",
        prompt="hello",
        default_cwd="/workspace",
        timeout_secs=120,
    )
    assert payload["conversation_id"] == "qq:private:1"
    assert payload["session_id"] == "qq:private:1"
    assert payload["chat_type"] == "private"
