use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};

use crate::process::ProcessManager;
use crate::protocol::ProcessId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandExitState {
    pub(crate) sequence: u64,
    pub(crate) exit: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunCompletionState {
    pub(crate) token: Option<String>,
    pub(crate) exit: Option<i32>,
}

#[derive(Component)]
struct ReadTerminal {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<String, String>>>,
}

#[derive(Component)]
struct ReadTerminalFull {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<String, String>>>,
}

#[derive(Component)]
struct ReadCommandExit {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<CommandExitState, String>>>,
}

#[derive(Component)]
struct ReadRunCompletion {
    process_id: ProcessId,
    response: Option<oneshot::Sender<Result<RunCompletionState, String>>>,
}

struct TerminalQueryReceivers {
    visible: mpsc::UnboundedReceiver<ReadTerminal>,
    full: mpsc::UnboundedReceiver<ReadTerminalFull>,
    command_exit: mpsc::UnboundedReceiver<ReadCommandExit>,
    run_completion: mpsc::UnboundedReceiver<ReadRunCompletion>,
}

#[derive(Component)]
struct TerminalQueryInbox(Mutex<TerminalQueryReceivers>);

#[derive(Component)]
struct ServiceProcesses(Arc<AsyncMutex<ProcessManager>>);

#[derive(Clone)]
pub(crate) struct TerminalQueryBridge {
    visible: mpsc::UnboundedSender<ReadTerminal>,
    full: mpsc::UnboundedSender<ReadTerminalFull>,
    command_exit: mpsc::UnboundedSender<ReadCommandExit>,
    run_completion: mpsc::UnboundedSender<ReadRunCompletion>,
    wake: mpsc::UnboundedSender<ProcessId>,
}

impl TerminalQueryBridge {
    fn new(wake: mpsc::UnboundedSender<ProcessId>) -> (Self, TerminalQueryReceivers) {
        let (visible, visible_rx) = mpsc::unbounded_channel();
        let (full, full_rx) = mpsc::unbounded_channel();
        let (command_exit, command_exit_rx) = mpsc::unbounded_channel();
        let (run_completion, run_completion_rx) = mpsc::unbounded_channel();
        (
            Self {
                visible,
                full,
                command_exit,
                run_completion,
                wake,
            },
            TerminalQueryReceivers {
                visible: visible_rx,
                full: full_rx,
                command_exit: command_exit_rx,
                run_completion: run_completion_rx,
            },
        )
    }

    pub(crate) async fn visible_text(&self, process_id: ProcessId) -> Result<String, String> {
        let (response, receiver) = oneshot::channel();
        self.visible
            .send(ReadTerminal {
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

    pub(crate) async fn full_text(&self, process_id: ProcessId) -> Result<String, String> {
        let (response, receiver) = oneshot::channel();
        self.full
            .send(ReadTerminalFull {
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
    ) -> Result<CommandExitState, String> {
        let (response, receiver) = oneshot::channel();
        self.command_exit
            .send(ReadCommandExit {
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
    ) -> Result<RunCompletionState, String> {
        let (response, receiver) = oneshot::channel();
        self.run_completion
            .send(ReadRunCompletion {
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

pub(crate) struct DaemonQueryPlugin {
    manager: Arc<AsyncMutex<ProcessManager>>,
    receivers: Mutex<Option<TerminalQueryReceivers>>,
}

impl DaemonQueryPlugin {
    pub(crate) fn new(
        manager: Arc<AsyncMutex<ProcessManager>>,
        wake: mpsc::UnboundedSender<ProcessId>,
    ) -> (Self, TerminalQueryBridge) {
        let (bridge, receivers) = TerminalQueryBridge::new(wake);
        (
            Self {
                manager,
                receivers: Mutex::new(Some(receivers)),
            },
            bridge,
        )
    }
}

impl Plugin for DaemonQueryPlugin {
    fn build(&self, app: &mut App) {
        let receivers = self
            .receivers
            .lock()
            .unwrap()
            .take()
            .expect("daemon query plugin can only be built once");
        app.world_mut().spawn((
            Name::new("vmux service process runtime"),
            ServiceProcesses(Arc::clone(&self.manager)),
            TerminalQueryInbox(Mutex::new(receivers)),
        ));
        app.add_systems(
            Update,
            (
                receive_terminal_queries,
                ApplyDeferred,
                answer_visible_terminal_queries,
                answer_full_terminal_queries,
                answer_command_exit_queries,
                answer_run_completion_queries,
                poll_service_processes,
            )
                .chain(),
        );
    }
}

fn receive_terminal_queries(inbox: Single<&TerminalQueryInbox>, mut commands: Commands) {
    let Ok(mut receivers) = inbox.0.lock() else {
        return;
    };
    while let Ok(request) = receivers.visible.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.full.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.command_exit.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = receivers.run_completion.try_recv() {
        commands.spawn(request);
    }
}

fn answer_visible_terminal_queries(
    processes: Single<&ServiceProcesses>,
    mut requests: Query<(Entity, &mut ReadTerminal)>,
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

fn answer_full_terminal_queries(
    processes: Single<&ServiceProcesses>,
    mut requests: Query<(Entity, &mut ReadTerminalFull)>,
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
    processes: Single<&ServiceProcesses>,
    mut requests: Query<(Entity, &mut ReadCommandExit)>,
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
                CommandExitState { sequence, exit }
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn answer_run_completion_queries(
    processes: Single<&ServiceProcesses>,
    mut requests: Query<(Entity, &mut ReadRunCompletion)>,
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
                RunCompletionState { token, exit }
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        if let Some(response) = request.response.take() {
            let _ = response.send(result);
        }
        commands.entity(entity).despawn();
    }
}

fn poll_service_processes(processes: Single<&ServiceProcesses>) {
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
        let (plugin, bridge) = DaemonQueryPlugin::new(manager, wake);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, plugin));
        let process_id = ProcessId::new();
        let query = tokio::spawn(async move { bridge.visible_text(process_id).await });
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
