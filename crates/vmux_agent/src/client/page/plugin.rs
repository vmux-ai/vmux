use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;

use crate::AgentVariant;
use crate::client::acp::AcpSession;
use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
use crate::events::{AgentApprovalRequest, AgentDelta};
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::run_state_kind::LastRunStateKind;
use crate::systems::{approval, surface_errors};
use crate::toast::AgentToast;
use crate::tools::mcp_tool_defs;
use vmux_service::agent_events::{
    PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentRunStatus, ClientMessage};

pub struct PageAgentPlugin;

impl Plugin for PageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentSnapshot>()
            .add_plugins(BinEventEmitterPlugin::<(AgentToast,)>::with_id(
                "vmux-agent-toast",
            ))
            .add_plugins(BinEventEmitterPlugin::<(
                vmux_core::event::FileTidyActionEvent,
            )>::default())
            .add_observer(crate::plugin::on_tidy_action)
            .add_observer(approval::handle_approval_reply)
            .add_observer(close_page_session_on_remove)
            .add_systems(
                Update,
                (
                    spawn_page_session_on_add,
                    send_page_agent_input,
                    consume_page_agent_stream,
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

fn close_page_session_on_remove(
    trigger: On<Remove, AgentSession>,
    sessions: Query<&AgentSession>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    if session.variant != AgentVariant::Page {
        return;
    }
    service.0.send(ClientMessage::DetachPageAgent {
        sid: session.sid.clone(),
    });
    service.0.send(ClientMessage::ClosePageAgent {
        sid: session.sid.clone(),
    });
}

#[allow(clippy::type_complexity)]
fn consume_page_agent_stream(
    mut deltas: MessageReader<PageAgentDelta>,
    mut statuses: MessageReader<PageAgentRunStatus>,
    mut approvals: MessageReader<PageAgentAwaitingApproval>,
    mut snapshots: MessageReader<PageAgentSnapshot>,
    mut q: Query<(
        Entity,
        &mut AgentMessages,
        &mut AgentRunState,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
    mut commands: Commands,
) {
    let by_sid: std::collections::HashMap<String, Entity> = q
        .iter()
        .filter_map(|(e, _, _, page, acp)| {
            let sid = page
                .map(|s| s.sid.clone())
                .or_else(|| acp.map(|s| s.sid.clone()))?;
            Some((sid, e))
        })
        .collect();

    for delta in deltas.read() {
        if let Some(&entity) = by_sid.get(&delta.sid) {
            commands.trigger(AgentDelta {
                session: entity,
                text: delta.text.clone(),
            });
        }
    }
    for snapshot in snapshots.read() {
        if let Some(&entity) = by_sid.get(&snapshot.sid)
            && let Ok((_, mut messages, _, _, _)) = q.get_mut(entity)
            && let Ok(parsed) = serde_json::from_str::<Vec<Message>>(&snapshot.messages_json)
        {
            messages.0 = parsed;
        }
    }
    for status in statuses.read() {
        if let Some(&entity) = by_sid.get(&status.sid)
            && let Ok((_, _, mut state, _, _)) = q.get_mut(entity)
        {
            *state = match &status.status {
                AgentRunStatus::Idle => AgentRunState::Idle,
                AgentRunStatus::Streaming => AgentRunState::Streaming,
                AgentRunStatus::Errored(message) => AgentRunState::Errored(message.clone()),
            };
        }
    }
    for approval in approvals.read() {
        let Some(&entity) = by_sid.get(&approval.sid) else {
            continue;
        };
        let args: serde_json::Value =
            serde_json::from_str(&approval.args_json).unwrap_or_else(|_| serde_json::json!({}));
        if let Ok((_, _, mut state, _, _)) = q.get_mut(entity) {
            *state = AgentRunState::AwaitingApproval {
                call_id: approval.call_id.clone(),
                name: approval.name.clone(),
                args: args.clone(),
            };
        }
        commands.trigger(AgentApprovalRequest {
            session: entity,
            call_id: approval.call_id.clone(),
            name: approval.name.clone(),
            args,
        });
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
