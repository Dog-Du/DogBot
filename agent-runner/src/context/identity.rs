#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActorId(String);

impl ActorId {
    pub fn new(actor_id: impl Into<String>) -> Option<Self> {
        let actor_id = actor_id.into().trim().to_string();
        if actor_id.is_empty() {
            None
        } else {
            Some(Self(actor_id))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    actor_id: ActorId,
}

impl Identity {
    pub fn new(actor_id: impl Into<String>) -> Option<Self> {
        ActorId::new(actor_id).map(|actor_id| Self { actor_id })
    }

    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }
}
