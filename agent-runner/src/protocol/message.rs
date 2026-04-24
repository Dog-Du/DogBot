use serde::{Deserialize, Serialize};

use super::AssetRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    Mention { actor_id: String, display: String },
    Image { asset: AssetRef },
    File { asset: AssetRef },
    Voice { asset: AssetRef },
    Video { asset: AssetRef },
    Sticker { asset: AssetRef },
    Quote { target_message_id: String, excerpt: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalMessage {
    pub message_id: String,
    pub reply_to: Option<String>,
    pub parts: Vec<MessagePart>,
    pub mentions: Vec<String>,
    pub native_metadata: serde_json::Value,
}

impl CanonicalMessage {
    pub fn plain_text(&self) -> String {
        self.project_plain_text()
    }

    pub fn project_plain_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                MessagePart::Text { text } => Some(text.as_str()),
                MessagePart::Mention { display, .. } => Some(display.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}
