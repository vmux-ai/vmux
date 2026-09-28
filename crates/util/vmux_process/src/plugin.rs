use std::sync::Mutex;

use bevy::prelude::{App, Bundle, Component, IntoScheduleConfigs, Plugin, Query, Update};
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_api::protocol::{
    AgentCommandExit, AgentProcessCommandExit, AgentProcessRunCompletion, AgentReadProcessOutput,
    AgentReadProcessTranscript, AgentRequest, AgentRequestId, AgentRunCompletion, CopyModeKey,
    ProcessInfo, ServiceMessage,
};
use vmux_api::{ProcessId, TermSelectionRange};

use crate::{Process, ProcessManager, ProcessSnapshot, ProcessUpdate};

pub struct ProcessPlugin;

impl Plugin for ProcessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (apply_process_operations, poll_processes).chain());
    }
}

pub struct ProcessLaunch {
    pub id: ProcessId,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    pub keep_after_exit: bool,
}

pub struct ProcessCreated {
    pub id: ProcessId,
    pub pid: u32,
    pub updates: broadcast::Receiver<ProcessUpdate>,
}

#[derive(Clone)]
pub struct ProcessRuntime {
    operations: mpsc::UnboundedSender<ProcessOperation>,
    wake: mpsc::UnboundedSender<()>,
}

impl ProcessRuntime {
    pub fn new(wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (operations, inbox) = mpsc::unbounded_channel();
        (
            Self {
                operations,
                wake: wake.clone(),
            },
            (
                ProcessRegistry(Mutex::new(ProcessManager::new(wake))),
                ProcessOperationInbox(inbox),
            ),
        )
    }

    pub async fn create(&self, launch: ProcessLaunch) -> Result<ProcessCreated, String> {
        self.request(|response| ProcessOperation::Create { launch, response })
            .await?
    }

    pub async fn subscribe(
        &self,
        process_id: ProcessId,
    ) -> Result<broadcast::Receiver<ProcessUpdate>, String> {
        self.request(|response| ProcessOperation::Subscribe {
            process_id,
            response,
        })
        .await?
    }

    pub async fn input(&self, process_id: ProcessId, data: Vec<u8>) -> Result<(), String> {
        self.request(|response| ProcessOperation::Input {
            process_id,
            data,
            response,
        })
        .await?
    }

    pub async fn mouse_wheel(
        &self,
        process_id: ProcessId,
        up: bool,
        col: u16,
        row: u16,
        modifiers: u8,
    ) -> Result<(), String> {
        self.request(|response| ProcessOperation::MouseWheel {
            process_id,
            up,
            col,
            row,
            modifiers,
            response,
        })
        .await?
    }

    pub async fn scroll_window(
        &self,
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
    ) -> Result<(), String> {
        self.request(|response| ProcessOperation::ScrollWindow {
            process_id,
            top_row,
            follow,
            response,
        })
        .await?
    }

    pub async fn resize(&self, process_id: ProcessId, cols: u16, rows: u16) -> Result<(), String> {
        self.request(|response| ProcessOperation::Resize {
            process_id,
            cols,
            rows,
            response,
        })
        .await?
    }

    pub async fn list(&self) -> Result<Vec<ProcessInfo>, String> {
        self.request(|response| ProcessOperation::List { response })
            .await
    }

    pub async fn remove(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(|response| ProcessOperation::Remove {
            process_id,
            response,
        })
        .await
    }

    pub async fn kill(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(|response| ProcessOperation::Kill {
            process_id,
            response,
        })
        .await?
    }

    pub async fn snapshot(&self, process_id: ProcessId) -> Result<ProcessSnapshot, String> {
        self.request(|response| ProcessOperation::Snapshot {
            process_id,
            response,
        })
        .await?
    }

    pub async fn set_selection(
        &self,
        process_id: ProcessId,
        range: Option<TermSelectionRange>,
    ) -> Result<(), String> {
        self.request(|response| ProcessOperation::SetSelection {
            process_id,
            range,
            response,
        })
        .await?
    }

    pub async fn extend_selection(
        &self,
        process_id: ProcessId,
        col: u16,
        row: u16,
    ) -> Result<(), String> {
        self.request(|response| ProcessOperation::ExtendSelection {
            process_id,
            col,
            row,
            response,
        })
        .await?
    }

