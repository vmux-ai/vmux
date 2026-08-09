use bevy::prelude::*;

use crate::{AgentKind, AgentVariant};

pub use crate::stream::{BuildRequest, ParseSse};

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

#[derive(Component, Clone, Copy)]
pub struct BuildRequestFn(pub BuildRequest);

#[derive(Component, Clone, Copy)]
pub struct ParseSseFn(pub ParseSse);

#[cfg(test)]
#[path = "strategy_components.test.rs"]
mod tests;
