use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};

use crate::process::ProcessManager;
use vmux_api::protocol::{AgentCommandExit, AgentRunCompletion, ProcessId};

#[derive(Component)]
struct ProcessOutputQuery {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<String, String>>>,
}

#[derive(Component)]
struct ProcessTranscriptQuery {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<String, String>>>,
}

#[derive(Component)]
struct ProcessCommandExitQuery {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<AgentCommandExit, String>>>,
}

#[derive(Component)]
struct ProcessRunCompletionQuery {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<AgentRunCompletion, String>>>,
}

struct ProcessQueryReceivers {
    output: mpsc::UnboundedReceiver<ProcessOutputQuery>,
    transcript: mpsc::UnboundedReceiver<ProcessTranscriptQuery>,
    command_exit: mpsc::UnboundedReceiver<ProcessCommandExitQuery>,
    run_completion: mpsc::UnboundedReceiver<ProcessRunCompletionQuery>,
}

#[derive(Component)]
struct ProcessQueryInbox(Mutex<ProcessQueryReceivers>);

#[derive(Component)]
struct ProcessRuntime(Arc<AsyncMutex<ProcessManager>>);

#[derive(Clone)]
pub(crate) struct ProcessQueries {
    output: mpsc::UnboundedSender<ProcessOutputQuery>,
    transcript: mpsc::UnboundedSender<ProcessTranscriptQuery>,
    command_exit: mpsc::UnboundedSender<ProcessCommandExitQuery>,
    run_completion: mpsc::UnboundedSender<ProcessRunCompletionQuery>,
    wake: mpsc::UnboundedSender<ProcessId>,
}

impl ProcessQueries {
    fn new(wake: mpsc::UnboundedSender<ProcessId>) -> (Self, ProcessQueryReceivers) {
        let (output, output_rx) = mpsc::unbounded_channel();
        let (transcript, transcript_rx) = mpsc::unbounded_channel();
        let (command_exit, command_exit_rx) = mpsc::unbounded_channel();
        let (run_completion, run_completion_rx) = mpsc::unbounded_channel();
        (
            Self {
                output,
                transcript,
                command_exit,
                run_completion,
                wake,
            },
            ProcessQueryReceivers {
                output: output_rx,
                transcript: transcript_rx,
                command_exit: command_exit_rx,
                run_completion: run_completion_rx,
            },
        )
    }

