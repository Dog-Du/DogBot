use super::scope::{ReadableScopes, ScopeKind};

pub fn render_context_pack(scopes: &[ReadableScopes]) -> String {
    render_context_pack_with_history(scopes, None)
}

pub fn render_context_pack_with_history(
    scopes: &[ReadableScopes],
    history_evidence: Option<&str>,
) -> String {
    let mut output = String::from("Readable scopes:\n");
    for scope in scopes {
        output.push_str(&format!("- {}: {}\n", scope_kind_label(&scope.kind), scope.id));
    }
    output.push('\n');
    if let Some(evidence) = history_evidence.filter(|value| !value.trim().is_empty()) {
        output.push_str(evidence);
    }
    output
}

fn scope_kind_label(kind: &ScopeKind) -> &'static str {
    match kind {
        ScopeKind::UserPrivate => "user-private",
        ScopeKind::ConversationShared => "conversation-shared",
        ScopeKind::PlatformAccountShared => "platform-account-shared",
        ScopeKind::BotGlobalAdmin => "bot-global-admin",
    }
}
