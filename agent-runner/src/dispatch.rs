use crate::protocol::{AssetSource, MessagePart, OutboundPlan};

pub fn dispatch_plan(plan: &OutboundPlan) -> Result<(), String> {
    validate_workspace_assets(plan)?;
    Ok(())
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
