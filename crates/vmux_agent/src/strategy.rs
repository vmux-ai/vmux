use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::AgentKind;
use crate::AgentVariant;
use crate::app::AppAgentStrategy;
use crate::cli_trait::CliAgentStrategy;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    cli: HashMap<AgentKind, Box<dyn CliAgentStrategy>>,
    app: HashMap<(String, String), Box<dyn AppAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.cli.insert(strategy.kind(), strategy);
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.cli.get(&kind).map(|b| b.as_ref())
    }

    pub fn register_app(&mut self, strategy: Box<dyn AppAgentStrategy>) {
        let key = (
            strategy.provider().to_string(),
            strategy.model().to_string(),
        );
        self.app.insert(key, strategy);
    }

    pub fn get_app_by_provider_model(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<&dyn AppAgentStrategy> {
        self.app
            .get(&(provider.to_string(), model.to_string()))
            .map(|b| b.as_ref())
    }

    pub fn app_strategies(&self) -> impl Iterator<Item = &dyn AppAgentStrategy> {
        self.app.values().map(|b| b.as_ref())
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

    #[test]
    fn register_app_lookup_by_provider_model() {
        struct StubApp {
            provider: String,
            model: String,
        }
        impl AgentStrategy for StubApp {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> AgentVariant {
                AgentVariant::App
            }
        }
        impl crate::app::AppAgentStrategy for StubApp {
            fn provider(&self) -> &str {
                &self.provider
            }
            fn model(&self) -> &str {
                &self.model
            }
            fn endpoint(&self) -> &str {
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

        let mut s = AgentStrategies::default();
        s.register_app(Box::new(StubApp {
            provider: "openai".into(),
            model: "gpt-5.5".into(),
        }));
        s.register_app(Box::new(StubApp {
            provider: "anthropic".into(),
            model: "claude-opus-4.7".into(),
        }));

        assert!(s.get_app_by_provider_model("openai", "gpt-5.5").is_some());
        assert!(
            s.get_app_by_provider_model("anthropic", "claude-opus-4.7")
                .is_some()
        );
        assert!(s.get_app_by_provider_model("nope", "nope").is_none());
        assert_eq!(s.app_strategies().count(), 2);
    }

    #[test]
    fn cli_and_app_coexist() {
        struct App;
        impl AgentStrategy for App {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> AgentVariant {
                AgentVariant::App
            }
        }
        impl crate::app::AppAgentStrategy for App {
            fn provider(&self) -> &str {
                "p"
            }
            fn model(&self) -> &str {
                "m"
            }
            fn endpoint(&self) -> &str {
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
        let mut s = AgentStrategies::default();
        s.register_cli(Box::new(StubStrategy));
        s.register_app(Box::new(App));
        assert!(s.get_cli(AgentKind::Claude).is_some());
        assert!(s.get_app_by_provider_model("p", "m").is_some());
    }
}
