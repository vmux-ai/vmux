use std::sync::Arc;

use bevy::prelude::*;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_api::protocol::{AgentCommandExit, AgentRunCompletion, CopyModeKey, ProcessInfo};
use vmux_api::{ProcessId, TermSelectionRange};

use crate::{ProcessSnapshot, ProcessUpdate};

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

#[derive(Component, Clone)]
pub struct ProcessRuntime {
    requests: Arc<ProcessRequestSenders>,
    wake: mpsc::UnboundedSender<()>,
}

struct ProcessRequestSenders {
    create: mpsc::UnboundedSender<CreateRequest>,
    subscribe: mpsc::UnboundedSender<SubscribeRequest>,
    input: mpsc::UnboundedSender<InputRequest>,
    mouse_wheel: mpsc::UnboundedSender<MouseWheelRequest>,
    scroll_window: mpsc::UnboundedSender<ScrollWindowRequest>,
    resize: mpsc::UnboundedSender<ResizeRequest>,
    list: mpsc::UnboundedSender<ListRequest>,
    remove: mpsc::UnboundedSender<RemoveRequest>,
    kill: mpsc::UnboundedSender<KillRequest>,
    snapshot: mpsc::UnboundedSender<SnapshotRequest>,
    set_selection: mpsc::UnboundedSender<SetSelectionRequest>,
    extend_selection: mpsc::UnboundedSender<ExtendSelectionRequest>,
    select_word: mpsc::UnboundedSender<SelectWordRequest>,
    select_line: mpsc::UnboundedSender<SelectLineRequest>,
    selection_text: mpsc::UnboundedSender<SelectionTextRequest>,
    enter_copy_mode: mpsc::UnboundedSender<EnterCopyModeRequest>,
    exit_copy_mode: mpsc::UnboundedSender<ExitCopyModeRequest>,
    copy_mode: mpsc::UnboundedSender<CopyModeRequest>,
    shutdown: mpsc::UnboundedSender<ShutdownRequest>,
    count: mpsc::UnboundedSender<CountRequest>,
    output: mpsc::UnboundedSender<OutputRequest>,
    transcript: mpsc::UnboundedSender<TranscriptRequest>,
    command_exit: mpsc::UnboundedSender<CommandExitRequest>,
    run_completion: mpsc::UnboundedSender<RunCompletionRequest>,
    exit_code: mpsc::UnboundedSender<ExitCodeRequest>,
}

#[derive(Bundle)]
struct ProcessRequestInboxes {
    create: RuntimeInbox<CreateRequest>,
    subscribe: RuntimeInbox<SubscribeRequest>,
    input: RuntimeInbox<InputRequest>,
    mouse_wheel: RuntimeInbox<MouseWheelRequest>,
    scroll_window: RuntimeInbox<ScrollWindowRequest>,
    resize: RuntimeInbox<ResizeRequest>,
    list: RuntimeInbox<ListRequest>,
    remove: RuntimeInbox<RemoveRequest>,
    kill: RuntimeInbox<KillRequest>,
    snapshot: RuntimeInbox<SnapshotRequest>,
    set_selection: RuntimeInbox<SetSelectionRequest>,
    extend_selection: RuntimeInbox<ExtendSelectionRequest>,
    select_word: RuntimeInbox<SelectWordRequest>,
    select_line: RuntimeInbox<SelectLineRequest>,
    selection_text: RuntimeInbox<SelectionTextRequest>,
    enter_copy_mode: RuntimeInbox<EnterCopyModeRequest>,
    exit_copy_mode: RuntimeInbox<ExitCopyModeRequest>,
    copy_mode: RuntimeInbox<CopyModeRequest>,
    shutdown: RuntimeInbox<ShutdownRequest>,
    count: RuntimeInbox<CountRequest>,
    output: RuntimeInbox<OutputRequest>,
    transcript: RuntimeInbox<TranscriptRequest>,
    command_exit: RuntimeInbox<CommandExitRequest>,
    run_completion: RuntimeInbox<RunCompletionRequest>,
    exit_code: RuntimeInbox<ExitCodeRequest>,
}

