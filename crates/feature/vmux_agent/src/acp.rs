mod driver;
mod projector;
pub(crate) mod route;

pub use driver::AcpInput;
use driver::AcpShared;
use projector::{AcpProjector, ApprovalDetailsQuery, Intent};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::{
    App, ApplyDeferred, Bundle, Commands, Component, Entity, IntoScheduleConfigs, Name, Plugin,
    Query, Single, Update,
};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_ecs::agent::SessionId;
use vmux_ecs::{CreatedAt, ProcessId};

use vmux_api::protocol::{
    AcpSessionConfig, AcpSessionConfigValue, AgentAttachment, AgentFileTouched, AgentRequest,
    AgentRequestId, AgentRunStatus, ManagedMcpServer, ManagedMcpTransport, ServiceMessage,
    SharedEvent,
};
use vmux_api::room::{Message, RemoteApproval, RemoteSession, RemoteStatus};
use vmux_process::ProcessRuntime;

pub struct AcpSessionPlugin;

impl Plugin for AcpSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                receive,
                ApplyDeferred,
                spawn,
                project,
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
}

enum AcpProjectionInput {
    BeginHistoryReplay,
    Update(agent_client_protocol::schema::v1::SessionUpdate),
    FinishHistoryReplay(bool),
    PushUser {
        text: String,
        attachments: Vec<AgentAttachment>,
    },
    Snapshot,
    ApprovalDetails {
        query: ApprovalDetailsQuery,
        response: oneshot::Sender<Option<(String, String)>>,
    },
    AgentInfo(String),
    ConfigState {
        config_options: Vec<agent_client_protocol::schema::v1::SessionConfigOption>,
        modes: Option<agent_client_protocol::schema::v1::SessionModeState>,
    },
    SelectedConfig {
        config_id: Option<String>,
        value: String,
        config_options: Vec<agent_client_protocol::schema::v1::SessionConfigOption>,
    },
    Status(AgentRunStatus),
    ApprovalRequested(RemoteApproval),
    ApprovalResolved(String),
    SelectionSnapshot(oneshot::Sender<AcpSelectionSnapshot>),
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct AcpSessionConfigs(Vec<AcpSessionConfig>);

impl AcpSessionConfigs {
    fn category_name(
        category: &agent_client_protocol::schema::v1::SessionConfigOptionCategory,
    ) -> String {
        use agent_client_protocol::schema::v1::SessionConfigOptionCategory;

        match category {
            SessionConfigOptionCategory::Mode => "mode".to_string(),
            SessionConfigOptionCategory::Model => "model".to_string(),
            SessionConfigOptionCategory::ModelConfig => "model_config".to_string(),
            SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
            SessionConfigOptionCategory::Other(value) => value.clone(),
            _ => "unknown".to_string(),
        }
    }

    fn options(
        options: &agent_client_protocol::schema::v1::SessionConfigSelectOptions,
    ) -> Vec<AcpSessionConfigValue> {
        use agent_client_protocol::schema::v1::SessionConfigSelectOptions;

        let mut values = Vec::new();
        match options {
            SessionConfigSelectOptions::Ungrouped(options) => {
                for option in options {
                    values.push(AcpSessionConfigValue {
                        value: option.value.to_string(),
                        name: option.name.clone(),
                        description: option.description.clone(),
                        group: None,
                    });
                }
            }
            SessionConfigSelectOptions::Grouped(groups) => {
                for group in groups {
                    for option in &group.options {
                        values.push(AcpSessionConfigValue {
                            value: option.value.to_string(),
                            name: option.name.clone(),
                            description: option.description.clone(),
                            group: Some(group.name.clone()),
                        });
                    }
                }
            }
            _ => {}
        }
        values
    }

