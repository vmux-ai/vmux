use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::echo;
use crate::{AgentKind, AgentVariant};

pub struct EchoPlugin;

impl Plugin for EchoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_echo_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct EchoProvider;

fn register_echo_strategy(mut commands: Commands, idx: Option<Res<PageStrategyIndex>>) {
    let key = StrategyKey {
        provider: echo::PROVIDER.to_string(),
        model: echo::DEFAULT_MODEL.to_string(),
    };
    if let Some(idx) = idx.as_deref()
        && idx.get(&key).is_some()
    {
        return;
    }
    commands.spawn((
        Strategy,
        EchoProvider,
        key,
        Endpoint(echo::ENDPOINT.to_string()),
        EnvVarName(echo::ENV_VAR),
        StrategyKind(AgentKind::Vibe),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(echo::build_request),
        ParseSseFn(echo::parse_sse),
    ));
}

#[cfg(test)]
#[path = "echo_plugin.test.rs"]
mod tests;
