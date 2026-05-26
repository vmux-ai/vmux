use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::echo;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, Copy)]
pub struct EchoProvider;

pub struct EchoPlugin;

impl Plugin for EchoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_echo_strategy.after(SettingsLoadSet));
    }
}

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
mod tests {
    use super::*;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default())
            .add_observer(on_strategy_added)
            .add_observer(on_strategy_removed)
            .add_plugins(EchoPlugin);
        app
    }

    #[test]
    fn spawns_echo_entity_without_any_env_var() {
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("echo", "echo").is_some());
    }

    #[test]
    fn dedup_guard_does_not_double_spawn() {
        let mut app = test_app();
        app.update();
        app.update();
        let count = app
            .world_mut()
            .query::<&EchoProvider>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get_by_strs("echo", "echo").is_some());
    }
}
