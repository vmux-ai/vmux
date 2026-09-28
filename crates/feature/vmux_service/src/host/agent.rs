use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};

use bevy::prelude::{
    App, ApplyDeferred, Bundle, Commands, Component, Entity, IntoScheduleConfigs, Name, Plugin,
    Query, Single, Update,
};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use vmux_core::agent::SessionId;
use vmux_core::{AgentWorkingDir, CreatedAt};

use super::request::PendingRequests;
use crate::providers::{anthropic, mistral, openai};
use crate::remote::{RemoteApproval, RemoteSession, RemoteStatus};
use crate::stream::{BuildRequest, ParseSse, StreamEvent, ToolDef};
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AGENT_QUERY_TIMEOUT, AGENT_REQUEST_TIMEOUT, AGENT_TOOL_TIMEOUT, AgentAttachment,
    AgentBrowserNavigate, AgentCommandResult, AgentRecordStop, AgentRequest, AgentRequestId,
    AgentRunStatus, ApprovalDecision, BROWSER_NAVIGATE_TIMEOUT, JsonValue, ProcessId,
    ServiceMessage, SharedEvent,
};
use vmux_api::room::{AssistantBlock, Message};

pub(crate) type AgentCommandResponses = PendingRequests<AgentRequestId, AgentCommandResult>;
pub(crate) type AgentQueryResponses = PendingRequests<AgentRequestId, ServiceMessage>;
pub(crate) type AgentToolResponses = PendingRequests<AgentRequestId, (String, bool)>;

const NO_AGENT_SUBSCRIBER: &str = "no desktop subscribed to agent commands";

#[derive(Clone)]
pub struct AgentBroker {
    outbound: broadcast::Sender<ServiceMessage>,
    commands: AgentCommandResponses,
    queries: AgentQueryResponses,
    tools: AgentToolResponses,
}

impl AgentBroker {
    pub(crate) fn new(
        outbound: broadcast::Sender<ServiceMessage>,
        commands: AgentCommandResponses,
        queries: AgentQueryResponses,
        tools: AgentToolResponses,
    ) -> Self {
        Self {
            outbound,
            commands,
            queries,
            tools,
        }
    }

    pub(crate) async fn command(
        &self,
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        request: AgentRequest,
    ) -> Result<AgentCommandResult, String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        let timeout = if request.id == AgentBrowserNavigate::ID {
            BROWSER_NAVIGATE_TIMEOUT
        } else {
            AGENT_REQUEST_TIMEOUT
        };
        self.commands
            .request(
                request_id,
                timeout,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentRequest {
                            request_id,
                            anchor,
                            request,
                        })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent command timed out",
            )
            .await
    }

    pub(crate) async fn query(
        &self,
        request_id: AgentRequestId,
        query: AgentRequest,
    ) -> Result<ServiceMessage, String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        let timeout = if query.id == AgentRecordStop::ID {
            vmux_api::protocol::RECORD_STOP_TIMEOUT
        } else {
            AGENT_QUERY_TIMEOUT
        };
        self.queries
            .request(
                request_id,
                timeout,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentQuery { request_id, query })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent query timed out",
            )
            .await
    }

    pub(crate) async fn tool_call(
        &self,
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args: JsonValue,
    ) -> Result<(String, bool), String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.tools
            .request(
                request_id,
                AGENT_TOOL_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentToolCall {
                            request_id,
                            sid,
                            name,
                            args,
                        })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent tool call timed out",
            )
            .await
    }

    pub(crate) async fn resolve_command(
        &self,
        request_id: AgentRequestId,
        result: AgentCommandResult,
    ) -> bool {
        self.commands.resolve(request_id, result).await
    }

    pub(crate) async fn resolve_tool(
        &self,
        request_id: AgentRequestId,
        content: String,
        is_error: bool,
    ) {
        self.tools.resolve(request_id, (content, is_error)).await;
    }
}

pub struct PageProvider {
    pub build_request: BuildRequest,
    pub parse_sse: ParseSse,
    pub env_var: &'static str,
}

