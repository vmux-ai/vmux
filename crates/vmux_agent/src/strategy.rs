use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::Resource;

use crate::{AgentKind, McpServerConfig};

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    inner: HashMap<AgentKind, Box<dyn AgentStrategy>>,
}

impl AgentStrategies {
    pub fn register(&mut self, strategy: Box<dyn AgentStrategy>) {
        self.inner.insert(strategy.kind(), strategy);
    }

    pub fn get(&self, kind: AgentKind) -> Option<&dyn AgentStrategy> {
        self.inner.get(&kind).map(|b| b.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AgentKind, &dyn AgentStrategy)> {
        self.inner.iter().map(|(k, v)| (k, v.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubStrategy;
    impl AgentStrategy for StubStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Claude
        }
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
