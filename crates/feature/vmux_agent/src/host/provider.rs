use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_core::Ready;
use vmux_core::agent::{AgentKind, AgentProviderTargetKind};
use vmux_setting::SettingsLoadSet;

use crate::AgentVariant;
use crate::runtime::strategy::{
    BuildRequest, BuildRequestFn, Endpoint, EnvVarName, ParseSse, ParseSseFn, Strategy,
    StrategyKey, StrategyKind, StrategyVariant,
};

pub(super) struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_builtin_agent_providers,
                detect_agent_provider_availability,
            )
                .chain(),
        )
        .add_systems(Startup, register_page_strategies.after(SettingsLoadSet));
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BuiltinProvider {
    pub provider: &'static str,
    pub kind: AgentKind,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const ECHO_DEFAULT: BuiltinProvider = BuiltinProvider {
    provider: "echo",
    kind: AgentKind::Vibe,
    default_model: "echo",
    env_var: "",
};

pub const BUILTIN_PROVIDERS: &[BuiltinProvider] = &[
    BuiltinProvider {
        provider: "mistral",
        kind: AgentKind::Vibe,
        default_model: "devstral-2",
        env_var: "MISTRAL_API_KEY",
    },
    BuiltinProvider {
        provider: "anthropic",
        kind: AgentKind::Claude,
        default_model: "claude-sonnet-4-6",
        env_var: "ANTHROPIC_API_KEY",
    },
    BuiltinProvider {
        provider: "openai",
        kind: AgentKind::Codex,
        default_model: "gpt-5",
        env_var: "OPENAI_API_KEY",
    },
];

pub fn resolve_default_app_provider() -> Option<&'static BuiltinProvider> {
    BUILTIN_PROVIDERS
        .iter()
        .find(|provider| std::env::var(provider.env_var).is_ok())
        .or(Some(&ECHO_DEFAULT))
}

const BUILTIN_AGENT_PROVIDERS: &[AgentKind] =
    &[AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex];

#[derive(Component, Clone, Default)]
pub(crate) struct AgentExecutableOverride(pub std::collections::HashMap<AgentKind, bool>);

#[derive(SystemParam)]
pub(crate) struct AgentExecutables<'w, 's> {
    override_: Option<Single<'w, 's, &'static AgentExecutableOverride>>,
}

impl AgentExecutables<'_, '_> {
    pub(crate) fn resolve(&self, kind: AgentKind) -> Option<PathBuf> {
        if let Some(forced) = self
            .override_
            .as_deref()
            .and_then(|override_| override_.0.get(&kind).copied())
        {
            return forced.then(|| PathBuf::from(kind.executable()));
        }
        crate::exec::find_executable(kind.executable())
    }
}

fn spawn_builtin_agent_providers(mut commands: Commands) {
    for kind in BUILTIN_AGENT_PROVIDERS {
        commands.spawn((
            AgentProviderTargetKind(*kind),
            Name::new(kind.display_name()),
        ));
    }
}

fn detect_agent_provider_availability(
    mut commands: Commands,
    providers: Query<(Entity, &AgentProviderTargetKind), Without<Ready>>,
) {
    for (entity, kind) in &providers {
        if crate::exec::find_executable(kind.0.executable()).is_some() {
            commands.entity(entity).insert(Ready);
        }
    }
}

struct PageStrategyDefinition {
    provider: &'static str,
    model: &'static str,
    endpoint: &'static str,
    env_var: &'static str,
    kind: AgentKind,
    build_request: BuildRequest,
    parse_sse: ParseSse,
}

