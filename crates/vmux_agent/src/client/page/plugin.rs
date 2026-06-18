use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;

use crate::AgentVariant;
use crate::components::{AgentApprovalPolicy, AgentSession, PendingUserInput};
use crate::run_state_kind::LastRunStateKind;
use crate::systems::{approval, surface_errors};
use crate::toast::AgentToast;
use crate::tools::mcp_tool_defs;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;

pub struct PageAgentPlugin;

impl Plugin for PageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_plugins(BinEventEmitterPlugin::<(AgentToast,)>::with_id(
                "vmux-agent-toast",
            ))
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    spawn_page_session_on_add,
                    send_page_agent_input,
                    surface_errors::surface_errors,
                    attach_last_run_state_kind,
                ),
            );

        if app
            .world()
            .get_resource::<crate::client::page::strategy_index::PageStrategyIndex>()
            .is_none()
        {
            app.insert_resource(crate::client::page::strategy_index::PageStrategyIndex::default());
        }
        app.add_observer(crate::client::page::strategy_indexer::on_strategy_added)
            .add_observer(crate::client::page::strategy_indexer::on_strategy_removed)
            .add_plugins(crate::providers::anthropic_plugin::AnthropicPlugin)
            .add_plugins(crate::providers::mistral_plugin::MistralPlugin)
            .add_plugins(crate::providers::openai_plugin::OpenAiPlugin)
            .add_plugins(crate::echo_plugin::EchoPlugin);
    }
}

fn attach_last_run_state_kind(
    mut commands: Commands,
    q: Query<Entity, (With<AgentSession>, Without<LastRunStateKind>)>,
) {
    for entity in &q {
        commands.entity(entity).insert(LastRunStateKind::default());
    }
}

fn spawn_page_session_on_add(
    q: Query<(&AgentSession, Option<&AgentApprovalPolicy>), Added<AgentSession>>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (session, policy) in &q {
        if session.variant != AgentVariant::Page {
            continue;
        }
        let auto_tools: Vec<String> = policy
            .map(|p| p.auto.iter().cloned().collect())
            .unwrap_or_default();
        let tools_json =
            serde_json::to_string(&mcp_tool_defs()).unwrap_or_else(|_| "[]".to_string());
        service.0.send(ClientMessage::SpawnPageAgent {
            sid: session.sid.clone(),
            provider: session.provider.clone(),
            model: session.model.clone(),
            cwd: String::new(),
            auto_tools,
            tools_json,
        });
        service.0.send(ClientMessage::AttachPageAgent {
            sid: session.sid.clone(),
        });
    }
}

fn send_page_agent_input(
    mut commands: Commands,
    q: Query<(Entity, &AgentSession, &PendingUserInput)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (entity, session, pending) in &q {
        if session.variant != AgentVariant::Page {
            continue;
        }
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text: pending.0.clone(),
        });
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::BinIpcEventRawBuffer;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .init_resource::<BinIpcEventRawBuffer>()
            .add_plugins(PageAgentPlugin);
        app.update();
    }
}
