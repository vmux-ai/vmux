use bevy::prelude::*;

use crate::ProcessId;

#[derive(Message, Clone)]
pub struct BellReceived {
    pub process_id: ProcessId,
}

#[derive(Message, Clone)]
pub struct AgentAttention {
    pub entity: Entity,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Message, Clone)]
pub struct OsNotify {
    pub title: String,
    pub body: String,
}

#[derive(Component)]
pub struct AgentDoneUnseen;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_attention_carries_optional_text() {
        let a = AgentAttention {
            entity: Entity::PLACEHOLDER,
            title: Some("done".into()),
            body: None,
        };
        assert_eq!(a.title.as_deref(), Some("done"));
        assert!(a.body.is_none());
    }
}