    pub(crate) async fn output(&self, process_id: ProcessId) -> Result<String, String> {
        let (response, receiver) = oneshot::channel();
        self.output
            .send(ProcessOutputQuery {
                process_id,
                response: Some(response),
            })
            .map_err(|_| "service query runtime unavailable".to_string())?;
        self.wake
            .send(process_id)
            .map_err(|_| "service query runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "service query was cancelled".to_string())?
    }

    pub(crate) async fn transcript(&self, process_id: ProcessId) -> Result<String, String> {
        let (response, receiver) = oneshot::channel();
        self.transcript
            .send(ProcessTranscriptQuery {
                process_id,
                response: Some(response),
            })
            .map_err(|_| "service query runtime unavailable".to_string())?;
        self.wake
            .send(process_id)
            .map_err(|_| "service query runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "service query was cancelled".to_string())?
    }

    pub(crate) async fn command_exit(
        &self,
        process_id: ProcessId,
    ) -> Result<AgentCommandExit, String> {
        let (response, receiver) = oneshot::channel();
        self.command_exit
            .send(ProcessCommandExitQuery {
                process_id,
                response: Some(response),
            })
            .map_err(|_| "service query runtime unavailable".to_string())?;
        self.wake
            .send(process_id)
            .map_err(|_| "service query runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "service query was cancelled".to_string())?
    }

    pub(crate) async fn run_completion(
        &self,
        process_id: ProcessId,
    ) -> Result<AgentRunCompletion, String> {
        let (response, receiver) = oneshot::channel();
        self.run_completion
            .send(ProcessRunCompletionQuery {
                process_id,
                response: Some(response),
            })
            .map_err(|_| "service query runtime unavailable".to_string())?;
        self.wake
            .send(process_id)
            .map_err(|_| "service query runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "service query was cancelled".to_string())?
    }
}

pub(crate) struct ProcessQueryPlugin {
    manager: Arc<AsyncMutex<ProcessManager>>,
    receivers: Mutex<Option<ProcessQueryReceivers>>,
}

impl ProcessQueryPlugin {
    pub(crate) fn new(
        manager: Arc<AsyncMutex<ProcessManager>>,
        wake: mpsc::UnboundedSender<ProcessId>,
    ) -> (Self, ProcessQueries) {
        let (queries, receivers) = ProcessQueries::new(wake);
        (
            Self {
                manager,
                receivers: Mutex::new(Some(receivers)),
            },
            queries,
        )
    }
}

impl Plugin for ProcessQueryPlugin {
    fn build(&self, app: &mut App) {
        let receivers = self
            .receivers
            .lock()
            .unwrap()
            .take()
            .expect("daemon query plugin can only be built once");
        app.world_mut().spawn((
            Name::new("vmux service process runtime"),
            ProcessRuntime(Arc::clone(&self.manager)),
            ProcessQueryInbox(Mutex::new(receivers)),
        ));
        app.add_systems(
            Update,
            (
                receive_process_queries,
                ApplyDeferred,
                answer_process_output_queries,
                answer_process_transcript_queries,
                answer_command_exit_queries,
                answer_run_completion_queries,
                poll_service_processes,
            )
                .chain(),
        );
    }
}

fn receive_process_queries(inbox: Single<&ProcessQueryInbox>, mut commands: Commands) {
    let Ok(mut receivers) = inbox.0.lock() else {
        return;
    };
    while let Ok(request) = receivers.output.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.transcript.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.command_exit.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.run_completion.try_recv() {
        commands.spawn(request);
    }
}

fn answer_process_output_queries(
    processes: Single<&ProcessRuntime>,
    mut requests: Query<(Entity, &mut ProcessOutputQuery)>,
    mut commands: Commands,
) {
    let Ok(manager) = processes.0.try_lock() else {
        return;
    };
    for (entity, mut request) in &mut requests {
        let result = manager
            .processes
            .get(&request.process_id)
            .map(|process| process.visible_text())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn answer_process_transcript_queries(
    processes: Single<&ProcessRuntime>,
    mut requests: Query<(Entity, &mut ProcessTranscriptQuery)>,
    mut commands: Commands,
) {
    let Ok(manager) = processes.0.try_lock() else {
        return;
    };
    for (entity, mut request) in &mut requests {
        let result = manager
            .processes
            .get(&request.process_id)
            .map(|process| process.full_text())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn answer_command_exit_queries(
    processes: Single<&ProcessRuntime>,
    mut requests: Query<(Entity, &mut ProcessCommandExitQuery)>,
    mut commands: Commands,
) {
    let Ok(manager) = processes.0.try_lock() else {
        return;
    };
    for (entity, mut request) in &mut requests {
        let result = manager
            .processes
            .get(&request.process_id)
            .map(|process| {
                let (sequence, exit) = process.command_status();
                AgentCommandExit { sequence, exit }
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn answer_run_completion_queries(
    processes: Single<&ProcessRuntime>,
    mut requests: Query<(Entity, &mut ProcessRunCompletionQuery)>,
    mut commands: Commands,
) {
    let Ok(manager) = processes.0.try_lock() else {
        return;
    };
    for (entity, mut request) in &mut requests {
        let result = manager
            .processes
            .get(&request.process_id)
            .map(|process| {
                let (token, exit) = match process.run_completion() {
                    Some((token, exit)) => (Some(token), Some(exit)),
                    None => (None, None),
                };
                AgentRunCompletion { token, exit }
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn poll_service_processes(processes: Single<&ProcessRuntime>) {
    let Ok(mut manager) = processes.0.try_lock() else {
        return;
    };
    manager.reap_exited();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_process_query_is_answered_by_ecs() {
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let manager = Arc::new(AsyncMutex::new(ProcessManager::new(
            mpsc::unbounded_channel().0,
        )));
        let (plugin, queries) = ProcessQueryPlugin::new(manager, wake);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, plugin));
        let process_id = ProcessId::new();
        let query = tokio::spawn(async move { queries.output(process_id).await });
        for _ in 0..4 {
            tokio::task::yield_now().await;
            app.update();
            if query.is_finished() {
                break;
            }
        }

        assert_eq!(
            query.await.unwrap(),
            Err(format!("process not found: {process_id}"))
        );
    }
}
