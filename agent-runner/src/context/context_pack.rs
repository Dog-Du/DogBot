use super::{
    repo_loader::PackItem,
    scope::{ReadableScopes, ScopeKind},
};

pub fn render_context_pack(scopes: &[ReadableScopes]) -> String {
    render_context_pack_with_history(scopes, None)
}

pub fn render_context_pack_with_history(
    scopes: &[ReadableScopes],
    history_evidence: Option<&str>,
) -> String {
    render_context_pack_with_history_and_items(scopes, history_evidence, &[])
}

pub fn render_context_pack_with_history_and_items(
    scopes: &[ReadableScopes],
    history_evidence: Option<&str>,
    items: &[PackItem],
) -> String {
    let mut output = String::from("Readable scopes:\n");
    for scope in scopes {
        output.push_str(&format!("- {}: {}\n", scope_kind_label(&scope.kind), scope.id));
    }
    output.push('\n');
    if let Some(evidence) = history_evidence.filter(|value| !value.trim().is_empty()) {
        output.push_str(evidence);
    }
    if !items.is_empty() {
        output.push_str("\nEnabled pack items:\n");
        for item in items {
            output.push_str(&format!("- {} [{}] {}\n", item.id, item.kind, item.summary));
        }
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
