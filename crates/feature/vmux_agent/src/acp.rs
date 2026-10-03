use bevy::prelude::{
    App, ApplyDeferred, Bundle, Commands, Component, Entity, IntoScheduleConfigs, Name, Query,
    Single, Update,
};
pub use driver::AcpInput;
#[cfg(test)]
use driver::AcpMcpServers;
use driver::{
    AcpConfigStateInput, AcpProjectionSenders, AcpSelectedConfigInput, AcpShared,
    AcpTranscriptInput,
};
use projection_driver::AcpProjector;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_api::protocol::{AcpSessionConfig, AgentRunStatus, ServiceMessage, SharedEvent};
#[cfg(test)]
use vmux_api::protocol::{ManagedMcpServer, ManagedMcpTransport};
use vmux_api::room::{Message, RemoteApproval, RemoteSession, RemoteStatus};
use vmux_ecs::agent::SessionId;
use vmux_ecs::{CreatedAt, ProcessId};
use vmux_process::ProcessRuntime;
use vmux_session::SessionId;

use agent_client_protocol::schema::v1::McpServer;

mod driver;
mod projection;
mod projection_driver;
mod session_driver;
mod workspace_driver;

pub(crate) fn add(app: &mut App) {
    projection::add(app);
    app.add_systems(
        Update,
        (
            receive,
            ApplyDeferred,
            spawn,
            project_info,
            project_config_state,
            project_selected_config,
            project_status,
            project_approval_requested,
            project_approval_resolved,
            snapshot_selection,
            route_input,
            subscribe,
            read,
            list,
            rebind,
            close,
            reap,
        )
            .chain(),
    );
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct AcpSessionConfigs(Vec<AcpSessionConfig>);

#[derive(Clone, Default)]
struct AcpSelectionSnapshot {
    configs: Vec<AcpSessionConfig>,
}

#[derive(Clone)]
pub struct AcpSessions {
    spawns: mpsc::UnboundedSender<SpawnAcpSession>,
    inputs: mpsc::UnboundedSender<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedSender<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedSender<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedSender<AcpSessionAgentInfo>,
    config_states: mpsc::UnboundedSender<AcpSessionConfigRequest>,
    statuses: mpsc::UnboundedSender<AcpSessionStatusRequest>,
    messages: mpsc::UnboundedSender<AcpSessionMessages>,
    lists: mpsc::UnboundedSender<ListAcpSessions>,
    lookups: mpsc::UnboundedSender<FindAcpSession>,
    rebinds: mpsc::UnboundedSender<RebindAcpSession>,
    closes: mpsc::UnboundedSender<CloseAcpSession>,
    wake: mpsc::UnboundedSender<()>,
}

struct AcpSessionReceivers {
    spawns: mpsc::UnboundedReceiver<SpawnAcpSession>,
    inputs: mpsc::UnboundedReceiver<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedReceiver<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedReceiver<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedReceiver<AcpSessionAgentInfo>,
    config_states: mpsc::UnboundedReceiver<AcpSessionConfigRequest>,
    statuses: mpsc::UnboundedReceiver<AcpSessionStatusRequest>,
    messages: mpsc::UnboundedReceiver<AcpSessionMessages>,
    lists: mpsc::UnboundedReceiver<ListAcpSessions>,
    lookups: mpsc::UnboundedReceiver<FindAcpSession>,
    rebinds: mpsc::UnboundedReceiver<RebindAcpSession>,
    closes: mpsc::UnboundedReceiver<CloseAcpSession>,
}

#[derive(Component)]
struct AcpSessionInbox(AcpSessionReceivers);

#[derive(Component)]
struct AcpSessionRuntime(Handle);

#[derive(Component)]
struct AcpSessionWake(mpsc::UnboundedSender<()>);

#[derive(Component)]
struct SpawnAcpSession {
    sid: String,
    agent_id: String,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: PathBuf,
    anchor: ProcessId,
    processes: ProcessRuntime,
    mcp_servers: Vec<McpServer>,
    resume: Option<String>,
    response: Option<oneshot::Sender<()>>,
}

#[derive(Component)]
struct AcpSessionInputRequest {
    sid: String,
    input: Option<AcpInput>,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct SubscribeAcpSession {
    sid: String,
    response: Option<oneshot::Sender<Option<broadcast::Receiver<ServiceMessage>>>>,
}

#[derive(Component)]
struct SnapshotAcpSession {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AcpSessionAgentInfo {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AcpSessionConfigRequest {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AcpSessionStatusRequest {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AcpSessionMessages {
    sid: String,
    response: Option<oneshot::Sender<Option<Vec<Message>>>>,
}

#[derive(Component)]
struct ListAcpSessions {
    response: Option<oneshot::Sender<Vec<RemoteSession>>>,
}

#[derive(Component)]
struct FindAcpSession {
    sid: String,
    response: Option<oneshot::Sender<Option<RemoteSession>>>,
}

#[derive(Component)]
struct RebindAcpSession {
    sid: String,
    cwd: PathBuf,
    response: Option<oneshot::Sender<Result<(), String>>>,
}

#[derive(Component)]
struct CloseAcpSession {
    sid: String,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct AcpSessionInput(mpsc::UnboundedSender<AcpInput>);

#[derive(Component)]
struct AcpSessionShared(Arc<AcpShared>);

#[derive(Component)]
struct AcpTranscriptInbox(mpsc::UnboundedReceiver<AcpTranscriptInput>);

#[derive(Component)]
struct AcpAgentInfoInbox(mpsc::UnboundedReceiver<String>);

#[derive(Component)]
struct AcpConfigStateInbox(mpsc::UnboundedReceiver<AcpConfigStateInput>);

#[derive(Component)]
struct AcpSelectedConfigInbox(mpsc::UnboundedReceiver<AcpSelectedConfigInput>);

#[derive(Component)]
struct AcpStatusInbox(mpsc::UnboundedReceiver<AgentRunStatus>);

#[derive(Component)]
struct AcpApprovalRequestedInbox(mpsc::UnboundedReceiver<RemoteApproval>);

#[derive(Component)]
struct AcpApprovalResolvedInbox(mpsc::UnboundedReceiver<String>);

#[derive(Component)]
struct AcpSelectionSnapshotInbox(mpsc::UnboundedReceiver<oneshot::Sender<AcpSelectionSnapshot>>);

#[derive(Bundle)]
struct AcpProjectionInboxes {
    transcript: AcpTranscriptInbox,
    agent_info: AcpAgentInfoInbox,
    config_state: AcpConfigStateInbox,
    selected_config: AcpSelectedConfigInbox,
    status: AcpStatusInbox,
    approval_requested: AcpApprovalRequestedInbox,
    approval_resolved: AcpApprovalResolvedInbox,
    selection_snapshot: AcpSelectionSnapshotInbox,
}

#[derive(Component, Default)]
struct AcpAgentName(Option<String>);

#[derive(Component)]
struct AcpRunState(AgentRunStatus);

#[derive(Component, Default)]
struct AcpApprovalState(Option<RemoteApproval>);

#[derive(Component, Default)]
struct AcpHistoryReplay {
    active: bool,
    updates: usize,
}

#[derive(Component)]
struct AcpSessionAgent(String);

#[derive(Component)]
struct AcpSessionTask(tokio::task::JoinHandle<()>);

#[derive(Component)]
struct AcpSessionClosing;

fn receive(mut inbox: Single<&mut AcpSessionInbox>, mut commands: Commands) {
    while let Ok(request) = inbox.0.spawns.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.inputs.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.subscriptions.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.snapshots.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.agent_infos.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.config_states.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.statuses.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.messages.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.lists.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.lookups.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.rebinds.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.closes.try_recv() {
        commands.spawn(request);
    }
}

fn spawn(
    runtime: Single<&AcpSessionRuntime>,
    wake: Single<&AcpSessionWake>,
    sessions: Query<&SessionId>,
    mut requests: Query<(Entity, &mut SpawnAcpSession)>,
    mut commands: Commands,
) {
    let mut session_ids = HashSet::new();
    for sid in &sessions {
        session_ids.insert(sid.0.clone());
    }
    for (request_entity, mut request) in &mut requests {
        if !session_ids.contains(&request.sid) {
            let (input_tx, input_rx) = mpsc::unbounded_channel();
            let (projection, projection_inboxes) = AcpProjectionSenders::open(wake.0.clone());
            let (stream_tx, _) = broadcast::channel(256);
            let shared = Arc::new(AcpShared::with_projection(
                request.sid.clone(),
                std::mem::take(&mut request.cwd),
                request.anchor,
                stream_tx,
                request.processes.clone(),
                projection,
            ));
            let task = runtime.0.spawn(driver::AcpDriver::run(
                std::mem::take(&mut request.command),
                std::mem::take(&mut request.args),
                std::mem::take(&mut request.env),
                std::mem::take(&mut request.mcp_servers),
                request.resume.take(),
                Arc::clone(&shared),
                input_rx,
            ));
            commands.spawn((
                Name::new(format!("ACP session {}", request.sid)),
                SessionId(request.sid.clone()),
                AcpSessionInput(input_tx),
                AcpSessionShared(shared),
                projection_inboxes,
                AcpProjector::default(),
                AcpAgentName::default(),
                AcpSessionConfigs::default(),
                AcpRunState(AgentRunStatus::Idle),
                AcpApprovalState::default(),
                AcpHistoryReplay::default(),
                AcpSessionAgent(request.agent_id.clone()),
                CreatedAt::now(),
                AcpSessionTask(task),
            ));
            session_ids.insert(request.sid.clone());
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(());
        }
        commands.entity(request_entity).despawn();
    }
}

fn project_info(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpAgentInfoInbox,
        &mut AcpAgentName,
    )>,
) {
    for (sid, shared, mut inbox, mut agent_name) in &mut sessions {
        while let Ok(name) = inbox.0.try_recv() {
            agent_name.0 = Some(name.clone());
            shared
                .0
                .emit(ServiceMessage::Shared(SharedEvent::AcpAgentInfo {
                    sid: sid.0.clone(),
                    name,
                }));
        }
    }
}

fn project_config_state(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpConfigStateInbox,
        &mut AcpSessionConfigs,
    )>,
) {
    for (sid, shared, mut inbox, mut configs) in &mut sessions {
        while let Ok(input) = inbox.0.try_recv() {
            let next = AcpSessionConfigs::from_acp(&input.config_options, input.modes.as_ref());
            if *configs == next {
                continue;
            }
            *configs = next;
            shared.0.emit(ServiceMessage::AcpSessionConfigState {
                sid: sid.0.clone(),
                configs: configs.0.clone(),
            });
        }
    }
}

fn project_selected_config(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpSelectedConfigInbox,
        &mut AcpSessionConfigs,
    )>,
) {
    for (sid, shared, mut inbox, mut configs) in &mut sessions {
        while let Ok(input) = inbox.0.try_recv() {
            if !input.config_options.is_empty() {
                let legacy = configs
                    .0
                    .iter()
                    .find(|config| config.config_id.is_none())
                    .cloned();
                let mut next = AcpSessionConfigs::from_acp(&input.config_options, None);
                if !next
                    .0
                    .iter()
                    .any(|config| config.category.as_deref() == Some("mode"))
                    && let Some(legacy) = legacy
                {
                    next.0.push(legacy);
                }
                *configs = next;
            }
            let Some(config) = configs
                .0
                .iter_mut()
                .find(|config| config.config_id.as_deref() == input.config_id.as_deref())
            else {
                continue;
            };
            if config.current_value == input.value
                || !config
                    .values
                    .iter()
                    .any(|option| option.value == input.value)
            {
                continue;
            }
            config.current_value = input.value;
            shared.0.emit(ServiceMessage::AcpSessionConfigState {
                sid: sid.0.clone(),
                configs: configs.0.clone(),
            });
        }
    }
}

fn project_status(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpStatusInbox,
        &mut AcpRunState,
        &mut AcpApprovalState,
    )>,
) {
    for (sid, shared, mut inbox, mut run_state, mut approval) in &mut sessions {
        while let Ok(status) = inbox.0.try_recv() {
            if let AgentRunStatus::Errored(message) = &status {
                tracing::warn!(target: "acp", sid = %sid.0, "{message}");
            }
            if !matches!(status, AgentRunStatus::Streaming) {
                approval.0 = None;
            }
            run_state.0 = status.clone();
            shared
                .0
                .emit(ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged {
                    sid: sid.0.clone(),
                    status,
                }));
        }
    }
}

fn project_approval_requested(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpApprovalRequestedInbox,
        &mut AcpApprovalState,
    )>,
) {
    for (sid, shared, mut inbox, mut approval) in &mut sessions {
        while let Ok(next) = inbox.0.try_recv() {
            approval.0 = Some(next.clone());
            shared
                .0
                .emit(ServiceMessage::Shared(SharedEvent::AgentAwaitingApproval {
                    sid: sid.0.clone(),
                    call_id: next.call_id,
                    name: next.name,
                    args: next.args,
                }));
        }
    }
}

fn project_approval_resolved(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpApprovalResolvedInbox,
        &mut AcpApprovalState,
    )>,
) {
    for (sid, shared, mut inbox, mut approval) in &mut sessions {
        while let Ok(call_id) = inbox.0.try_recv() {
            if approval
                .0
                .as_ref()
                .is_none_or(|pending| pending.call_id != call_id)
            {
                continue;
            }
            approval.0 = None;
            shared
                .0
                .emit(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved {
                    sid: sid.0.clone(),
                    call_id,
                }));
        }
    }
}

