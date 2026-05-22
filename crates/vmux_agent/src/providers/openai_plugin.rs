use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, Copy)]
pub struct OpenAiProvider;

pub struct OpenAiPlugin;

impl Plugin for OpenAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_openai_strategy.after(SettingsLoadSet));
    }
}

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
mod tests {
    use super::*;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app.add_plugins(OpenAiPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(super::super::openai::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("openai", "gpt-5").is_some());
        unsafe { std::env::remove_var(super::super::openai::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(super::super::openai::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("openai", "gpt-5").is_none());
    }
}
