use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

pub struct OpenAiPlugin;

impl Plugin for OpenAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_openai_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct OpenAiProvider;

fn register_openai_strategy(mut commands: Commands, idx: Option<Res<PageStrategyIndex>>) {
    if std::env::var(super::openai::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: super::openai::PROVIDER.to_string(),
        model: super::openai::DEFAULT_MODEL.to_string(),
    };
    if let Some(idx) = idx.as_deref()
        && idx.get(&key).is_some()
    {
        return;
    }
    commands.spawn((
        Strategy,
        OpenAiProvider,
        key,
        Endpoint(super::openai::ENDPOINT.to_string()),
        EnvVarName(super::openai::ENV_VAR),
        StrategyKind(AgentKind::Codex),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(super::openai::build_request),
        ParseSseFn(super::openai::parse_sse),
    ));
}

#[cfg(test)]
#[path = "openai_plugin.test.rs"]
mod tests;
