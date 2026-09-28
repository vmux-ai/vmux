mod driver;
mod projector;

pub use driver::{AcpInput, AcpShared};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::{
    App, ApplyDeferred, Bundle, Commands, Component, Entity, IntoScheduleConfigs, Name, Plugin,
    Query, Single, Update,
};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_core::agent::SessionId;
use vmux_core::{CreatedAt, ProcessId};

use crate::process::ProcessManager;
use crate::remote::RemoteSession;
use vmux_api::protocol::ServiceMessage;
use vmux_api::room::Message;

pub struct AcpSessionPlugin;

impl Plugin for AcpSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                receive_acp_session_requests,
                ApplyDeferred,
                spawn_acp_sessions,
                route_acp_session_inputs,
                subscribe_acp_sessions,
                read_acp_session_state,
                list_acp_sessions,
                rebind_acp_sessions,
                close_acp_sessions,
                reap_closed_acp_sessions,
            )
                .chain(),
        );
    }
}

#[derive(Clone)]
pub(crate) struct AcpSessions {
    spawns: mpsc::UnboundedSender<SpawnAcpSession>,
    inputs: mpsc::UnboundedSender<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedSender<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedSender<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedSender<AcpSessionAgentInfo>,
    model_infos: mpsc::UnboundedSender<AcpSessionModelInfo>,
    mode_infos: mpsc::UnboundedSender<AcpSessionModeInfo>,
    messages: mpsc::UnboundedSender<AcpSessionMessages>,
    lists: mpsc::UnboundedSender<ListAcpSessions>,
    lookups: mpsc::UnboundedSender<FindAcpSession>,
    rebinds: mpsc::UnboundedSender<RebindAcpSession>,
    closes: mpsc::UnboundedSender<CloseAcpSession>,
    wake: mpsc::UnboundedSender<()>,
}

