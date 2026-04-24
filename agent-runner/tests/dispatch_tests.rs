use agent_runner::dispatch::{CapabilityProfile, dispatch_plan};
use agent_runner::protocol::{
    AssetRef, AssetSource, MessagePart, OutboundAction, OutboundMessage, OutboundPlan,
    ReactionAction,
};

#[tokio::test]
async fn dispatcher_drops_best_effort_reaction_when_platform_does_not_support_it() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage::text("done")],
        actions: vec![OutboundAction::ReactionAdd(ReactionAction {
            target_message_id: "msg-1".into(),
            emoji: "👍".into(),
        })],
        delivery_report_policy: None,
    };

    let result = dispatch_plan(
        "wechatpadpro",
        &CapabilityProfile {
            supports_reply: true,
            supports_reaction: false,
            supports_sticker: false,
        },
        &plan,
    )
    .await
    .unwrap();

    assert_eq!(result.degraded_actions, vec!["reaction_add".to_string()]);
}

#[tokio::test]
async fn dispatcher_rejects_workspace_escape_asset_paths() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage {
            parts: vec![MessagePart::Image {
                asset: AssetRef {
                    asset_id: "asset-1".into(),
                    kind: "image".into(),
                    mime: "image/png".into(),
                    size_bytes: 8,
                    source: AssetSource::WorkspacePath("/tmp/not-allowed.png".into()),
                },
            }],
            reply_to: None,
            delivery_policy: None,
        }],
        actions: vec![],
        delivery_report_policy: None,
    };

    let error = dispatch_plan(
        "qq",
        &CapabilityProfile {
            supports_reply: true,
            supports_reaction: true,
            supports_sticker: true,
        },
        &plan,
    )
    .await
    .unwrap_err();

    assert!(error.contains("/workspace"));
}