    pub async fn select_word(
        &self,
        process_id: ProcessId,
        col: u16,
        row: u16,
    ) -> Result<(), String> {
        self.request(|response| ProcessOperation::SelectWord {
            process_id,
            col,
            row,
            response,
        })
        .await?
    }

    pub async fn select_line(&self, process_id: ProcessId, row: u16) -> Result<(), String> {
        self.request(|response| ProcessOperation::SelectLine {
            process_id,
            row,
            response,
        })
        .await?
    }

    pub async fn selection_text(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(|response| ProcessOperation::SelectionText {
            process_id,
            response,
        })
        .await?
    }

    pub async fn enter_copy_mode(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(|response| ProcessOperation::EnterCopyMode {
            process_id,
            response,
        })
        .await?
    }

    pub async fn exit_copy_mode(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(|response| ProcessOperation::ExitCopyMode {
            process_id,
            response,
        })
        .await?
    }

    pub async fn copy_mode_key(
        &self,
        process_id: ProcessId,
        key: CopyModeKey,
    ) -> Result<Option<String>, String> {
        self.request(|response| ProcessOperation::CopyModeKey {
            process_id,
            key,
            response,
        })
        .await?
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        self.request(|response| ProcessOperation::Shutdown { response })
            .await
    }

    pub async fn count(&self) -> Result<u32, String> {
        self.request(|response| ProcessOperation::Count { response })
            .await
    }

    pub async fn output(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(|response| ProcessOperation::Output {
            process_id,
            response,
        })
        .await?
    }

    pub async fn transcript(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(|response| ProcessOperation::Transcript {
            process_id,
            response,
        })
        .await?
    }

    pub async fn command_exit(&self, process_id: ProcessId) -> Result<AgentCommandExit, String> {
        self.request(|response| ProcessOperation::CommandExit {
            process_id,
            response,
        })
        .await?
    }

    pub async fn run_completion(
        &self,
        process_id: ProcessId,
    ) -> Result<AgentRunCompletion, String> {
        self.request(|response| ProcessOperation::RunCompletion {
            process_id,
            response,
        })
        .await?
    }

    pub async fn exit_code(&self, process_id: ProcessId) -> Result<Option<i32>, String> {
        self.request(|response| ProcessOperation::ExitCode {
            process_id,
            response,
        })
        .await?
    }

    pub async fn response(
        &self,
        request_id: AgentRequestId,
        request: &AgentRequest,
    ) -> Result<Option<ServiceMessage>, String> {
        if let Some(request) = request.decode::<AgentReadProcessOutput>()? {
            return Ok(Some(ServiceMessage::ProcessOutputResult {
                request_id,
                result: self.output(request.process_id).await,
            }));
        }
        if let Some(request) = request.decode::<AgentReadProcessTranscript>()? {
            return Ok(Some(ServiceMessage::ProcessTranscriptResult {
                request_id,
                result: self.transcript(request.process_id).await,
            }));
        }
        if let Some(request) = request.decode::<AgentProcessCommandExit>()? {
            return Ok(Some(ServiceMessage::ProcessCommandExitResult {
                request_id,
                result: self.command_exit(request.process_id).await,
            }));
        }
        if let Some(request) = request.decode::<AgentProcessRunCompletion>()? {
            return Ok(Some(ServiceMessage::ProcessRunCompletionResult {
                request_id,
                result: self.run_completion(request.process_id).await,
            }));
        }
        Ok(None)
    }

    async fn request<T>(
        &self,
        operation: impl FnOnce(oneshot::Sender<T>) -> ProcessOperation,
    ) -> Result<T, String> {
        let (response, receiver) = oneshot::channel();
        self.operations
            .send(operation(response))
            .map_err(|_| "process runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "process runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "process operation was cancelled".to_string())
    }
}

#[derive(Component)]
struct ProcessRegistry(Mutex<ProcessManager>);

#[derive(Component)]
struct ProcessOperationInbox(mpsc::UnboundedReceiver<ProcessOperation>);

