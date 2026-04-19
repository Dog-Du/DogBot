use crate::context::identity::ActorId;

pub const BOT_GLOBAL_ADMIN_SCOPE_ID: &str = "dogbot";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    UserPrivate,
    ConversationShared,
    PlatformAccountShared,
    BotGlobalAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableScopes {
    pub kind: ScopeKind,
    pub id: String,
}

impl ReadableScopes {
    pub fn new(kind: ScopeKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

pub struct ScopeResolver;

impl ScopeResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn readable_scopes(
        &self,
        actor_id: &ActorId,
        conversation_id: &str,
        platform_account_id: &str,
    ) -> Vec<ReadableScopes> {
        vec![
            ReadableScopes::new(ScopeKind::UserPrivate, actor_id.as_str()),
            ReadableScopes::new(ScopeKind::ConversationShared, conversation_id),
            ReadableScopes::new(ScopeKind::PlatformAccountShared, platform_account_id),
            ReadableScopes::new(ScopeKind::BotGlobalAdmin, BOT_GLOBAL_ADMIN_SCOPE_ID),
        ]
    }
}