pub fn resolve_provider(provider: &str) -> Option<PageProvider> {
    match provider {
        "anthropic" => Some(PageProvider {
            build_request: anthropic::build_request,
            parse_sse: anthropic::parse_sse,
            env_var: "ANTHROPIC_API_KEY",
        }),
        "openai" => Some(PageProvider {
            build_request: openai::build_request,
            parse_sse: openai::parse_sse,
            env_var: "OPENAI_API_KEY",
        }),
        "mistral" => Some(PageProvider {
            build_request: mistral::build_request,
            parse_sse: mistral::parse_sse,
            env_var: "MISTRAL_API_KEY",
        }),
        _ => None,
    }
}

pub enum SessionInput {
    User {
        text: String,
        attachments: Vec<AgentAttachment>,
    },
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    Cancel,
    Close,
}

pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                receive_agent_session_requests,
                ApplyDeferred,
                spawn_agent_sessions,
                route_agent_session_inputs,
                subscribe_agent_sessions,
                snapshot_agent_sessions,
                read_agent_session_messages,
                list_agent_sessions,
                find_agent_sessions,
                close_agent_sessions,
            )
                .chain(),
        );
    }
}

#[derive(Clone)]
pub(crate) struct AgentSessions {
    spawns: mpsc::UnboundedSender<SpawnAgentSession>,
    inputs: mpsc::UnboundedSender<AgentSessionInputRequest>,
    subscriptions: mpsc::UnboundedSender<SubscribeAgentSession>,
    snapshots: mpsc::UnboundedSender<SnapshotAgentSession>,
    messages: mpsc::UnboundedSender<AgentSessionMessages>,
    lists: mpsc::UnboundedSender<ListAgentSessions>,
    lookups: mpsc::UnboundedSender<FindAgentSession>,
    closes: mpsc::UnboundedSender<CloseAgentSession>,
    wake: mpsc::UnboundedSender<()>,
}

