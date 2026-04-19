use agent_runner::context::{
    policy::PermissionPolicy,
    scope::{ReadableScopes, ScopeKind, ScopeResolver, BOT_GLOBAL_ADMIN_SCOPE_ID},
};
use agent_runner::context::identity::{ActorId, Identity};
use std::collections::BTreeSet;

#[test]
fn scope_resolver_orders_readable_scopes_from_local_to_global() {
    let actor_id = ActorId::new("qq:user:1").expect("actor id");
    let scopes =
        ScopeResolver::new().readable_scopes(&actor_id, "qq:group:100", "qq:bot_uin:123");

    assert_eq!(
        scopes,
        vec![
            ReadableScopes::new(ScopeKind::UserPrivate, "qq:user:1"),
            ReadableScopes::new(ScopeKind::ConversationShared, "qq:group:100"),
            ReadableScopes::new(ScopeKind::PlatformAccountShared, "qq:bot_uin:123"),
            ReadableScopes::new(ScopeKind::BotGlobalAdmin, BOT_GLOBAL_ADMIN_SCOPE_ID),
        ]
    );
}

#[test]
fn permission_policy_blocks_unapproved_conversation_shared_write() {
    let policy = PermissionPolicy::new(vec![ActorId::new("qq:user:admin").expect("actor id")]);
    let actor_id = ActorId::new("qq:user:1").expect("actor id");
    let authorized_actor_ids: BTreeSet<ActorId> = BTreeSet::new();

    let result = policy.can_write_conversation_shared(
        &actor_id,
        "qq:group:100",
        &authorized_actor_ids,
    );

    assert!(!result.allowed);
    assert_eq!(result.reason.as_deref(), Some("actor_not_authorized_for_conversation"));
}

#[test]
fn permission_policy_allows_admin_conversation_shared_write() {
    let policy = PermissionPolicy::new(vec![ActorId::new("qq:user:admin").expect("actor id")]);
    let actor_id = ActorId::new("  qq:user:admin  ").expect("actor id");
    let authorized_actor_ids: BTreeSet<ActorId> = BTreeSet::new();

    let result =
        policy.can_write_conversation_shared(&actor_id, "qq:group:100", &authorized_actor_ids);

    assert!(result.allowed);
    assert!(result.reason.is_none());
}

#[test]
fn permission_policy_allows_explicitly_authorized_actor_conversation_shared_write() {
    let policy = PermissionPolicy::new(vec![]);
    let actor_id = ActorId::new("qq:user:1").expect("actor id");
    let authorized_actor_ids: BTreeSet<ActorId> =
        [ActorId::new("  qq:user:1  ").expect("actor id")]
            .into_iter()
            .collect();

    let result =
        policy.can_write_conversation_shared(&actor_id, "qq:group:100", &authorized_actor_ids);

    assert!(result.allowed);
    assert!(result.reason.is_none());
}

#[test]
fn readable_scopes_new_accepts_owned_strings() {
    let scope = ReadableScopes::new(ScopeKind::UserPrivate, String::from("qq:user:1"));
    assert_eq!(scope.id, "qq:user:1");
}

#[test]
fn identity_new_trims_and_rejects_empty_actor_ids() {
    let identity = Identity::new("  qq:user:1  ").expect("identity");
    assert_eq!(identity.actor_id().as_str(), "qq:user:1");
    assert!(Identity::new("   ").is_none());
}
