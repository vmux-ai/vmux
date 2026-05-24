use bevy::prelude::*;

use crate::message::Message;
use crate::stream::{StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Strategy;

#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StrategyKey {
    pub provider: String,
    pub model: String,
}

#[derive(Component, Debug, Clone)]
pub struct Endpoint(pub String);

#[derive(Component, Debug, Clone, Copy)]
pub struct EnvVarName(pub &'static str);

#[derive(Component, Debug, Clone, Copy)]
pub struct StrategyKind(pub AgentKind);

#[derive(Component, Debug, Clone, Copy)]
pub struct StrategyVariant(pub AgentVariant);

pub type BuildRequest =
    fn(model: &str, messages: &[Message], tools: &[ToolDef], api_key: &str) -> reqwest::Request;

pub type ParseSse = fn(payload: &str) -> Option<StreamEvent>;

#[derive(Component, Clone, Copy)]
pub struct BuildRequestFn(pub BuildRequest);

#[derive(Component, Clone, Copy)]
pub struct ParseSseFn(pub ParseSse);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_key_equality_is_provider_then_model() {
        let a = StrategyKey {
            provider: "mistral".into(),
            model: "devstral-2".into(),
        };
        let b = StrategyKey {
            provider: "mistral".into(),
            model: "devstral-2".into(),
        };
        let c = StrategyKey {
            provider: "mistral".into(),
            model: "other".into(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
