use std::collections::BTreeSet;

use crate::context::identity::ActorId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    pub allowed: bool,
    pub reason: Option<&'static str>,
}

pub struct PermissionPolicy {
    admin_actor_ids: BTreeSet<ActorId>,
}

impl PermissionPolicy {
    pub fn new(admin_actor_ids: Vec<ActorId>) -> Self {
        Self {
            admin_actor_ids: admin_actor_ids.into_iter().collect(),
        }
    }

    pub fn can_write_conversation_shared(
        &self,
        actor_id: &ActorId,
        _conversation_id: &str,
        authorized_actor_ids: &BTreeSet<ActorId>,
    ) -> PermissionDecision {
        if self.admin_actor_ids.contains(actor_id) || authorized_actor_ids.contains(actor_id) {
            PermissionDecision {
                allowed: true,
                reason: None,
            }
        } else {
            PermissionDecision {
                allowed: false,
                reason: Some("actor_not_authorized_for_conversation"),
            }
        }
    }
}
