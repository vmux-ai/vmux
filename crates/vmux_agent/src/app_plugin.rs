use bevy::prelude::*;
use vmux_setting::{AppSettings, SettingsLoadSet};

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::echo::EchoAppStrategy;
use crate::strategy::AgentStrategies;
use crate::systems::{approval, dispatch_tool, drain_stream, process_input};

pub struct AppAgentPlugin;

impl Plugin for AppAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                ),
            )
            .add_systems(
                Startup,
                register_app_agents_from_settings.after(SettingsLoadSet),
            );

        if app.world().get_resource::<AgentStrategies>().is_none() {
            app.insert_resource(AgentStrategies::default());
        }
    }
}

fn register_app_agents_from_settings(
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
            strategies.register_app(Box::new(EchoAppStrategy::new(
                provider_settings.provider.clone(),
                model.clone(),
                kind,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), AppAgentPlugin));
        app.update();
    }
}
