use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::{AgentKind, AgentVariant};

use crate::cli_trait::CliAgentStrategy;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    inner: HashMap<AgentKind, Box<dyn CliAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.inner.insert(strategy.kind(), strategy);
    }

    pub fn get(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.inner.get(&kind).map(|b| b.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AgentKind, &dyn CliAgentStrategy)> {
        self.inner.iter().map(|(k, v)| (k, v.as_ref()))
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
    fn register_and_lookup_by_kind() {
        let mut s = AgentStrategies::default();
        s.register(Box::new(StubStrategy));
        assert!(s.get(AgentKind::Claude).is_some());
        assert!(s.get(AgentKind::Vibe).is_none());
    }
}
