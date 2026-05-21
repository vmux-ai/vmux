use std::sync::Arc;

use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_setting::{AppSettings, SettingsLoadSet};

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::echo::EchoPageStrategy;
use crate::providers::{BUILTIN_PROVIDERS, ECHO_DEFAULT, instantiate_builtin};
use crate::run_state_kind::LastRunStateKind;
use crate::strategy::AgentStrategies;
use crate::systems::{
    approval, continue_after_tool, dispatch_tool, drain_stream, process_input, surface_errors,
};
use crate::toast::AgentToast;

pub struct PageAgentPlugin;

impl Plugin for PageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_plugins(BinEventEmitterPlugin::<(AgentToast,)>::with_id(
                "vmux-agent-toast",
            ))
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                    continue_after_tool::continue_after_tool,
                    surface_errors::surface_errors,
                    attach_last_run_state_kind,
                ),
            )
            .add_systems(
                Startup,
                (
                    register_page_agents_from_settings,
                    register_builtin_providers,
                )
                    .after(SettingsLoadSet),
            );

        if app.world().get_resource::<AgentStrategies>().is_none() {
            app.insert_resource(AgentStrategies::default());
        }

        if app
            .world()
            .get_resource::<crate::client::page::strategy_index::PageStrategyIndex>()
            .is_none()
        {
            app.insert_resource(crate::client::page::strategy_index::PageStrategyIndex::default());
        }
        app.add_observer(crate::client::page::strategy_indexer::on_strategy_added);
        app.add_observer(crate::client::page::strategy_indexer::on_strategy_removed);
    }
}

fn register_page_agents_from_settings(
    settings: Option<Res<AppSettings>>,
    strategies: Option<ResMut<AgentStrategies>>,
) {
    let Some(settings) = settings else { return };
    let Some(mut strategies) = strategies else {
        return;
    };
    for provider_settings in &settings.agent.app_providers {
        let kind = match provider_settings.kind.as_str() {
            "vibe" => vmux_core::agent::AgentKind::Vibe,
            "claude" => vmux_core::agent::AgentKind::Claude,
            "codex" => vmux_core::agent::AgentKind::Codex,
            other => {
                bevy::log::warn!(
                    "agent.app_providers: unknown kind '{other}' for provider '{}'; defaulting to vibe",
                    provider_settings.provider
                );
                vmux_core::agent::AgentKind::Vibe
            }
        };
        for model in &provider_settings.models {
            strategies.register_page_if_absent(Arc::new(EchoPageStrategy::new(
                provider_settings.provider.clone(),
                model.clone(),
                kind,
            )));
        }
    }
}

fn register_builtin_providers(strategies: Option<ResMut<AgentStrategies>>) {
    let Some(mut strategies) = strategies else {
        return;
    };
    let mut registered: Vec<&'static str> = Vec::new();
    for p in BUILTIN_PROVIDERS {
        if std::env::var(p.env_var).is_err() {
            continue;
        }
        strategies.register_page_if_absent(instantiate_builtin(p, p.default_model));
        registered.push(p.provider);
    }
    strategies.register_page_if_absent(instantiate_builtin(
        &ECHO_DEFAULT,
        ECHO_DEFAULT.default_model,
    ));
    registered.push(ECHO_DEFAULT.provider);
    bevy::log::info!("registered built-in Page agent providers: {registered:?}");
}

fn attach_last_run_state_kind(
    mut commands: Commands,
    q: Query<Entity, (With<AgentSession>, Without<LastRunStateKind>)>,
) {
    for entity in &q {
        commands.entity(entity).insert(LastRunStateKind::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::BinIpcEventRawBuffer;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.init_resource::<BinIpcEventRawBuffer>();
        app.add_plugins(PageAgentPlugin);
        app.update();
    }
}
