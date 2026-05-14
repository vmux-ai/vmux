use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;

use crate::AgentKind;

#[derive(Component, Debug, Clone)]
pub struct AgentSession {
    pub kind: AgentKind,
}

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug, Clone)]
pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Resource, Default, Debug)]
pub struct AgentSessionToEntity(pub HashMap<(AgentKind, String), Entity>);

#[derive(Resource, Default, Debug)]
pub struct AgentSessionDirty(pub bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_session_to_entity_starts_empty() {
        let map = AgentSessionToEntity::default();
        assert!(map.0.is_empty());
    }

    #[test]
    fn pending_session_carries_cwd_and_kind() {
        let pending = PendingAgentSession {
            kind: AgentKind::Claude,
            spawn_time: SystemTime::UNIX_EPOCH,
            cwd: PathBuf::from("/tmp/x"),
        };
        assert_eq!(pending.kind, AgentKind::Claude);
        assert_eq!(pending.cwd, PathBuf::from("/tmp/x"));
    }
}