enum ProcessOperation {
    Create {
        launch: ProcessLaunch,
        response: oneshot::Sender<Result<ProcessCreated, String>>,
    },
    Subscribe {
        process_id: ProcessId,
        response: oneshot::Sender<Result<broadcast::Receiver<ProcessUpdate>, String>>,
    },
    Input {
        process_id: ProcessId,
        data: Vec<u8>,
        response: oneshot::Sender<Result<(), String>>,
    },
    MouseWheel {
        process_id: ProcessId,
        up: bool,
        col: u16,
        row: u16,
        modifiers: u8,
        response: oneshot::Sender<Result<(), String>>,
    },
    ScrollWindow {
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
        response: oneshot::Sender<Result<(), String>>,
    },
    Resize {
        process_id: ProcessId,
        cols: u16,
        rows: u16,
        response: oneshot::Sender<Result<(), String>>,
    },
    List {
        response: oneshot::Sender<Vec<ProcessInfo>>,
    },
    Remove {
        process_id: ProcessId,
        response: oneshot::Sender<()>,
    },
    Kill {
        process_id: ProcessId,
        response: oneshot::Sender<Result<(), String>>,
    },
    Snapshot {
        process_id: ProcessId,
        response: oneshot::Sender<Result<ProcessSnapshot, String>>,
    },
    SetSelection {
        process_id: ProcessId,
        range: Option<TermSelectionRange>,
        response: oneshot::Sender<Result<(), String>>,
    },
    ExtendSelection {
        process_id: ProcessId,
        col: u16,
        row: u16,
        response: oneshot::Sender<Result<(), String>>,
    },
    SelectWord {
        process_id: ProcessId,
        col: u16,
        row: u16,
        response: oneshot::Sender<Result<(), String>>,
    },
    SelectLine {
        process_id: ProcessId,
        row: u16,
        response: oneshot::Sender<Result<(), String>>,
    },
    SelectionText {
        process_id: ProcessId,
        response: oneshot::Sender<Result<String, String>>,
    },
    EnterCopyMode {
        process_id: ProcessId,
        response: oneshot::Sender<Result<(), String>>,
    },
    ExitCopyMode {
        process_id: ProcessId,
        response: oneshot::Sender<Result<(), String>>,
    },
    CopyModeKey {
        process_id: ProcessId,
        key: CopyModeKey,
        response: oneshot::Sender<Result<Option<String>, String>>,
    },
    Shutdown {
        response: oneshot::Sender<()>,
    },
    Count {
        response: oneshot::Sender<u32>,
    },
    Output {
        process_id: ProcessId,
        response: oneshot::Sender<Result<String, String>>,
    },
    Transcript {
        process_id: ProcessId,
        response: oneshot::Sender<Result<String, String>>,
    },
    CommandExit {
        process_id: ProcessId,
        response: oneshot::Sender<Result<AgentCommandExit, String>>,
    },
    RunCompletion {
        process_id: ProcessId,
        response: oneshot::Sender<Result<AgentRunCompletion, String>>,
    },
    ExitCode {
        process_id: ProcessId,
        response: oneshot::Sender<Result<Option<i32>, String>>,
    },
}

