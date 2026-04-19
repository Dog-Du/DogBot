use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboundMessage {
    pub platform: String,
    pub platform_account: String,
    pub conversation_id: String,
    pub actor_id: String,
    pub message_id: String,
    pub reply_to_message_id: Option<String>,
    pub raw_segments_json: String,
    pub normalized_text: String,
    pub mentions: Vec<String>,
    pub is_group: bool,
    pub is_private: bool,
    pub timestamp_epoch_secs: i64,
}
