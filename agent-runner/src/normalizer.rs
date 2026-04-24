use serde::Deserialize;

use crate::protocol::{OutboundAction, OutboundMessage, OutboundPlan, ReactionAction};

#[derive(Debug, Deserialize)]
struct ActionEnvelope {
    #[serde(default)]
    actions: Vec<ActionItem>,
}

#[derive(Debug, Deserialize)]
struct ActionItem {
    #[serde(rename = "type")]
    action_type: String,
    target_message_id: Option<String>,
    emoji: Option<String>,
}

pub fn normalize_agent_output(output: &str) -> Result<OutboundPlan, serde_json::Error> {
    let mut body = String::new();
    let mut actions = Vec::new();
    let mut lines = output.lines();

    while let Some(line) = lines.next() {
        if line.trim() == "```dogbot-action" {
            let mut block = String::new();
            for block_line in lines.by_ref() {
                if block_line.trim() == "```" {
                    break;
                }
                if !block.is_empty() {
                    block.push('\n');
                }
                block.push_str(block_line);
            }

            let envelope: ActionEnvelope = serde_json::from_str(block.trim())?;
            for item in envelope.actions {
                if item.action_type == "reaction_add" {
                    actions.push(OutboundAction::ReactionAdd(ReactionAction {
                        target_message_id: item.target_message_id.unwrap_or_default(),
                        emoji: item.emoji.unwrap_or_default(),
                    }));
                }
            }
            continue;
        }

        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }

    let body = body.trim();
    let messages = if body.is_empty() {
        Vec::new()
    } else {
        vec![OutboundMessage::text(body)]
    };

    Ok(OutboundPlan {
        messages,
        actions,
        delivery_report_policy: None,
    })
}