impl AcpSessions {
    pub(crate) fn new(runtime: Handle, wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (inputs, input_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (agent_infos, agent_info_inbox) = mpsc::unbounded_channel();
        let (model_infos, model_info_inbox) = mpsc::unbounded_channel();
        let (mode_infos, mode_info_inbox) = mpsc::unbounded_channel();
        let (messages, message_inbox) = mpsc::unbounded_channel();
        let (lists, list_inbox) = mpsc::unbounded_channel();
        let (lookups, lookup_inbox) = mpsc::unbounded_channel();
        let (rebinds, rebind_inbox) = mpsc::unbounded_channel();
        let (closes, close_inbox) = mpsc::unbounded_channel();
        (
            Self {
                spawns,
                inputs,
                subscriptions,
                snapshots,
                agent_infos,
                model_infos,
                mode_infos,
                messages,
                lists,
                lookups,
                rebinds,
                closes,
                wake,
            },
            (
                AcpSessionRuntime(runtime),
                AcpSessionInbox(AcpSessionReceivers {
                    spawns: spawn_inbox,
                    inputs: input_inbox,
                    subscriptions: subscription_inbox,
                    snapshots: snapshot_inbox,
                    agent_infos: agent_info_inbox,
                    model_infos: model_info_inbox,
                    mode_infos: mode_info_inbox,
                    messages: message_inbox,
                    lists: list_inbox,
                    lookups: lookup_inbox,
                    rebinds: rebind_inbox,
                    closes: close_inbox,
                }),
            ),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn(
        &self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        anchor: ProcessId,
        manager: Arc<tokio::sync::Mutex<ProcessManager>>,
        mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
        resume: Option<String>,
        effort: Option<String>,
    ) -> Result<(), String> {
        let (response, receiver) = oneshot::channel();
        self.spawns
            .send(SpawnAcpSession {
                sid,
                agent_id,
                command,
                args,
                env,
                cwd,
                anchor,
                manager,
                mcp_servers,
                resume,
                effort,
                response: Some(response),
            })
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "ACP session spawn was cancelled".to_string())
    }

    pub(crate) async fn input(&self, sid: String, input: AcpInput) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .inputs
            .send(AcpSessionInputRequest {
                sid,
                input: Some(input),
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub(crate) async fn subscribe(
        &self,
        sid: String,
    ) -> Option<broadcast::Receiver<ServiceMessage>> {
        let (response, receiver) = oneshot::channel();
        self.subscriptions
            .send(SubscribeAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn snapshot(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.snapshots
            .send(SnapshotAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn agent_info(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.agent_infos
            .send(AcpSessionAgentInfo {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn model_info(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.model_infos
            .send(AcpSessionModelInfo {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn mode_info(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.mode_infos
            .send(AcpSessionModeInfo {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn remote_messages(&self, sid: String) -> Option<Vec<Message>> {
        let (response, receiver) = oneshot::channel();
        self.messages
            .send(AcpSessionMessages {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn remote_sessions(&self) -> Vec<RemoteSession> {
        let (response, receiver) = oneshot::channel();
        if self
            .lists
            .send(ListAcpSessions {
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return Vec::new();
        }
        receiver.await.unwrap_or_default()
    }

    pub(crate) async fn remote_session(&self, sid: String) -> Option<RemoteSession> {
        let (response, receiver) = oneshot::channel();
        self.lookups
            .send(FindAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn rebind_cwd(&self, sid: String, cwd: PathBuf) -> Result<(), String> {
        let (response, receiver) = oneshot::channel();
        self.rebinds
            .send(RebindAcpSession {
                sid,
                cwd,
                response: Some(response),
            })
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "ACP workspace rebind was cancelled".to_string())?
    }

    pub(crate) async fn close(&self, sid: String) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .closes
            .send(CloseAcpSession {
                sid,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn closed() -> Self {
        let (wake, wake_inbox) = mpsc::unbounded_channel();
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (inputs, input_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (agent_infos, agent_info_inbox) = mpsc::unbounded_channel();
        let (model_infos, model_info_inbox) = mpsc::unbounded_channel();
        let (mode_infos, mode_info_inbox) = mpsc::unbounded_channel();
        let (messages, message_inbox) = mpsc::unbounded_channel();
        let (lists, list_inbox) = mpsc::unbounded_channel();
        let (lookups, lookup_inbox) = mpsc::unbounded_channel();
        let (rebinds, rebind_inbox) = mpsc::unbounded_channel();
        let (closes, close_inbox) = mpsc::unbounded_channel();
        drop((
            wake_inbox,
            spawn_inbox,
            input_inbox,
            subscription_inbox,
            snapshot_inbox,
            agent_info_inbox,
            model_info_inbox,
            mode_info_inbox,
            message_inbox,
            list_inbox,
            lookup_inbox,
            rebind_inbox,
            close_inbox,
        ));
        Self {
            spawns,
            inputs,
            subscriptions,
            snapshots,
            agent_infos,
            model_infos,
            mode_infos,
            messages,
            lists,
            lookups,
            rebinds,
            closes,
            wake,
        }
    }
}

struct AcpSessionReceivers {
    spawns: mpsc::UnboundedReceiver<SpawnAcpSession>,
    inputs: mpsc::UnboundedReceiver<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedReceiver<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedReceiver<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedReceiver<AcpSessionAgentInfo>,
    model_infos: mpsc::UnboundedReceiver<AcpSessionModelInfo>,
    mode_infos: mpsc::UnboundedReceiver<AcpSessionModeInfo>,
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
struct SpawnAcpSession {
    sid: String,
    agent_id: String,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: PathBuf,
    anchor: ProcessId,
    manager: Arc<tokio::sync::Mutex<ProcessManager>>,
    mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
    resume: Option<String>,
    effort: Option<String>,
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
struct AcpSessionModelInfo {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AcpSessionModeInfo {
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
struct AcpSessionAgent(String);

#[derive(Component)]
struct AcpSessionTask(tokio::task::JoinHandle<()>);

#[derive(Component)]
struct AcpSessionClosing;

fn receive_acp_session_requests(mut inbox: Single<&mut AcpSessionInbox>, mut commands: Commands) {
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
    while let Ok(request) = inbox.0.model_infos.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.mode_infos.try_recv() {
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

fn spawn_acp_sessions(
    runtime: Single<&AcpSessionRuntime>,
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
            let (stream_tx, _) = broadcast::channel(256);
            let shared = Arc::new(AcpShared::new(
                request.sid.clone(),
                std::mem::take(&mut request.cwd),
                request.anchor,
                stream_tx,
                Arc::clone(&request.manager),
            ));
            let task = runtime.0.spawn(driver::run(
                std::mem::take(&mut request.command),
                std::mem::take(&mut request.args),
                std::mem::take(&mut request.env),
                request.agent_id.clone(),
                std::mem::take(&mut request.mcp_servers),
                request.resume.take(),
                request.effort.take(),
                Arc::clone(&shared),
                input_rx,
            ));
            commands.spawn((
                Name::new(format!("ACP session {}", request.sid)),
                SessionId(request.sid.clone()),
                AcpSessionInput(input_tx),
                AcpSessionShared(shared),
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

fn route_acp_session_inputs(
    sessions: Query<(&SessionId, &AcpSessionInput, &AcpSessionShared)>,
    mut requests: Query<(Entity, &mut AcpSessionInputRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input, shared) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            if let Some(AcpInput::Approve { call_id, .. }) = &request.input {
                shared.0.resolve_approval(call_id);
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

fn subscribe_acp_sessions(
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

fn read_acp_session_state(
    sessions: Query<(&SessionId, &AcpSessionShared)>,
    mut snapshots: Query<(Entity, &mut SnapshotAcpSession)>,
    mut agent_infos: Query<(Entity, &mut AcpSessionAgentInfo)>,
    mut model_infos: Query<(Entity, &mut AcpSessionModelInfo)>,
    mut mode_infos: Query<(Entity, &mut AcpSessionModeInfo)>,
    mut messages: Query<(Entity, &mut AcpSessionMessages)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut snapshots {
        let mut result = None;
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = Some(shared.0.snapshot_message());
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
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = shared.0.agent_info_message();
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut model_infos {
        let mut result = None;
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = shared.0.model_info_message();
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut mode_infos {
        let mut result = None;
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = shared.0.mode_info_message();
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
        for (sid, shared) in &sessions {
            if sid.0 == request.sid {
                result = Some(shared.0.remote_messages());
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn list_acp_sessions(
    sessions: Query<(&SessionId, &AcpSessionShared, &AcpSessionAgent, &CreatedAt)>,
    mut lists: Query<(Entity, &mut ListAcpSessions)>,
    mut lookups: Query<(Entity, &mut FindAcpSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut lists {
        let mut result = Vec::new();
        for (_, shared, agent, created_at) in &sessions {
            result.push(
                shared
                    .0
                    .remote_session(&agent.0, created_at.0.max(0) as u64),
            );
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
    for (request_entity, mut request) in &mut lookups {
        let mut result = None;
        for (sid, shared, agent, created_at) in &sessions {
            if sid.0 == request.sid {
                result = Some(
                    shared
                        .0
                        .remote_session(&agent.0, created_at.0.max(0) as u64),
                );
                break;
            }
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn rebind_acp_sessions(
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

fn close_acp_sessions(
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

fn reap_closed_acp_sessions(
    sessions: Query<(Entity, &AcpSessionTask), bevy::prelude::With<AcpSessionClosing>>,
    mut commands: Commands,
) {
    for (entity, task) in &sessions {
        if task.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
