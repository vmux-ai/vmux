use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::runtime::strategy::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::{AgentKind, AgentVariant};

pub struct AnthropicPlugin;

impl Plugin for AnthropicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_anthropic_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AnthropicProvider;

fn register_anthropic_strategy(
    mut commands: Commands,
    strategies: Query<&StrategyKey, With<Strategy>>,
) {
    if std::env::var(super::anthropic::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: super::anthropic::PROVIDER.to_string(),
        model: super::anthropic::DEFAULT_MODEL.to_string(),
    };
    if strategies.iter().any(|registered| registered == &key) {
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
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(AnthropicPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(super::super::anthropic::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<(&StrategyKey, &AnthropicProvider)>()
            .iter(app.world())
            .filter(|(key, _)| key.provider == "anthropic" && key.model == "claude-sonnet-4-6")
            .count();
        assert_eq!(count, 1);
        unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(super::super::anthropic::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<&AnthropicProvider>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0);
    }
}
