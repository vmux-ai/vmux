use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::app::AppAgentStrategy;
use crate::cli_trait::CliAgentStrategy;
use crate::{AgentKind, AgentVariant};

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

pub enum BoxedStrategy {
    Cli(Box<dyn CliAgentStrategy>),
    App(Box<dyn AppAgentStrategy>),
}

impl BoxedStrategy {
    pub fn kind(&self) -> AgentKind {
        match self {
            BoxedStrategy::Cli(s) => s.kind(),
            BoxedStrategy::App(s) => s.kind(),
        }
    }

    pub fn variant(&self) -> AgentVariant {
        match self {
            BoxedStrategy::Cli(s) => s.variant(),
            BoxedStrategy::App(s) => s.variant(),
        }
    }

    pub fn as_cli(&self) -> Option<&dyn CliAgentStrategy> {
        match self {
            BoxedStrategy::Cli(s) => Some(s.as_ref()),
            BoxedStrategy::App(_) => None,
        }
    }

    pub fn as_app(&self) -> Option<&dyn AppAgentStrategy> {
        match self {
            BoxedStrategy::App(s) => Some(s.as_ref()),
            BoxedStrategy::Cli(_) => None,
        }
    }
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    inner: HashMap<(AgentKind, AgentVariant), BoxedStrategy>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        let key = (strategy.kind(), strategy.variant());
        self.inner.insert(key, BoxedStrategy::Cli(strategy));
    }

    pub fn get(&self, kind: AgentKind, variant: AgentVariant) -> Option<&BoxedStrategy> {
        self.inner.get(&(kind, variant))
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.get(kind, AgentVariant::Cli)
            .and_then(BoxedStrategy::as_cli)
    }

    pub fn register_app(&mut self, strategy: Box<dyn AppAgentStrategy>) {
        let key = (strategy.kind(), strategy.variant());
        self.inner.insert(key, BoxedStrategy::App(strategy));
    }

    pub fn get_app(&self, kind: AgentKind) -> Option<&dyn AppAgentStrategy> {
        self.get(kind, AgentVariant::App)
            .and_then(BoxedStrategy::as_app)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(AgentKind, AgentVariant), &BoxedStrategy)> {
        self.inner.iter()
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
        s.register_cli(Box::new(StubStrategy));
        assert!(s.get(AgentKind::Claude, AgentVariant::Cli).is_some());
        assert!(s.get(AgentKind::Vibe, AgentVariant::Cli).is_none());
    }

    #[test]
    fn registers_cli_for_kind_with_variant() {
        let mut s = AgentStrategies::default();
        s.register_cli(Box::new(StubStrategy));
        assert!(s.get(AgentKind::Claude, AgentVariant::Cli).is_some());
        assert!(s.get(AgentKind::Claude, AgentVariant::App).is_none());
        assert!(s.get_cli(AgentKind::Claude).is_some());
    }

    #[test]
    fn registers_cli_and_app_independently_for_same_kind() {
        struct StubApp;
        impl AgentStrategy for StubApp {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> AgentVariant {
                AgentVariant::App
            }
        }
        impl crate::app::AppAgentStrategy for StubApp {
            fn models(&self) -> &'static [&'static str] {
                &[]
            }
            fn default_model(&self) -> &'static str {
                ""
            }
            fn endpoint(&self) -> &'static str {
                "stub://"
            }
            fn build_request(
                &self,
                _: &str,
                _: &[crate::message::Message],
                _: &[crate::stream::ToolDef],
                _: &str,
            ) -> reqwest::Request {
                reqwest::Client::new()
                    .get("http://localhost/")
                    .build()
                    .unwrap()
            }
            fn parse_sse_event(&self, _: &str) -> Option<crate::stream::StreamEvent> {
                None
            }
        }

        struct StubCli;
        impl AgentStrategy for StubCli {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> AgentVariant {
                AgentVariant::Cli
            }
        }
        impl CliAgentStrategy for StubCli {
            fn sessions_root(&self) -> PathBuf {
                PathBuf::from("/tmp/none")
            }
            fn build_args(&self, _: &McpServerConfig, _: Option<&str>) -> Vec<String> {
                vec![]
            }
            fn build_env(&self, _: &McpServerConfig) -> Vec<(String, String)> {
                vec![]
            }
            fn discover_session(
                &self,
                _: &Path,
                _: SystemTime,
                _: &HashSet<String>,
            ) -> Option<String> {
                None
            }
            fn detect_end_time(&self, _: &str) -> bool {
                false
            }
        }

        let mut s = AgentStrategies::default();
        s.register_cli(Box::new(StubCli));
        s.register_app(Box::new(StubApp));

        assert!(s.get(AgentKind::Vibe, AgentVariant::Cli).is_some());
        assert!(s.get(AgentKind::Vibe, AgentVariant::App).is_some());
        assert!(s.get_cli(AgentKind::Vibe).is_some());
        assert!(s.get_app(AgentKind::Vibe).is_some());
    }
}
