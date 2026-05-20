use bevy::prelude::*;
use std::collections::HashMap;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WriteCommandBarSnapshots;

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarAgentsSnapshot {
    pub providers: Vec<AgentProviderSummary>,
    pub strategies: Vec<AgentStrategySummary>,
}

#[derive(Clone, Debug)]
pub struct AgentProviderSummary {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug)]
pub struct AgentStrategySummary {
    pub provider: String,
    pub model: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSettingsSnapshot {
    pub settings_page_url: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSpacesSnapshot {
    pub spaces: Vec<SpaceSummary>,
    pub active_space_id: String,
    pub active_space_name: String,
    pub spaces_page_url: String,
}

#[derive(Clone, Debug)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub profile: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarTerminalsSnapshot {
    pub pid_to_entity: HashMap<u32, Entity>,
    pub processes: Vec<TerminalProcessSummary>,
    pub agent_session_to_entity: HashMap<String, Entity>,
    pub terminal_page_url: String,
}

#[derive(Clone, Debug)]
pub struct TerminalProcessSummary {
    pub pid: u32,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_snapshot_default_is_empty() {
        let s = CommandBarAgentsSnapshot::default();
        assert!(s.providers.is_empty());
        assert!(s.strategies.is_empty());
    }

    #[test]
    fn terminals_snapshot_default_is_empty() {
        let s = CommandBarTerminalsSnapshot::default();
        assert!(s.pid_to_entity.is_empty());
        assert!(s.processes.is_empty());
        assert!(s.agent_session_to_entity.is_empty());
    }
}
