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
        plain_text: String::new(),
        mentions: vec!["qq:bot_uin:123".into()],
        native_metadata: serde_json::json!({}),
    };

    assert_eq!(message.project_plain_text(), "@DogDu please check");
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
    );

    assert_eq!(event.kind_name(), "reaction_added");
}
