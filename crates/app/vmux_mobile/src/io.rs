use std::sync::Arc;

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query};
use bevy_tasks::{IoTaskPool, Task, TaskPool, futures_lite::future};
use tokio::sync::Notify;
use vmux_api::protocol::{AgentAttachment, ApprovalDecision};
use vmux_api::session::{
    ApprovalRequest, ClientOpId, ModelOptionEntry, NewChatRequest, PromptRequest, RemoteAgent,
    RemoteEvent, RemoteMediaEntry, RemoteModelState, RemoteSession, SessionId,
};
use vmux_api::team::TeamMemberRow;

use crate::credentials::{Credentials, CredentialsPath};
use crate::quic::{StreamFrame, Subscription};
use crate::remote::{Api, ApiError};
use crate::{CollectSet, SubmitSet};

pub(crate) struct MobileIoPlugin;

impl BevyPlugin for MobileIoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (
                collect_load_credentials,
                collect_persist_credentials,
                collect_remove_credentials,
                collect_open_connection,
                collect_close_connection,
                collect_reset_connection,
                collect_read_sessions,
                collect_read_agents,
                collect_read_upgraded,
                collect_open_stream,
                collect_read_stream_event,
                collect_create_chat,
                collect_send_prompt,
                collect_cancel_chat,
                collect_approve_call,
                collect_select_model,
                collect_set_effort,
                collect_read_models,
                collect_read_media,
                collect_read_team,
            )
                .in_set(CollectSet),
        )
        .add_systems(
            PreUpdate,
            (
                submit_load_credentials,
                submit_persist_credentials,
                submit_remove_credentials,
                submit_open_connection,
                submit_close_connection,
                submit_reset_connection,
                submit_read_sessions,
                submit_read_agents,
                submit_read_upgraded,
                submit_open_stream,
                submit_read_stream_event,
                submit_create_chat,
                submit_send_prompt,
                submit_cancel_chat,
                submit_approve_call,
                submit_select_model,
                submit_set_effort,
                submit_read_models,
                submit_read_media,
                submit_read_team,
            )
                .in_set(SubmitSet),
        );
    }
}

#[derive(Clone)]
pub(crate) struct ConnectionHandle {
    #[cfg(test)]
    id: u64,
    api: Option<Api>,
}

impl ConnectionHandle {
    pub(crate) fn live(api: Api) -> Self {
        Self {
            #[cfg(test)]
            id: 0,
            api: Some(api),
        }
    }

    #[cfg(test)]
    pub(crate) fn fake(id: u64) -> Self {
        Self { id, api: None }
    }

    #[cfg(test)]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn seal(&self) {
        if let Some(api) = &self.api {
            api.seal();
        }
    }

    fn require_api(&self) -> Result<Api, ApiError> {
        self.api
            .clone()
            .ok_or_else(|| ApiError::Message("connection handle is closed".to_string()))
    }
}

pub(crate) struct StreamHandle {
    subscription: Option<Subscription>,
}

impl StreamHandle {
    pub(crate) fn live(subscription: Subscription) -> Self {
        Self {
            subscription: Some(subscription),
        }
    }

    #[cfg(test)]
    pub(crate) fn fake() -> Self {
        Self { subscription: None }
    }

    pub(crate) fn stop(&mut self) {
        if let Some(subscription) = &mut self.subscription {
            subscription.stop();
        }
    }
}

#[derive(Component, Clone, Default)]
pub(crate) struct StreamCancel(Arc<Notify>);

impl StreamCancel {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Notify::new()))
    }

    pub(crate) fn cancel(&self) {
        self.0.notify_one();
    }

    pub(crate) async fn wait(&self) {
        self.0.notified().await;
    }
}

#[derive(Debug)]
pub(crate) enum StreamReadOutcome {
    Event(RemoteEvent),
    Malformed,
    Ended,
    Refused(ApiError),
    Canceled,
}

#[derive(Component)]
pub(crate) struct LoadCredentialsJob {
    pub(crate) path: CredentialsPath,
}

#[derive(Component)]
pub(crate) struct LoadCredentialsReceipt(pub(crate) Result<Option<Credentials>, String>);

#[derive(Component)]
struct LoadCredentialsTask(Task<Result<Option<Credentials>, String>>);

