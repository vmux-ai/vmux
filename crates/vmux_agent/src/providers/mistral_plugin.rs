use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, Copy)]
pub struct MistralProvider;

pub struct MistralPlugin;

impl Plugin for MistralPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mistral_strategy.after(SettingsLoadSet));
    }
}

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
mod tests {
    use super::*;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app.add_plugins(MistralPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(super::super::mistral::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("mistral", "devstral-2").is_some());
        unsafe { std::env::remove_var(super::super::mistral::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(super::super::mistral::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("mistral", "devstral-2").is_none());
    }
}
