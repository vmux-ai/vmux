use super::*;
use std::path::Path;

#[test]
fn detect_file_end_time_exit_strips_components_when_strategy_says_ended() {
    struct EndedStrategy;
    impl crate::strategy::AgentStrategy for EndedStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> crate::AgentVariant {
            crate::AgentVariant::Cli
        }
    }
    impl crate::CliAgentStrategy for EndedStrategy {
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
            true
        }
    }

    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(EndedStrategy));
    app.insert_resource(strategies)
        .add_message::<AgentSessionExited>()
        .add_systems(Update, detect_file_end_time_exit);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            SessionId("x".into()),
        ))
        .id();
    app.update();
    assert!(app.world().get::<AgentSession>(entity).is_none());
    assert!(app.world().get::<SessionId>(entity).is_none());
}