fn register_page_strategies(
    mut commands: Commands,
    strategies: Query<&StrategyKey, With<Strategy>>,
) {
    let definitions: &[PageStrategyDefinition] = &[
        PageStrategyDefinition {
            provider: crate::provider::anthropic::PROVIDER,
            model: crate::provider::anthropic::DEFAULT_MODEL,
            endpoint: crate::provider::anthropic::ENDPOINT,
            env_var: crate::provider::anthropic::ENV_VAR,
            kind: AgentKind::Claude,
            build_request: crate::provider::anthropic::build_request,
            parse_sse: crate::provider::anthropic::parse_messages_sse,
        },
        PageStrategyDefinition {
            provider: crate::provider::mistral::PROVIDER,
            model: crate::provider::mistral::DEFAULT_MODEL,
            endpoint: crate::provider::mistral::ENDPOINT,
            env_var: crate::provider::mistral::ENV_VAR,
            kind: AgentKind::Vibe,
            build_request: crate::provider::mistral::build_request,
            parse_sse: crate::provider::openai_shared::parse_chat_completions_sse,
        },
        PageStrategyDefinition {
            provider: crate::provider::openai::PROVIDER,
            model: crate::provider::openai::DEFAULT_MODEL,
            endpoint: crate::provider::openai::ENDPOINT,
            env_var: crate::provider::openai::ENV_VAR,
            kind: AgentKind::Codex,
            build_request: crate::provider::openai::build_request,
            parse_sse: crate::provider::openai::parse_responses_sse,
        },
        PageStrategyDefinition {
            provider: crate::echo::PROVIDER,
            model: crate::echo::DEFAULT_MODEL,
            endpoint: crate::echo::ENDPOINT,
            env_var: crate::echo::ENV_VAR,
            kind: AgentKind::Vibe,
            build_request: crate::echo::build_request,
            parse_sse: crate::echo::parse_sse,
        },
    ];
    for definition in definitions {
        if !definition.env_var.is_empty() && std::env::var(definition.env_var).is_err() {
            continue;
        }
        let key = StrategyKey {
            provider: definition.provider.to_string(),
            model: definition.model.to_string(),
        };
        if strategies.iter().any(|registered| registered == &key) {
            continue;
        }
        commands.spawn((
            Strategy,
            key,
            Endpoint(definition.endpoint.to_string()),
            EnvVarName(definition.env_var),
            StrategyKind(definition.kind),
            StrategyVariant(AgentVariant::Page),
            BuildRequestFn(definition.build_request),
            ParseSseFn(definition.parse_sse),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn clear_all_keys() {
        for provider in BUILTIN_PROVIDERS {
            unsafe { std::env::remove_var(provider.env_var) };
        }
    }

    #[test]
    #[serial]
    fn priority_is_mistral_then_anthropic_then_openai() {
        clear_all_keys();
        unsafe { std::env::set_var("MISTRAL_API_KEY", "x") };
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        assert_eq!(resolve_default_app_provider().unwrap().provider, "mistral");
        clear_all_keys();
    }

    #[test]
    #[serial]
    fn anthropic_wins_when_mistral_absent() {
        clear_all_keys();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        assert_eq!(
            resolve_default_app_provider().unwrap().provider,
            "anthropic"
        );
        clear_all_keys();
    }

    #[test]
    #[serial]
    fn no_keys_returns_echo_fallback() {
        clear_all_keys();
        assert_eq!(resolve_default_app_provider().unwrap().provider, "echo");
    }

    #[test]
    #[serial]
    fn plugin_registers_available_page_strategies() {
        clear_all_keys();
        unsafe { std::env::set_var("MISTRAL_API_KEY", "x") };
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        let mut app = App::new();
        app.add_plugins(ProviderPlugin).update();
        let mut providers = app
            .world_mut()
            .query::<&StrategyKey>()
            .iter(app.world())
            .map(|key| key.provider.as_str())
            .collect::<Vec<_>>();
        providers.sort_unstable();
        assert_eq!(providers, ["anthropic", "echo", "mistral", "openai"]);
        clear_all_keys();
    }

    #[test]
    #[serial]
    fn plugin_registers_only_echo_without_credentials() {
        clear_all_keys();
        let mut app = App::new();
        app.add_plugins(ProviderPlugin).update();
        let providers = app
            .world_mut()
            .query::<&StrategyKey>()
            .iter(app.world())
            .map(|key| key.provider.as_str())
            .collect::<Vec<_>>();
        assert_eq!(providers, ["echo"]);
    }
}