impl AgentSessions {
    pub(crate) fn new(runtime: Handle, wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (inputs, input_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (messages, message_inbox) = mpsc::unbounded_channel();
        let (lists, list_inbox) = mpsc::unbounded_channel();
        let (lookups, lookup_inbox) = mpsc::unbounded_channel();
        let (closes, close_inbox) = mpsc::unbounded_channel();
        (
            Self {
                spawns,
                inputs,
                subscriptions,
                snapshots,
                messages,
                lists,
                lookups,
                closes,
                wake,
            },
            (
                AgentSessionRuntime(runtime),
                AgentSessionInbox(AgentSessionReceivers {
                    spawns: spawn_inbox,
                    inputs: input_inbox,
                    subscriptions: subscription_inbox,
                    snapshots: snapshot_inbox,
                    messages: message_inbox,
                    lists: list_inbox,
                    lookups: lookup_inbox,
                    closes: close_inbox,
                }),
            ),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn(
        &self,
        sid: String,
        provider: String,
        model: String,
        cwd: String,
        tools: Vec<ToolDef>,
        auto_tools: HashSet<String>,
        broker: AgentBroker,
    ) -> Result<(), String> {
        let (response, receiver) = oneshot::channel();
        self.spawns
            .send(SpawnAgentSession {
                sid,
                provider,
                model,
                cwd,
                tools,
                auto_tools,
                broker,
                response: Some(response),
            })
            .map_err(|_| "agent session runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "agent session runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "agent session spawn was cancelled".to_string())?
    }

    pub(crate) async fn input(&self, sid: String, input: SessionInput) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .inputs
            .send(AgentSessionInputRequest {
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
            .send(SubscribeAgentSession {
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
            .send(SnapshotAgentSession {
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
            .send(AgentSessionMessages {
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
            .send(ListAgentSessions {
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
            .send(FindAgentSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub(crate) async fn close(&self, sid: String) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .closes
            .send(CloseAgentSession {
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
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (inputs, input_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (messages, message_inbox) = mpsc::unbounded_channel();
        let (lists, list_inbox) = mpsc::unbounded_channel();
        let (lookups, lookup_inbox) = mpsc::unbounded_channel();
        let (closes, close_inbox) = mpsc::unbounded_channel();
        let (wake, wake_inbox) = mpsc::unbounded_channel();
        drop((
            spawn_inbox,
            input_inbox,
            subscription_inbox,
            snapshot_inbox,
            message_inbox,
            list_inbox,
            lookup_inbox,
            close_inbox,
            wake_inbox,
        ));
        Self {
            spawns,
            inputs,
            subscriptions,
            snapshots,
            messages,
            lists,
            lookups,
            closes,
            wake,
        }
    }
}

struct AgentSessionReceivers {
    spawns: mpsc::UnboundedReceiver<SpawnAgentSession>,
    inputs: mpsc::UnboundedReceiver<AgentSessionInputRequest>,
    subscriptions: mpsc::UnboundedReceiver<SubscribeAgentSession>,
    snapshots: mpsc::UnboundedReceiver<SnapshotAgentSession>,
    messages: mpsc::UnboundedReceiver<AgentSessionMessages>,
    lists: mpsc::UnboundedReceiver<ListAgentSessions>,
    lookups: mpsc::UnboundedReceiver<FindAgentSession>,
    closes: mpsc::UnboundedReceiver<CloseAgentSession>,
}

#[derive(Component)]
struct AgentSessionInbox(AgentSessionReceivers);

#[derive(Component)]
struct AgentSessionRuntime(Handle);

#[derive(Component)]
struct SpawnAgentSession {
    sid: String,
    provider: String,
    model: String,
    cwd: String,
    tools: Vec<ToolDef>,
    auto_tools: HashSet<String>,
    broker: AgentBroker,
    response: Option<oneshot::Sender<Result<(), String>>>,
}

#[derive(Component)]
struct AgentSessionInputRequest {
    sid: String,
    input: Option<SessionInput>,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct SubscribeAgentSession {
    sid: String,
    response: Option<oneshot::Sender<Option<broadcast::Receiver<ServiceMessage>>>>,
}

#[derive(Component)]
struct SnapshotAgentSession {
    sid: String,
    response: Option<oneshot::Sender<Option<ServiceMessage>>>,
}

#[derive(Component)]
struct AgentSessionMessages {
    sid: String,
    response: Option<oneshot::Sender<Option<Vec<Message>>>>,
}

#[derive(Component)]
struct ListAgentSessions {
    response: Option<oneshot::Sender<Vec<RemoteSession>>>,
}

#[derive(Component)]
struct FindAgentSession {
    sid: String,
    response: Option<oneshot::Sender<Option<RemoteSession>>>,
}

#[derive(Component)]
struct CloseAgentSession {
    sid: String,
    response: Option<oneshot::Sender<bool>>,
}

#[derive(Component)]
struct AgentSessionInput(mpsc::UnboundedSender<SessionInput>);

#[derive(Component)]
struct AgentSessionStream(broadcast::Sender<ServiceMessage>);

#[derive(Component)]
struct AgentSessionHistory(Arc<Mutex<Vec<Message>>>);

#[derive(Component)]
struct AgentSessionTask(tokio::task::JoinHandle<()>);

impl Drop for AgentSessionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Component)]
struct AgentSessionProvider(String);

#[derive(Component)]
struct AgentSessionModel(String);

#[derive(Component)]
struct AgentSessionStatus(Arc<StdMutex<AgentRunStatus>>);

#[derive(Component)]
struct AgentSessionApproval(Arc<StdMutex<Option<RemoteApproval>>>);

fn receive_agent_session_requests(
    mut inbox: Single<&mut AgentSessionInbox>,
    mut commands: Commands,
) {
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
    while let Ok(request) = inbox.0.messages.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.lists.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.lookups.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.closes.try_recv() {
        commands.spawn(request);
    }
}

fn spawn_agent_sessions(
    runtime: Single<&AgentSessionRuntime>,
    sessions: Query<&SessionId>,
    mut requests: Query<(Entity, &mut SpawnAgentSession)>,
    mut commands: Commands,
) {
    let mut session_ids: HashSet<String> = sessions.iter().map(|sid| sid.0.clone()).collect();
    for (request_entity, mut request) in &mut requests {
        let result = if session_ids.contains(&request.sid) {
            Ok(())
        } else if let Some(provider) = resolve_provider(&request.provider) {
            let (input_tx, input_rx) = mpsc::unbounded_channel();
            let (stream_tx, _) = broadcast::channel(256);
            let messages = Arc::new(Mutex::new(Vec::new()));
            let status = Arc::new(StdMutex::new(AgentRunStatus::Idle));
            let approval = Arc::new(StdMutex::new(None));
            let task = runtime.0.spawn(run_session(
                request.sid.clone(),
                provider,
                request.model.clone(),
                std::mem::take(&mut request.tools),
                std::mem::take(&mut request.auto_tools),
                input_rx,
                stream_tx.clone(),
                request.broker.clone(),
                messages.clone(),
                status.clone(),
                approval.clone(),
            ));
            commands.spawn((
                Name::new(format!("agent session {}", request.sid)),
                SessionId(request.sid.clone()),
                AgentSessionInput(input_tx),
                AgentSessionStream(stream_tx),
                AgentSessionHistory(messages),
                AgentSessionTask(task),
                AgentSessionProvider(request.provider.clone()),
                AgentSessionModel(request.model.clone()),
                AgentWorkingDir(request.cwd.clone()),
                AgentSessionStatus(status),
                AgentSessionApproval(approval),
                CreatedAt::now(),
            ));
            session_ids.insert(request.sid.clone());
            Ok(())
        } else {
            Err(format!("unknown page-agent provider: {}", request.provider))
        };
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn route_agent_session_inputs(
    sessions: Query<(
        &SessionId,
        &AgentSessionInput,
        &AgentSessionStream,
        &AgentSessionApproval,
    )>,
    mut requests: Query<(Entity, &mut AgentSessionInputRequest)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut accepted = false;
        for (sid, input, stream, approval) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            if let Some(SessionInput::Approve { call_id, .. }) = &request.input {
                let mut pending = approval.0.lock().unwrap();
                if pending
                    .as_ref()
                    .is_some_and(|pending| pending.call_id == *call_id)
                {
                    *pending = None;
                    let _ =
                        stream
                            .0
                            .send(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved {
                                sid: request.sid.clone(),
                                call_id: call_id.clone(),
                            }));
                }
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

fn subscribe_agent_sessions(
    sessions: Query<(&SessionId, &AgentSessionStream)>,
    mut requests: Query<(Entity, &mut SubscribeAgentSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let receiver = sessions
            .iter()
            .find(|(sid, _)| sid.0 == request.sid)
            .map(|(_, stream)| stream.0.subscribe());
        if let Some(response) = request.response.take() {
            let _ = response.send(receiver);
        }
        commands.entity(request_entity).despawn();
    }
}

fn snapshot_agent_sessions(
    sessions: Query<(&SessionId, &AgentSessionHistory)>,
    mut requests: Query<(Entity, &mut SnapshotAgentSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut snapshot = None;
        let mut pending = false;
        for (sid, history) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let Ok(messages) = history.0.try_lock() else {
                pending = true;
                break;
            };
            snapshot = Some(ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot {
                sid: request.sid.clone(),
                messages: messages.clone(),
            }));
            break;
        }
        if pending {
            continue;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(snapshot);
        }
        commands.entity(request_entity).despawn();
    }
}

fn read_agent_session_messages(
    sessions: Query<(&SessionId, &AgentSessionHistory)>,
    mut requests: Query<(Entity, &mut AgentSessionMessages)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut result = None;
        let mut pending = false;
        for (sid, history) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let Ok(messages) = history.0.try_lock() else {
                pending = true;
                break;
            };
            result = Some(messages.clone());
            break;
        }
        if pending {
            continue;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

type AgentSessionQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SessionId,
        &'static AgentSessionProvider,
        &'static AgentSessionModel,
        &'static AgentWorkingDir,
        &'static AgentSessionStatus,
        &'static AgentSessionApproval,
        &'static CreatedAt,
    ),
>;

fn list_agent_sessions(
    sessions: AgentSessionQuery<'_, '_>,
    mut requests: Query<(Entity, &mut ListAgentSessions)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let result = sessions.iter().map(remote_session).collect();
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn find_agent_sessions(
    sessions: AgentSessionQuery<'_, '_>,
    mut requests: Query<(Entity, &mut FindAgentSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let result = sessions
            .iter()
            .find(|(sid, ..)| sid.0 == request.sid)
            .map(remote_session);
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(request_entity).despawn();
    }
}

fn close_agent_sessions(
    sessions: Query<(Entity, &SessionId, &AgentSessionInput)>,
    mut requests: Query<(Entity, &mut CloseAgentSession)>,
    mut commands: Commands,
) {
    for (request_entity, mut request) in &mut requests {
        let mut closed = false;
        for (session_entity, sid, input) in &sessions {
            if sid.0 != request.sid {
                continue;
            }
            let _ = input.0.send(SessionInput::Close);
            commands.entity(session_entity).despawn();
            closed = true;
            break;
        }
        if let Some(response) = request.response.take() {
            let _ = response.send(closed);
        }
        commands.entity(request_entity).despawn();
    }
}

fn remote_session(
    (sid, provider, model, cwd, status, approval, created_at): (
        &SessionId,
        &AgentSessionProvider,
        &AgentSessionModel,
        &AgentWorkingDir,
        &AgentSessionStatus,
        &AgentSessionApproval,
        &CreatedAt,
    ),
) -> RemoteSession {
    RemoteSession {
        sid: sid.0.clone(),
        room_id: vmux_api::room::RoomId::for_session(&sid.0),
        title: provider.0.clone(),
        name: provider.0.clone(),
        runtime: "page".to_string(),
        model: Some(model.0.clone()),
        cwd: cwd.0.clone(),
        status: RemoteStatus::from(&*status.0.lock().unwrap()),
        approval: approval.0.lock().unwrap().clone(),
        created_at_ms: created_at.0.max(0) as u64,
    }
}

async fn snapshot_message(sid: &str, messages: &Arc<Mutex<Vec<Message>>>) -> ServiceMessage {
    let messages = messages.lock().await;
    ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot {
        sid: sid.to_string(),
        messages: messages.clone(),
    })
}

fn spawn_sse(
    request: reqwest::Request,
    parse: ParseSse,
) -> (
    mpsc::UnboundedReceiver<StreamEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (cb_tx, cb_rx) = crossbeam_channel::unbounded::<StreamEvent>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<StreamEvent>();
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = cb_rx.recv() {
            if ev_tx.send(event).is_err() {
                break;
            }
        }
    });
    let http = tokio::spawn(async move {
        crate::http::drive_sse(request, parse, cb_tx).await;
    });
    (ev_rx, http)
}

fn append_text(blocks: &mut Vec<AssistantBlock>, text: &str) {
    if let Some(AssistantBlock::Text(buf)) = blocks.last_mut() {
        buf.push_str(text);
    } else {
        blocks.push(AssistantBlock::Text(text.to_string()));
    }
}

enum Decision {
    Allow,
    Deny,
    Cancelled,
    Closed,
}

async fn recv_user(
    input_rx: &mut mpsc::UnboundedReceiver<SessionInput>,
) -> Option<(String, Vec<AgentAttachment>)> {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::User { text, attachments }) => return Some((text, attachments)),
            Some(SessionInput::Approve { .. }) | Some(SessionInput::Cancel) => continue,
            Some(SessionInput::Close) | None => return None,
        }
    }
}

async fn await_decision(
    input_rx: &mut mpsc::UnboundedReceiver<SessionInput>,
    call_id: &str,
) -> Decision {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::Approve {
                call_id: cid,
                decision,
            }) if cid == call_id => {
                return match decision {
                    ApprovalDecision::Allow | ApprovalDecision::AllowAlways => Decision::Allow,
                    ApprovalDecision::Deny => Decision::Deny,
                };
            }
            Some(SessionInput::Cancel) => return Decision::Cancelled,
            Some(SessionInput::Approve { .. }) | Some(SessionInput::User { .. }) => continue,
            Some(SessionInput::Close) | None => return Decision::Closed,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    sid: String,
    provider: PageProvider,
    model: String,
    tools: Vec<ToolDef>,
    auto_tools: HashSet<String>,
    mut input_rx: mpsc::UnboundedReceiver<SessionInput>,
    stream_tx: broadcast::Sender<ServiceMessage>,
    broker: AgentBroker,
    messages: Arc<Mutex<Vec<Message>>>,
    status: Arc<StdMutex<AgentRunStatus>>,
    approval: Arc<StdMutex<Option<RemoteApproval>>>,
) {
    let api_key: Option<String> = if provider.env_var.is_empty() {
        Some(String::new())
    } else {
        std::env::var(provider.env_var).ok()
    };

    loop {
        let Some((text, attachments)) = recv_user(&mut input_rx).await else {
            return;
        };
        messages
            .lock()
            .await
            .push(Message::user_with_attachments(text, attachments));

        let Some(key) = api_key.as_deref() else {
            emit_status(
                &sid,
                AgentRunStatus::Errored(format!("Missing {}", provider.env_var)),
                &stream_tx,
                &status,
                &approval,
            );
            continue;
        };

        loop {
            let request = {
                let msgs = messages.lock().await;
                (provider.build_request)(&model, msgs.as_slice(), &tools, key)
            };
            emit_status(
                &sid,
                AgentRunStatus::Streaming,
                &stream_tx,
                &status,
                &approval,
            );

            let (mut ev_rx, http) = spawn_sse(request, provider.parse_sse);
            let mut blocks: Vec<AssistantBlock> = Vec::new();
            let mut partial: Option<(String, String, String)> = None;
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut errored: Option<String> = None;
            let mut cancelled = false;

            loop {
                tokio::select! {
                    biased;
                    signal = input_rx.recv() => {
                        match signal {
                            Some(SessionInput::Cancel) => {
                                cancelled = true;
                                break;
                            }
                            Some(SessionInput::Close) | None => {
                                http.abort();
                                return;
                            }
                            Some(SessionInput::User { .. }) | Some(SessionInput::Approve { .. }) => {}
                        }
                    }
                    event = ev_rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        match event {
                            StreamEvent::TextDelta(text) => {
                                append_text(&mut blocks, &text);
                                let _ = stream_tx.send(ServiceMessage::Shared(SharedEvent::AgentDelta {
                                    sid: sid.clone(),
                                    text,
                                }));
                            }
                            StreamEvent::ToolUseStart { call_id, name } => {
                                partial = Some((call_id, name, String::new()));
                            }
                            StreamEvent::ToolUseArgsDelta {
                                call_id,
                                json_chunk,
                            } => {
                                if let Some(p) = &mut partial {
                                    if p.0.is_empty() && !call_id.is_empty() {
                                        p.0 = call_id;
                                    }
                                    p.2.push_str(&json_chunk);
                                }
                            }
                            StreamEvent::ToolUseEnd { call_id } => {
                                if let Some((mut cid, name, args)) = partial.take() {
                                    if cid.is_empty() && !call_id.is_empty() {
                                        cid = call_id;
                                    }
                                    blocks.push(AssistantBlock::ToolUse {
                                        call_id: cid.clone(),
                                        name: name.clone(),
                                        args: args.clone(),
                                        parent_call_id: None,
                                    });
                                    pending_tool = Some((cid, name, args));
                                }
                            }
                            StreamEvent::StopTurn { .. } => {}
                            StreamEvent::Error(msg) => errored = Some(msg),
                        }
                    }
                }
            }

            if cancelled {
                http.abort();
            }
            if !blocks.is_empty() {
                messages.lock().await.push(Message::Assistant { blocks });
            }
            if cancelled {
                let _ = stream_tx.send(snapshot_message(&sid, &messages).await);
                emit_status(
                    &sid,
                    AgentRunStatus::Interrupted,
                    &stream_tx,
                    &status,
                    &approval,
                );
                break;
            }

            if let Some(msg) = errored {
                emit_status(
                    &sid,
                    AgentRunStatus::Errored(msg),
                    &stream_tx,
                    &status,
                    &approval,
                );
                break;
            }

            let _ = stream_tx.send(snapshot_message(&sid, &messages).await);

            let Some((call_id, name, args)) = pending_tool else {
                emit_status(&sid, AgentRunStatus::Idle, &stream_tx, &status, &approval);
                break;
            };

            let args = if args.trim().is_empty() {
                JsonValue::Object(Vec::new())
            } else {
                JsonValue::parse_or_string(&args)
            };

            if !auto_tools.contains(&name) {
                let next_approval = RemoteApproval {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    args: args.clone(),
                };
                *approval.lock().unwrap() = Some(next_approval.clone());
                let _ =
                    stream_tx.send(ServiceMessage::Shared(SharedEvent::AgentAwaitingApproval {
                        sid: sid.clone(),
                        call_id: call_id.clone(),
                        name: name.clone(),
                        args: args.clone(),
                    }));
                match await_decision(&mut input_rx, &call_id).await {
                    Decision::Closed => return,
                    Decision::Cancelled => {
                        emit_status(
                            &sid,
                            AgentRunStatus::Interrupted,
                            &stream_tx,
                            &status,
                            &approval,
                        );
                        break;
                    }
                    Decision::Deny => {
                        *approval.lock().unwrap() = None;
                        messages.lock().await.push(Message::ToolResult {
                            call_id,
                            content: "Tool call denied by user.".to_string(),
                            is_error: true,
                        });
                        continue;
                    }
                    Decision::Allow => *approval.lock().unwrap() = None,
                }
            }

            let (content, is_error) = match broker
                .tool_call(AgentRequestId::new(), sid.clone(), name, args)
                .await
            {
                Ok(result) => result,
                Err(e) => (e, true),
            };
            messages.lock().await.push(Message::ToolResult {
                call_id,
                content,
                is_error,
            });
            let _ = stream_tx.send(snapshot_message(&sid, &messages).await);
        }
    }
}

fn emit_status(
    sid: &str,
    next: AgentRunStatus,
    stream_tx: &broadcast::Sender<ServiceMessage>,
    status: &StdMutex<AgentRunStatus>,
    approval: &StdMutex<Option<RemoteApproval>>,
) {
    if !matches!(next, AgentRunStatus::Streaming) {
        *approval.lock().unwrap() = None;
    }
    *status.lock().unwrap() = next.clone();
    let _ = stream_tx.send(ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged {
        sid: sid.to_string(),
        status: next,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_broker() -> AgentBroker {
        let (agent_tx, _) = broadcast::channel::<ServiceMessage>(16);
        AgentBroker::new(
            agent_tx,
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    #[test]
    fn resolve_provider_known_and_unknown() {
        assert!(resolve_provider("anthropic").is_some());
        assert!(resolve_provider("openai").is_some());
        assert!(resolve_provider("mistral").is_some());
        assert!(resolve_provider("nope").is_none());
    }

    #[test]
    fn session_lifecycle_is_entity_owned() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let (wake, _wake_inbox) = mpsc::unbounded_channel();
        let (_sessions, session_runtime) = AgentSessions::new(runtime.handle().clone(), wake);
        let mut app = App::new();
        app.add_plugins(AgentSessionPlugin);
        app.world_mut()
            .spawn((Name::new("agent session runtime"), session_runtime));
        let (spawn_response, mut spawn_result) = oneshot::channel();
        app.world_mut().spawn(SpawnAgentSession {
            sid: "s".into(),
            provider: "openai".into(),
            model: "gpt-test".into(),
            cwd: "/tmp/project".into(),
            tools: Vec::new(),
            auto_tools: HashSet::new(),
            broker: test_broker(),
            response: Some(spawn_response),
        });

        app.update();

        assert_eq!(spawn_result.try_recv().unwrap(), Ok(()));
        let mut sessions = app.world_mut().query::<&SessionId>();
        assert_eq!(sessions.iter(app.world()).count(), 1);

        let (close_response, mut close_result) = oneshot::channel();
        app.world_mut().spawn(CloseAgentSession {
            sid: "s".into(),
            response: Some(close_response),
        });
        app.update();

        assert!(close_result.try_recv().unwrap());
        assert_eq!(sessions.iter(app.world()).count(), 0);
    }
}