    fn from_acp(
        config_options: &[agent_client_protocol::schema::v1::SessionConfigOption],
        legacy: Option<&agent_client_protocol::schema::v1::SessionModeState>,
    ) -> Self {
        use agent_client_protocol::schema::v1::SessionConfigKind;

        let mut configs = Vec::new();
        for config in config_options {
            let SessionConfigKind::Select(select) = &config.kind else {
                continue;
            };
            configs.push(AcpSessionConfig {
                config_id: Some(config.id.to_string()),
                name: config.name.clone(),
                description: config.description.clone(),
                category: config.category.as_ref().map(Self::category_name),
                current_value: select.current_value.to_string(),
                values: Self::options(&select.options),
            });
        }
        let has_mode = configs
            .iter()
            .any(|config| config.category.as_deref() == Some("mode"));
        if !has_mode && let Some(legacy) = legacy {
            configs.push(AcpSessionConfig {
                config_id: None,
                name: "Mode".to_string(),
                description: None,
                category: Some("mode".to_string()),
                current_value: legacy.current_mode_id.to_string(),
                values: legacy
                    .available_modes
                    .iter()
                    .map(|mode| AcpSessionConfigValue {
                        value: mode.id.to_string(),
                        name: mode.name.clone(),
                        description: mode.description.clone(),
                        group: None,
                    })
                    .collect(),
            });
        }
        Self(configs)
    }

    fn replace(&mut self, next: Self, sid: &str) -> Option<ServiceMessage> {
        if *self == next {
            return None;
        }
        *self = next;
        Some(self.message(sid))
    }

    fn replace_options(
        &mut self,
        config_options: &[agent_client_protocol::schema::v1::SessionConfigOption],
        sid: &str,
    ) -> Option<ServiceMessage> {
        let legacy = self
            .0
            .iter()
            .find(|config| config.config_id.is_none())
            .cloned();
        let mut next = Self::from_acp(config_options, None);
        if !next
            .0
            .iter()
            .any(|config| config.category.as_deref() == Some("mode"))
            && let Some(legacy) = legacy
        {
            next.0.push(legacy);
        }
        self.replace(next, sid)
    }

    fn select(
        &mut self,
        config_id: Option<&str>,
        value: &str,
        config_options: &[agent_client_protocol::schema::v1::SessionConfigOption],
        sid: &str,
    ) -> Option<ServiceMessage> {
        if !config_options.is_empty() {
            let _ = self.replace_options(config_options, sid);
        }
        let config = self
            .0
            .iter_mut()
            .find(|config| config.config_id.as_deref() == config_id)?;
        if config.current_value == value
            || !config.values.iter().any(|option| option.value == value)
        {
            return None;
        }
        config.current_value = value.to_string();
        Some(self.message(sid))
    }

    fn message(&self, sid: &str) -> ServiceMessage {
        ServiceMessage::AcpSessionConfigState {
            sid: sid.to_string(),
            configs: self.0.clone(),
        }
    }

    fn category(&self, category: &str) -> Option<&AcpSessionConfig> {
        self.0
            .iter()
            .find(|config| config.category.as_deref() == Some(category))
    }
}

#[derive(Clone, Default)]
struct AcpSelectionSnapshot {
    configs: Vec<AcpSessionConfig>,
}

struct AcpMcpServers(Vec<agent_client_protocol::schema::v1::McpServer>);

impl AcpMcpServers {
    fn from_sources(
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        managed: Vec<ManagedMcpServer>,
    ) -> Self {
        use agent_client_protocol::schema::v1::{McpServer, McpServerStdio};

        let mut servers = Vec::new();
        if let Some(command) = mcp_command {
            servers.push(McpServer::Stdio(
                McpServerStdio::new("vmux", PathBuf::from(command)).args(mcp_args),
            ));
        }
        for server in managed {
            if let Some(server) = Self::from_managed(server) {
                servers.push(server);
            }
        }
        Self(servers)
    }

