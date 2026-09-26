pub mod index;
pub mod indexer;
pub mod strategy;

use bevy::prelude::*;

use crate::AgentVariant;
use crate::approval;
use crate::events::{AgentApprovalRequest, AgentDelta};
use crate::handoff::{ImportedConversation, PendingHandoff, sanitize_replayed_messages};
use crate::run_state::AgentRunState;
use crate::run_state_kind::LastRunStateKind;
use crate::toast::ToastPlugin;
use vmux_service::agent_events::{
    PageAgentApprovalResolved, PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus,
    PageAgentSnapshot,
};
use vmux_service::client::ServiceRequest;
use vmux_service::plugin::ServiceConnected;
use vmux_service::protocol::{AgentRunStatus, ClientMessage, SharedMessage};
use vmux_session::AcpSession;
use vmux_session::{
    AgentApprovalPolicy, AgentMessageTimes, AgentMessages, AgentSession, PromptQueue,
};

impl Plugin for ProviderAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>();
        if !app.is_plugin_added::<vmux_mcp::tool::ToolRuntimePlugin>() {
            app.add_plugins(vmux_mcp::tool::ToolRuntimePlugin);
        }
        app.register_type::<AgentSession>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_plugins(approval::ApprovalPlugin)
            .add_plugins(ToastPlugin)
            .add_plugins(crate::tidy::TidyPlugin)
            .add_observer(close_provider_session_on_remove)
            .add_systems(
                Update,
                (
                    ensure_prompt_queue,
                    request_provider_session_spawn.before(vmux_mcp::tool::ToolResolveSet),
                    spawn_provider_session.after(vmux_mcp::tool::ToolResolveSet),
                    send_provider_agent_input,
                    consume_provider_agent_stream.after(approval::ApprovalSyncSet),
                    attach_last_run_state_kind,
                ),
            );

        if app
            .world()
            .get_resource::<crate::runtime::provider::index::ProviderStrategyIndex>()
            .is_none()
        {
            app.insert_resource(crate::runtime::provider::index::ProviderStrategyIndex::default());
        }
        app.add_observer(crate::runtime::provider::indexer::on_strategy_added)
            .add_observer(crate::runtime::provider::indexer::on_strategy_removed)
            .add_plugins(crate::providers::anthropic_plugin::AnthropicPlugin)
            .add_plugins(crate::providers::mistral_plugin::MistralPlugin)
            .add_plugins(crate::providers::openai_plugin::OpenAiPlugin)
            .add_plugins(crate::echo_plugin::EchoPlugin);
    }
}

pub struct ProviderAgentPlugin;

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

fn request_provider_session_spawn(q: Query<Entity, Added<AgentSession>>, mut commands: Commands) {
    for entity in &q {
        commands
            .entity(entity)
            .insert(vmux_mcp::tool::ToolCatalogRequest);
    }
}

fn spawn_provider_session(
    q: Query<
        (
            Entity,
            &AgentSession,
            Option<&AgentApprovalPolicy>,
            &vmux_mcp::tool::ToolCatalog,
        ),
        Added<vmux_mcp::tool::ToolCatalog>,
    >,
    commands: Query<&vmux_command::CommandDefinition>,
    mut ecs: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, session, policy, catalog) in &q {
        if session.variant != AgentVariant::Page {
            continue;
        }
        let auto_tools: Vec<String> = policy
            .map(|p| p.auto.iter().cloned().collect())
            .unwrap_or_default();
        let command_tools = commands
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .collect();
        let definitions = match vmux_mcp::tool::ToolDefinition::merge_commands(
            catalog.0.clone(),
            command_tools,
        ) {
            Ok(definitions) => definitions,
            Err(error) => {
                bevy::log::error!("provider agent tool catalog is invalid: {error}");
                continue;
            }
        };
        let definitions = definitions
            .into_iter()
            .map(|definition| crate::stream::ToolDef {
                name: definition.name,
                description: definition.description,
                input_schema: definition.input_schema,
                read_only: false,
            })
            .collect::<Vec<_>>();
        let tools_json = serde_json::to_string(&definitions).unwrap_or_else(|_| "[]".to_string());
        service_requests.write(ServiceRequest(ClientMessage::SpawnPageAgent {
            sid: session.sid.clone(),
            provider: session.provider.clone(),
            model: session.model.clone(),
            cwd: String::new(),
            auto_tools,
            tools_json,
        }));
        service_requests.write(ServiceRequest(ClientMessage::Shared(SharedMessage::agent(
            session.sid.clone(),
            vmux_api::protocol::AgentRequest::Attach,
        ))));
        ecs.entity(entity)
            .remove::<vmux_mcp::tool::ToolCatalogRequest>()
            .remove::<vmux_mcp::tool::ToolCatalog>();
    }
}

