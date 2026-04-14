from wechatpadpro_adapter.wechat_client import build_text_reply


def test_group_reply_prefixes_sender_name():
    event = {
        "isGroup": True,
        "senderNickName": "张三",
        "roomId": "room-1",
        "fromUserName": "wxid_user",
    }
    payload = build_text_reply(event, "done")
    assert payload["MsgItem"][0]["TextContent"].startswith("@张三 ")
    assert payload["MsgItem"][0]["ToUserName"] == "room-1"
    assert payload["MsgItem"][0]["AtWxIDList"] == ["wxid_user"]


def test_private_reply_keeps_plain_text():
    event = {"isGroup": False, "fromUserName": "wxid_user"}
    payload = build_text_reply(event, "done")
    assert payload["MsgItem"][0]["TextContent"] == "done"
    assert payload["MsgItem"][0]["ToUserName"] == "wxid_user"
