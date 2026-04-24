use agent_runner::platforms::wechatpadpro::{compile_text_reply, decode_webhook_event};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn wechat_group_event_maps_leading_mention_to_structured_bot_mention() {
    let payload = serde_json::json!({
        "message": {
            "msgId": "wx-1",
            "roomId": "123@chatroom",
            "senderWxid": "wxid_user_1",
            "senderNickName": "Alice",
            "content": "@DogDu 你好"
        }
    });

    let event = decode_webhook_event(&payload, "wechatpadpro:account:bot", &["DogDu"]).unwrap();
    let message = event.message().unwrap();

    assert_eq!(
        message.mentions,
        vec!["wechatpadpro:account:bot".to_string()]
    );
    assert_eq!(message.project_plain_text(), "你好");
    assert_eq!(event.conversation, "wechatpadpro:group:123@chatroom");
    assert_eq!(event.actor, "wechatpadpro:user:wxid_user_1");
}

#[test]
fn wechat_group_outbound_uses_at_list_and_display_prefix() {
    let payload = serde_json::json!({
        "roomId": "123@chatroom",
        "senderWxid": "wxid_user_1",
        "senderNickName": "Alice"
    });

    let reply = compile_text_reply(&payload, "done");
    assert_eq!(reply["MsgItem"][0]["MsgType"], 1);
    assert_eq!(reply["MsgItem"][0]["ToUserName"], "123@chatroom");
    assert_eq!(reply["MsgItem"][0]["AtWxIDList"][0], "wxid_user_1");
    assert_eq!(reply["MsgItem"][0]["TextContent"], "@Alice done");
}

#[test]
fn wechat_nested_group_event_uses_transport_sender_and_from_user_name_chatroom() {
    let payload = serde_json::json!({
        "data": {
            "message": {
                "msgId": "wx-2",
                "fromUserName": "456@chatroom",
                "content": "wxid_user_2:\n@DogDu 早上好"
            }
        }
    });

    let event = decode_webhook_event(&payload, "wechatpadpro:account:bot", &["DogDu"]).unwrap();
    let message = event.message().unwrap();

    assert_eq!(event.conversation, "wechatpadpro:group:456@chatroom");
    assert_eq!(event.actor, "wechatpadpro:user:wxid_user_2");
    assert_eq!(
        message.mentions,
        vec!["wechatpadpro:account:bot".to_string()]
    );
    assert_eq!(message.project_plain_text(), "早上好");
}

#[test]
fn wechat_group_outbound_falls_back_to_from_user_name_for_mentions() {
    let payload = serde_json::json!({
        "roomId": "789@chatroom",
        "fromUserName": "wxid_user_3",
        "senderNickName": "Bob"
    });

    let reply = compile_text_reply(&payload, "收到");
    assert_eq!(reply["MsgItem"][0]["MsgType"], 1);
    assert_eq!(reply["MsgItem"][0]["ToUserName"], "789@chatroom");
    assert_eq!(reply["MsgItem"][0]["AtWxIDList"][0], "wxid_user_3");
    assert_eq!(reply["MsgItem"][0]["TextContent"], "@Bob 收到");
}

#[test]
fn wechat_non_text_events_are_ignored() {
    let payload = serde_json::json!({
        "message": {
            "msgId": "wx-3",
            "msgType": 49,
            "fromUserName": "wxid_user_4",
            "content": "<msg><title>file</title></msg>"
        }
    });

    assert!(decode_webhook_event(&payload, "wechatpadpro:account:bot", &["DogDu"]).is_none());
}

#[test]
fn wechat_missing_timestamp_falls_back_to_current_time() {
    let payload = serde_json::json!({
        "message": {
            "msgId": "wx-4",
            "fromUserName": "wxid_user_5",
            "content": "你好"
        }
    });
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let event = decode_webhook_event(&payload, "wechatpadpro:account:bot", &["DogDu"]).unwrap();

    let after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(event.timestamp_epoch_secs >= before);
    assert!(event.timestamp_epoch_secs <= after);
}

#[test]
fn wechat_private_outbound_targets_sender_without_group_prefix() {
    let payload = serde_json::json!({
        "fromUserName": "wxid_private_1"
    });

    let reply = compile_text_reply(&payload, "done");
    assert_eq!(reply["MsgItem"][0]["MsgType"], 1);
    assert_eq!(reply["MsgItem"][0]["ToUserName"], "wxid_private_1");
    assert_eq!(reply["MsgItem"][0]["TextContent"], "done");
    assert!(reply["MsgItem"][0].get("AtWxIDList").is_none());
}
