from wechatpadpro_adapter.events import (
    RecentMessageCache,
    build_message_dedupe_key,
    is_text_event,
)


def test_recent_message_cache_marks_duplicates():
    cache = RecentMessageCache()
    assert cache.check_and_mark("msg-1") is False
    assert cache.check_and_mark("msg-1") is True


def test_is_text_event_accepts_numeric_text_type():
    payload = {"message": {"msgType": 1, "content": "hello"}}
    assert is_text_event(payload) is True


def test_build_message_dedupe_key_prefers_msg_id():
    payload = {"message": {"msgId": "abc", "content": "hello"}}
    assert build_message_dedupe_key(payload) == "abc"