#[derive(Component)]
pub(crate) struct PersistCredentialsJob {
    pub(crate) path: CredentialsPath,
    pub(crate) credentials: Credentials,
}

#[derive(Component)]
pub(crate) struct PersistCredentialsReceipt(pub(crate) Result<(), String>);

#[derive(Component)]
struct PersistCredentialsTask(Task<Result<(), String>>);

#[derive(Component)]
pub(crate) struct RemoveCredentialsJob {
    pub(crate) path: CredentialsPath,
}

#[derive(Component)]
pub(crate) struct RemoveCredentialsReceipt(pub(crate) Result<(), String>);

#[derive(Component)]
struct RemoveCredentialsTask(Task<Result<(), String>>);

#[derive(Component)]
pub(crate) struct OpenConnectionJob {
    pub(crate) credentials: Credentials,
}

#[derive(Component)]
pub(crate) struct OpenConnectionReceipt(pub(crate) Result<ConnectionHandle, ApiError>);

#[derive(Component)]
struct OpenConnectionTask(Task<Result<ConnectionHandle, ApiError>>);

#[derive(Component)]
pub(crate) struct CloseConnectionJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct CloseConnectionReceipt;

#[derive(Component)]
struct CloseConnectionTask(Task<()>);

#[derive(Component)]
pub(crate) struct ResetConnectionJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct ResetConnectionReceipt;

#[derive(Component)]
struct ResetConnectionTask(Task<()>);

#[derive(Component)]
pub(crate) struct ReadSessionsJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct ReadSessionsReceipt(pub(crate) Result<Vec<RemoteSession>, ApiError>);

#[derive(Component)]
struct ReadSessionsTask(Task<Result<Vec<RemoteSession>, ApiError>>);

#[derive(Component)]
pub(crate) struct ReadAgentsJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct ReadAgentsReceipt(pub(crate) Result<Vec<RemoteAgent>, ApiError>);

#[derive(Component)]
struct ReadAgentsTask(Task<Result<Vec<RemoteAgent>, ApiError>>);

#[derive(Component)]
pub(crate) struct ReadUpgradedCredentialJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct ReadUpgradedCredentialReceipt(pub(crate) Option<Credentials>);

#[derive(Component)]
struct ReadUpgradedCredentialTask(Task<Option<Credentials>>);

#[derive(Component)]
pub(crate) struct OpenStreamJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
}

#[derive(Component)]
pub(crate) struct OpenStreamReceipt(pub(crate) Result<StreamHandle, ApiError>);

#[derive(Component)]
struct OpenStreamTask(Task<Result<StreamHandle, ApiError>>);

#[derive(Component)]
pub(crate) struct ReadStreamEventJob {
    pub(crate) stream: Option<StreamHandle>,
    pub(crate) cancel: StreamCancel,
}

#[derive(Component)]
pub(crate) struct ReadStreamEventReceipt {
    pub(crate) outcome: StreamReadOutcome,
    pub(crate) stream: Option<StreamHandle>,
}

#[derive(Component)]
struct ReadStreamEventTask(Task<ReadStreamEventReceipt>);

#[derive(Component)]
pub(crate) struct CreateChatJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) client_op_id: ClientOpId,
    pub(crate) text: String,
    pub(crate) agent_url: Option<String>,
}

#[derive(Component)]
pub(crate) struct CreateChatReceipt(pub(crate) Result<SessionId, ApiError>);

#[derive(Component)]
struct CreateChatTask(Task<Result<SessionId, ApiError>>);

#[derive(Component)]
pub(crate) struct SendPromptJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
    pub(crate) client_op_id: ClientOpId,
    pub(crate) text: String,
    pub(crate) attachments: Vec<AgentAttachment>,
}

#[derive(Component)]
pub(crate) struct SendPromptReceipt(pub(crate) Result<(), ApiError>);

#[derive(Component)]
struct SendPromptTask(Task<Result<(), ApiError>>);

#[derive(Component)]
pub(crate) struct CancelChatJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
}

#[derive(Component)]
pub(crate) struct CancelChatReceipt(pub(crate) Result<(), ApiError>);

#[derive(Component)]
struct CancelChatTask(Task<Result<(), ApiError>>);

