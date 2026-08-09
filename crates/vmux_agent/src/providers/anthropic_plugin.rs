use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

pub struct AnthropicPlugin;

impl Plugin for AnthropicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_anthropic_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AnthropicProvider;

fn register_anthropic_strategy(mut commands: Commands, idx: Option<Res<PageStrategyIndex>>) {
    if std::env::var(super::anthropic::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: super::anthropic::PROVIDER.to_string(),
        model: super::anthropic::DEFAULT_MODEL.to_string(),
    };
    if let Some(idx) = idx.as_deref()
        && idx.get(&key).is_some()
    {
        return;
    }
    commands.spawn((
        Strategy,
        AnthropicProvider,
        key,
        Endpoint(super::anthropic::ENDPOINT.to_string()),
        EnvVarName(super::anthropic::ENV_VAR),
        StrategyKind(AgentKind::Claude),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(super::anthropic::build_request),
        ParseSseFn(super::anthropic::parse_sse),
    ));
}

#[cfg(test)]
#[path = "anthropic_plugin.test.rs"]
mod tests;
