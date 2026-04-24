use serde::{Deserialize, Serialize};

use super::CanonicalMessage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    Message { message: CanonicalMessage },
    ReactionAdded { target_message_id: String, emoji: String },
    ReactionRemoved { target_message_id: String, emoji: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub platform: String,
    pub platform_account: String,
    pub conversation: String,
    pub actor: String,
    pub event_id: String,
    pub timestamp_epoch_secs: i64,
    pub kind: EventKind,
    pub raw_native_payload: serde_json::Value,
}

impl CanonicalEvent {
    pub fn reaction_added(
        platform: &str,
        platform_account: &str,
        conversation: &str,
        actor: &str,
        event_id: &str,
        timestamp_epoch_secs: i64,
        target_message_id: &str,
        emoji: &str,
    ) -> Self {
        Self {
            platform: platform.to_string(),
            platform_account: platform_account.to_string(),
            conversation: conversation.to_string(),
            actor: actor.to_string(),
            event_id: event_id.to_string(),
            timestamp_epoch_secs,
            kind: EventKind::ReactionAdded {
                target_message_id: target_message_id.to_string(),
                emoji: emoji.to_string(),
            },
            raw_native_payload: serde_json::json!({}),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match &self.kind {
            EventKind::Message { .. } => "message",
            EventKind::ReactionAdded { .. } => "reaction_added",
            EventKind::ReactionRemoved { .. } => "reaction_removed",
        }
    }

    pub fn message(&self) -> Option<&CanonicalMessage> {
        match &self.kind {
            EventKind::Message { message } => Some(message),
            _ => None,
        }
    }
}
