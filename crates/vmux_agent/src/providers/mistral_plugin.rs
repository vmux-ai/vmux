use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

pub struct MistralPlugin;

impl Plugin for MistralPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mistral_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MistralProvider;

fn register_mistral_strategy(mut commands: Commands, idx: Option<Res<PageStrategyIndex>>) {
    if std::env::var(super::mistral::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: super::mistral::PROVIDER.to_string(),
        model: super::mistral::DEFAULT_MODEL.to_string(),
    };
    if let Some(idx) = idx.as_deref()
        && idx.get(&key).is_some()
    {
        return;
    }
    commands.spawn((
        Strategy,
        MistralProvider,
        key,
        Endpoint(super::mistral::ENDPOINT.to_string()),
        EnvVarName(super::mistral::ENV_VAR),
        StrategyKind(AgentKind::Vibe),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(super::mistral::build_request),
        ParseSseFn(super::mistral::parse_sse),
    ));
}

#[cfg(test)]
#[path = "mistral_plugin.test.rs"]
mod tests;
