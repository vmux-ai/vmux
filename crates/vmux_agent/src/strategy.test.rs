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
