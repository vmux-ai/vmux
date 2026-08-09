use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;

use crate::AgentVariant;
use crate::client::acp::AcpSession;
use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue};
use crate::events::{AgentApprovalRequest, AgentDelta};
use crate::handoff::{ImportedConversation, PendingHandoff, sanitize_replayed_messages};
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::run_state_kind::LastRunStateKind;
use crate::systems::{approval, surface_errors};
use crate::toast::AgentToast;
use crate::tools::mcp_tool_defs;
use vmux_service::agent_events::{
    PageAgentApprovalResolved, PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus,
    PageAgentSnapshot,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentRunStatus, ClientMessage, SharedMessage};

impl Plugin for PageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(approval::AgentApprovalStore::load())
            .register_type::<AgentSession>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
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
                    ensure_prompt_queue,
                    spawn_page_session_on_add,
                    send_page_agent_input,
                    approval::sync_persisted_acp_approval_policy.before(consume_page_agent_stream),
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

pub struct PageAgentPlugin;

fn attach_last_run_state_kind(
    mut commands: Commands,
    q: Query<
        Entity,
        (
            Or<(With<AgentSession>, With<AcpSession>)>,
            Without<LastRunStateKind>,
        ),
    >,
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
        service.0.send(ClientMessage::Shared(SharedMessage::agent(
            session.sid.clone(),
            vmux_wire::protocol::AgentAction::Attach,
        )));
    }
}

fn send_page_agent_input(
    mut q: Query<(&AgentSession, &mut AgentRunState, &mut PromptQueue)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (session, mut state, mut queue) in &mut q {
        if session.variant != AgentVariant::Page {
            continue;
        }
        if !queue.ready(matches!(*state, AgentRunState::Idle)) {
            continue;
        }
        let Some(prompt) = queue.take_next() else {
            continue;
        };
        service.0.send(ClientMessage::agent_input(
            session.sid.clone(),
            prompt.text,
            None,
            prompt.attachments,
        ));
        *state = AgentRunState::Streaming;
    }
}

fn ensure_prompt_queue(
    mut commands: Commands,
    q: Query<
        Entity,
        (
            Or<(Added<AcpSession>, Added<AgentSession>)>,
            Without<PromptQueue>,
        ),
    >,
) {
    for entity in &q {
        commands.entity(entity).insert(PromptQueue::default());
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
    mut resolved_approvals: MessageReader<PageAgentApprovalResolved>,
    mut snapshots: MessageReader<PageAgentSnapshot>,
    mut q: Query<(
        Entity,
        &mut AgentMessages,
        &mut AgentRunState,
        &mut PromptQueue,
        Option<&AgentSession>,
        Option<&AcpSession>,
        Option<&AgentApprovalPolicy>,
        Option<&mut PendingHandoff>,
        Option<&ImportedConversation>,
    )>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let by_sid: std::collections::HashMap<String, Entity> = q
        .iter()
        .filter_map(|(e, _, _, _, page, acp, _, _, _)| {
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
            && let Ok((_, mut messages, _, _, _, _, _, _, imported)) = q.get_mut(entity)
            && let Ok(mut parsed) = serde_json::from_str::<Vec<Message>>(&snapshot.messages_json)
        {
            sanitize_replayed_messages(
                &mut parsed,
                imported.and_then(|imported| imported.first_prompt.as_deref()),
            );
            messages.0 = parsed;
        }
    }
    for status in statuses.read() {
        if let Some(&entity) = by_sid.get(&status.sid)
            && let Ok((_, _, mut state, mut queue, _, _, _, mut pending, _)) = q.get_mut(entity)
        {
            let was_streaming = matches!(*state, AgentRunState::Streaming);
            match &status.status {
                AgentRunStatus::Idle => {
                    *state = AgentRunState::Idle;
                    if pending.as_deref().is_some_and(|pending| pending.sent) {
                        commands.entity(entity).remove::<PendingHandoff>();
                    }
                }
                AgentRunStatus::Streaming => *state = AgentRunState::Streaming,
                AgentRunStatus::Interrupted => {
                    *state = AgentRunState::Idle;
                    if !queue.flush_pending() {
                        queue.paused = true;
                    }
                }
                AgentRunStatus::Errored(message) => {
                    if queue.flush_pending() {
                        *state = AgentRunState::Idle;
                    } else {
                        *state = AgentRunState::Errored(message.clone());
                    }
                    if let Some(pending) = pending.as_deref_mut() {
                        pending.retry();
                    }
                }
            }
            // A run settling from Streaming back to Idle means the agent finished its turn.
            // Raise attention so the done-dot + OS notification fire (mark_agent_done gates on
            // whether the pane is viewed). Covers ACP and Page agents; an Interrupt (a distinct
            // status) is not completion, so it does not raise attention.
            if was_streaming && matches!(status.status, AgentRunStatus::Idle) {
                attention.write(vmux_core::notify::AgentAttention {
                    entity,
                    title: None,
                    body: None,
                });
            }
        }
    }
    for approval in approvals.read() {
        let Some(&entity) = by_sid.get(&approval.sid) else {
            continue;
        };
        let args: serde_json::Value =
            serde_json::from_str(&approval.args_json).unwrap_or_else(|_| serde_json::json!({}));
        if let Ok((_, _, mut state, _, _, acp, policy, _, _)) = q.get_mut(entity) {
            let auto_allowed = service.is_some()
                && acp.is_some()
                && policy.is_some_and(|policy| policy.allows(&approval.name));
            if !auto_allowed {
                *state = AgentRunState::AwaitingApproval {
                    call_id: approval.call_id.clone(),
                    name: approval.name.clone(),
                    args: args.clone(),
                };
            }
        }
        commands.trigger(AgentApprovalRequest {
            session: entity,
            call_id: approval.call_id.clone(),
            name: approval.name.clone(),
            args,
        });
    }
    for resolved in resolved_approvals.read() {
        let Some(&entity) = by_sid.get(&resolved.sid) else {
            continue;
        };
        if let Ok((_, _, mut state, _, _, _, _, _, _)) = q.get_mut(entity)
            && matches!(
                &*state,
                AgentRunState::AwaitingApproval { call_id, .. }
                    if call_id.as_str() == resolved.call_id.as_str()
            )
        {
            *state = AgentRunState::Streaming;
        }
    }
}

#[cfg(test)]
#[path = "plugin.test.rs"]
mod tests;
