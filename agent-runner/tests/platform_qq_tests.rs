use agent_runner::platforms::qq::{compile_outbound_message, decode_napcat_event};
use agent_runner::protocol::{MessagePart, OutboundMessage};

#[test]
fn qq_group_event_maps_at_segment_to_structured_mention_and_detects_bot_mention() {
    let payload = serde_json::json!({
        "time": 1_710_000_000,
        "post_type": "message",
        "message_type": "group",
        "group_id": 5566,
        "user_id": 42,
        "message_id": 99,
        "raw_message": "[CQ:at,qq=123] hello",
        "message": [
            {"type":"at","data":{"qq":"123"}},
            {"type":"text","data":{"text":" hello"}}
        ]
    });

    let event = decode_napcat_event(&payload, "qq:bot_uin:123").unwrap();
    let message = event.message().unwrap();

    assert_eq!(event.platform, "qq");
    assert_eq!(event.platform_account, "qq:bot_uin:123");
    assert_eq!(event.conversation, "qq:group:5566");
    assert_eq!(event.actor, "qq:user:42");
    assert_eq!(message.mentions, vec!["qq:bot_uin:123".to_string()]);
    assert_eq!(
        message.parts[0],
        MessagePart::Mention {
            actor_id: "qq:bot_uin:123".into(),
            display: "@123".into(),
        }
    );
    assert_eq!(message.project_plain_text(), "@123 hello");
}

#[test]
fn qq_group_outbound_uses_reply_and_at_cq_codes_in_order() {
    let outbound = OutboundMessage {
        parts: vec![MessagePart::Text {
            text: "done".into(),
        }],
        reply_to: Some("991".into()),
        suppress_default_reply: false,
        delivery_policy: None,
    };

    let encoded =
        compile_outbound_message(&outbound, outbound.reply_to.as_deref(), Some("42")).unwrap();

    assert_eq!(encoded, "[CQ:reply,id=991][CQ:at,qq=42] done");
}

#[test]
fn qq_raw_message_fallback_keeps_bot_mentions_when_segments_are_missing() {
    let payload = serde_json::json!({
        "time": 1_710_000_000,
        "post_type": "message",
        "message_type": "group",
        "group_id": 5566,
        "user_id": 42,
        "message_id": 100,
        "raw_message": "[CQ:at,qq=123] hello",
        "message": [
            {"type":"text","data":{"text":" hello"}}
        ]
    });

    let event = decode_napcat_event(&payload, "qq:bot_uin:123").unwrap();
    let message = event.message().unwrap();

    assert_eq!(message.mentions, vec!["qq:bot_uin:123".to_string()]);
    assert_eq!(message.project_plain_text(), "@123 hello");
}

#[test]
fn qq_outbound_escapes_text_and_rejects_invalid_target_ids() {
    let outbound = OutboundMessage {
        parts: vec![MessagePart::Text {
            text: "[CQ:at,qq=99] & done".into(),
        }],
        reply_to: Some("991".into()),
        suppress_default_reply: false,
        delivery_policy: None,
    };

    let encoded =
        compile_outbound_message(&outbound, outbound.reply_to.as_deref(), Some("42")).unwrap();
    assert_eq!(
        encoded,
        "[CQ:reply,id=991][CQ:at,qq=42] &#91;CQ:at,qq=99&#93; &amp; done"
    );

    assert!(
        compile_outbound_message(
            &outbound,
            outbound.reply_to.as_deref(),
            Some("not-a-number")
        )
        .is_err()
    );
}

#[test]
fn qq_outbound_can_skip_reply_prefix() {
    let outbound = OutboundMessage {
        parts: vec![MessagePart::Text {
            text: "done".into(),
        }],
        reply_to: None,
        suppress_default_reply: true,
        delivery_policy: None,
    };

    let encoded = compile_outbound_message(&outbound, None, Some("42")).unwrap();

    assert_eq!(encoded, "[CQ:at,qq=42] done");
}
