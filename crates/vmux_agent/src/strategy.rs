use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::cli_trait::CliAgentStrategy;
use crate::gui::GuiAgentStrategy;
use crate::{AgentKind, AgentVariant};

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

pub enum BoxedStrategy {
    Cli(Box<dyn CliAgentStrategy>),
    Gui(Box<dyn GuiAgentStrategy>),
}

impl BoxedStrategy {
    pub fn kind(&self) -> AgentKind {
        match self {
            BoxedStrategy::Cli(s) => s.kind(),
            BoxedStrategy::Gui(s) => s.kind(),
        }
    }

    pub fn variant(&self) -> AgentVariant {
        match self {
            BoxedStrategy::Cli(s) => s.variant(),
            BoxedStrategy::Gui(s) => s.variant(),
        }
    }

    pub fn as_cli(&self) -> Option<&dyn CliAgentStrategy> {
        match self {
            BoxedStrategy::Cli(s) => Some(s.as_ref()),
            BoxedStrategy::Gui(_) => None,
        }
    }

    pub fn as_gui(&self) -> Option<&dyn GuiAgentStrategy> {
        match self {
            BoxedStrategy::Gui(s) => Some(s.as_ref()),
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

    pub fn register_gui(&mut self, strategy: Box<dyn GuiAgentStrategy>) {
        let key = (strategy.kind(), strategy.variant());
        self.inner.insert(key, BoxedStrategy::Gui(strategy));
    }

    pub fn get_gui(&self, kind: AgentKind) -> Option<&dyn GuiAgentStrategy> {
        self.get(kind, AgentVariant::Gui)
            .and_then(BoxedStrategy::as_gui)
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
        assert!(s.get(AgentKind::Claude, AgentVariant::Gui).is_none());
        assert!(s.get_cli(AgentKind::Claude).is_some());
    }

    #[test]
    fn registers_cli_and_gui_independently_for_same_kind() {
        struct StubGui;
        impl AgentStrategy for StubGui {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> AgentVariant {
                AgentVariant::Gui
            }
        }
        impl crate::gui::GuiAgentStrategy for StubGui {
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
        s.register_gui(Box::new(StubGui));

        assert!(s.get(AgentKind::Vibe, AgentVariant::Cli).is_some());
        assert!(s.get(AgentKind::Vibe, AgentVariant::Gui).is_some());
        assert!(s.get_cli(AgentKind::Vibe).is_some());
        assert!(s.get_gui(AgentKind::Vibe).is_some());
    }
}