fn apply_process_operations(
    mut runtime: Query<(&mut ProcessRegistry, &mut ProcessOperationInbox)>,
) {
    let Ok((registry, mut inbox)) = runtime.single_mut() else {
        return;
    };
    let Ok(mut processes) = registry.0.lock() else {
        return;
    };
    while let Ok(operation) = inbox.0.try_recv() {
        match operation {
            ProcessOperation::Create { launch, response } => {
                let result = processes
                    .create_process(
                        launch.id,
                        launch.command,
                        launch.args,
                        launch.cwd,
                        launch.env,
                        launch.cols,
                        launch.rows,
                    )
                    .and_then(|(id, pid)| {
                        let process = processes
                            .processes
                            .get_mut(&id)
                            .ok_or_else(|| format!("process not found after creation: {id}"))?;
                        if launch.keep_after_exit {
                            process.set_keep_after_exit();
                        }
                        Ok(ProcessCreated {
                            id,
                            pid,
                            updates: process.subscribe(),
                        })
                    });
                let _ = response.send(result);
            }
            ProcessOperation::Subscribe {
                process_id,
                response,
            } => {
                let result = processes
                    .processes
                    .get(&process_id)
                    .map(Process::subscribe)
                    .ok_or_else(|| format!("process not found: {process_id}"));
                let _ = response.send(result);
            }
            ProcessOperation::Input {
                process_id,
                data,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    if !process.is_copy_mode() {
                        process.write_input(&data);
                    }
                });
                let _ = response.send(result);
            }
            ProcessOperation::MouseWheel {
                process_id,
                up,
                col,
                row,
                modifiers,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.handle_mouse_wheel(up, col, row, modifiers)
                });
                let _ = response.send(result);
            }
            ProcessOperation::ScrollWindow {
                process_id,
                top_row,
                follow,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.handle_scroll_window(top_row, follow)
                });
                let _ = response.send(result);
            }
            ProcessOperation::Resize {
                process_id,
                cols,
                rows,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.resize(cols, rows)
                });
                let _ = response.send(result);
            }
            ProcessOperation::List { response } => {
                let mut list = Vec::new();
                for process in processes.processes.values() {
                    list.push(process.info());
                }
                let _ = response.send(list);
            }
            ProcessOperation::Remove {
                process_id,
                response,
            } => {
                processes.remove_process(&process_id);
                let _ = response.send(());
            }
            ProcessOperation::Kill {
                process_id,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, Process::kill);
                let _ = response.send(result);
            }
            ProcessOperation::Snapshot {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, Process::snapshot);
                let _ = response.send(result);
            }
            ProcessOperation::SetSelection {
                process_id,
                range,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.set_selection(range)
                });
                let _ = response.send(result);
            }
            ProcessOperation::ExtendSelection {
                process_id,
                col,
                row,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.extend_selection_to(col, row)
                });
                let _ = response.send(result);
            }
            ProcessOperation::SelectWord {
                process_id,
                col,
                row,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.select_word_at(col, row)
                });
                let _ = response.send(result);
            }
            ProcessOperation::SelectLine {
                process_id,
                row,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.select_line_at(row)
                });
                let _ = response.send(result);
            }
            ProcessOperation::SelectionText {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, |process| {
                    process.selection_text().unwrap_or_default()
                });
                let _ = response.send(result);
            }
            ProcessOperation::EnterCopyMode {
                process_id,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, Process::enter_copy_mode);
                let _ = response.send(result);
            }
            ProcessOperation::ExitCopyMode {
                process_id,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, Process::exit_copy_mode);
                let _ = response.send(result);
            }
            ProcessOperation::CopyModeKey {
                process_id,
                key,
                response,
            } => {
                let result = process_mut(&mut processes, process_id, |process| {
                    process.copy_mode_key(key)
                });
                let _ = response.send(result);
            }
            ProcessOperation::Shutdown { response } => {
                processes.shutdown();
                let _ = response.send(());
            }
            ProcessOperation::Count { response } => {
                let _ = response.send(processes.processes.len() as u32);
            }
            ProcessOperation::Output {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, Process::visible_text);
                let _ = response.send(result);
            }
            ProcessOperation::Transcript {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, Process::full_text);
                let _ = response.send(result);
            }
            ProcessOperation::CommandExit {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, |process| {
                    let (sequence, exit) = process.command_status();
                    AgentCommandExit { sequence, exit }
                });
                let _ = response.send(result);
            }
            ProcessOperation::RunCompletion {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, |process| {
                    let (token, exit) = match process.run_completion() {
                        Some((token, exit)) => (Some(token), Some(exit)),
                        None => (None, None),
                    };
                    AgentRunCompletion { token, exit }
                });
                let _ = response.send(result);
            }
            ProcessOperation::ExitCode {
                process_id,
                response,
            } => {
                let result = process_ref(&processes, process_id, Process::process_exit);
                let _ = response.send(result);
            }
        }
    }
}

fn poll_processes(runtime: Query<&ProcessRegistry>) {
    let Ok(registry) = runtime.single() else {
        return;
    };
    let Ok(mut processes) = registry.0.lock() else {
        return;
    };
    processes.reap_exited();
}

fn process_ref<T>(
    processes: &ProcessManager,
    process_id: ProcessId,
    read: impl FnOnce(&Process) -> T,
) -> Result<T, String> {
    processes
        .processes
        .get(&process_id)
        .map(read)
        .ok_or_else(|| format!("process not found: {process_id}"))
}

fn process_mut<T>(
    processes: &mut ProcessManager,
    process_id: ProcessId,
    mutate: impl FnOnce(&mut Process) -> T,
) -> Result<T, String> {
    processes
        .processes
        .get_mut(&process_id)
        .map(mutate)
        .ok_or_else(|| format!("process not found: {process_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, MinimalPlugins, Name};

    #[tokio::test]
    async fn missing_process_query_is_answered_by_ecs() {
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let (processes, runtime) = ProcessRuntime::new(wake);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ProcessPlugin));
        app.world_mut()
            .spawn((Name::new("vmux process runtime"), runtime));
        let process_id = ProcessId::new();
        let query = tokio::spawn(async move { processes.output(process_id).await });
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