fn send_provider_agent_input(
    mut q: Query<(&AgentSession, &mut AgentRunState, &mut PromptQueue)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
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
        service_requests.write(ServiceRequest(ClientMessage::agent_input(
            session.sid.clone(),
            prompt.text,
            None,
            prompt.attachments,
        )));
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

fn close_provider_session_on_remove(
    trigger: On<Remove, AgentSession>,
    sessions: Query<&AgentSession>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    if session.variant != AgentVariant::Page {
        return;
    }
    service_requests.write(ServiceRequest(ClientMessage::DetachPageAgent {
        sid: session.sid.clone(),
    }));
    service_requests.write(ServiceRequest(ClientMessage::ClosePageAgent {
        sid: session.sid.clone(),
    }));
}

#[allow(clippy::type_complexity)]
fn consume_provider_agent_stream(
    mut deltas: MessageReader<PageAgentDelta>,
    mut statuses: MessageReader<PageAgentRunStatus>,
    mut approvals: MessageReader<PageAgentAwaitingApproval>,
    mut resolved_approvals: MessageReader<PageAgentApprovalResolved>,
    mut snapshots: MessageReader<PageAgentSnapshot>,
    mut q: Query<(
        Entity,
        &mut AgentMessages,
        &mut AgentMessageTimes,
        &mut AgentRunState,
        &mut PromptQueue,
        Option<&AgentSession>,
        Option<&AcpSession>,
        Option<&AgentApprovalPolicy>,
        Option<&mut PendingHandoff>,
        Option<&ImportedConversation>,
    )>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
) {
    let by_sid: std::collections::HashMap<String, Entity> = q
        .iter()
        .filter_map(|(e, _, _, _, _, page, acp, _, _, _)| {
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
            && let Ok((_, mut messages, mut times, _, _, _, _, _, _, imported)) = q.get_mut(entity)
        {
            let mut parsed = snapshot.messages.clone();
            sanitize_replayed_messages(
                &mut parsed,
                imported.and_then(|imported| imported.first_prompt.as_deref()),
            );
            times.reconcile(&messages.0, &parsed);
            messages.0 = parsed;
        }
    }
    for status in statuses.read() {
        if !by_sid.contains_key(&status.sid) {
            warn!(sid = %status.sid, "dropping a run status no stack claims");
        }
        if let Some(&entity) = by_sid.get(&status.sid)
            && let Ok((_, _, _, mut state, mut queue, _, _, _, mut pending, _)) = q.get_mut(entity)
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
        if let Ok((_, _, _, mut state, _, _, acp, policy, _, _)) = q.get_mut(entity) {
            let auto_allowed = connected.is_some()
                && acp.is_some()
                && policy.is_some_and(|policy| policy.allows(&approval.name));
            if !auto_allowed {
                *state = AgentRunState::AwaitingApproval {
                    call_id: approval.call_id.clone(),
                    name: approval.name.clone(),
                    args: approval.args.clone(),
                };
            }
        }
        commands.trigger(AgentApprovalRequest {
            session: entity,
            call_id: approval.call_id.clone(),
            name: approval.name.clone(),
            args: approval.args.clone(),
        });
    }
    for resolved in resolved_approvals.read() {
        let Some(&entity) = by_sid.get(&resolved.sid) else {
            continue;
        };
        if let Ok((_, _, _, mut state, _, _, _, _, _, _)) = q.get_mut(entity)
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
mod tests {
    use super::*;
    use bevy_cef::prelude::BinIpcEventRawBuffer;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .init_resource::<BinIpcEventRawBuffer>()
            .add_plugins(ProviderAgentPlugin);
        app.update();
    }

    #[test]
    fn auto_approved_acp_request_without_service_falls_back_to_awaiting_state() {
        let mut app = App::new();
        app.add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);
        let mut policy = AgentApprovalPolicy::default();
        policy.allow("run");
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "a".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                PromptQueue::default(),
                policy,
            ))
            .id();
        app.world_mut().write_message(PageAgentAwaitingApproval {
            sid: "s1".into(),
            call_id: "call-1".into(),
            name: "run".into(),
            args: serde_json::json!({}),
        });

        app.update();

        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::AwaitingApproval { call_id, name, args })
                if call_id == "call-1" && name == "run" && args == &serde_json::json!({})
        ));
    }

    #[test]
    fn interrupted_status_pauses_queue_and_idles() {
        use vmux_service::agent_events::{
            PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
        };
        use vmux_service::protocol::AgentRunStatus;
        use vmux_session::AcpSession;
        use vmux_session::PromptQueue;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);

        let mut queue = PromptQueue::default();
        queue.enqueue("next".into());
        let e = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "a".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                queue,
            ))
            .id();
        app.world_mut().write_message(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Interrupted,
        });
        app.update();

        let world = app.world();
        assert!(matches!(
            world.get::<AgentRunState>(e),
            Some(AgentRunState::Idle)
        ));
        let q = world.get::<PromptQueue>(e).unwrap();
        assert!(q.paused, "queue must pause after interrupt");
        assert_eq!(q.items.len(), 1, "held item must not auto-advance");
    }

    #[test]
    fn flush_pending_interrupt_does_not_pause() {
        use vmux_service::agent_events::{
            PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
        };
        use vmux_service::protocol::AgentRunStatus;
        use vmux_session::AcpSession;
        use vmux_session::PromptQueue;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);

        let mut queue = PromptQueue::default();
        queue.enqueue("a".into());
        queue.enqueue("b".into());
        assert!(queue.request_flush());
        let e = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "a".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                queue,
            ))
            .id();
        app.world_mut().write_message(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Interrupted,
        });
        app.update();

        let world = app.world();
        assert!(matches!(
            world.get::<AgentRunState>(e),
            Some(AgentRunState::Idle)
        ));
        let q = world.get::<PromptQueue>(e).unwrap();
        assert!(
            !q.paused,
            "flush interrupt must leave the queue running to drain"
        );
        assert_eq!(
            q.items.len(),
            2,
            "items wait for the idle drain to batch them"
        );
    }

    #[test]
    fn flush_pending_error_rearms_queue() {
        use vmux_service::agent_events::{
            PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
        };
        use vmux_service::protocol::AgentRunStatus;
        use vmux_session::AcpSession;
        use vmux_session::PromptQueue;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);

        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        assert!(queue.request_flush());
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "a".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                queue,
            ))
            .id();
        app.world_mut().write_message(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Errored("cancel race".into()),
        });
        app.update();

        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
        let queue = app.world().get::<PromptQueue>(entity).unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
        assert_eq!(
            queue.items.front().map(|item| item.text.as_str()),
            Some("retry")
        );
    }

    #[test]
    fn acp_streaming_to_idle_raises_attention() {
        use vmux_session::PromptQueue;
        let mut app = App::new();
        app.add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "mistral-vibe".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                PromptQueue::default(),
            ))
            .id();

        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<PageAgentRunStatus>>()
            .write(PageAgentRunStatus {
                sid: "s1".into(),
                status: AgentRunStatus::Idle,
            });
        app.update();

        let atts: Vec<_> = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
            .drain()
            .collect();
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].entity, entity);
    }

    #[test]
    fn idle_to_idle_does_not_raise_attention() {
        use vmux_session::PromptQueue;
        let mut app = App::new();
        app.add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);
        app.world_mut().spawn((
            AcpSession {
                agent_id: "mistral-vibe".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Idle,
            PromptQueue::default(),
        ));

        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<PageAgentRunStatus>>()
            .write(PageAgentRunStatus {
                sid: "s1".into(),
                status: AgentRunStatus::Idle,
            });
        app.update();

        let count = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
            .drain()
            .count();
        assert_eq!(count, 0);
    }

    #[test]
    fn remote_approval_resolution_restores_streaming_state() {
        let mut app = App::new();
        app.add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, consume_provider_agent_stream);
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "mistral-vibe".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentMessages::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "call-1".into(),
                    name: "run".into(),
                    args: serde_json::json!({}),
                },
                PromptQueue::default(),
            ))
            .id();
        app.world_mut().write_message(PageAgentApprovalResolved {
            sid: "s1".into(),
            call_id: "call-1".into(),
        });

        app.update();

        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming)
        ));
    }
}
