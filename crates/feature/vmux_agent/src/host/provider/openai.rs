use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::runtime::strategy::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::{AgentKind, AgentVariant};

pub struct OpenAiPlugin;

impl Plugin for OpenAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_openai_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct OpenAiProvider;

fn register_openai_strategy(
    mut commands: Commands,
    strategies: Query<&StrategyKey, With<Strategy>>,
) {
    if std::env::var(crate::provider::openai::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: crate::provider::openai::PROVIDER.to_string(),
        model: crate::provider::openai::DEFAULT_MODEL.to_string(),
    };
    if strategies.iter().any(|registered| registered == &key) {
        return;
    }
    commands.spawn((
        Strategy,
        OpenAiProvider,
        key,
        Endpoint(crate::provider::openai::ENDPOINT.to_string()),
        EnvVarName(crate::provider::openai::ENV_VAR),
        StrategyKind(AgentKind::Codex),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(crate::provider::openai::build_request),
        ParseSseFn(crate::provider::openai::parse_responses_sse),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(OpenAiPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(crate::provider::openai::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<(&StrategyKey, &OpenAiProvider)>()
            .iter(app.world())
            .filter(|(key, _)| key.provider == "openai" && key.model == "gpt-5")
            .count();
        assert_eq!(count, 1);
        unsafe { std::env::remove_var(crate::provider::openai::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(crate::provider::openai::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<&OpenAiProvider>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0);
    }
}
