use crate::protocol::{AssetSource, MessagePart, OutboundAction, OutboundPlan};

#[derive(Debug, Clone)]
pub struct CapabilityProfile {
    pub supports_reply: bool,
    pub supports_reaction: bool,
    pub supports_sticker: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchResult {
    pub degraded_actions: Vec<String>,
}

pub async fn dispatch_plan(
    _platform: &str,
    capabilities: &CapabilityProfile,
    plan: &OutboundPlan,
) -> Result<DispatchResult, String> {
    validate_workspace_assets(plan)?;

    let mut degraded_actions = Vec::new();
    for action in &plan.actions {
        match action {
            OutboundAction::ReactionAdd(_) | OutboundAction::ReactionRemove(_)
                if !capabilities.supports_reaction =>
            {
                degraded_actions.push(action_name(action).to_string());
            }
            _ => {}
        }
    }

    for message in &plan.messages {
        if message.reply_to.is_some() && !capabilities.supports_reply {
            return Err("platform does not support reply".to_string());
        }

        for part in &message.parts {
            if matches!(part, MessagePart::Sticker { .. }) && !capabilities.supports_sticker {
                return Err("platform does not support sticker".to_string());
            }
        }
    }

    Ok(DispatchResult { degraded_actions })
}

fn validate_workspace_assets(plan: &OutboundPlan) -> Result<(), String> {
    for message in &plan.messages {
        for part in &message.parts {
            let asset = match part {
                MessagePart::Image { asset }
                | MessagePart::File { asset }
                | MessagePart::Voice { asset }
                | MessagePart::Video { asset }
                | MessagePart::Sticker { asset } => asset,
                _ => continue,
            };

            if let AssetSource::WorkspacePath(path) = &asset.source
                && !path.starts_with("/workspace/")
            {
                return Err(format!("asset path must stay under /workspace: {path}"));
            }
        }
    }

    Ok(())
}

fn action_name(action: &OutboundAction) -> &'static str {
    match action {
        OutboundAction::ReactionAdd(_) => "reaction_add",
        OutboundAction::ReactionRemove(_) => "reaction_remove",
    }
}