#[derive(Component)]
pub(crate) struct ApproveCallJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
    pub(crate) call_id: String,
    pub(crate) decision: ApprovalDecision,
}

#[derive(Component)]
pub(crate) struct ApproveCallReceipt(pub(crate) Result<(), ApiError>);

#[derive(Component)]
struct ApproveCallTask(Task<Result<(), ApiError>>);

#[derive(Component)]
pub(crate) struct SelectRemoteModelJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
    pub(crate) model_id: String,
}

#[derive(Component)]
pub(crate) struct SelectRemoteModelReceipt(pub(crate) Result<(), ApiError>);

#[derive(Component)]
struct SelectRemoteModelTask(Task<Result<(), ApiError>>);

#[derive(Component)]
pub(crate) struct SetRemoteEffortJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
    pub(crate) level: String,
}

#[derive(Component)]
pub(crate) struct SetRemoteEffortReceipt(pub(crate) Result<(), ApiError>);

#[derive(Component)]
struct SetRemoteEffortTask(Task<Result<(), ApiError>>);

#[derive(Component)]
pub(crate) struct ReadModelsJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
}

#[derive(Component)]
pub(crate) struct ReadModelsReceipt {
    pub(crate) models: Vec<ModelOptionEntry>,
    pub(crate) selected_id: String,
    pub(crate) effort_levels: Vec<String>,
    pub(crate) effort: String,
    pub(crate) error: Option<ApiError>,
}

impl ReadModelsReceipt {
    pub(crate) fn from_result(result: Result<RemoteModelState, ApiError>) -> Self {
        match result {
            Ok(state) => Self {
                models: state.models,
                selected_id: state.selected_id,
                effort_levels: state.effort_levels,
                effort: state.effort,
                error: None,
            },
            Err(error) => Self {
                models: Vec::new(),
                selected_id: String::new(),
                effort_levels: Vec::new(),
                effort: String::new(),
                error: Some(error),
            },
        }
    }

    pub(crate) fn to_state(&self) -> RemoteModelState {
        RemoteModelState {
            models: self.models.clone(),
            selected_id: self.selected_id.clone(),
            effort_levels: self.effort_levels.clone(),
            effort: self.effort.clone(),
        }
    }
}

#[derive(Component)]
struct ReadModelsTask(Task<ReadModelsReceipt>);

#[derive(Component)]
pub(crate) struct ReadMediaJob {
    pub(crate) handle: ConnectionHandle,
    pub(crate) sid: SessionId,
    pub(crate) query: String,
}

#[derive(Component)]
pub(crate) struct ReadMediaReceipt(pub(crate) Result<Vec<RemoteMediaEntry>, ApiError>);

#[derive(Component)]
struct ReadMediaTask(Task<Result<Vec<RemoteMediaEntry>, ApiError>>);

#[derive(Component)]
pub(crate) struct ReadTeamJob {
    pub(crate) handle: ConnectionHandle,
}

#[derive(Component)]
pub(crate) struct ReadTeamReceipt(pub(crate) Result<Vec<TeamMemberRow>, ApiError>);

#[derive(Component)]
struct ReadTeamTask(Task<Result<Vec<TeamMemberRow>, ApiError>>);

struct IoRunner;

impl IoRunner {
    fn pool() -> &'static IoTaskPool {
        IoTaskPool::get_or_init(TaskPool::default)
    }

    fn poll<T>(task: &mut Task<T>) -> Option<T> {
        future::block_on(future::poll_once(task))
    }
}

fn submit_load_credentials(jobs: Query<(Entity, &LoadCredentialsJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let path = job.path.clone();
        let task = IoRunner::pool().spawn(async move { path.load() });
        commands
            .entity(entity)
            .remove::<LoadCredentialsJob>()
            .insert(LoadCredentialsTask(task));
    }
}

fn collect_load_credentials(
    mut tasks: Query<(Entity, &mut LoadCredentialsTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<LoadCredentialsTask>()
            .insert(LoadCredentialsReceipt(result));
    }
}

