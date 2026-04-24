use serde::Deserialize;
use tracing::warn;

use crate::protocol::{OutboundAction, OutboundMessage, OutboundPlan, ReactionAction};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionEnvelope {
    #[serde(default)]
    actions: Vec<ActionItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionItem {
    #[serde(rename = "type")]
    action_type: String,
    target_message_id: Option<String>,
    emoji: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ActionBlock {
    Envelope(ActionEnvelope),
    Single(ActionItem),
}

pub fn normalize_agent_output(output: &str) -> Result<OutboundPlan, serde_json::Error> {
    let normalized = output.replace("\r\n", "\n");
    let mut body = String::new();
    let mut actions = Vec::new();
    let mut remaining = normalized.as_str();

    while let Some(index) = remaining.find("```dogbot-action") {
        body.push_str(&remaining[..index]);
        let block = &remaining[index + "```dogbot-action".len()..];
        let block = block.trim_start_matches(|ch: char| ch.is_whitespace());

        let Some((json_block, rest)) = block.split_once("\n```") else {
            break;
        };

        match serde_json::from_str::<ActionBlock>(json_block.trim()) {
            Ok(ActionBlock::Envelope(envelope)) => {
                append_actions(&mut actions, envelope.actions);
            }
            Ok(ActionBlock::Single(item)) => append_actions(&mut actions, vec![item]),
            Err(err) => warn!("failed to parse dogbot-action block: {err}"),
        }

        remaining = rest;
    }

    body.push_str(remaining);

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

fn append_actions(actions: &mut Vec<OutboundAction>, items: Vec<ActionItem>) {
    for item in items {
        if item.action_type == "reaction_add" {
            let Some(target_message_id) = item.target_message_id.filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(emoji) = item.emoji.filter(|value| !value.is_empty()) else {
                continue;
            };

            actions.push(OutboundAction::ReactionAdd(ReactionAction {
                target_message_id,
                emoji,
            }));
        }
    }
}
