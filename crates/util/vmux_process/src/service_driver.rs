use std::collections::HashMap;

use crate::{ProcessLaunch, ProcessRuntime};
use tokio::sync::{broadcast, mpsc};
use vmux_api::protocol::{ClientMessage, ProcessId, ServiceMessage};
use vmux_transport::service::{RemoteFuture, ServiceProtocolConnection, ServiceProtocolDriver};

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
            created: Vec::new(),
        })
    }
}

struct ProcessConnection {
    processes: ProcessRuntime,
    outbound: mpsc::UnboundedSender<ServiceMessage>,
    attached: HashMap<ProcessId, tokio::task::JoinHandle<()>>,
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
                let created = self
                    .processes
                    .create(ProcessLaunch {
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
                let response = match created {
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
            ClientMessage::AttachProcess { process_id } => {
                let Ok(mut updates) = self.processes.subscribe(process_id).await else {
                    let _ = self.outbound.send(ServiceMessage::Error {
                        message: format!("process not found: {process_id}"),
                    });
                    return Ok(());
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
                                bevy::log::warn!(
                                    dropped,
                                    "service stream lagged; frames were dropped"
                                );
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
                self.attached.insert(process_id, task);
            }
            ClientMessage::DetachProcess { process_id } => self.detach(process_id),
            ClientMessage::ProcessInput { process_id, data } => {
                let _ = self.processes.input(process_id, data).await;
            }
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

    fn detach(&mut self, process_id: ProcessId) {
        if let Some(task) = self.attached.remove(&process_id) {
            task.abort();
        }
    }

    async fn disconnect(&mut self) {
        for (_, task) in self.attached.drain() {
            task.abort();
        }
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
        for (_, task) in self.attached.drain() {
            task.abort();
        }
    }
}
