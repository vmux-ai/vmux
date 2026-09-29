use bevy::prelude::*;
use vmux_setting::SettingsLoadSet;

use crate::runtime::strategy::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::{AgentKind, AgentVariant};

pub struct MistralPlugin;

impl Plugin for MistralPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mistral_strategy.after(SettingsLoadSet));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MistralProvider;

fn register_mistral_strategy(
    mut commands: Commands,
    strategies: Query<&StrategyKey, With<Strategy>>,
) {
    if std::env::var(crate::provider::mistral::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: crate::provider::mistral::PROVIDER.to_string(),
        model: crate::provider::mistral::DEFAULT_MODEL.to_string(),
    };
    if strategies.iter().any(|registered| registered == &key) {
        return;
    }
    commands.spawn((
        Strategy,
        MistralProvider,
        key,
        Endpoint(crate::provider::mistral::ENDPOINT.to_string()),
        EnvVarName(crate::provider::mistral::ENV_VAR),
        StrategyKind(AgentKind::Vibe),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(crate::provider::mistral::build_request),
        ParseSseFn(crate::provider::openai_shared::parse_chat_completions_sse),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MistralPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(crate::provider::mistral::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<(&StrategyKey, &MistralProvider)>()
            .iter(app.world())
            .filter(|(key, _)| key.provider == "mistral" && key.model == "devstral-2")
            .count();
        assert_eq!(count, 1);
        unsafe { std::env::remove_var(crate::provider::mistral::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(crate::provider::mistral::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let count = app
            .world_mut()
            .query::<&MistralProvider>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0);
    }
}
