use crate::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerDecision {
    Run,
    Status,
    Ignore,
}

#[derive(Default)]
pub struct TriggerResolver;

impl TriggerResolver {
    pub fn resolve(&self, event: &CanonicalEvent) -> TriggerDecision {
        let EventKind::Message { message } = &event.kind else {
            return TriggerDecision::Ignore;
        };

        let normalized = canonical_trigger_text(message, &event.platform_account);
        match conversation_scope(&event.conversation) {
            Some("private") => {
                if normalized.is_empty() {
                    TriggerDecision::Ignore
                } else if normalized == "/agent-status" {
                    TriggerDecision::Status
                } else {
                    TriggerDecision::Run
                }
            }
            Some("group") => {
                let mentions_bot = message
                    .mentions
                    .iter()
                    .any(|mention| mention == &event.platform_account);
                if !mentions_bot {
                    TriggerDecision::Ignore
                } else if normalized == "/agent-status" {
                    TriggerDecision::Status
                } else {
                    TriggerDecision::Run
                }
            }
            _ => TriggerDecision::Ignore,
        }
    }
}

pub fn should_trigger_run(event: &CanonicalEvent) -> bool {
    matches!(
        TriggerResolver::default().resolve(event),
        TriggerDecision::Run | TriggerDecision::Status
    )
}

fn conversation_scope(conversation: &str) -> Option<&str> {
    let mut parts = conversation.splitn(3, ':');
    let _platform = parts.next()?;
    parts.next()
}

fn canonical_trigger_text(message: &CanonicalMessage, platform_account: &str) -> String {
    let mut projected = String::new();
    let mut skipping_leading_bot_mentions = true;

    for part in &message.parts {
        match part {
            MessagePart::Mention { actor_id, .. }
                if skipping_leading_bot_mentions && actor_id == platform_account => {}
            MessagePart::Text { text } => {
                if skipping_leading_bot_mentions && text.trim().is_empty() {
                    continue;
                }
                skipping_leading_bot_mentions = false;
                projected.push_str(text);
            }
            MessagePart::Mention { display, .. } => {
                skipping_leading_bot_mentions = false;
                projected.push_str(display);
            }
            _ => {
                skipping_leading_bot_mentions = false;
            }
        }
    }

    if projected.trim().is_empty() {
        message.project_plain_text().trim().to_string()
    } else {
        projected.trim().to_string()
    }
}
