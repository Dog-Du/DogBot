use agent_runner::protocol::{
    AssetRef, AssetSource, CanonicalEvent, CanonicalMessage, MessagePart, OutboundAction,
    OutboundMessage, OutboundPlan, ReactionAction,
};

#[test]
fn canonical_message_plain_text_only_projects_text_and_mentions() {
    let message = CanonicalMessage {
        message_id: "msg-1".into(),
        reply_to: None,
        parts: vec![
            MessagePart::Mention {
                actor_id: "qq:user:42".into(),
                display: "@DogDu".into(),
            },
            MessagePart::Text {
                text: " please check".into(),
            },
            MessagePart::Image {
                asset: AssetRef {
                    asset_id: "asset-1".into(),
                    kind: "image".into(),
                    mime: "image/png".into(),
                    size_bytes: 16,
                    source: AssetSource::WorkspacePath("/workspace/inbox/a.png".into()),
                },
            },
        ],
        mentions: vec!["qq:bot_uin:123".into()],
        native_metadata: serde_json::json!({}),
    };

    assert_eq!(message.project_plain_text(), "@DogDu please check");
    assert_eq!(message.plain_text(), "@DogDu please check");
}

#[test]
fn outbound_plan_keeps_reaction_actions_separate_from_messages() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage::text("done")],
        actions: vec![OutboundAction::ReactionAdd(ReactionAction {
            target_message_id: "msg-2".into(),
            emoji: "👍".into(),
        })],
        delivery_report_policy: None,
    };

    assert_eq!(plan.messages.len(), 1);
    assert_eq!(plan.actions.len(), 1);
}

#[test]
fn canonical_event_kind_distinguishes_message_and_reaction() {
    let event = CanonicalEvent::reaction_added(
        "wechatpadpro",
        "wechatpadpro:account:bot",
        "wechatpadpro:group:123@chatroom",
        "wechatpadpro:user:alice",
        "evt-1",
        1710000000,
        "msg-9",
        "❤️",
        serde_json::json!({"reaction_source": "bridge"}),
    );

    assert_eq!(event.kind_name(), "reaction_added");
    assert_eq!(event.raw_native_payload, serde_json::json!({"reaction_source": "bridge"}));
}

#[test]
fn canonical_event_message_accessor_returns_message_payload() {
    let message = CanonicalMessage {
        message_id: "msg-2".into(),
        reply_to: Some("msg-1".into()),
        parts: vec![MessagePart::Text {
            text: "hello".into(),
        }],
        mentions: vec![],
        native_metadata: serde_json::json!({"source": "native"}),
    };
    let event = CanonicalEvent {
        platform: "qq".into(),
        platform_account: "qq:account:bot".into(),
        conversation: "qq:group:123".into(),
        actor: "qq:user:alice".into(),
        event_id: "evt-2".into(),
        timestamp_epoch_secs: 1710000001,
        kind: agent_runner::protocol::EventKind::Message {
            message: message.clone(),
        },
        raw_native_payload: serde_json::json!({"native": true}),
    };

    assert_eq!(event.kind_name(), "message");
    assert_eq!(event.message(), Some(&message));
}

#[test]
fn outbound_message_text_uses_exact_defaults() {
    let message = OutboundMessage::text("done");

    assert_eq!(
        message,
        OutboundMessage {
            parts: vec![MessagePart::Text {
                text: "done".into(),
            }],
            reply_to: None,
            delivery_policy: None,
        }
    );
}

#[test]
fn serde_shape_is_explicit_for_message_part() {
    let value = serde_json::to_value(MessagePart::Mention {
        actor_id: "qq:user:42".into(),
        display: "@DogDu".into(),
    })
    .unwrap();

    assert_eq!(
        value,
        serde_json::json!({
            "type": "mention",
            "data": {
                "actor_id": "qq:user:42",
                "display": "@DogDu"
            }
        })
    );
}
