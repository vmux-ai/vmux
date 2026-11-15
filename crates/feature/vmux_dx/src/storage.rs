use crate::policy::Policy;

#[derive(bevy_ecs::prelude::Component, Clone)]
pub struct Storage {
    pub(crate) policy: Policy,
}

impl Storage {
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    pub(crate) fn policy(&self) -> &Policy {
        &self.policy
    }
}
