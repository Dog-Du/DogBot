use agent_runner::dispatch::dispatch_plan;
use agent_runner::protocol::{
    AssetRef, AssetSource, MessagePart, OutboundAction, OutboundMessage, OutboundPlan,
    ReactionAction,
};

#[test]
fn dispatcher_accepts_reaction_actions_without_global_capability_matrix() {
    let plan = OutboundPlan {
        messages: vec![OutboundMessage::text("done")],
        actions: vec![
            OutboundAction::ReactionAdd(ReactionAction {
                target_message_id: "msg-1".into(),
                emoji: "👍".into(),
            }),
            OutboundAction::ReactionRemove(ReactionAction {
                target_message_id: "msg-1".into(),
                emoji: "👍".into(),
            }),
        ],
        delivery_report_policy: None,
    };

    dispatch_plan(&plan).unwrap();
}

#[test]
fn dispatcher_rejects_workspace_escape_asset_paths() {
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

    let error = dispatch_plan(&plan).unwrap_err();
    assert!(error.contains("/workspace"));
}
