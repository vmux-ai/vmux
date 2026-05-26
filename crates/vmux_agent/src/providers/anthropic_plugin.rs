use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, Copy)]
pub struct AnthropicProvider;

pub struct AnthropicPlugin;

impl Plugin for AnthropicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_anthropic_strategy.after(SettingsLoadSet));
    }
}

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
mod tests {
    use super::*;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default())
            .add_observer(on_strategy_added)
            .add_observer(on_strategy_removed)
            .add_plugins(AnthropicPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(super::super::anthropic::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("anthropic", "claude-sonnet-4-6").is_some());
        unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("anthropic", "claude-sonnet-4-6").is_none());
    }
}