fn submit_persist_credentials(
    jobs: Query<(Entity, &PersistCredentialsJob)>,
    mut commands: Commands,
) {
    for (entity, job) in &jobs {
        let path = job.path.clone();
        let credentials = job.credentials.clone();
        let task = IoRunner::pool().spawn(async move { path.save(&credentials) });
        commands
            .entity(entity)
            .remove::<PersistCredentialsJob>()
            .insert(PersistCredentialsTask(task));
    }
}

fn collect_persist_credentials(
    mut tasks: Query<(Entity, &mut PersistCredentialsTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PersistCredentialsTask>()
            .insert(PersistCredentialsReceipt(result));
    }
}

fn submit_remove_credentials(jobs: Query<(Entity, &RemoveCredentialsJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let path = job.path.clone();
        let task = IoRunner::pool().spawn(async move { path.remove() });
        commands
            .entity(entity)
            .remove::<RemoveCredentialsJob>()
            .insert(RemoveCredentialsTask(task));
    }
}

fn collect_remove_credentials(
    mut tasks: Query<(Entity, &mut RemoveCredentialsTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<RemoveCredentialsTask>()
            .insert(RemoveCredentialsReceipt(result));
    }
}

fn submit_open_connection(jobs: Query<(Entity, &OpenConnectionJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let credentials = job.credentials.clone();
        let task = IoRunner::pool().spawn(async move {
            let api = Api::new(credentials)?;
            Ok(ConnectionHandle::live(api))
        });
        commands
            .entity(entity)
            .remove::<OpenConnectionJob>()
            .insert(OpenConnectionTask(task));
    }
}

fn collect_open_connection(
    mut tasks: Query<(Entity, &mut OpenConnectionTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<OpenConnectionTask>()
            .insert(OpenConnectionReceipt(result));
    }
}

fn submit_close_connection(jobs: Query<(Entity, &CloseConnectionJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        handle.seal();
        let task = IoRunner::pool().spawn(async move {
            if let Some(api) = handle.api {
                api.close_and_drain().await;
            }
        });
        commands
            .entity(entity)
            .remove::<CloseConnectionJob>()
            .insert(CloseConnectionTask(task));
    }
}

fn collect_close_connection(
    mut tasks: Query<(Entity, &mut CloseConnectionTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        if IoRunner::poll(&mut task.0).is_none() {
            continue;
        }
        commands
            .entity(entity)
            .remove::<CloseConnectionTask>()
            .insert(CloseConnectionReceipt);
    }
}

fn submit_reset_connection(jobs: Query<(Entity, &ResetConnectionJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let task = IoRunner::pool().spawn(async move {
            if let Some(api) = handle.api {
                api.reset_transport().await;
            }
        });
        commands
            .entity(entity)
            .remove::<ResetConnectionJob>()
            .insert(ResetConnectionTask(task));
    }
}

fn collect_reset_connection(
    mut tasks: Query<(Entity, &mut ResetConnectionTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        if IoRunner::poll(&mut task.0).is_none() {
            continue;
        }
        commands
            .entity(entity)
            .remove::<ResetConnectionTask>()
            .insert(ResetConnectionReceipt);
    }
}

fn submit_read_sessions(jobs: Query<(Entity, &ReadSessionsJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let task = IoRunner::pool().spawn(async move { handle.require_api()?.sessions().await });
        commands
            .entity(entity)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsTask(task));
    }
}

fn collect_read_sessions(
    mut tasks: Query<(Entity, &mut ReadSessionsTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadSessionsTask>()
            .insert(ReadSessionsReceipt(result));
    }
}

fn submit_read_agents(jobs: Query<(Entity, &ReadAgentsJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let task = IoRunner::pool().spawn(async move { handle.require_api()?.agents().await });
        commands
            .entity(entity)
            .remove::<ReadAgentsJob>()
            .insert(ReadAgentsTask(task));
    }
}

fn collect_read_agents(mut tasks: Query<(Entity, &mut ReadAgentsTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadAgentsTask>()
            .insert(ReadAgentsReceipt(result));
    }
}

fn submit_read_upgraded(jobs: Query<(Entity, &ReadUpgradedCredentialJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let task = IoRunner::pool().spawn(async move {
            let api = handle.api?;
            api.paired_credentials().await
        });
        commands
            .entity(entity)
            .remove::<ReadUpgradedCredentialJob>()
            .insert(ReadUpgradedCredentialTask(task));
    }
}

