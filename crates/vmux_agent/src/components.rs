use std::collections::HashSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentSession {
    pub kind: AgentKind,
    pub variant: AgentVariant,
    pub sid: String,
    pub provider: String,
    pub model: String,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentMessages(pub Vec<Message>);

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentApprovalPolicy {
    pub auto: HashSet<String>,
}

#[derive(Component, Clone, Debug)]
pub struct PendingUserInput(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_default_constructible() {
        let _ = AgentMessages::default();
        let _ = AgentApprovalPolicy::default();
    }
}
