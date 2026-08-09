use super::*;
use crate::client::cli::vibe::VibeStrategy;

#[test]
fn pending_with_no_match_keeps_pending() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .init_resource::<AgentSessionToEntity>()
        .add_systems(Update, discover_pending_agent_sessions);

    let pending = PendingAgentSession {
        kind: AgentKind::Vibe,
        spawn_time: std::time::SystemTime::now(),
        cwd: PathBuf::from("/this/path/does/not/exist"),
    };
    let entity = app.world_mut().spawn(pending).id();
    app.update();
    assert!(app.world().get::<PendingAgentSession>(entity).is_some());
    assert!(app.world().get::<SessionId>(entity).is_none());
}

#[test]
fn pending_is_retained_long_after_spawn_for_late_session_dir() {
    use std::path::Path;

    struct NeverDiscovers;
    impl crate::strategy::AgentStrategy for NeverDiscovers {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> crate::AgentVariant {
            crate::AgentVariant::Cli
        }
    }
    impl crate::CliAgentStrategy for NeverDiscovers {
        fn sessions_root(&self) -> PathBuf {
            PathBuf::from("/tmp/none")
        }
        fn build_args(&self, _: &crate::McpServerConfig, _: Option<&str>) -> Vec<String> {
            vec![]
        }
        fn build_env(&self, _: &crate::McpServerConfig) -> Vec<(String, String)> {
            vec![]
        }
        fn discover_session(&self, _: &Path, _: SystemTime, _: &HashSet<String>) -> Option<String> {
            None
        }
        fn detect_end_time(&self, _: &str) -> bool {
            false
        }
    }

    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(NeverDiscovers));
    app.insert_resource(strategies)
        .init_resource::<AgentSessionToEntity>()
        .add_systems(Update, discover_pending_agent_sessions);

    let pending = PendingAgentSession {
        kind: AgentKind::Vibe,
        spawn_time: SystemTime::UNIX_EPOCH,
        cwd: PathBuf::from("/this/path/does/not/exist"),
    };
    let entity = app.world_mut().spawn(pending).id();
    app.update();
    assert!(
        app.world().get::<PendingAgentSession>(entity).is_some(),
        "pending must survive long after spawn so a vibe session dir written mid-session is still discovered"
    );
}
