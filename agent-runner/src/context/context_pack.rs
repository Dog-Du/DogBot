use super::scope::{ReadableScopes, ScopeKind};

pub fn render_context_pack(scopes: &[ReadableScopes]) -> String {
    let mut output = String::from("Readable scopes:\n");
    for scope in scopes {
        output.push_str(&format!("- {}: {}\n", scope_kind_label(&scope.kind), scope.id));
    }
    output.push('\n');
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
