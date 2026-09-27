use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::echo;
use crate::runtime::provider::strategy::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::{AgentKind, AgentVariant};

pub struct EchoPlugin;

impl Plugin for EchoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_echo_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct EchoProvider;

fn register_echo_strategy(mut commands: Commands, strategies: Query<&StrategyKey, With<Strategy>>) {
    let key = StrategyKey {
        provider: echo::PROVIDER.to_string(),
        model: echo::DEFAULT_MODEL.to_string(),
    };
    if strategies.iter().any(|registered| registered == &key) {
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

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(EchoPlugin);
        app
    }

    #[test]
    fn spawns_echo_entity_without_any_env_var() {
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<(&StrategyKey, &EchoProvider)>()
            .iter(app.world())
            .filter(|(key, _)| key.provider == "echo" && key.model == "echo")
            .count();
        assert_eq!(count, 1);
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
    }
}