fn collect_read_upgraded(
    mut tasks: Query<(Entity, &mut ReadUpgradedCredentialTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadUpgradedCredentialTask>()
            .insert(ReadUpgradedCredentialReceipt(result));
    }
}

fn submit_open_stream(jobs: Query<(Entity, &OpenStreamJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let task = IoRunner::pool().spawn(async move {
            let subscription = handle.require_api()?.subscribe(&sid).await?;
            Ok(StreamHandle::live(subscription))
        });
        commands
            .entity(entity)
            .remove::<OpenStreamJob>()
            .insert(OpenStreamTask(task));
    }
}

fn collect_open_stream(mut tasks: Query<(Entity, &mut OpenStreamTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<OpenStreamTask>()
            .insert(OpenStreamReceipt(result));
    }
}

fn submit_read_stream_event(
    mut jobs: Query<(Entity, &mut ReadStreamEventJob)>,
    mut commands: Commands,
) {
    for (entity, mut job) in &mut jobs {
        let mut stream = job.stream.take();
        let cancel = job.cancel.clone();
        let task = IoRunner::pool().spawn(async move {
            let Some(mut handle) = stream.take() else {
                return ReadStreamEventReceipt {
                    outcome: StreamReadOutcome::Ended,
                    stream: None,
                };
            };
            let Some(subscription) = handle.subscription.as_mut() else {
                return ReadStreamEventReceipt {
                    outcome: StreamReadOutcome::Ended,
                    stream: None,
                };
            };
            let frame = tokio::select! {
                biased;
                _ = cancel.wait() => {
                    handle.stop();
                    return ReadStreamEventReceipt {
                        outcome: StreamReadOutcome::Canceled,
                        stream: None,
                    };
                }
                frame = subscription.next_frame() => frame,
            };
            match frame {
                StreamFrame::Event(shared) => match Api::event(*shared) {
                    Some(event) => ReadStreamEventReceipt {
                        outcome: StreamReadOutcome::Event(event),
                        stream: Some(handle),
                    },
                    None => ReadStreamEventReceipt {
                        outcome: StreamReadOutcome::Malformed,
                        stream: Some(handle),
                    },
                },
                StreamFrame::Malformed => ReadStreamEventReceipt {
                    outcome: StreamReadOutcome::Malformed,
                    stream: Some(handle),
                },
                StreamFrame::Ended => {
                    handle.stop();
                    ReadStreamEventReceipt {
                        outcome: StreamReadOutcome::Ended,
                        stream: None,
                    }
                }
                StreamFrame::Refused(error) => {
                    handle.stop();
                    ReadStreamEventReceipt {
                        outcome: StreamReadOutcome::Refused(error.into()),
                        stream: None,
                    }
                }
            }
        });
        commands
            .entity(entity)
            .remove::<ReadStreamEventJob>()
            .insert(ReadStreamEventTask(task));
    }
}

fn collect_read_stream_event(
    mut tasks: Query<(Entity, &mut ReadStreamEventTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(receipt) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadStreamEventTask>()
            .insert(receipt);
    }
}

fn submit_create_chat(jobs: Query<(Entity, &CreateChatJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let request = NewChatRequest {
            client_op_id: job.client_op_id.clone(),
            text: job.text.clone(),
            agent_url: job.agent_url.clone(),
        };
        let task = IoRunner::pool()
            .spawn(async move { handle.require_api()?.create_chat(&request).await });
        commands
            .entity(entity)
            .remove::<CreateChatJob>()
            .insert(CreateChatTask(task));
    }
}

fn collect_create_chat(mut tasks: Query<(Entity, &mut CreateChatTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CreateChatTask>()
            .insert(CreateChatReceipt(result));
    }
}

fn submit_send_prompt(jobs: Query<(Entity, &SendPromptJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let request = PromptRequest {
            client_op_id: job.client_op_id.clone(),
            text: job.text.clone(),
            attachments: job.attachments.clone(),
        };
        let task = IoRunner::pool()
            .spawn(async move { handle.require_api()?.send_prompt(&sid, &request).await });
        commands
            .entity(entity)
            .remove::<SendPromptJob>()
            .insert(SendPromptTask(task));
    }
}

