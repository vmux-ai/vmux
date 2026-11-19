use bevy::prelude::{
    App, ApplyDeferred, Bundle, Commands, Component, Entity, IntoScheduleConfigs, Name, Query,
    Single, Update,
};
pub(crate) use catalog_driver::{CatalogDriver, CatalogRequest};
pub use driver::AcpInput;
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
use vmux_api::session::Message;
use vmux_api::session::{RemoteApproval, RemoteSession, RemoteStatus};
use vmux_ecs::service::{AbortTask, Shutdown, ShutdownComplete, ShutdownPending};
use vmux_ecs::{CreatedAt, ProcessId};
use vmux_process::ProcessRuntime;
use vmux_session::SessionId;

use agent_client_protocol::schema::v1::McpServer;

mod agent_process;
mod catalog_driver;
mod driver;
mod projection;
mod projection_driver;
mod session_driver;
mod workspace_driver;

pub(crate) struct AcpRuntimePlugin;

impl bevy::prelude::Plugin for AcpRuntimePlugin {
    fn build(&self, app: &mut App) {
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
        )
        .add_systems(Shutdown, begin_shutdown)
        .add_systems(ShutdownComplete, shutdown);
    }
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
    response: Option<oneshot::Sender<Result<PathBuf, String>>>,
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
}

