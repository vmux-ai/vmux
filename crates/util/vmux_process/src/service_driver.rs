use std::collections::HashMap;
use std::path::Path;

use tokio::sync::{broadcast, mpsc};
use vmux_api::protocol::{ClientMessage, ProcessId, ServiceMessage};
use vmux_transport::service::{RemoteFuture, ServiceProtocolConnection, ServiceProtocolDriver};

use crate::{ProcessLaunch, ProcessRuntime};

pub struct ProcessService {
    processes: ProcessRuntime,
}

impl ProcessService {
    pub fn new(processes: ProcessRuntime) -> Self {
        Self { processes }
    }
}

impl ServiceProtocolDriver for ProcessService {
    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        Box::new(ProcessConnection {
            processes: self.processes.clone(),
            outbound,
            attached: HashMap::new(),
            directory_tasks: Vec::new(),
            created: Vec::new(),
        })
    }
}

struct ProcessConnection {
    processes: ProcessRuntime,
    outbound: mpsc::UnboundedSender<ServiceMessage>,
    attached: HashMap<ProcessId, tokio::task::JoinHandle<()>>,
    directory_tasks: Vec<tokio::task::JoinHandle<()>>,
    created: Vec<ProcessId>,
}

impl ProcessConnection {
    async fn dispatch(&mut self, message: ClientMessage) -> Result<(), ClientMessage> {
        match message {
            ClientMessage::CreateProcess {
                process_id,
                command,
                args,
                cwd,
                env,
                cols,
                rows,
            } => {
                self.create_process(ProcessLaunch {
                    id: process_id,
                    command,
                    args,
                    cwd,
                    env,
                    cols,
                    rows,
                    keep_after_exit: false,
                })
                .await;
            }
            ClientMessage::AttachProcess { process_id } => self.attach_process(process_id).await,
            ClientMessage::DetachProcess { process_id } => self.detach(process_id),
            ClientMessage::ProcessInput { process_id, data } => {
                let _ = self.processes.input(process_id, data).await;
            }
            ClientMessage::ChangeProcessDirectory {
                process_id,
                cwd,
                path,
            } => self.change_directory(process_id, cwd, path).await,
            ClientMessage::ListProcessDirectories {
                process_id,
                cwd,
                path,
            } => self.list_directories(process_id, cwd, path),
            ClientMessage::MouseWheel {
                process_id,
                up,
                col,
                row,
                modifiers,
            } => {
                let _ = self
                    .processes
                    .mouse_wheel(process_id, up, col, row, modifiers)
                    .await;
            }
            ClientMessage::ScrollWindow {
                process_id,
                top_row,
                follow,
            } => {
                let _ = self
                    .processes
                    .scroll_window(process_id, top_row, follow)
                    .await;
            }
            ClientMessage::ResizeProcess {
                process_id,
                cols,
                rows,
            } => {
                let _ = self.processes.resize(process_id, cols, rows).await;
            }
            ClientMessage::ListProcesses => {
                let _ = self.outbound.send(ServiceMessage::ProcessList {
                    processes: self.processes.list().await.unwrap_or_default(),
                });
            }
            ClientMessage::KillProcess { process_id } => {
                let _ = self.processes.remove(process_id).await;
                self.detach(process_id);
            }
            ClientMessage::RequestSnapshot { process_id } => {
                let response = match self.processes.snapshot(process_id).await {
                    Ok(snapshot) => snapshot.into_service_message(process_id),
                    Err(_) => ServiceMessage::Error {
                        message: format!("process not found: {process_id}"),
                    },
                };
                let _ = self.outbound.send(response);
            }
            other => return self.dispatch_selection_or_copy(other).await,
        }
        Ok(())
    }

    async fn dispatch_selection_or_copy(
        &mut self,
        message: ClientMessage,
    ) -> Result<(), ClientMessage> {
        match message {
            ClientMessage::SetSelection { process_id, range } => {
                let _ = self.processes.set_selection(process_id, range).await;
            }
            ClientMessage::ExtendSelectionTo {
                process_id,
                col,
                row,
            } => {
                let _ = self.processes.extend_selection(process_id, col, row).await;
            }
            ClientMessage::SelectWordAt {
                process_id,
                col,
                row,
            } => {
                let _ = self.processes.select_word(process_id, col, row).await;
            }
            ClientMessage::SelectLineAt { process_id, row } => {
                let _ = self.processes.select_line(process_id, row).await;
            }
            ClientMessage::GetSelectionText { process_id } => {
                let _ = self.outbound.send(ServiceMessage::SelectionText {
                    process_id,
                    text: self
                        .processes
                        .selection_text(process_id)
                        .await
                        .unwrap_or_default(),
                });
            }
            ClientMessage::EnterCopyMode { process_id } => {
                let _ = self.processes.enter_copy_mode(process_id).await;
            }
            ClientMessage::ExitCopyMode { process_id } => {
                let _ = self.processes.exit_copy_mode(process_id).await;
            }
            ClientMessage::CopyModeKey { process_id, key } => {
                if let Ok(Some(text)) = self.processes.copy_mode_key(process_id, key).await {
                    let _ = self
                        .outbound
                        .send(ServiceMessage::SelectionText { process_id, text });
                }
            }
            other => return Err(other),
        }
        Ok(())
    }