fn snapshot_selection(mut sessions: Query<(&mut AcpSelectionSnapshotInbox, &AcpSessionConfigs)>) {
    for (mut inbox, configs) in &mut sessions {
        while let Ok(response) = inbox.0.try_recv() {
            let _ = response.send(AcpSelectionSnapshot {
                configs: configs.0.clone(),
            });
        }
    }
}

fn route_input(
    sessions: Query<(&SessionId, &AcpSessionInput)>,
    mut requests: Query<(Entity, &mut AcpSessionInputRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            if let Some(session_input) = request.input.take() {
                accepted = input.0.send(session_input).is_ok();
            }
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(accepted);
        }
        commands.entity(request_entity).despawn();
    }
}

fn subscribe(
    sessions: Query<(&SessionId, &AcpSessionShared)>,
    mut requests: Query<(Entity, &mut SubscribeAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut receiver = None;
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                receiver = Some(shared.0.stream_tx.subscribe());
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(receiver);
        }
        commands.entity(request_entity).despawn();
    }
}

fn read(
    sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &AcpProjector,
        &AcpAgentName,
        &AcpSessionConfigs,
        &AcpRunState,
    )>,
    mut snapshots: Query<(Entity, &mut SnapshotAcpSession)>,
    mut agent_infos: Query<(Entity, &mut AcpSessionAgentInfo)>,
    mut config_states: Query<(Entity, &mut AcpSessionConfigRequest)>,
    mut statuses: Query<(Entity, &mut AcpSessionStatusRequest)>,
    mut messages: Query<(Entity, &mut AcpSessionMessages)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut snapshots {
        let mut result = None;
        for (sid, shared, projector, _, _, _) in &sessions {
            if sid.0 == request.sid {
                result = Some(shared.0.snapshot_message(projector.messages()));
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut agent_infos {
        let mut result = None;
        for (sid, _, _, agent_name, _, _) in &sessions {
            if sid.0 == request.sid {
                result = agent_name.0.as_ref().map(|name| {
                    ServiceMessage::Shared(SharedEvent::AcpAgentInfo {
                        sid: sid.0.clone(),
                        name: name.clone(),
                    })
                });
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut config_states {
        let mut result = None;
        for (sid, _, _, _, configs, _) in &sessions {
            if sid.0 == request.sid {
                result = Some(ServiceMessage::AcpSessionConfigState {
                    sid: sid.0.clone(),
                    configs: configs.0.clone(),
                });
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut statuses {
        let mut result = None;
        for (sid, _, _, _, _, status) in &sessions {
            if sid.0 == request.sid {
                result = Some(ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged {
                    sid: sid.0.clone(),
                    status: status.0.clone(),
                }));
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut messages {
        let mut result = None;
        for (sid, _, projector, _, _, _) in &sessions {
            if sid.0 == request.sid {
                result = Some(projector.messages().to_vec());
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn list(
    sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &AcpSessionAgent,
        &CreatedAt,
        &AcpProjector,
        &AcpAgentName,
        &AcpSessionConfigs,
        &AcpRunState,
        &AcpApprovalState,
    )>,
    mut lists: Query<(Entity, &mut ListAcpSessions)>,
    mut lookups: Query<(Entity, &mut FindAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut lists {
        let mut result = Vec::new();
        for (sid, shared, agent, created_at, projector, name, configs, status, approval) in
            &sessions
        {
            let name = name.0.clone().unwrap_or_else(|| agent.0.clone());
            result.push(RemoteSession {
                sid: sid.0.clone(),
                url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
                room_id: vmux_api::room::RoomId::for_session(&sid.0),
                title: vmux_session::ConversationTitle::from_messages(projector.messages(), &name),
                name,
                runtime: "acp".to_string(),
                model: configs
                    .0
                    .iter()
                    .find(|config| config.category.as_deref() == Some("model"))
                    .map(|config| config.current_value.clone())
                    .filter(|model| !model.is_empty()),
                cwd: shared.0.cwd().to_string_lossy().into_owned(),
                status: RemoteStatus::from(&status.0),
                approval: approval.0.clone(),
                created_at_ms: created_at.0.max(0) as u64,
            });
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut lookups {
        let mut result = None;
        for (sid, shared, agent, created_at, projector, name, configs, status, approval) in
            &sessions
        {
            if sid.0 == request.sid {
                let name = name.0.clone().unwrap_or_else(|| agent.0.clone());
                result = Some(RemoteSession {
                    sid: sid.0.clone(),
                    url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
                    room_id: vmux_api::room::RoomId::for_session(&sid.0),
                    title: vmux_session::ConversationTitle::from_messages(
                        projector.messages(),
                        &name,
                    ),
                    name,
                    runtime: "acp".to_string(),
                    model: configs
                        .0
                        .iter()
                        .find(|config| config.category.as_deref() == Some("model"))
                        .map(|config| config.current_value.clone())
                        .filter(|model| !model.is_empty()),
                    cwd: shared.0.cwd().to_string_lossy().into_owned(),
                    status: RemoteStatus::from(&status.0),
                    approval: approval.0.clone(),
                    created_at_ms: created_at.0.max(0) as u64,
                });
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn rebind(
    sessions: Query<(&SessionId, &AcpSessionShared)>,
    mut requests: Query<(Entity, &mut RebindAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut result = Err("ACP session not found".to_string());
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = shared.0.rebind_cwd(std::mem::take(&mut request.cwd));
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn close(
    sessions: Query<(Entity, &SessionId, &AcpSessionInput)>,
    mut requests: Query<(Entity, &mut CloseAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut closed = false;
        for (session_entity, sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let _ = input.0.send(AcpInput::Close);
            commands
                .entity(session_entity)
                .remove::<SessionId>()
                .insert(AcpSessionClosing);
            closed = true;
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(closed);
        }
        commands.entity(request_entity).despawn();
    }
}

fn reap(
    sessions: Query<(Entity, &AcpSessionTask), bevy::prelude::With<AcpSessionClosing>>,
    mut commands: Commands,
) {
    for (entity, task) in &sessions {
        if task.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_mcp_server_with_working_directory_is_rejected() {
        assert!(
            AcpMcpServers::from_managed(ManagedMcpServer {
                name: "local".into(),
                transport: ManagedMcpTransport::Stdio,
                command: Some("server".into()),
                args: Vec::new(),
                env: Vec::new(),
                cwd: Some("/tmp/project".into()),
                url: None,
                headers: Vec::new(),
            })
            .is_none()
        );
    }
}