impl ProcessRuntime {
    pub fn new(wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (create, create_inbox) = RuntimeInbox::channel();
        let (subscribe, subscribe_inbox) = RuntimeInbox::channel();
        let (input, input_inbox) = RuntimeInbox::channel();
        let (mouse_wheel, mouse_wheel_inbox) = RuntimeInbox::channel();
        let (scroll_window, scroll_window_inbox) = RuntimeInbox::channel();
        let (resize, resize_inbox) = RuntimeInbox::channel();
        let (list, list_inbox) = RuntimeInbox::channel();
        let (remove, remove_inbox) = RuntimeInbox::channel();
        let (kill, kill_inbox) = RuntimeInbox::channel();
        let (snapshot, snapshot_inbox) = RuntimeInbox::channel();
        let (set_selection, set_selection_inbox) = RuntimeInbox::channel();
        let (extend_selection, extend_selection_inbox) = RuntimeInbox::channel();
        let (select_word, select_word_inbox) = RuntimeInbox::channel();
        let (select_line, select_line_inbox) = RuntimeInbox::channel();
        let (selection_text, selection_text_inbox) = RuntimeInbox::channel();
        let (enter_copy_mode, enter_copy_mode_inbox) = RuntimeInbox::channel();
        let (exit_copy_mode, exit_copy_mode_inbox) = RuntimeInbox::channel();
        let (copy_mode, copy_mode_inbox) = RuntimeInbox::channel();
        let (shutdown, shutdown_inbox) = RuntimeInbox::channel();
        let (count, count_inbox) = RuntimeInbox::channel();
        let (output, output_inbox) = RuntimeInbox::channel();
        let (transcript, transcript_inbox) = RuntimeInbox::channel();
        let (command_exit, command_exit_inbox) = RuntimeInbox::channel();
        let (run_completion, run_completion_inbox) = RuntimeInbox::channel();
        let (exit_code, exit_code_inbox) = RuntimeInbox::channel();
        (
            Self {
                requests: Arc::new(ProcessRequestSenders {
                    create,
                    subscribe,
                    input,
                    mouse_wheel,
                    scroll_window,
                    resize,
                    list,
                    remove,
                    kill,
                    snapshot,
                    set_selection,
                    extend_selection,
                    select_word,
                    select_line,
                    selection_text,
                    enter_copy_mode,
                    exit_copy_mode,
                    copy_mode,
                    shutdown,
                    count,
                    output,
                    transcript,
                    command_exit,
                    run_completion,
                    exit_code,
                }),
                wake: wake.clone(),
            },
            (
                ProcessWake(wake),
                ProcessRequestInboxes {
                    create: create_inbox,
                    subscribe: subscribe_inbox,
                    input: input_inbox,
                    mouse_wheel: mouse_wheel_inbox,
                    scroll_window: scroll_window_inbox,
                    resize: resize_inbox,
                    list: list_inbox,
                    remove: remove_inbox,
                    kill: kill_inbox,
                    snapshot: snapshot_inbox,
                    set_selection: set_selection_inbox,
                    extend_selection: extend_selection_inbox,
                    select_word: select_word_inbox,
                    select_line: select_line_inbox,
                    selection_text: selection_text_inbox,
                    enter_copy_mode: enter_copy_mode_inbox,
                    exit_copy_mode: exit_copy_mode_inbox,
                    copy_mode: copy_mode_inbox,
                    shutdown: shutdown_inbox,
                    count: count_inbox,
                    output: output_inbox,
                    transcript: transcript_inbox,
                    command_exit: command_exit_inbox,
                    run_completion: run_completion_inbox,
                    exit_code: exit_code_inbox,
                },
            ),
        )
    }

    pub async fn create(&self, launch: ProcessLaunch) -> Result<ProcessCreated, String> {
        self.request(&self.requests.create, |reply| CreateRequest {
            launch,
            reply,
        })
        .await?
    }

