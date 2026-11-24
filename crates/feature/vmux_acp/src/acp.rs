use bevy::prelude::{
    App, ApplyDeferred, Commands, Component, Entity, IntoScheduleConfigs, Name, On, PreUpdate,
    Query, Single, Update,
};
pub(crate) use catalog_driver::{CatalogDriver, CatalogRequest};
use driver::{
    AcpApprovalCommand, AcpConfigCommand, AcpConfigStateInput, AcpMcpServers, AcpProjectionSenders,
    AcpPromptCommand, AcpSelectedConfigInput, AcpShared,
};
use projection::{
    AcpProjectionOrder, AcpProjectionPlugin, AcpProjectionSet, AcpTranscripts,
    ApprovalDetailsRequest, RecordUserMessage,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Waker;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_api::protocol::{
    AcpSessionConfig, AgentAttachment, AgentRunStatus, ApprovalDecision, ServiceMessage,
    SharedEvent,
};
#[cfg(test)]
use vmux_api::session::Message;
use vmux_api::session::{RemoteApproval, RemoteSession, RemoteStatus};
use vmux_ecs::service::{AbortTask, Shutdown, ShutdownComplete, ShutdownPending};
use vmux_ecs::{
    AsyncChannel, ChannelDrain, ChannelReceived, CreatedAt, ProcessId, SpawnReceiver, SpawnSender,
};
use vmux_process::ProcessRuntime;
use vmux_session::SessionId;

use agent_client_protocol::schema::v1::SessionUpdate;
mod agent_process;
mod catalog_driver;
mod driver;
mod projection;
mod session_driver;
mod workspace_driver;

pub(crate) struct AcpRuntimePlugin;

impl bevy::prelude::Plugin for AcpRuntimePlugin {
    fn build(&self, app: &mut App) {
        AcpSessions::add_channels(app);
        AcpProjectionSenders::add_channels(app);
        app.add_plugins(AcpProjectionPlugin)
            .configure_sets(
                PreUpdate,
                (
                    ChannelDrain::of::<RemoteApproval>(),
                    ChannelDrain::of::<AcpApprovalResolved>(),
                )
                    .chain(),
            )
            .add_observer(project_info)
            .add_observer(project_config_state)
            .add_observer(project_status)
            .add_observer(project_startup)
            .add_observer(project_approval_requested)
            .add_observer(project_approval_resolved)
            .add_systems(
                Update,
                (spawn, ApplyDeferred).chain().before(AcpProjectionSet),
            )
            .add_systems(
                Update,
                (
                    ApplyDeferred,
                    (project_selected_config, snapshot_selection).chain(),
                    (route_prompts, route_approvals, route_configs, route_cancels).chain(),
                    ApplyDeferred,
                    (subscribe, read, list, rebind, close, reap).chain(),
                )
                    .chain()
                    .after(AcpProjectionSet),
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

pub(super) struct AcpWake(mpsc::UnboundedSender<()>);

impl AcpWake {
    pub(super) fn waker(wake: mpsc::UnboundedSender<()>) -> Waker {
        Waker::from(Arc::new(Self(wake)))
    }
}

impl std::task::Wake for AcpWake {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

#[vmux_ecs::channels]
#[derive(Clone)]
pub struct AcpSessions {
    spawns: SpawnSender<SpawnAcpSession>,
    prompts: SpawnSender<SendAcpPrompt>,
    approvals: SpawnSender<ApproveAcpTool>,
    configs: SpawnSender<SetAcpConfig>,
    cancels: SpawnSender<CancelAcpSession>,
    subscriptions: SpawnSender<SubscribeAcpSession>,
    snapshots: SpawnSender<SnapshotAcpSession>,
    agent_infos: SpawnSender<AcpSessionAgentInfo>,
    config_states: SpawnSender<AcpSessionConfigRequest>,
    statuses: SpawnSender<AcpSessionStatusRequest>,
    lists: SpawnSender<ListAcpSessions>,
    lookups: SpawnSender<FindAcpSession>,
    rebinds: SpawnSender<RebindAcpSession>,
    closes: SpawnSender<CloseAcpSession>,
}

#[derive(Component)]
struct AcpSessionRuntime(Handle);

#[derive(Component)]
struct AcpSessionWake(Waker);

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
    mcp_servers: AcpMcpServers,
    resume: Option<String>,
    response: Option<oneshot::Sender<()>>,
}

#[derive(Component)]
struct SendAcpPrompt {
    sid: String,
    text: String,
    context: Option<String>,
    attachments: Vec<AgentAttachment>,
    preferred_mode: Option<String>,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct ApproveAcpTool {
    sid: String,
    call_id: String,
    decision: ApprovalDecision,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct SetAcpConfig {
    sid: String,
    request_id: u64,
    config_id: Option<String>,
    value: String,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct CancelAcpSession {
    sid: String,
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
struct AcpPromptInput(mpsc::UnboundedSender<AcpPromptCommand>);

#[derive(Component)]
struct AcpApprovalInput(mpsc::UnboundedSender<AcpApprovalCommand>);

#[derive(Component)]
struct AcpConfigInput(mpsc::UnboundedSender<AcpConfigCommand>);

#[derive(Component)]
struct AcpCancelInput(mpsc::UnboundedSender<()>);

#[derive(Component)]
struct AcpCloseInput(mpsc::UnboundedSender<()>);

#[derive(Component)]
struct AcpSessionShared(Arc<AcpShared>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct AcpSessionCwd(PathBuf);

impl AcpSessionCwd {
    fn rebind(&mut self, cwd: PathBuf) -> Result<PathBuf, String> {
        let cwd = cwd
            .canonicalize()
            .map_err(|error| format!("invalid workspace directory: {error}"))?;
        if !cwd.is_dir() {
            return Err("workspace path is not a directory".to_string());
        }
        self.0 = cwd.clone();
        Ok(cwd)
    }
}

#[derive(Component)]
struct AcpStartupReady;

#[derive(Component)]
struct AcpStartupFailed;

#[derive(Clone, Copy)]
struct AcpStartup(bool);

#[derive(Clone, Copy)]
struct AcpReplayBegin(u64);

struct AcpSessionUpdate(u64, Box<SessionUpdate>);

#[derive(Clone, Copy)]
struct AcpReplayFinish(u64, bool);

#[derive(Clone, Copy)]
struct AcpSnapshotRequest(u64);

struct AcpApprovalDetails(u64, ApprovalDetailsRequest);

struct AcpAgentInfoUpdate(String);

struct AcpApprovalResolved(String);

struct AcpSelectionSnapshotRequest(oneshot::Sender<AcpSelectionSnapshot>);

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

fn spawn(
    runtime: Single<&AcpSessionRuntime>,
    wake: Single<&AcpSessionWake>,
    sessions: Query<(Entity, &SessionId, Option<&AcpStartupFailed>)>,
    mut requests: Query<(Entity, &mut SpawnAcpSession)>,
    mut commands: Commands,
) {
    let mut session_ids = HashSet::new();
    for (entity, sid, startup_failed) in &sessions {
        if requests.iter().any(|(_, request)| request.sid == sid.0) && startup_failed.is_some() {
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
            let (prompt_tx, prompt_rx) = mpsc::unbounded_channel();
            let (approval_tx, approval_rx) = mpsc::unbounded_channel();
            let (config_tx, config_rx) = mpsc::unbounded_channel();
            let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
            let (close_tx, close_rx) = mpsc::unbounded_channel();
            let (projection, projection_inboxes) =
                AcpProjectionSenders::open_with_waker(wake.0.clone());
            let (stream_tx, _) = broadcast::channel(256);
            let cwd = std::mem::take(&mut request.cwd);
            let shared = Arc::new(AcpShared::with_projection(
                request.sid.clone(),
                cwd.clone(),
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
                (prompt_rx, approval_rx, config_rx, cancel_rx, close_rx),
            ));
            commands.spawn((
                Name::new(format!("ACP session {}", request.sid)),
                SessionId(request.sid.clone()),
                (
                    AcpPromptInput(prompt_tx),
                    AcpApprovalInput(approval_tx),
                    AcpConfigInput(config_tx),
                    AcpCancelInput(cancel_tx),
                    AcpCloseInput(close_tx),
                ),
                AcpSessionShared(shared),
                AcpSessionCwd(cwd),
                AcpProjectionOrder::default(),
                projection_inboxes,
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

fn project_startup(trigger: On<ChannelReceived<AcpStartup>>, mut commands: Commands) {
    let entity = trigger.event().entity;
    let Some(ready) = trigger.event().get() else {
        return;
    };
    if ready.0 {
        commands
            .entity(entity)
            .remove::<AcpStartupFailed>()
            .insert(AcpStartupReady);
    } else {
        commands
            .entity(entity)
            .remove::<AcpStartupReady>()
            .insert(AcpStartupFailed);
    }
}

fn project_info(
    mut trigger: On<ChannelReceived<AcpAgentInfoUpdate>>,
    mut sessions: Query<(&SessionId, &AcpSessionShared, &mut AcpAgentName)>,
) {
    let entity = trigger.event().entity;
    let Some(update) = trigger.event_mut().take() else {
        return;
    };
    let Ok((sid, shared, mut agent_name)) = sessions.get_mut(entity) else {
        return;
    };
    agent_name.0 = Some(update.0.clone());
    shared
        .0
        .emit(ServiceMessage::Shared(SharedEvent::AcpAgentInfo {
            sid: sid.0.clone(),
            name: update.0,
        }));
}

fn project_config_state(
    mut trigger: On<ChannelReceived<AcpConfigStateInput>>,
    mut sessions: Query<(&SessionId, &AcpSessionShared, &mut AcpSessionConfigs)>,
) {
    let entity = trigger.event().entity;
    let Some(input) = trigger.event_mut().take() else {
        return;
    };
    let Ok((sid, shared, mut configs)) = sessions.get_mut(entity) else {
        return;
    };
    let next = AcpSessionConfigs::from_acp(&input.config_options, input.modes.as_ref());
    if *configs == next {
        return;
    }
    *configs = next;
    shared.0.emit(ServiceMessage::AcpSessionConfigState {
        sid: sid.0.clone(),
        configs: configs.0.clone(),
    });
}

fn project_selected_config(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &AsyncChannel<AcpSelectedConfigInput>,
        &mut AcpSessionConfigs,
    )>,
) {
    for (sid, shared, channel, mut configs) in &mut sessions {
        for input in channel.drain() {
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
    mut trigger: On<ChannelReceived<AgentRunStatus>>,
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpRunState,
        &mut AcpApprovalState,
    )>,
) {
    let entity = trigger.event().entity;
    let Some(status) = trigger.event_mut().take() else {
        return;
    };
    let Ok((sid, shared, mut run_state, mut approval)) = sessions.get_mut(entity) else {
        return;
    };
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

fn project_approval_requested(
    mut trigger: On<ChannelReceived<RemoteApproval>>,
    mut sessions: Query<(&SessionId, &AcpSessionShared, &mut AcpApprovalState)>,
) {
    let entity = trigger.event().entity;
    let Some(next) = trigger.event_mut().take() else {
        return;
    };
    let Ok((sid, shared, mut approval)) = sessions.get_mut(entity) else {
        return;
    };
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

fn project_approval_resolved(
    mut trigger: On<ChannelReceived<AcpApprovalResolved>>,
    mut sessions: Query<(&SessionId, &AcpSessionShared, &mut AcpApprovalState)>,
) {
    let entity = trigger.event().entity;
    let Some(resolved) = trigger.event_mut().take() else {
        return;
    };
    let Ok((sid, shared, mut approval)) = sessions.get_mut(entity) else {
        return;
    };
    let call_id = resolved.0;
    if approval
        .0
        .as_ref()
        .is_none_or(|pending| pending.call_id != call_id)
    {
        return;
    }
    approval.0 = None;
    shared
        .0
        .emit(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved {
            sid: sid.0.clone(),
            call_id,
        }));
}

fn snapshot_selection(
    sessions: Query<(
        &AsyncChannel<AcpSelectionSnapshotRequest>,
        &AcpSessionConfigs,
    )>,
) {
    for (channel, configs) in &sessions {
        for request in channel.drain() {
            let _ = request.0.send(AcpSelectionSnapshot {
                configs: configs.0.clone(),
            });
        }
    }
}

fn route_prompts(
    sessions: Query<(Entity, &SessionId, &AcpPromptInput)>,
    mut requests: Query<(Entity, &mut SendAcpPrompt)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (session, sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let text = std::mem::take(&mut request.text);
            let attachments = std::mem::take(&mut request.attachments);
            accepted = input
                .0
                .send(AcpPromptCommand {
                    text: text.clone(),
                    context: request.context.take(),
                    attachments: attachments.clone(),
                    preferred_mode: request.preferred_mode.take(),
                })
                .is_ok();
            if accepted {
                commands.trigger(RecordUserMessage {
                    session,
                    text,
                    attachments,
                });
            }
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(accepted);
        }
        commands.entity(request_entity).despawn();
    }
}

fn route_approvals(
    sessions: Query<(&SessionId, &AcpApprovalInput)>,
    mut requests: Query<(Entity, &mut ApproveAcpTool)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            accepted = input
                .0
                .send(AcpApprovalCommand {
                    call_id: std::mem::take(&mut request.call_id),
                    decision: request.decision,
                })
                .is_ok();
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(accepted);
        }
        commands.entity(request_entity).despawn();
    }
}

fn route_configs(
    sessions: Query<(&SessionId, &AcpConfigInput)>,
    mut requests: Query<(Entity, &mut SetAcpConfig)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            accepted = input
                .0
                .send(AcpConfigCommand {
                    request_id: request.request_id,
                    config_id: request.config_id.take(),
                    value: std::mem::take(&mut request.value),
                })
                .is_ok();
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(accepted);
        }
        commands.entity(request_entity).despawn();
    }
}

fn route_cancels(
    sessions: Query<(&SessionId, &AcpCancelInput)>,
    mut requests: Query<(Entity, &mut CancelAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            accepted = input.0.send(()).is_ok();
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
        Entity,
        &SessionId,
        &AcpSessionShared,
        &AcpAgentName,
        &AcpSessionConfigs,
        &AcpRunState,
        &AcpHistoryReplay,
    )>,
    transcripts: AcpTranscripts,
    mut snapshots: Query<(Entity, &mut SnapshotAcpSession)>,
    mut agent_infos: Query<(Entity, &mut AcpSessionAgentInfo)>,
    mut config_states: Query<(Entity, &mut AcpSessionConfigRequest)>,
    mut statuses: Query<(Entity, &mut AcpSessionStatusRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut snapshots {
        let mut result = None;
        for (entity, sid, shared, _, _, _, replay) in &sessions {
            if sid.0 == request.sid {
                if !replay.active {
                    let messages = transcripts.messages(entity);
                    result = Some(shared.0.snapshot_message(&messages));
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
        for (_, sid, _, agent_name, _, _, _) in &sessions {
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
        for (_, sid, _, _, configs, _, _) in &sessions {
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
        for (_, sid, _, _, _, status, _) in &sessions {
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
        Entity,
        &SessionId,
        &AcpSessionCwd,
        &AcpSessionAgent,
        &CreatedAt,
        &AcpAgentName,
        &AcpSessionConfigs,
        &AcpRunState,
        &AcpApprovalState,
    )>,
    transcripts: AcpTranscripts,
    mut lists: Query<(Entity, &mut ListAcpSessions)>,
    mut lookups: Query<(Entity, &mut FindAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut lists {
        let mut result = Vec::new();
        for (entity, sid, cwd, agent, created_at, name, configs, status, approval) in &sessions {
            let name = name.0.clone().unwrap_or_else(|| agent.0.clone());
            let messages = transcripts.messages(entity);
            result.push(RemoteSession {
                id: sid.clone(),
                url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
                title: vmux_session::ConversationTitle::from_messages(&messages, &name),
                name,
                runtime: "acp".to_string(),
                model: configs
                    .0
                    .iter()
                    .find(|config| config.category.as_deref() == Some("model"))
                    .map(|config| config.current_value.clone())
                    .filter(|model| !model.is_empty()),
                cwd: cwd.0.to_string_lossy().into_owned(),
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
        for (entity, sid, cwd, agent, created_at, name, configs, status, approval) in &sessions {
            if sid.0 == request.sid {
                let name = name.0.clone().unwrap_or_else(|| agent.0.clone());
                let messages = transcripts.messages(entity);
                result = Some(RemoteSession {
                    id: sid.clone(),
                    url: format!("{}{}", vmux_api::VmuxRoute::SESSIONS_ROOT, sid.0),
                    title: vmux_session::ConversationTitle::from_messages(&messages, &name),
                    name,
                    runtime: "acp".to_string(),
                    model: configs
                        .0
                        .iter()
                        .find(|config| config.category.as_deref() == Some("model"))
                        .map(|config| config.current_value.clone())
                        .filter(|model| !model.is_empty()),
                    cwd: cwd.0.to_string_lossy().into_owned(),
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
    mut sessions: Query<(&SessionId, &AcpSessionShared, &mut AcpSessionCwd)>,
    mut requests: Query<(Entity, &mut RebindAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut result = Err("ACP session not found".to_string());
        for (sid, shared, mut session_cwd) in &mut sessions {
            if sid.0 == request.sid {
                result = session_cwd
                    .rebind(std::mem::take(&mut request.cwd))
                    .map(|updated| {
                        shared.0.set_cwd(updated.clone());
                        updated
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

fn close(
    sessions: Query<(Entity, &SessionId, &AcpCloseInput)>,
    mut requests: Query<(Entity, &mut CloseAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut closed = false;
        for (session_entity, sid, close_input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let _ = close_input.0.send(());
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
    channels: Single<(
        &SpawnReceiver<SpawnAcpSession>,
        &SpawnReceiver<SendAcpPrompt>,
        &SpawnReceiver<ApproveAcpTool>,
        &SpawnReceiver<SetAcpConfig>,
        &SpawnReceiver<CancelAcpSession>,
    )>,
    tasks: Query<(Entity, &AcpCloseInput), bevy::prelude::With<AcpSessionShared>>,
    mut commands: Commands,
) {
    let (spawns, prompts, approvals, configs, cancels) = channels.into_inner();
    spawns.close_and_clear();
    prompts.close_and_clear();
    approvals.close_and_clear();
    configs.close_and_clear();
    cancels.close_and_clear();
    for (entity, close_input) in &tasks {
        let _ = close_input.0.send(());
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
    use bevy::ecs::system::RunSystemOnce;
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
            let (wake, _wake_rx) = mpsc::unbounded_channel();
            let (processes, _process_runtime) = ProcessRuntime::new(wake);
            let shared = Arc::new(AcpShared::new(
                "remote-session".into(),
                PathBuf::from("/tmp"),
                ProcessId::new(),
                stream,
                processes,
            ));
            let (prompt_tx, _prompt_rx) = mpsc::unbounded_channel();
            let (approval_tx, _approval_rx) = mpsc::unbounded_channel();
            let (config_tx, _config_rx) = mpsc::unbounded_channel();
            let (cancel_tx, _cancel_rx) = mpsc::unbounded_channel();
            let (close_tx, _close_rx) = mpsc::unbounded_channel();
            self.app.world_mut().spawn((
                SessionId("remote-session".into()),
                AcpSessionShared(shared),
                AcpSessionCwd(PathBuf::from("/tmp")),
                AcpStartupReady,
                AcpProjectionOrder::default(),
                (
                    AcpPromptInput(prompt_tx),
                    AcpApprovalInput(approval_tx),
                    AcpConfigInput(config_tx),
                    AcpCancelInput(cancel_tx),
                    AcpCloseInput(close_tx),
                ),
                AcpSessionAgent("mock".into()),
                CreatedAt(1),
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

    #[test]
    fn subscriptions_receive_no_empty_snapshot_while_history_is_replaying() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _entered = runtime.enter();
        let mut app = App::new();
        app.add_plugins(crate::service::AcpServicePlugin);
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let (processes, _process_runtime) = ProcessRuntime::new(wake.clone());
        let (_sessions, runtime) = AcpSessions::new(Handle::current(), wake);
        app.world_mut().spawn(runtime);
        let (stream, _receiver) = broadcast::channel(4);
        let shared = Arc::new(AcpShared::new(
            "session".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream,
            processes,
        ));
        let session = app
            .world_mut()
            .spawn((
                SessionId("session".into()),
                AcpSessionShared(shared),
                AcpProjectionOrder::default(),
                AcpAgentName::default(),
                AcpSessionConfigs::default(),
                AcpRunState(AgentRunStatus::Idle),
                AcpHistoryReplay { active: true },
            ))
            .id();
        app.world_mut().trigger(RecordUserMessage {
            session,
            text: "restored".into(),
            attachments: Vec::new(),
        });
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
        app.add_plugins(AcpRuntimePlugin);
        let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel();
        let (stream_tx, mut stream_rx) = broadcast::channel(4);
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let (processes, _process_runtime) = ProcessRuntime::new(wake);
        let shared = Arc::new(AcpShared::new(
            "s1".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream_tx,
            processes,
        ));
        let session = app
            .world_mut()
            .spawn((
                SessionId("s1".into()),
                AcpPromptInput(prompt_tx),
                AcpSessionShared(shared),
                AcpSessionCwd(PathBuf::from("/tmp")),
                AcpProjectionOrder::default(),
            ))
            .id();
        app.world_mut().spawn(SendAcpPrompt {
            sid: "s1".into(),
            text: "hello".into(),
            context: None,
            attachments: Vec::new(),
            preferred_mode: None,
            response: None,
        });

        app.update();

        assert!(
            matches!(prompt_rx.try_recv(), Ok(AcpPromptCommand { text, .. }) if text == "hello")
        );
        let messages = app
            .world_mut()
            .run_system_once(move |transcripts: AcpTranscripts| transcripts.messages(session))
            .unwrap();
        assert!(matches!(
            messages.as_slice(),
            [Message::User { text, .. }] if text == "hello"
        ));
        assert!(matches!(
            stream_rx.try_recv(),
            Ok(ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages, .. }))
                if matches!(messages.as_slice(), [Message::User { text, .. }] if text == "hello")
        ));
    }
}
