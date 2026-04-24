use serde::{Deserialize, Serialize};

use super::MessagePart;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionAction {
    pub target_message_id: String,
    pub emoji: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub parts: Vec<MessagePart>,
    pub reply_to: Option<String>,
    pub delivery_policy: Option<String>,
}

impl OutboundMessage {
    pub fn text(text: &str) -> Self {
        Self {
            parts: vec![MessagePart::Text {
                text: text.to_string(),
            }],
            reply_to: None,
            delivery_policy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum OutboundAction {
    ReactionAdd(ReactionAction),
    ReactionRemove(ReactionAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundPlan {
    pub messages: Vec<OutboundMessage>,
    pub actions: Vec<OutboundAction>,
    pub delivery_report_policy: Option<String>,
}