fn collect_send_prompt(mut tasks: Query<(Entity, &mut SendPromptTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<SendPromptTask>()
            .insert(SendPromptReceipt(result));
    }
}

fn submit_cancel_chat(jobs: Query<(Entity, &CancelChatJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let task = IoRunner::pool().spawn(async move { handle.require_api()?.cancel(&sid).await });
        commands
            .entity(entity)
            .remove::<CancelChatJob>()
            .insert(CancelChatTask(task));
    }
}

fn collect_cancel_chat(mut tasks: Query<(Entity, &mut CancelChatTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CancelChatTask>()
            .insert(CancelChatReceipt(result));
    }
}

fn submit_approve_call(jobs: Query<(Entity, &ApproveCallJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let request = ApprovalRequest {
            call_id: job.call_id.clone(),
            decision: job.decision,
        };
        let task = IoRunner::pool()
            .spawn(async move { handle.require_api()?.approve(&sid, &request).await });
        commands
            .entity(entity)
            .remove::<ApproveCallJob>()
            .insert(ApproveCallTask(task));
    }
}

fn collect_approve_call(mut tasks: Query<(Entity, &mut ApproveCallTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ApproveCallTask>()
            .insert(ApproveCallReceipt(result));
    }
}

fn submit_select_model(jobs: Query<(Entity, &SelectRemoteModelJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let model_id = job.model_id.clone();
        let task = IoRunner::pool()
            .spawn(async move { handle.require_api()?.select_model(&sid, &model_id).await });
        commands
            .entity(entity)
            .remove::<SelectRemoteModelJob>()
            .insert(SelectRemoteModelTask(task));
    }
}

fn collect_select_model(
    mut tasks: Query<(Entity, &mut SelectRemoteModelTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<SelectRemoteModelTask>()
            .insert(SelectRemoteModelReceipt(result));
    }
}

fn submit_set_effort(jobs: Query<(Entity, &SetRemoteEffortJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let level = job.level.clone();
        let task = IoRunner::pool()
            .spawn(async move { handle.require_api()?.set_effort(&sid, &level).await });
        commands
            .entity(entity)
            .remove::<SetRemoteEffortJob>()
            .insert(SetRemoteEffortTask(task));
    }
}

fn collect_set_effort(
    mut tasks: Query<(Entity, &mut SetRemoteEffortTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<SetRemoteEffortTask>()
            .insert(SetRemoteEffortReceipt(result));
    }
}

fn submit_read_models(jobs: Query<(Entity, &ReadModelsJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let task = IoRunner::pool().spawn(async move {
            let result = match handle.require_api() {
                Ok(api) => api.models(&sid).await,
                Err(error) => Err(error),
            };
            ReadModelsReceipt::from_result(result)
        });
        commands
            .entity(entity)
            .remove::<ReadModelsJob>()
            .insert(ReadModelsTask(task));
    }
}

fn collect_read_models(mut tasks: Query<(Entity, &mut ReadModelsTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(receipt) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadModelsTask>()
            .insert(receipt);
    }
}

fn submit_read_media(jobs: Query<(Entity, &ReadMediaJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let sid = job.sid.0.clone();
        let query = job.query.clone();
        let task =
            IoRunner::pool().spawn(async move { handle.require_api()?.media(&sid, &query).await });
        commands
            .entity(entity)
            .remove::<ReadMediaJob>()
            .insert(ReadMediaTask(task));
    }
}

fn collect_read_media(mut tasks: Query<(Entity, &mut ReadMediaTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadMediaTask>()
            .insert(ReadMediaReceipt(result));
    }
}

fn submit_read_team(jobs: Query<(Entity, &ReadTeamJob)>, mut commands: Commands) {
    for (entity, job) in &jobs {
        let handle = job.handle.clone();
        let task = IoRunner::pool().spawn(async move { handle.require_api()?.team().await });
        commands
            .entity(entity)
            .remove::<ReadTeamJob>()
            .insert(ReadTeamTask(task));
    }
}

fn collect_read_team(mut tasks: Query<(Entity, &mut ReadTeamTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = IoRunner::poll(&mut task.0) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ReadTeamTask>()
            .insert(ReadTeamReceipt(result));
    }
}
