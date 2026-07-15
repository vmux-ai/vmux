use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::AgentKind;
use crate::AgentVariant;
use crate::client::cli::strategy::{CliAgentStrategy, ResumableSession};
use crate::message::Message;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default, Clone)]
pub struct AgentStrategies {
    cli: HashMap<AgentKind, Arc<dyn CliAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.cli.insert(strategy.kind(), strategy.into());
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.cli.get(&kind).map(Arc::as_ref)
    }

    pub fn cli_strategies(&self) -> impl Iterator<Item = &dyn CliAgentStrategy> {
        self.cli.values().map(Arc::as_ref)
    }

    /// All resumable sessions across every registered CLI strategy, newest-first, deduped.
    pub fn list_all_sessions(&self) -> Vec<ResumableSession> {
        let all = self
            .cli_strategies()
            .flat_map(|s| s.list_sessions())
            .collect();
        sort_sessions(all)
    }

    pub fn load_transcript(&self, kind: AgentKind, sid: &str) -> Result<Vec<Message>, String> {
        self.get_cli(kind)
            .ok_or_else(|| format!("no session strategy registered for {}", kind.display_name()))?
            .load_transcript(sid)
    }
}

/// Whether a kind's ACP and CLI runtimes share the same session id (so a session can be
/// handed off between them). Single source of truth for the `cross_runtime` flag.
pub fn kind_supports_cross_runtime(kind: AgentKind) -> bool {
    matches!(kind, AgentKind::Vibe | AgentKind::Claude | AgentKind::Codex)
}

/// Maps a built-in launcher id or its ACP registry id to the shared agent kind.
pub(crate) fn acp_agent_kind(agent_id: &str) -> Option<AgentKind> {
    AgentKind::all().into_iter().find(|kind| {
        let segment = kind.as_url_segment();
        agent_id == segment || agent_id == crate::acp_install::registry_id_alias(segment)
    })
}

/// Sort newest-first and drop duplicate `(kind, sid)` keeping the newest.
pub fn sort_sessions(mut sessions: Vec<ResumableSession>) -> Vec<ResumableSession> {
    sessions.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|s| seen.insert((s.kind, s.sid.clone())));
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use crate::McpServerConfig;

    struct StubStrategy;
    impl AgentStrategy for StubStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Claude
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::Cli
        }
    }
    impl CliAgentStrategy for StubStrategy {
        fn sessions_root(&self) -> PathBuf {
            PathBuf::from("/tmp/none")
        }
        fn build_args(&self, _: &McpServerConfig, _: Option<&str>) -> Vec<String> {
            vec![]
        }
        fn build_env(&self, _: &McpServerConfig) -> Vec<(String, String)> {
            vec![]
        }
        fn discover_session(&self, _: &Path, _: SystemTime, _: &HashSet<String>) -> Option<String> {
            None
        }
        fn detect_end_time(&self, _: &str) -> bool {
            false
        }
    }

    #[test]
    fn register_cli_and_lookup_by_kind() {
        let mut s = AgentStrategies::default();
        s.register_cli(Box::new(StubStrategy));
        assert!(s.get_cli(AgentKind::Claude).is_some());
        assert!(s.get_cli(AgentKind::Vibe).is_none());
    }

    #[test]
    fn sort_sessions_is_newest_first_and_deduped() {
        use std::time::Duration;
        let mk = |sid: &str, secs: u64| ResumableSession {
            kind: AgentKind::Claude,
            sid: sid.into(),
            cwd: PathBuf::from("/w"),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            title: sid.into(),
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
