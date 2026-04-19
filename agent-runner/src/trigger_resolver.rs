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
        if contains_command_token(&message.normalized_text, "/agent-status") {
            return TriggerDecision::Status;
        }

        if !contains_command_token(&message.normalized_text, "/agent") {
            return TriggerDecision::Ignore;
        }

        if message.is_private {
            return TriggerDecision::Run;
        }

        if !message.is_group {
            return TriggerDecision::Ignore;
        }

        let mentions_bot = message.mentions.iter().any(|mention| mention == &message.platform_account);
        if mentions_bot || message.reply_to_message_id.is_some() {
            return TriggerDecision::Run;
        }

        TriggerDecision::Ignore
    }
}

fn contains_command_token(text: &str, command: &str) -> bool {
    text.match_indices(command).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + command.len()..].chars().next();
        is_command_boundary(before) && is_command_boundary(after)
    })
}

fn is_command_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(ch) => {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ',' | '.'
                        | '!'
                        | '?'
                        | ':'
                        | ';'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '"'
                        | '\''
                        | '，'
                        | '。'
                        | '！'
                        | '？'
                        | '：'
                        | '；'
                        | '（'
                        | '）'
                        | '【'
                        | '】'
                        | '《'
                        | '》'
                )
        }
    }
}
