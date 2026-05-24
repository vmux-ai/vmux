use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::AgentKind;
use crate::AgentVariant;
use crate::client::cli::strategy::CliAgentStrategy;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    cli: HashMap<AgentKind, Box<dyn CliAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.cli.insert(strategy.kind(), strategy);
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.cli.get(&kind).map(|b| b.as_ref())
    }

    pub fn cli_strategies(&self) -> impl Iterator<Item = &dyn CliAgentStrategy> {
        self.cli.values().map(|b| b.as_ref())
    }
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
}