    async fn create_process(&mut self, launch: ProcessLaunch) {
        let process_id = launch.id;
        let response = match self.processes.create(launch).await {
            Ok(created) => {
                self.created.push(created.id);
                ServiceMessage::ProcessCreated {
                    process_id: created.id,
                    pid: created.pid,
                }
            }
            Err(reason) => ServiceMessage::ProcessCreateFailed { process_id, reason },
        };
        let _ = self.outbound.send(response);
    }

    async fn attach_process(&mut self, process_id: ProcessId) {
        let Ok(mut updates) = self.processes.subscribe(process_id).await else {
            let _ = self.outbound.send(ServiceMessage::Error {
                message: format!("process not found: {process_id}"),
            });
            return;
        };
        self.detach(process_id);
        let outbound = self.outbound.clone();
        let task = tokio::spawn(async move {
            loop {
                match updates.recv().await {
                    Ok(update) => {
                        if outbound
                            .send(update.into_service_message(process_id))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        bevy::log::warn!(dropped, "service stream lagged; frames were dropped");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.attached.insert(process_id, task);
    }

    async fn change_directory(&mut self, process_id: ProcessId, cwd: String, path: String) {
        if !self.attached.contains_key(&process_id) {
            return;
        }
        if let Err(message) = self.processes.change_directory(process_id, cwd, path).await {
            let _ = self.outbound.send(ServiceMessage::Error { message });
        }
    }

    fn list_directories(&mut self, process_id: ProcessId, cwd: String, path: String) {
        if !self.attached.contains_key(&process_id) {
            return;
        }
        self.directory_tasks.retain(|task| !task.is_finished());
        let processes = self.processes.clone();
        let outbound = self.outbound.clone();
        let task = tokio::spawn(async move {
            let result = Self::read_directories(&processes, process_id, &cwd, &path).await;
            let (directories, error) = match result {
                Ok(directories) => (directories, None),
                Err(error) => (Vec::new(), Some(error)),
            };
            let _ = outbound.send(ServiceMessage::ProcessDirectories {
                process_id,
                cwd,
                path,
                directories,
                error,
            });
        });
        self.directory_tasks.push(task);
    }

    async fn read_directories(
        processes: &ProcessRuntime,
        process_id: ProcessId,
        cwd: &str,
        path: &str,
    ) -> Result<Vec<String>, String> {
        let current = processes.list().await?;
        if !current
            .iter()
            .any(|process| process.id == process_id && process.cwd == cwd)
        {
            return Err("terminal directory has changed".to_string());
        }
        let root = Path::new(path);
        if !root.is_absolute() {
            return Err("terminal directory is not absolute".into());
        }
        let mut directories = Vec::new();
        if let Some(parent) = root.parent() {
            directories.push(parent.to_string_lossy().into_owned());
        }
        let mut entries = tokio::fs::read_dir(root)
            .await
            .map_err(|error| error.to_string())?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| error.to_string())?
        {
            if directories.len() == 1024 {
                break;
            }
            if entry
                .metadata()
                .await
                .is_ok_and(|metadata| metadata.is_dir())
            {
                directories.push(entry.path().to_string_lossy().into_owned());
            }
        }
        directories.sort();
        Ok(directories)
    }

    fn detach(&mut self, process_id: ProcessId) {
        if let Some(task) = self.attached.remove(&process_id) {
            task.abort();
        }
    }

    fn abort_tasks(&mut self) {
        for (_, task) in self.attached.drain() {
            task.abort();
        }
        for task in self.directory_tasks.drain(..) {
            task.abort();
        }
    }

    async fn disconnect(&mut self) {
        self.abort_tasks();
        for process_id in self.created.drain(..) {
            let _ = self.processes.remove(process_id).await;
        }
    }
}

impl ServiceProtocolConnection for ProcessConnection {
    fn dispatch(&mut self, message: ClientMessage) -> RemoteFuture<'_, Result<(), ClientMessage>> {
        Box::pin(ProcessConnection::dispatch(self, message))
    }

    fn disconnect(&mut self) -> RemoteFuture<'_, ()> {
        Box::pin(ProcessConnection::disconnect(self))
    }
}

impl Drop for ProcessConnection {
    fn drop(&mut self) {
        self.abort_tasks();
    }
}
