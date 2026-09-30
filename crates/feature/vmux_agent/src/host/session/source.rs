use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::super::cli::{CliPromptHistory, CliSessionSource};
use crate::AgentKind;

#[derive(SystemParam)]
pub struct CliSessionSources<'w, 's> {
    cli: Query<'w, 's, &'static CliSessionSource>,
    history: Query<'w, 's, (&'static CliSessionSource, &'static CliPromptHistory)>,
}

impl CliSessionSources<'_, '_> {
    pub fn get(&self, kind: AgentKind) -> Option<CliSessionSource> {
        self.cli
            .iter()
            .find(|strategy| strategy.kind == kind)
            .copied()
    }

    pub fn all(&self) -> Vec<CliSessionSource> {
        self.cli.iter().copied().collect()
    }

    pub fn prompt_history(&self, kind: AgentKind) -> Option<CliPromptHistory> {
        self.history
            .iter()
            .find(|(source, _)| source.kind == kind)
            .map(|(_, history)| *history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::cli::ResumableSession;
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[test]
    fn register_cli_and_lookup_by_kind() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.spawn(crate::host::cli::CLAUDE);
        let found = world
            .run_system_once(|sources: CliSessionSources| {
                (
                    sources.get(AgentKind::Claude).is_some(),
                    sources.get(AgentKind::Vibe).is_some(),
                )
            })
            .unwrap();
        assert_eq!(found, (true, false));
    }

    #[test]
    fn sort_sessions_is_newest_first_and_deduped() {
        use std::time::Duration;
        let mk = |sid: &str, secs: u64| ResumableSession {
            kind: AgentKind::Claude,
            sid: sid.into(),
            cwd: PathBuf::from("/w"),
            transcript: PathBuf::from("/w/none.jsonl"),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            title: sid.into(),
            latest: String::new(),
            cross_runtime: true,
        };
        let got = ResumableSession::newest_unique(vec![mk("a", 10), mk("b", 30), mk("a", 20)]);
        assert_eq!(
            got.iter().map(|s| s.sid.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn acp_agent_kind_maps_launcher_and_registry_ids() {
        use crate::acp_registry::RegistryAgent;

        assert_eq!(RegistryAgent::kind("claude"), Some(AgentKind::Claude));
        assert_eq!(RegistryAgent::kind("claude-acp"), Some(AgentKind::Claude));
        assert_eq!(RegistryAgent::kind("codex"), Some(AgentKind::Codex));
        assert_eq!(RegistryAgent::kind("codex-acp"), Some(AgentKind::Codex));
        assert_eq!(RegistryAgent::kind("vibe"), Some(AgentKind::Vibe));
        assert_eq!(RegistryAgent::kind("mistral-vibe"), Some(AgentKind::Vibe));
        assert_eq!(RegistryAgent::kind("custom"), None);
    }
}
