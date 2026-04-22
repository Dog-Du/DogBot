use crate::inbound_models::InboundMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerDecision {
    Run,
    Status,
    Ignore,
}

#[derive(Default)]
pub struct TriggerResolver;

impl TriggerResolver {
    pub fn resolve(&self, message: &InboundMessage) -> TriggerDecision {
        let normalized = message.normalized_text.trim();
        if normalized.is_empty() && !message.is_group {
            return TriggerDecision::Ignore;
        }

        if message.is_private {
            return if normalized == "/agent-status" {
                TriggerDecision::Status
            } else {
                TriggerDecision::Run
            };
        }

        if !message.is_group {
            return TriggerDecision::Ignore;
        }

        let mentions_bot = message
            .mentions
            .iter()
            .any(|mention| mention == &message.platform_account);
        if !mentions_bot {
            return TriggerDecision::Ignore;
        }

        if normalized == "/agent-status" {
            return TriggerDecision::Status;
        }

        TriggerDecision::Run
    }
}