    fn from_managed(
        server: ManagedMcpServer,
    ) -> Option<agent_client_protocol::schema::v1::McpServer> {
        use agent_client_protocol::schema::v1::{
            EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
        };

        if server.transport == ManagedMcpTransport::Stdio && server.cwd.is_some() {
            tracing::warn!(
                "managed MCP server {} skipped for ACP because ACP v1 does not support stdio cwd",
                server.name
            );
            return None;
        }
        let mut headers = Vec::new();
        for (name, value) in server.headers {
            headers.push(HttpHeader::new(name, value));
        }
        match server.transport {
            ManagedMcpTransport::Stdio => {
                let command = server.command?;
                let mut env = Vec::new();
                for (name, value) in server.env {
                    env.push(EnvVariable::new(name, value));
                }
                Some(McpServer::Stdio(
                    McpServerStdio::new(server.name, command)
                        .args(server.args)
                        .env(env),
                ))
            }
            ManagedMcpTransport::Http => server
                .url
                .map(|url| McpServer::Http(McpServerHttp::new(server.name, url).headers(headers))),
            ManagedMcpTransport::Sse => server
                .url
                .map(|url| McpServer::Sse(McpServerSse::new(server.name, url).headers(headers))),
        }
    }
}

#[derive(Clone)]
pub struct AcpSessions {
    spawns: mpsc::UnboundedSender<SpawnAcpSession>,
    inputs: mpsc::UnboundedSender<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedSender<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedSender<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedSender<AcpSessionAgentInfo>,
    config_states: mpsc::UnboundedSender<AcpSessionConfigRequest>,
    messages: mpsc::UnboundedSender<AcpSessionMessages>,
    lists: mpsc::UnboundedSender<ListAcpSessions>,
    lookups: mpsc::UnboundedSender<FindAcpSession>,
    rebinds: mpsc::UnboundedSender<RebindAcpSession>,
    closes: mpsc::UnboundedSender<CloseAcpSession>,
    wake: mpsc::UnboundedSender<()>,
}

impl AcpSessions {
    pub fn new(runtime: Handle, wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (inputs, input_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (agent_infos, agent_info_inbox) = mpsc::unbounded_channel();
        let (config_states, config_state_inbox) = mpsc::unbounded_channel();
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
                config_states,
                messages,
                lists,
                lookups,
                rebinds,
                closes,
                wake: wake.clone(),
            },
            (
                AcpSessionRuntime(runtime),
                AcpSessionWake(wake),
                AcpSessionInbox(AcpSessionReceivers {
                    spawns: spawn_inbox,
                    inputs: input_inbox,
                    subscriptions: subscription_inbox,
                    snapshots: snapshot_inbox,
                    agent_infos: agent_info_inbox,
                    config_states: config_state_inbox,
                    messages: message_inbox,
                    lists: list_inbox,
                    lookups: lookup_inbox,
                    rebinds: rebind_inbox,
                    closes: close_inbox,
                }),
            ),
        )
    }
    pub async fn spawn(
        &self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        anchor: ProcessId,
        processes: ProcessRuntime,
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        managed_mcp_servers: Vec<ManagedMcpServer>,
        resume: Option<String>,
    ) -> Result<(), String> {
        let mcp_servers = AcpMcpServers::from_sources(mcp_command, mcp_args, managed_mcp_servers);
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
                processes,
                mcp_servers: mcp_servers.0,
                resume,
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

    pub async fn input(&self, sid: String, input: AcpInput) -> bool {
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

    pub async fn subscribe(&self, sid: String) -> Option<broadcast::Receiver<ServiceMessage>> {
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

    pub async fn snapshot(&self, sid: String) -> Option<ServiceMessage> {
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

    pub async fn agent_info(&self, sid: String) -> Option<ServiceMessage> {
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

    pub async fn config_state(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.config_states
            .send(AcpSessionConfigRequest {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn remote_messages(&self, sid: String) -> Option<Vec<Message>> {
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

    pub async fn remote_sessions(&self) -> Vec<RemoteSession> {
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

    pub async fn remote_session(&self, sid: String) -> Option<RemoteSession> {
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

    pub async fn rebind_cwd(&self, sid: String, cwd: PathBuf) -> Result<(), String> {
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

    pub async fn close(&self, sid: String) -> bool {
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
}

struct AcpSessionReceivers {
    spawns: mpsc::UnboundedReceiver<SpawnAcpSession>,
    inputs: mpsc::UnboundedReceiver<AcpSessionInputRequest>,
    subscriptions: mpsc::UnboundedReceiver<SubscribeAcpSession>,
    snapshots: mpsc::UnboundedReceiver<SnapshotAcpSession>,
    agent_infos: mpsc::UnboundedReceiver<AcpSessionAgentInfo>,
    config_states: mpsc::UnboundedReceiver<AcpSessionConfigRequest>,
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
    mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
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
struct AcpProjectionInbox(mpsc::UnboundedReceiver<AcpProjectionInput>);

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
            let (projection_tx, projection_rx) = mpsc::unbounded_channel();
            let (stream_tx, _) = broadcast::channel(256);
            let shared = Arc::new(AcpShared::with_projection(
                request.sid.clone(),
                std::mem::take(&mut request.cwd),
                request.anchor,
                stream_tx,
                request.processes.clone(),
                projection_tx,
                wake.0.clone(),
            ));
            let task = runtime.0.spawn(driver::run(
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
                AcpProjectionInbox(projection_rx),
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

fn project(
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &mut AcpProjectionInbox,
        &mut AcpProjector,
        &mut AcpHistoryReplay,
        &mut AcpAgentName,
        &mut AcpSessionConfigs,
        &mut AcpRunState,
        &mut AcpApprovalState,
    )>,
) {
    for (
        sid,
        shared,
        mut inbox,
        mut projector,
        mut replay,
        mut agent_name,
        mut configs,
        mut run_state,
        mut approval,
    ) in &mut sessions
    {
        while let Ok(input) = inbox.0.try_recv() {
            match input {
                AcpProjectionInput::BeginHistoryReplay => {
                    *projector = AcpProjector::default();
                    replay.active = true;
                    replay.updates = 0;
                }
                AcpProjectionInput::Update(update) => {
                    match &update {
                        agent_client_protocol::schema::v1::SessionUpdate::ConfigOptionUpdate(
                            config,
                        ) => {
                            if let Some(message) =
                                configs.replace_options(&config.config_options, &sid.0)
                            {
                                shared.0.emit(message);
                            }
                        }
                        agent_client_protocol::schema::v1::SessionUpdate::CurrentModeUpdate(
                            current,
                        ) => {
                            if let Some(message) = configs.select(
                                None,
                                &current.current_mode_id.to_string(),
                                &[],
                                &sid.0,
                            ) {
                                shared.0.emit(message);
                            }
                        }
                        _ => {}
                    }
                    let intents = projector.apply(update);
                    shared
                        .0
                        .projector_updates
                        .send_modify(|revision| *revision += 1);
                    if replay.active {
                        for intent in &intents {
                            if let Intent::WorkspaceChanged(workspace) = intent {
                                shared.0.publish_workspace_change(workspace);
                            }
                        }
                        replay.updates += 1;
                        if replay.updates == 1
                            || replay
                                .updates
                                .is_multiple_of(driver::HISTORY_REPLAY_SNAPSHOT_INTERVAL)
                        {
                            shared
                                .0
                                .emit(shared.0.snapshot_message(projector.messages()));
                        }
                        continue;
                    }
                    for intent in intents {
                        match intent {
                            Intent::Delta(text) => {
                                shared
                                    .0
                                    .emit(ServiceMessage::Shared(SharedEvent::AgentDelta {
                                        sid: sid.0.clone(),
                                        text,
                                    }))
                            }
                            Intent::Snapshot => shared
                                .0
                                .emit(shared.0.snapshot_message(projector.messages())),
                            Intent::ProposedDiff {
                                call_id,
                                path,
                                old_text,
                                new_text,
                            } => shared.0.emit(ServiceMessage::AcpProposedDiff {
                                sid: sid.0.clone(),
                                call_id,
                                path,
                                old_text,
                                new_text,
                            }),
                            Intent::FileTouched { path, line, kind } => {
                                let Ok(request) = AgentRequest::encode(&AgentFileTouched {
                                    anchor: shared.0.anchor,
                                    path,
                                    line,
                                    col: None,
                                    end_col: None,
                                    kind,
                                }) else {
                                    continue;
                                };
                                shared.0.emit(ServiceMessage::AgentRequest {
                                    request_id: AgentRequestId::new(),
                                    anchor: Some(shared.0.anchor),
                                    request,
                                });
                            }
                            Intent::WorkspaceChanged(workspace) => {
                                shared.0.publish_workspace_change(&workspace)
                            }
                        }
                    }
                }
                AcpProjectionInput::FinishHistoryReplay(loaded) => {
                    if !loaded {
                        *projector = AcpProjector::default();
                    }
                    replay.active = false;
                    replay.updates = 0;
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpProjectionInput::PushUser { text, attachments } => {
                    projector.push_user(text, attachments);
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpProjectionInput::Snapshot => {
                    shared
                        .0
                        .emit(shared.0.snapshot_message(projector.messages()));
                }
                AcpProjectionInput::ApprovalDetails { query, response } => {
                    let _ = response.send(projector.approval_details(&query));
                }
                AcpProjectionInput::AgentInfo(name) => {
                    agent_name.0 = Some(name.clone());
                    shared
                        .0
                        .emit(ServiceMessage::Shared(SharedEvent::AcpAgentInfo {
                            sid: sid.0.clone(),
                            name,
                        }));
                }
                AcpProjectionInput::ConfigState {
                    config_options,
                    modes,
                } => {
                    if let Some(message) = configs.replace(
                        AcpSessionConfigs::from_acp(&config_options, modes.as_ref()),
                        &sid.0,
                    ) {
                        shared.0.emit(message);
                    }
                }
                AcpProjectionInput::SelectedConfig {
                    config_id,
                    value,
                    config_options,
                } => {
                    if let Some(message) =
                        configs.select(config_id.as_deref(), &value, &config_options, &sid.0)
                    {
                        shared.0.emit(message);
                    }
                }
                AcpProjectionInput::Status(status) => {
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
                AcpProjectionInput::ApprovalRequested(next) => {
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
                AcpProjectionInput::ApprovalResolved(call_id) => {
                    if approval
                        .0
                        .as_ref()
                        .is_some_and(|pending| pending.call_id == call_id)
                    {
                        approval.0 = None;
                        shared
                            .0
                            .emit(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved {
                                sid: sid.0.clone(),
                                call_id,
                            }));
                    }
                }
                AcpProjectionInput::SelectionSnapshot(response) => {
                    let _ = response.send(AcpSelectionSnapshot {
                        configs: configs.0.clone(),
                    });
                }
            }
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
    )>,
    mut snapshots: Query<(Entity, &mut SnapshotAcpSession)>,
    mut agent_infos: Query<(Entity, &mut AcpSessionAgentInfo)>,
    mut config_states: Query<(Entity, &mut AcpSessionConfigRequest)>,
    mut messages: Query<(Entity, &mut AcpSessionMessages)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut snapshots {
        let mut result = None;
        for (sid, shared, projector, _, _) in &sessions {
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
        for (sid, _, _, agent_name, _) in &sessions {
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
        for (sid, _, _, _, configs) in &sessions {
            if sid.0 == request.sid {
                result = Some(configs.message(&sid.0));
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
        for (sid, _, projector, _, _) in &sessions {
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
                url: format!("{}{}", vmux_chat::ChatPlugin::URL, sid.0),
                room_id: vmux_api::room::RoomId::for_session(&sid.0),
                title: vmux_session::ConversationTitle::from_messages(projector.messages(), &name),
                name,
                runtime: "acp".to_string(),
                model: configs
                    .category("model")
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
                    url: format!("{}{}", vmux_chat::ChatPlugin::URL, sid.0),
                    room_id: vmux_api::room::RoomId::for_session(&sid.0),
                    title: vmux_session::ConversationTitle::from_messages(
                        projector.messages(),
                        &name,
                    ),
                    name,
                    runtime: "acp".to_string(),
                    model: configs
                        .category("model")
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