#[derive(Component)]
struct AcpSessionAgent(String);

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
    sessions: Query<(Entity, &SessionId, Option<&AcpSessionShared>)>,
    mut requests: Query<(Entity, &mut SpawnAcpSession)>,
    mut commands: Commands,
) {
    let mut session_ids = HashSet::new();
    for (entity, sid, shared) in &sessions {
        if requests.iter().any(|(_, request)| request.sid == sid.0)
            && shared.is_some_and(|shared| shared.0.startup_failed())
        {
            commands
                .entity(entity)
                .remove::<SessionId>()
                .insert(AcpSessionClosing);
        } else {
            session_ids.insert(sid.0.clone());
        }
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
            let replay_pending = request.resume.is_some();
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
                AcpHistoryReplay {
                    active: replay_pending,
                },
                AcpSessionAgent(request.agent_id.clone()),
                CreatedAt::now(),
                AbortTask(task),
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
    mut sessions: Query<(
        &SessionId,
        &AcpSessionInput,
        &AcpSessionShared,
        &mut AcpProjector,
    )>,
    mut requests: Query<(Entity, &mut AcpSessionInputRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input, shared, mut projector) in &mut sessions {
            if sid.0 != request.sid {
                continue;
            }
            if let Some(session_input) = request.input.take() {
                let user = match &session_input {
                    AcpInput::User {
                        text, attachments, ..
                    } => Some((text.clone(), attachments.clone())),
                    _ => None,
                };
                accepted = input.0.send(session_input).is_ok();
                if accepted && let Some((text, attachments)) = user {
                    projector.push_user(text, attachments);
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
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
        &AcpHistoryReplay,
    )>,
    mut snapshots: Query<(Entity, &mut SnapshotAcpSession)>,
    mut agent_infos: Query<(Entity, &mut AcpSessionAgentInfo)>,
    mut config_states: Query<(Entity, &mut AcpSessionConfigRequest)>,
    mut statuses: Query<(Entity, &mut AcpSessionStatusRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut snapshots {
        let mut result = None;
        for (sid, shared, projector, _, _, _, replay) in &sessions {
            if sid.0 == request.sid {
                if !replay.active {
                    result = Some(shared.0.snapshot_message(projector.messages()));
                }
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
        for (sid, _, _, agent_name, _, _, _) in &sessions {
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
        for (sid, _, _, _, configs, _, _) in &sessions {
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
        for (sid, _, _, _, _, status, _) in &sessions {
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
                id: sid.clone(),
                url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
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
        result.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
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
                    id: sid.clone(),
                    url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
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
                result = shared
                    .0
                    .rebind_cwd(std::mem::take(&mut request.cwd))
                    .map(|()| shared.0.cwd());
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
    sessions: Query<(Entity, &AbortTask), bevy::prelude::With<AcpSessionClosing>>,
    mut commands: Commands,
) {
    for (entity, task) in &sessions {
        if task.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn begin_shutdown(
    mut inbox: Single<&mut AcpSessionInbox>,
    tasks: Query<(Entity, &AcpSessionInput), bevy::prelude::With<AcpSessionShared>>,
    mut commands: Commands,
) {
    inbox.0.spawns.close();
    inbox.0.inputs.close();
    while inbox.0.spawns.try_recv().is_ok() {}
    while inbox.0.inputs.try_recv().is_ok() {}
    for (entity, input) in &tasks {
        let _ = input.0.send(AcpInput::Close);
        commands.entity(entity).insert(ShutdownPending);
    }
}

fn shutdown(
    runtime: Single<&AcpSessionRuntime>,
    mut tasks: Query<(Entity, &mut AbortTask), bevy::prelude::With<AcpSessionShared>>,
    mut commands: Commands,
) {
    runtime.0.block_on(async {
        for (entity, mut task) in &mut tasks {
            task.0.abort();
            let _ = (&mut task.0).await;
            commands.entity(entity).despawn();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use vmux_api::protocol::{
        AgentCommandResult, AgentNewChat, AgentRequest, AgentRequestId, ClientMessage,
        SharedFailure, SharedMessage, SharedResponse,
    };
    use vmux_api::session::ClientOpId;
    use vmux_transport::service::{
        RemoteDriver, RemoteFuture, RemoteOperationStore, ServiceProtocolConnection,
        ServiceProtocolDriver,
    };

    struct ScriptedOperations {
        replies: std::sync::Mutex<VecDeque<bool>>,
        events: std::sync::Mutex<Vec<&'static str>>,
    }

    impl RemoteOperationStore for ScriptedOperations {
        fn claim(&self, _id: ClientOpId) -> RemoteFuture<'_, bool> {
            Box::pin(async {
                self.events.lock().unwrap().push("claim");
                self.replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected claim")
            })
        }

        fn release(&self, _id: ClientOpId) -> RemoteFuture<'_, ()> {
            Box::pin(async {
                self.events.lock().unwrap().push("release");
            })
        }
    }

    struct RemoteFixture {
        app: App,
        owner: Entity,
        driver: Arc<dyn RemoteDriver>,
        connection: Box<dyn ServiceProtocolConnection>,
        received: mpsc::UnboundedReceiver<ServiceMessage>,
        wake: mpsc::UnboundedReceiver<()>,
        operations: Arc<ScriptedOperations>,
    }

    impl RemoteFixture {
        async fn new(replies: &[bool], subscribe: bool) -> Self {
            let mut app = App::new();
            app.add_plugins((
                bevy::prelude::MinimalPlugins,
                crate::service::AcpServicePlugin,
            ));
            let (wake, wake_rx) = mpsc::unbounded_channel();
            let (processes, _process_runtime) = ProcessRuntime::new(wake.clone());
            let (service, native) =
                crate::service_driver::AcpService::new(Handle::current(), wake.clone(), processes);
            let (driver, inbox) = service.remote_driver(wake.clone());
            let operations = Arc::new(ScriptedOperations {
                replies: std::sync::Mutex::new(replies.iter().copied().collect()),
                events: std::sync::Mutex::new(Vec::new()),
            });
            let owner = app
                .world_mut()
                .spawn((
                    Name::new("Test remote service"),
                    service.clone(),
                    native,
                    inbox,
                    vmux_ecs::service::Executor(Handle::current()),
                    vmux_ecs::service::Wake(wake),
                    vmux_ecs::service::Operations(operations.clone()),
                ))
                .id();
            let (outbound, received) = mpsc::unbounded_channel();
            let mut connection = service.connect(outbound);
            if subscribe {
                connection
                    .dispatch(ClientMessage::SubscribeAgentCommands)
                    .await
                    .unwrap();
            }
            Self {
                app,
                owner,
                driver,
                connection,
                received,
                wake: wake_rx,
                operations,
            }
        }

        fn request(&self, request: SharedMessage) -> tokio::task::JoinHandle<SharedResponse> {
            let driver = self.driver.clone();
            tokio::spawn(async move { driver.dispatch(request).await })
        }

        fn chat(&self) -> tokio::task::JoinHandle<SharedResponse> {
            self.request(SharedMessage::AgentNewChat {
                request: AgentNewChat {
                    client_op_id: ClientOpId::new("test-operation"),
                    prompt: "Start the task".into(),
                    agent_url: None,
                },
            })
        }

        async fn command(&mut self) -> (AgentRequestId, AgentRequest) {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
            tokio::pin!(timeout);
            loop {
                tokio::select! {
                    message = self.received.recv() => {
                        if let Some(ServiceMessage::AgentRequest { request_id, request, .. }) = message { return (request_id, request); }
                    }
                    wake = self.wake.recv() => { assert!(wake.is_some()); self.app.update(); }
                    _ = &mut timeout => panic!("remote command did not wake the owning App"),
                }
            }
        }

        async fn respond(&mut self, id: AgentRequestId, result: AgentCommandResult) {
            self.connection
                .dispatch(ClientMessage::AgentCommandResponse {
                    request_id: id,
                    result,
                })
                .await
                .unwrap();
        }

        async fn settle(
            &mut self,
            mut task: tokio::task::JoinHandle<SharedResponse>,
        ) -> SharedResponse {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
            tokio::pin!(timeout);
            loop {
                tokio::select! {
                    result = &mut task => return result.unwrap(),
                    wake = self.wake.recv() => { assert!(wake.is_some()); self.app.update(); }
                    _ = &mut timeout => panic!("remote completion did not wake the owning App"),
                }
            }
        }

        async fn drain_native(&mut self) {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
            tokio::pin!(timeout);
            loop {
                self.app.update();
                if self
                    .app
                    .world_mut()
                    .query::<&crate::remote_io::Accepted>()
                    .iter(self.app.world())
                    .next()
                    .is_none()
                {
                    break;
                }
                tokio::select! {
                    wake = self.wake.recv() => assert!(wake.is_some()),
                    _ = &mut timeout => panic!("native remote IO did not drain"),
                }
            }
        }

        fn session(&mut self) {
            let (stream, _stream_rx) = broadcast::channel(4);
            let shared = Arc::new(AcpShared::new(
                "remote-session".into(),
                PathBuf::from("/tmp"),
                ProcessId::new(),
                stream,
                Arc::new(tokio::sync::Mutex::new(
                    vmux_process::ProcessManager::default(),
                )),
            ));
            let (input, _input_rx) = mpsc::unbounded_channel();
            self.app.world_mut().spawn((
                SessionId("remote-session".into()),
                AcpSessionShared(shared),
                AcpSessionInput(input),
                AcpSessionAgent("mock".into()),
                CreatedAt(1),
                AcpProjector::default(),
                AcpAgentName::default(),
                AcpSessionConfigs::default(),
                AcpRunState(AgentRunStatus::Idle),
                AcpApprovalState::default(),
                AcpHistoryReplay::default(),
            ));
        }
    }

    #[tokio::test]
    async fn remote_failed_chat_releases_before_retry_and_success_blocks_duplicate() {
        let mut fixture = RemoteFixture::new(&[true, true, false], false).await;
        let failed = fixture.chat();
        assert!(matches!(
            fixture.settle(failed).await,
            SharedResponse::Failed(SharedFailure::NoDesktop)
        ));
        assert_eq!(
            *fixture.operations.events.lock().unwrap(),
            ["claim", "release"]
        );
        fixture
            .connection
            .dispatch(ClientMessage::SubscribeAgentCommands)
            .await
            .unwrap();
        let retry = fixture.chat();
        let (id, request) = fixture.command().await;
        assert_eq!(request.id, "agent_new_chat@1");
        fixture.respond(id, AgentCommandResult::Ok).await;
        assert!(matches!(fixture.settle(retry).await, SharedResponse::Ok));
        let duplicate = fixture.chat();
        assert!(matches!(
            fixture.settle(duplicate).await,
            SharedResponse::AlreadyApplied
        ));
        assert_eq!(
            *fixture.operations.events.lock().unwrap(),
            ["claim", "release", "claim", "claim"]
        );
        assert!(fixture.received.try_recv().is_err());
        fixture.connection.disconnect().await;
    }

    #[tokio::test]
    async fn remote_canonical_model_request_reaches_the_native_broker() {
        let mut fixture = RemoteFixture::new(&[], true).await;
        let request = fixture.request(SharedMessage::AgentSelectModel {
            request: vmux_api::protocol::AgentSelectModel {
                sid: "target-session".into(),
                model_id: "provider-model".into(),
            },
        });
        let (id, envelope) = fixture.command().await;
        assert_eq!(envelope.id, "agent_select_model@1");
        assert_eq!(
            envelope.body,
            br#"{"sid":"target-session","model_id":"provider-model"}"#
        );
        fixture
            .respond(id, AgentCommandResult::Error("not offered".into()))
            .await;
        assert!(matches!(
            fixture.settle(request).await,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        assert!(fixture.operations.events.lock().unwrap().is_empty());
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&crate::remote::Response>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&crate::remote_broker::Broker<vmux_api::protocol::AgentSelectModel>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        fixture.connection.disconnect().await;
    }

    #[test]
    fn remote_shutdown_does_not_poll_an_already_joined_native_task_again() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut fixture = runtime.block_on(RemoteFixture::new(&[], false));
        let request = {
            let _entered = runtime.enter();
            fixture.request(SharedMessage::AgentListModels {
                request: vmux_api::protocol::AgentListModels {
                    sid: "missing".into(),
                },
            })
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world_mut()
                .query::<(&crate::remote_io::Accepted, &crate::remote_io::Joined)>()
                .iter(fixture.app.world())
                .next()
                .is_some()
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(1), fixture.wake.recv())
                    .await
                    .unwrap()
                    .unwrap();
            });
        }
        fixture
            .app
            .world_mut()
            .run_schedule(vmux_ecs::service::Shutdown);
        fixture
            .app
            .world_mut()
            .run_schedule(vmux_ecs::service::ShutdownComplete);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&crate::remote_io::Accepted>()
                .iter(fixture.app.world())
                .count(),
            0
        );
        assert!(matches!(
            runtime.block_on(request).unwrap(),
            SharedResponse::Failed(SharedFailure::NoDesktop)
        ));
        runtime.block_on(fixture.connection.disconnect());
    }

    #[tokio::test]
    async fn remote_owner_retirement_retains_broker_io_until_failure_cleanup() {
        let mut fixture = RemoteFixture::new(&[true], true).await;
        let request = fixture.chat();
        let (id, _) = fixture.command().await;
        fixture.app.world_mut().despawn(fixture.owner);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&crate::remote_io::Accepted>()
                .iter(fixture.app.world())
                .count(),
            1
        );
        fixture
            .respond(id, AgentCommandResult::Error("retired".into()))
            .await;
        assert!(matches!(
            fixture.settle(request).await,
            SharedResponse::Failed(SharedFailure::NoDesktop)
        ));
        fixture.drain_native().await;
        assert_eq!(
            *fixture.operations.events.lock().unwrap(),
            ["claim", "release"]
        );
        fixture.connection.disconnect().await;
    }

    #[tokio::test]
    async fn remote_caller_disappearance_does_not_abandon_claim_cleanup() {
        let mut fixture = RemoteFixture::new(&[true], true).await;
        let request = fixture.chat();
        let (id, _) = fixture.command().await;
        request.abort();
        let _ = request.await;
        fixture
            .respond(id, AgentCommandResult::Error("refused".into()))
            .await;
        fixture.drain_native().await;
        assert_eq!(
            *fixture.operations.events.lock().unwrap(),
            ["claim", "release"]
        );
        fixture.connection.disconnect().await;
    }

    #[tokio::test]
    async fn remote_prompt_refuses_invalid_text_and_files_outside_home() {
        let mut fixture = RemoteFixture::new(&[], false).await;
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("outside.txt");
        std::fs::write(&path, "private").unwrap();
        let oversized = fixture.request(SharedMessage::AgentInput {
            sid: "missing".into(),
            text: "x".repeat(64 * 1024 + 1),
            context: None,
            attachments: Vec::new(),
            preferred_mode: None,
        });
        assert!(matches!(
            fixture.settle(oversized).await,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        let outside = fixture.request(SharedMessage::AgentInput {
            sid: "missing".into(),
            text: "Read this".into(),
            context: None,
            attachments: vec![vmux_api::protocol::AgentAttachment {
                path: path.to_string_lossy().into_owned(),
                name: "untrusted".into(),
                mime_type: "untrusted".into(),
                size: 0,
            }],
            preferred_mode: None,
        });
        assert!(matches!(
            fixture.settle(outside).await,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        fixture.connection.disconnect().await;
    }

    #[tokio::test]
    async fn remote_media_returns_filtered_directory_first_results_and_png_preview() {
        let mut fixture = RemoteFixture::new(&[], false).await;
        fixture.session();
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        std::fs::create_dir(directory.path().join("b-folder")).unwrap();
        std::fs::write(directory.path().join("hidden.txt"), "excluded").unwrap();
        std::fs::write(directory.path().join(".hidden.png"), "excluded").unwrap();
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=").unwrap();
        std::fs::write(directory.path().join("a-image.png"), bytes).unwrap();
        let request = fixture.request(SharedMessage::AgentListMedia {
            sid: "remote-session".into(),
            query: directory.path().to_string_lossy().into_owned(),
        });
        let SharedResponse::Media(entries) = fixture.settle(request).await else {
            panic!("missing media projection");
        };
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["b-folder", "a-image.png"]
        );
        assert!(entries[0].is_dir);
        let encoded = entries[1]
            .preview_data_url
            .strip_prefix("data:image/png;base64,")
            .unwrap();
        let png =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert!(
            image
                .to_rgba8()
                .pixels()
                .all(|pixel| pixel.0 == [255, 255, 255, 255])
        );
        fixture.connection.disconnect().await;
    }

    #[tokio::test]
    async fn subscriptions_receive_no_empty_snapshot_while_history_is_replaying() {
        let mut app = App::new();
        app.add_plugins(crate::service::AcpServicePlugin);
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let (_sessions, runtime) = AcpSessions::new(Handle::current(), wake);
        app.world_mut().spawn(runtime);
        let (stream, _receiver) = broadcast::channel(4);
        let shared = Arc::new(AcpShared::new(
            "session".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream,
            Arc::new(tokio::sync::Mutex::new(
                vmux_process::ProcessManager::default(),
            )),
        ));
        let mut projector = AcpProjector::default();
        projector.push_user("restored".into(), Vec::new());
        let session = app
            .world_mut()
            .spawn((
                SessionId("session".into()),
                AcpSessionShared(shared),
                projector,
                AcpAgentName::default(),
                AcpSessionConfigs::default(),
                AcpRunState(AgentRunStatus::Idle),
                AcpHistoryReplay { active: true },
            ))
            .id();
        let (response, mut receiver) = oneshot::channel();
        app.world_mut().spawn(SnapshotAcpSession {
            sid: "session".into(),
            response: Some(response),
        });
        app.update();
        assert!(receiver.try_recv().unwrap().is_none());
        app.world_mut()
            .get_mut::<AcpHistoryReplay>(session)
            .unwrap()
            .active = false;
        let (response, mut receiver) = oneshot::channel();
        app.world_mut().spawn(SnapshotAcpSession {
            sid: "session".into(),
            response: Some(response),
        });
        app.update();
        assert!(
            matches!(receiver.try_recv().unwrap(), Some(ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages, .. })) if matches!(messages.as_slice(), [Message::User { text, .. }] if text == "restored"))
        );
    }

    #[test]
    fn accepted_user_input_is_projected_before_agent_startup() {
        let mut app = App::new();
        app.add_systems(Update, route_input);
        let (input_tx, mut input_rx) = mpsc::unbounded_channel();
        let (stream_tx, mut stream_rx) = broadcast::channel(4);
        let shared = Arc::new(AcpShared::new(
            "s1".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream_tx,
            Arc::new(tokio::sync::Mutex::new(
                vmux_process::ProcessManager::default(),
            )),
        ));
        let session = app
            .world_mut()
            .spawn((
                SessionId("s1".into()),
                AcpSessionInput(input_tx),
                AcpSessionShared(shared),
                AcpProjector::default(),
            ))
            .id();
        app.world_mut().spawn(AcpSessionInputRequest {
            sid: "s1".into(),
            input: Some(AcpInput::User {
                text: "hello".into(),
                context: None,
                attachments: Vec::new(),
                preferred_mode: None,
            }),
            response: None,
        });

        app.update();

        assert!(matches!(input_rx.try_recv(), Ok(AcpInput::User { text, .. }) if text == "hello"));
        assert!(matches!(
            app.world().get::<AcpProjector>(session).unwrap().messages(),
            [Message::User { text, .. }] if text == "hello"
        ));
        assert!(matches!(
            stream_rx.try_recv(),
            Ok(ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages, .. }))
                if matches!(messages.as_slice(), [Message::User { text, .. }] if text == "hello")
        ));
    }
}
