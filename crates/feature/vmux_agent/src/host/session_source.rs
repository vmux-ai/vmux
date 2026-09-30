use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::cli::{CliPromptHistory, CliSessionSource, ResumableSession};
use crate::AgentKind;
use crate::acp_registry::RegistryAgent;

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

pub fn kind_supports_cross_runtime(kind: AgentKind) -> bool {
    matches!(kind, AgentKind::Vibe | AgentKind::Claude | AgentKind::Codex)
}

pub(crate) fn acp_agent_kind(agent_id: &str) -> Option<AgentKind> {
    AgentKind::all().into_iter().find(|kind| {
        let segment = kind.as_url_segment();
        agent_id == segment || agent_id == RegistryAgent::canonical_id(segment)
    })
}

pub(crate) fn sort_sessions(mut sessions: Vec<ResumableSession>) -> Vec<ResumableSession> {
    sessions.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|s| seen.insert((s.kind, s.sid.clone())));
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let got = sort_sessions(vec![mk("a", 10), mk("b", 30), mk("a", 20)]);
        assert_eq!(
            got.iter().map(|s| s.sid.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn all_builtin_kinds_support_cross_runtime_handoff() {
        for kind in AgentKind::all() {
            assert!(kind_supports_cross_runtime(kind));
        }
    }

    #[test]
    fn acp_agent_kind_maps_launcher_and_registry_ids() {
        assert_eq!(acp_agent_kind("claude"), Some(AgentKind::Claude));
        assert_eq!(acp_agent_kind("claude-acp"), Some(AgentKind::Claude));
        assert_eq!(acp_agent_kind("codex"), Some(AgentKind::Codex));
        assert_eq!(acp_agent_kind("codex-acp"), Some(AgentKind::Codex));
        assert_eq!(acp_agent_kind("vibe"), Some(AgentKind::Vibe));
        assert_eq!(acp_agent_kind("mistral-vibe"), Some(AgentKind::Vibe));
        assert_eq!(acp_agent_kind("custom"), None);
    }
}