    pub async fn subscribe(
        &self,
        process_id: ProcessId,
    ) -> Result<broadcast::Receiver<ProcessUpdate>, String> {
        self.request(&self.requests.subscribe, |reply| SubscribeRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn input(&self, process_id: ProcessId, data: Vec<u8>) -> Result<(), String> {
        self.request(&self.requests.input, |reply| InputRequest {
            process_id,
            data,
            reply,
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
        self.request(&self.requests.mouse_wheel, |reply| MouseWheelRequest {
            process_id,
            up,
            col,
            row,
            modifiers,
            reply,
        })
        .await?
    }

    pub async fn scroll_window(
        &self,
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
    ) -> Result<(), String> {
        self.request(&self.requests.scroll_window, |reply| ScrollWindowRequest {
            process_id,
            top_row,
            follow,
            reply,
        })
        .await?
    }

    pub async fn resize(&self, process_id: ProcessId, cols: u16, rows: u16) -> Result<(), String> {
        self.request(&self.requests.resize, |reply| ResizeRequest {
            process_id,
            cols,
            rows,
            reply,
        })
        .await?
    }

    pub async fn list(&self) -> Result<Vec<ProcessInfo>, String> {
        self.request(&self.requests.list, |reply| ListRequest { reply })
            .await
    }

    pub async fn remove(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(&self.requests.remove, |reply| RemoveRequest {
            process_id,
            reply,
        })
        .await
    }

    pub async fn kill(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(&self.requests.kill, |reply| KillRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn snapshot(&self, process_id: ProcessId) -> Result<ProcessSnapshot, String> {
        self.request(&self.requests.snapshot, |reply| SnapshotRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn set_selection(
        &self,
        process_id: ProcessId,
        range: Option<TermSelectionRange>,
    ) -> Result<(), String> {
        self.request(&self.requests.set_selection, |reply| SetSelectionRequest {
            process_id,
            range,
            reply,
        })
        .await?
    }

    pub async fn extend_selection(
        &self,
        process_id: ProcessId,
        col: u16,
        row: u16,
    ) -> Result<(), String> {
        self.request(&self.requests.extend_selection, |reply| {
            ExtendSelectionRequest {
                process_id,
                col,
                row,
                reply,
            }
        })
        .await?
    }

    pub async fn select_word(
        &self,
        process_id: ProcessId,
        col: u16,
        row: u16,
    ) -> Result<(), String> {
        self.request(&self.requests.select_word, |reply| SelectWordRequest {
            process_id,
            col,
            row,
            reply,
        })
        .await?
    }

    pub async fn select_line(&self, process_id: ProcessId, row: u16) -> Result<(), String> {
        self.request(&self.requests.select_line, |reply| SelectLineRequest {
            process_id,
            row,
            reply,
        })
        .await?
    }

    pub async fn selection_text(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(&self.requests.selection_text, |reply| {
            SelectionTextRequest { process_id, reply }
        })
        .await?
    }

    pub async fn enter_copy_mode(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(&self.requests.enter_copy_mode, |reply| {
            EnterCopyModeRequest { process_id, reply }
        })
        .await?
    }

    pub async fn exit_copy_mode(&self, process_id: ProcessId) -> Result<(), String> {
        self.request(&self.requests.exit_copy_mode, |reply| ExitCopyModeRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn copy_mode_key(
        &self,
        process_id: ProcessId,
        key: CopyModeKey,
    ) -> Result<Option<String>, String> {
        self.request(&self.requests.copy_mode, |reply| CopyModeRequest {
            process_id,
            key,
            reply,
        })
        .await?
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        self.request(&self.requests.shutdown, |reply| ShutdownRequest { reply })
            .await
    }

    pub async fn count(&self) -> Result<u32, String> {
        self.request(&self.requests.count, |reply| CountRequest { reply })
            .await
    }

    pub async fn output(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(&self.requests.output, |reply| OutputRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn transcript(&self, process_id: ProcessId) -> Result<String, String> {
        self.request(&self.requests.transcript, |reply| TranscriptRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn command_exit(&self, process_id: ProcessId) -> Result<AgentCommandExit, String> {
        self.request(&self.requests.command_exit, |reply| CommandExitRequest {
            process_id,
            reply,
        })
        .await?
    }

    pub async fn run_completion(
        &self,
        process_id: ProcessId,
    ) -> Result<AgentRunCompletion, String> {
        self.request(&self.requests.run_completion, |reply| {
            RunCompletionRequest { process_id, reply }
        })
        .await?
    }

    pub async fn exit_code(&self, process_id: ProcessId) -> Result<Option<i32>, String> {
        self.request(&self.requests.exit_code, |reply| ExitCodeRequest {
            process_id,
            reply,
        })
        .await?
    }

    async fn request<T, R>(
        &self,
        sender: &mpsc::UnboundedSender<R>,
        request: impl FnOnce(oneshot::Sender<T>) -> R,
    ) -> Result<T, String> {
        let (reply, receiver) = oneshot::channel();
        sender
            .send(request(reply))
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
pub(crate) struct ProcessWake(pub(crate) mpsc::UnboundedSender<()>);

#[derive(Component)]
pub(crate) struct RuntimeInbox<T: Send + Sync + 'static>(pub(crate) mpsc::UnboundedReceiver<T>);

impl<T: Send + Sync + 'static> RuntimeInbox<T> {
    fn channel() -> (mpsc::UnboundedSender<T>, Self) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (sender, Self(receiver))
    }
}

pub(crate) struct CreateRequest {
    pub(crate) launch: ProcessLaunch,
    pub(crate) reply: oneshot::Sender<Result<ProcessCreated, String>>,
}

pub(crate) struct SubscribeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<broadcast::Receiver<ProcessUpdate>, String>>,
}

pub(crate) struct InputRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) data: Vec<u8>,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct MouseWheelRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) up: bool,
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) modifiers: u8,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct ScrollWindowRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) top_row: u32,
    pub(crate) follow: bool,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct ResizeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct ListRequest {
    pub(crate) reply: oneshot::Sender<Vec<ProcessInfo>>,
}

pub(crate) struct RemoveRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<()>,
}

pub(crate) struct KillRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct SnapshotRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<ProcessSnapshot, String>>,
}

pub(crate) struct SetSelectionRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) range: Option<TermSelectionRange>,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct ExtendSelectionRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct SelectWordRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct SelectLineRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) row: u16,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct SelectionTextRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<String, String>>,
}

pub(crate) struct EnterCopyModeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct ExitCopyModeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) struct CopyModeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) key: CopyModeKey,
    pub(crate) reply: oneshot::Sender<Result<Option<String>, String>>,
}

pub(crate) struct ShutdownRequest {
    pub(crate) reply: oneshot::Sender<()>,
}

pub(crate) struct CountRequest {
    pub(crate) reply: oneshot::Sender<u32>,
}

pub(crate) struct OutputRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<String, String>>,
}

pub(crate) struct TranscriptRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<String, String>>,
}

pub(crate) struct CommandExitRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<AgentCommandExit, String>>,
}

pub(crate) struct RunCompletionRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<AgentRunCompletion, String>>,
}

pub(crate) struct ExitCodeRequest {
    pub(crate) process_id: ProcessId,
    pub(crate) reply: oneshot::Sender<Result<Option<i32>, String>>,
}
