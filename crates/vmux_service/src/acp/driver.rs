//! Per-session ACP driver: spawns the agent subprocess, runs the `Client` connection,
//! and pumps prompts/approvals through it while projecting `session/update` to the UI.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, CreateTerminalRequest, CreateTerminalResponse,
    Implementation, InitializeRequest, KillTerminalRequest, KillTerminalResponse,
    LoadSessionRequest, McpServer, NewSessionRequest, PermissionOption, PermissionOptionId,
    PromptRequest, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
    ReleaseTerminalResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionId, SessionNotification,
    TerminalExitStatus, TerminalId, TerminalOutputRequest, TerminalOutputResponse, TextContent,
    WaitForTerminalExitRequest, WaitForTerminalExitResponse, WriteTextFileRequest,
    WriteTextFileResponse,
};
use agent_client_protocol::{Client, Responder};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vmux_core::ProcessId;

use super::projector::{AcpProjector, Intent};
use crate::process::{ProcessManager, PtyInputWriter};
use crate::protocol::{
    AgentCommand, AgentRequestId, AgentRunStatus, ApprovalDecision, ServiceMessage,
    compose_agent_prompt,
};

/// A command pushed into a live ACP session from the GUI side.
pub enum AcpInput {
    User {
        text: String,
        context: Option<String>,
    },
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    /// Interrupt the in-flight prompt (ACP `session/cancel`); keep the session alive.
    Cancel,
    Close,
}

#[derive(Clone, Copy)]
pub enum AcpTerminalExit {
    Pending,
    Exited(Option<i32>),
    Removed,
}

/// A live ACP-native terminal and its process exit state.
pub struct AcpTerminal {
    pub process_id: ProcessId,
    pub exit_rx: watch::Receiver<AcpTerminalExit>,
    pub output_byte_limit: Option<u64>,
}

/// State shared between the driver's request handlers and its prompt loop.
pub struct AcpShared {
    pub sid: String,
    pub cwd: PathBuf,
    pub anchor: ProcessId,
    pub stream_tx: broadcast::Sender<ServiceMessage>,
    pub projector: Mutex<AcpProjector>,
    pub pending_perms: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    /// ACP-native terminals keyed by their ACP `terminalId` (the vmux `ProcessId` string).
    pub terminals: Mutex<HashMap<String, AcpTerminal>>,
    /// Daemon process manager (shared with the IPC server) so terminal handlers spawn / read / kill
    /// PTYs directly, without a GUI round-trip.
    pub manager: Arc<tokio::sync::Mutex<ProcessManager>>,
    /// PTY input writers (shared with the server) so the user can take over an ACP terminal by
    /// typing into its pane.
    pub input_writers: Arc<tokio::sync::Mutex<HashMap<ProcessId, PtyInputWriter>>>,
    agent_name: Mutex<Option<String>>,
    /// Set by `AcpInput::Cancel`; read (and reset) when the in-flight prompt resolves so it
    /// reports `Interrupted` rather than `Idle`.
    pub cancel_requested: AtomicBool,
}

impl AcpShared {
    pub fn new(
        sid: String,
        cwd: PathBuf,
        anchor: ProcessId,
        stream_tx: broadcast::Sender<ServiceMessage>,
        manager: Arc<tokio::sync::Mutex<ProcessManager>>,
        input_writers: Arc<tokio::sync::Mutex<HashMap<ProcessId, PtyInputWriter>>>,
    ) -> Self {
        Self {
            sid,
            cwd,
            anchor,
            stream_tx,
            projector: Mutex::new(AcpProjector::new()),
            pending_perms: Mutex::new(HashMap::new()),
            terminals: Mutex::new(HashMap::new()),
            manager,
            input_writers,
            agent_name: Mutex::new(None),
            cancel_requested: AtomicBool::new(false),
        }
    }

    pub fn snapshot_message(&self) -> ServiceMessage {
        let projector = self.projector.lock().unwrap();
        let messages_json =
            serde_json::to_string(projector.messages()).unwrap_or_else(|_| "[]".to_string());
        ServiceMessage::AgentMessagesSnapshot {
            sid: self.sid.clone(),
            messages_json,
        }
    }

    pub fn agent_info_message(&self) -> Option<ServiceMessage> {
        self.agent_name
            .lock()
            .unwrap()
            .as_ref()
            .map(|name| ServiceMessage::AcpAgentInfo {
                sid: self.sid.clone(),
                name: name.clone(),
            })
    }

    fn publish_agent_info(&self, name: String) {
        *self.agent_name.lock().unwrap() = Some(name.clone());
        self.emit(ServiceMessage::AcpAgentInfo {
            sid: self.sid.clone(),
            name,
        });
    }

    fn emit(&self, msg: ServiceMessage) {
        let _ = self.stream_tx.send(msg);
    }

    fn emit_status(&self, status: AgentRunStatus) {
        self.emit(ServiceMessage::AgentRunStatusChanged {
            sid: self.sid.clone(),
            status,
        });
    }
}

fn approval_details(
    request: &RequestPermissionRequest,
    projector: &AcpProjector,
) -> (String, String) {
    let call_id = request.tool_call.tool_call_id.to_string();
    let (projected_name, projected_args) =
        projector.tool_call_details(&call_id).unwrap_or_default();
    let name = request
        .tool_call
        .fields
        .title
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| (!projected_name.is_empty()).then_some(projected_name))
        .unwrap_or_else(|| "tool call".to_string());
    let args_json = request
        .tool_call
        .fields
        .raw_input
        .as_ref()
        .map(serde_json::Value::to_string)
        .or_else(|| (!projected_args.is_empty()).then_some(projected_args))
        .unwrap_or_else(|| "{}".to_string());
    (name, args_json)
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    agent_id: String,
    mcp_servers: Vec<McpServer>,
    resume: Option<String>,
    shared: Arc<AcpShared>,
    mut input_rx: mpsc::UnboundedReceiver<AcpInput>,
) {
    let session_meta = session_meta_for_agent(&agent_id);
    let mut child = match Command::new(&command)
        .args(&args)
        .envs(env)
        .current_dir(&shared.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            shared.emit_status(AgentRunStatus::Errored(format!("acp spawn failed: {err}")));
            return;
        }
    };
    let stdin = child.stdin.take().expect("piped stdin").compat_write();
    let stdout = child.stdout.take().expect("piped stdout").compat();
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(drain_stderr(stderr));
    }
    let transport = agent_client_protocol::ByteStreams::new(stdin, stdout);

    let perm_shared = shared.clone();
    let update_shared = shared.clone();
    let main_shared = shared.clone();
    let create_shared = shared.clone();
    let output_shared = shared.clone();
    let wait_shared = shared.clone();
    let kill_shared = shared.clone();
    let release_shared = shared.clone();
    let read_cwd = shared.cwd.clone();
    let write_cwd = shared.cwd.clone();

    let result = Client
        .builder()
        .on_receive_request(
            async move |req: RequestPermissionRequest,
                        responder: Responder<RequestPermissionResponse>,
                        _cx| {
                let call_id = req.tool_call.tool_call_id.to_string();
                let (name, args_json) = {
                    let projector = perm_shared.projector.lock().unwrap();
                    approval_details(&req, &projector)
                };
                perm_shared.emit(ServiceMessage::AgentAwaitingApproval {
                    sid: perm_shared.sid.clone(),
                    call_id: call_id.clone(),
                    name,
                    args_json,
                });
                let (tx, rx) = oneshot::channel();
                perm_shared
                    .pending_perms
                    .lock()
                    .unwrap()
                    .insert(call_id, tx);
                let decision = rx.await.unwrap_or(ApprovalDecision::Deny);
                let outcome = match pick_permission_option(&req.options, decision) {
                    Some(id) => {
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id))
                    }
                    None => RequestPermissionOutcome::Cancelled,
                };
                responder.respond(RequestPermissionResponse::new(outcome))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ReadTextFileRequest,
                        responder: Responder<ReadTextFileResponse>,
                        _cx| {
                match read_text_file(&read_cwd, &req) {
                    Ok(content) => responder.respond(ReadTextFileResponse::new(content)),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: WriteTextFileRequest,
                        responder: Responder<WriteTextFileResponse>,
                        _cx| {
                match write_text_file(&write_cwd, &req) {
                    Ok(()) => responder.respond(WriteTextFileResponse::new()),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: CreateTerminalRequest,
                        responder: Responder<CreateTerminalResponse>,
                        _cx| {
                match create_terminal(&create_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: TerminalOutputRequest,
                        responder: Responder<TerminalOutputResponse>,
                        _cx| {
                match terminal_output(&output_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: WaitForTerminalExitRequest,
                        responder: Responder<WaitForTerminalExitResponse>,
                        _cx| {
                match wait_for_terminal_exit(&wait_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: KillTerminalRequest,
                        responder: Responder<KillTerminalResponse>,
                        _cx| {
                match kill_terminal(&kill_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ReleaseTerminalRequest,
                        responder: Responder<ReleaseTerminalResponse>,
                        _cx| {
                match release_terminal(&release_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |note: SessionNotification, _cx| {
                let intents = update_shared.projector.lock().unwrap().apply(note.update);
                for intent in intents {
                    match intent {
                        Intent::Delta(text) => update_shared.emit(ServiceMessage::AgentDelta {
                            sid: update_shared.sid.clone(),
                            text,
                        }),
                        Intent::Snapshot => update_shared.emit(update_shared.snapshot_message()),
                        Intent::ProposedDiff {
                            call_id,
                            path,
                            old_text,
                            new_text,
                        } => update_shared.emit(ServiceMessage::AcpProposedDiff {
                            sid: update_shared.sid.clone(),
                            call_id,
                            path,
                            old_text,
                            new_text,
                        }),
                        Intent::FileTouched { path, line, kind } => {
                            update_shared.emit(ServiceMessage::AgentCommand {
                                request_id: AgentRequestId::new(),
                                anchor: Some(update_shared.anchor),
                                command: AgentCommand::FileTouched {
                                    anchor: update_shared.anchor,
                                    path,
                                    line,
                                    col: None,
                                    end_col: None,
                                    kind,
                                },
                            });
                        }
                    }
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(transport, async move |cx| {
            let mut init = InitializeRequest::new(ProtocolVersion::V1);
            init.client_capabilities.fs.read_text_file = true;
            init.client_capabilities.fs.write_text_file = true;
            // ACP-native terminals: the agent's shell/Bash execution flows through vmux's five
            // terminal methods, backed by real visible panes (see `create_terminal` et al.).
            init.client_capabilities.terminal = true;
            let init_resp = cx.send_request(init).block_task().await?;

            if let Some(name) = acp_display_name(init_resp.agent_info.as_ref()) {
                main_shared.publish_agent_info(name);
            }

            let mut session_id =
                load_requested_session(resume, init_resp.agent_capabilities.load_session, |sid| {
                    let mut load = LoadSessionRequest::new(sid, main_shared.cwd.clone());
                    load.mcp_servers = mcp_servers.clone();
                    load.meta = session_meta.clone();
                    async { cx.send_request(load).block_task().await.map(|_| ()) }
                })
                .await;
            if let Some(sid) = &session_id {
                main_shared.emit(ServiceMessage::AcpSessionCreated {
                    sid: main_shared.sid.clone(),
                    acp_session_id: sid.to_string(),
                });
            }
            main_shared.emit_status(AgentRunStatus::Idle);

            while let Some(input) = input_rx.recv().await {
                match input {
                    AcpInput::User { text, context } => {
                        main_shared.cancel_requested.store(false, Ordering::SeqCst);
                        main_shared
                            .projector
                            .lock()
                            .unwrap()
                            .push_user(text.clone());
                        main_shared.emit(main_shared.snapshot_message());
                        main_shared.emit_status(AgentRunStatus::Streaming);
                        let ensured = ensure_session(&mut session_id, || {
                            let mut new_session = NewSessionRequest::new(main_shared.cwd.clone());
                            new_session.mcp_servers = mcp_servers.clone();
                            new_session.meta = session_meta.clone();
                            async {
                                cx.send_request(new_session)
                                    .block_task()
                                    .await
                                    .map(|response| response.session_id)
                            }
                        })
                        .await;
                        let (active_session_id, created) = match ensured {
                            Ok(value) => value,
                            Err(err) => {
                                main_shared.emit_status(AgentRunStatus::Errored(format!(
                                    "acp session/new failed: {err}"
                                )));
                                continue;
                            }
                        };
                        if created {
                            main_shared.emit(ServiceMessage::AcpSessionCreated {
                                sid: main_shared.sid.clone(),
                                acp_session_id: active_session_id.to_string(),
                            });
                        }
                        let cx_prompt = cx.clone();
                        let shared = main_shared.clone();
                        cx.spawn(async move {
                            let prompt = PromptRequest::new(
                                active_session_id,
                                vec![ContentBlock::Text(TextContent::new(compose_agent_prompt(
                                    &text,
                                    context.as_deref(),
                                )))],
                            );
                            let errored = match cx_prompt.send_request(prompt).block_task().await {
                                Ok(_) => None,
                                Err(err) => Some(err.to_string()),
                            };
                            let cancelled = shared.cancel_requested.swap(false, Ordering::SeqCst);
                            shared.emit(shared.snapshot_message());
                            shared.emit_status(status_after_prompt(cancelled, errored));
                            Ok(())
                        })?;
                    }
                    AcpInput::Approve { call_id, decision } => {
                        if let Some(tx) = main_shared.pending_perms.lock().unwrap().remove(&call_id)
                        {
                            let _ = tx.send(decision);
                        }
                    }
                    AcpInput::Cancel => {
                        main_shared.cancel_requested.store(true, Ordering::SeqCst);
                        for (_id, tx) in main_shared.pending_perms.lock().unwrap().drain() {
                            let _ = tx.send(ApprovalDecision::Deny);
                        }
                        if let Some(sid) = &session_id {
                            let _ = cx.send_notification(CancelNotification::new(sid.clone()));
                        }
                    }
                    AcpInput::Close => {
                        if let Some(sid) = &session_id {
                            let _ = cx.send_notification(CancelNotification::new(sid.clone()));
                        }
                        break;
                    }
                }
            }
            Ok(())
        })
        .await;

    if let Err(err) = result {
        shared.emit_status(AgentRunStatus::Errored(format!(
            "acp connection ended: {err}"
        )));
    }
    let _ = child.kill().await;
}

fn acp_display_name(info: Option<&Implementation>) -> Option<String> {
    let info = info?;
    info.title
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let name = info.name.trim();
            (!name.is_empty()).then_some(name)
        })
        .map(str::to_string)
}

async fn load_requested_session<F, Fut, E>(
    resume: Option<String>,
    load_supported: bool,
    load: F,
) -> Option<SessionId>
where
    F: FnOnce(SessionId) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    let sid = resume.filter(|_| load_supported).map(SessionId::new)?;
    load(sid.clone()).await.ok()?;
    Some(sid)
}

async fn ensure_session<F, Fut, E>(
    session_id: &mut Option<SessionId>,
    create: F,
) -> Result<(SessionId, bool), E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<SessionId, E>>,
{
    if let Some(sid) = session_id.clone() {
        return Ok((sid, false));
    }
    let sid = create().await?;
    *session_id = Some(sid.clone());
    Ok((sid, true))
}

const CLAUDE_ACP_STEER_PROMPT: &str = "The native Bash, WebSearch, and WebFetch tools are disabled. \
Run ALL shell commands via the mcp__vmux__run tool, which opens a visible terminal the user can \
watch and take over. Use mcp__vmux__read_terminal to inspect continued output. Omit the pane \
argument because it targets your own terminal pane. Do ALL web access via the vmux browser tools. \
If you invoke a required Skill tool, continue the original user request in the same turn after \
the skill loads. Never end the turn after skill activation or answer only Ready.";

fn session_meta_for_agent(agent_id: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    if agent_id != "claude" {
        return None;
    }
    let serde_json::Value::Object(meta) = serde_json::json!({
        "systemPrompt": {
            "append": CLAUDE_ACP_STEER_PROMPT,
        },
        "claudeCode": {
            "options": {
                "disallowedTools": ["Bash", "Monitor", "WebSearch", "WebFetch"],
                "allowedTools": [
                    "mcp__vmux__run",
                    "mcp__vmux__read_terminal",
                    "mcp__vmux__browser_navigate",
                    "mcp__vmux__browser_snapshot",
                    "mcp__vmux__browser_scroll",
                ],
            },
        },
    }) else {
        unreachable!()
    };
    Some(meta)
}

async fn drain_stderr(stderr: tokio::process::ChildStderr) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::warn!(target: "acp", "{line}");
    }
}

/// Default PTY geometry for an ACP-native terminal. ACP `terminal/create` is size-less; the GUI
/// pane resizes the PTY (`ResizeProcess`) once it mounts.
const ACP_TERMINAL_COLS: u16 = 80;
const ACP_TERMINAL_ROWS: u16 = 24;

/// `terminal/create`: spawn a real (visible) PTY on the daemon's process manager, register it as an
/// ACP terminal, and tell the GUI to open a pane bound to it. Returns the ACP `terminalId` (the
/// vmux `ProcessId` string).
async fn create_terminal(
    shared: &AcpShared,
    req: CreateTerminalRequest,
) -> Result<CreateTerminalResponse, String> {
    let CreateTerminalRequest {
        command,
        args,
        env,
        cwd,
        output_byte_limit,
        ..
    } = req;
    let env: Vec<(String, String)> = env.into_iter().map(|var| (var.name, var.value)).collect();
    let cwd = cwd.unwrap_or_else(|| shared.cwd.clone());
    if !cwd.is_absolute() {
        return Err(format!(
            "acp: terminal cwd must be absolute: {}",
            cwd.display()
        ));
    }
    if !cwd.is_dir() {
        return Err(format!(
            "acp: terminal cwd is not a directory: {}",
            cwd.display()
        ));
    }
    let cwd = cwd.to_string_lossy().into_owned();
    let id = ProcessId::new();

    let (exit_stream, writer) = {
        let mut mgr = shared.manager.lock().await;
        mgr.create_process_keep_alive(
            id,
            command.clone(),
            args.clone(),
            cwd.clone(),
            env,
            ACP_TERMINAL_COLS,
            ACP_TERMINAL_ROWS,
        )?;
        let exit_stream = mgr.processes.get(&id).map(|process| process.subscribe());
        (exit_stream, mgr.input_writer(&id))
    };

    // Let the user take over the pane by typing.
    if let Some(writer) = writer {
        shared.input_writers.lock().await.insert(id, writer);
    }

    // Resolve the child's exit code once, off the process broadcast, for wait_for_exit / output.
    let (exit_tx, exit_rx) = watch::channel(AcpTerminalExit::Pending);
    if let Some(mut exit_stream) = exit_stream {
        tokio::spawn(async move {
            loop {
                match exit_stream.recv().await {
                    Ok(ServiceMessage::ProcessExited { exit_code, .. }) => {
                        let _ = exit_tx.send(AcpTerminalExit::Exited(exit_code));
                        break;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {
                        let _ = exit_tx.send(AcpTerminalExit::Removed);
                        break;
                    }
                }
            }
        });
    }

    let terminal_id = id.to_string();
    shared.terminals.lock().unwrap().insert(
        terminal_id.clone(),
        AcpTerminal {
            process_id: id,
            exit_rx,
            output_byte_limit,
        },
    );
    shared.emit(ServiceMessage::AcpTerminalCreated {
        sid: shared.sid.clone(),
        terminal_id: terminal_id.clone(),
        process_id: id,
        command,
        args,
        cwd: Some(cwd),
    });
    Ok(CreateTerminalResponse::new(TerminalId::new(terminal_id)))
}

/// Look up the vmux `ProcessId` and last-known exit code for an ACP `terminalId`.
fn lookup_terminal(
    shared: &AcpShared,
    terminal_id: &TerminalId,
) -> Result<(ProcessId, AcpTerminalExit, Option<u64>), String> {
    let key = terminal_id.0.to_string();
    let terminals = shared.terminals.lock().unwrap();
    let terminal = terminals
        .get(&key)
        .ok_or_else(|| format!("acp: unknown terminal {key}"))?;
    let exit = *terminal.exit_rx.borrow();
    if matches!(exit, AcpTerminalExit::Removed) {
        return Err(format!("acp: terminal {key} process no longer exists"));
    }
    Ok((terminal.process_id, exit, terminal.output_byte_limit))
}

fn terminal_exit_status(code: Option<i32>) -> TerminalExitStatus {
    let status = TerminalExitStatus::new();
    match code {
        Some(code) => status.exit_code(code as u32),
        None => status,
    }
}

/// `terminal/output`: current scrollback of the backing process plus its exit status (if it ended).
async fn terminal_output(
    shared: &AcpShared,
    req: TerminalOutputRequest,
) -> Result<TerminalOutputResponse, String> {
    let (process_id, exit, output_byte_limit) = lookup_terminal(shared, &req.terminal_id)?;
    let output = {
        let mgr = shared.manager.lock().await;
        mgr.processes
            .get(&process_id)
            .map(|process| process.full_text())
            .ok_or_else(|| {
                format!(
                    "acp: terminal {} process no longer exists",
                    req.terminal_id.0
                )
            })?
    };
    let (output, truncated) = truncate_terminal_output(output, output_byte_limit);
    let mut resp = TerminalOutputResponse::new(output, truncated);
    if let AcpTerminalExit::Exited(code) = exit {
        resp = resp.exit_status(terminal_exit_status(code));
    }
    Ok(resp)
}

/// `terminal/wait_for_exit`: block until the backing child exits, then report its status.
async fn wait_for_terminal_exit(
    shared: &AcpShared,
    req: WaitForTerminalExitRequest,
) -> Result<WaitForTerminalExitResponse, String> {
    let key = req.terminal_id.0.to_string();
    let mut exit_rx = {
        let terminals = shared.terminals.lock().unwrap();
        terminals
            .get(&key)
            .map(|terminal| terminal.exit_rx.clone())
            .ok_or_else(|| format!("acp: unknown terminal {key}"))?
    };
    let code = loop {
        match *exit_rx.borrow() {
            AcpTerminalExit::Pending => {}
            AcpTerminalExit::Exited(code) => break code,
            AcpTerminalExit::Removed => {
                return Err(format!("acp: terminal {key} process no longer exists"));
            }
        }
        if exit_rx.changed().await.is_err() {
            return Err(format!("acp: terminal {key} exit state closed"));
        }
    };
    Ok(WaitForTerminalExitResponse::new(terminal_exit_status(code)))
}

/// `terminal/kill`: kill the child but keep the pane (its output stays readable).
async fn kill_terminal(
    shared: &AcpShared,
    req: KillTerminalRequest,
) -> Result<KillTerminalResponse, String> {
    let (process_id, _, _) = lookup_terminal(shared, &req.terminal_id)?;
    shared.manager.lock().await.kill_process(&process_id);
    Ok(KillTerminalResponse::new())
}

fn truncate_terminal_output(output: String, output_byte_limit: Option<u64>) -> (String, bool) {
    let Some(limit) = output_byte_limit else {
        return (output, false);
    };
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if output.len() <= limit {
        return (output, false);
    }
    let mut start = output.len().saturating_sub(limit);
    while !output.is_char_boundary(start) {
        start += 1;
    }
    (output[start..].to_string(), true)
}

/// `terminal/release`: stop tracking the terminal and kill the backing process. The visible pane is
/// left in place; the GUI reaps it when the user closes it.
async fn release_terminal(
    shared: &AcpShared,
    req: ReleaseTerminalRequest,
) -> Result<ReleaseTerminalResponse, String> {
    let terminal = shared
        .terminals
        .lock()
        .unwrap()
        .remove(&req.terminal_id.0.to_string())
        .ok_or_else(|| format!("acp: unknown terminal {}", req.terminal_id.0))?;
    shared
        .manager
        .lock()
        .await
        .kill_process(&terminal.process_id);
    Ok(ReleaseTerminalResponse::new())
}

/// Maps a host decision (the wire `Allow`/`Deny`) onto an ACP permission option, preferring the
/// one-shot kind, then the always-kind. Returns `None` (→ the request is cancelled) when the agent
/// offers no option matching the decision — never falls back to an option that could approve a
/// denied call.
fn pick_permission_option(
    options: &[PermissionOption],
    decision: ApprovalDecision,
) -> Option<PermissionOptionId> {
    use agent_client_protocol::schema::v1::PermissionOptionKind as Kind;
    let preferred: &[Kind] = match decision {
        ApprovalDecision::Allow => &[Kind::AllowOnce, Kind::AllowAlways],
        ApprovalDecision::Deny => &[Kind::RejectOnce, Kind::RejectAlways],
    };
    preferred
        .iter()
        .find_map(|kind| options.iter().find(|option| &option.kind == kind))
        .map(|option| option.option_id.clone())
}

/// Resolve an ACP fs path against the session cwd, rejecting traversal and anything outside
/// the session root (ACP sends absolute paths).
fn resolve_in_cwd(cwd: &std::path::Path, path: &std::path::Path) -> Option<PathBuf> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if !abs.starts_with(cwd) {
        return None;
    }
    // Lexical `starts_with` is not enough: a symlink inside cwd can point outside. When cwd is a
    // real directory, require the deepest existing ancestor (canonicalized) to stay inside it. The
    // target itself may not exist yet (writes), so we canonicalize the nearest existing ancestor.
    if let Ok(real_cwd) = cwd.canonicalize()
        && let Some(anchor) = abs.ancestors().find_map(|a| a.canonicalize().ok())
        && !anchor.starts_with(&real_cwd)
    {
        return None;
    }
    Some(abs)
}

fn slice_lines(text: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return text.to_string();
    }
    let start = line.unwrap_or(1).saturating_sub(1) as usize;
    let lines: Vec<&str> = text.lines().collect();
    let end = limit
        .map(|l| start.saturating_add(l as usize).min(lines.len()))
        .unwrap_or(lines.len());
    lines.get(start..end).unwrap_or(&[]).join("\n")
}

fn read_text_file(cwd: &std::path::Path, req: &ReadTextFileRequest) -> Result<String, String> {
    let path = resolve_in_cwd(cwd, &req.path).ok_or("path outside session cwd")?;
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(slice_lines(&text, req.line, req.limit))
}

fn write_text_file(cwd: &std::path::Path, req: &WriteTextFileRequest) -> Result<(), String> {
    let path = resolve_in_cwd(cwd, &req.path).ok_or("path outside session cwd")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, &req.content).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Decide the run status to emit after a prompt future resolves. A cancel in flight wins over
/// both success and error so the UI shows `Interrupted`.
fn status_after_prompt(cancelled: bool, errored: Option<String>) -> AgentRunStatus {
    if cancelled {
        AgentRunStatus::Interrupted
    } else if let Some(err) = errored {
        AgentRunStatus::Errored(err)
    } else {
        AgentRunStatus::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        Implementation, PermissionOptionKind, ToolCall, ToolCallUpdateFields,
    };

    fn opt(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), id.to_string(), kind)
    }

    #[test]
    fn acp_display_name_prefers_title_then_name() {
        let titled = Implementation::new("antigravity", "1.0").title("Antigravity");
        assert_eq!(
            acp_display_name(Some(&titled)).as_deref(),
            Some("Antigravity")
        );

        let named = Implementation::new("claude-code-acp", "1.0");
        assert_eq!(
            acp_display_name(Some(&named)).as_deref(),
            Some("claude-code-acp")
        );
    }

    #[test]
    fn acp_display_name_ignores_blank_metadata() {
        let blank_title = Implementation::new("codex-acp", "1.0").title("   ");
        assert_eq!(
            acp_display_name(Some(&blank_title)).as_deref(),
            Some("codex-acp")
        );

        let blank = Implementation::new("   ", "1.0");
        assert_eq!(acp_display_name(Some(&blank)), None);
        assert_eq!(acp_display_name(None), None);
    }

    #[test]
    fn acp_agent_info_is_replayable_without_a_subscriber() {
        let (stream_tx, stream_rx) = broadcast::channel(1);
        drop(stream_rx);
        let shared = AcpShared::new(
            "s1".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream_tx,
            Arc::new(tokio::sync::Mutex::new(ProcessManager::default())),
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        );

        shared.publish_agent_info("Antigravity".into());

        match shared.agent_info_message() {
            Some(ServiceMessage::AcpAgentInfo { sid, name }) => {
                assert_eq!(sid, "s1");
                assert_eq!(name, "Antigravity");
            }
            other => panic!("expected replayable ACP agent info, got {other:?}"),
        }
    }

    #[test]
    fn approval_details_fall_back_to_projected_tool_call() {
        let mut projector = AcpProjector::new();
        projector.apply(agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
            ToolCall::new("call-1", "vmux.run")
                .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
        ));
        let request = RequestPermissionRequest::new(
            "session-1",
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new(),
            ),
            Vec::new(),
        );

        assert_eq!(
            approval_details(&request, &projector),
            (
                "vmux.run".to_string(),
                r#"{"command":"echo hi","focus":true}"#.to_string(),
            )
        );
    }

    #[test]
    fn approval_details_prefer_permission_request_fields() {
        let mut projector = AcpProjector::new();
        projector.apply(agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
            ToolCall::new("call-1", "old").raw_input(serde_json::json!({"command": "old"})),
        ));
        let request = RequestPermissionRequest::new(
            "session-1",
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new()
                    .title("new")
                    .raw_input(serde_json::json!({"command": "new"})),
            ),
            Vec::new(),
        );

        assert_eq!(
            approval_details(&request, &projector),
            ("new".to_string(), r#"{"command":"new"}"#.to_string(),)
        );
    }

    #[tokio::test]
    async fn requested_resume_loads_only_when_supported() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let loaded = load_requested_session(Some("resume-1".into()), true, |sid| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                assert_eq!(sid.to_string(), "resume-1");
                Ok::<(), ()>(())
            }
        })
        .await;
        assert_eq!(loaded.unwrap().to_string(), "resume-1");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        let skipped = load_requested_session(Some("resume-2".into()), false, |_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Ok::<(), ()>(()) }
        })
        .await;
        assert!(skipped.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_requested_resume_stays_unassigned() {
        let loaded = load_requested_session(Some("stale".into()), true, |_| async {
            Err::<(), &'static str>("missing")
        })
        .await;
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn ensure_session_creates_once_then_reuses_id() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let mut session_id = None;
        let (created_id, created) = ensure_session(&mut session_id, || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Ok::<SessionId, ()>(SessionId::new("created")) }
        })
        .await
        .unwrap();
        assert!(created);
        assert_eq!(created_id.to_string(), "created");

        let (reused_id, created) = ensure_session(&mut session_id, || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Ok::<SessionId, ()>(SessionId::new("unexpected")) }
        })
        .await
        .unwrap();
        assert!(!created);
        assert_eq!(reused_id.to_string(), "created");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_session_creation_remains_retryable() {
        let mut session_id = None;
        let result = ensure_session(&mut session_id, || async {
            Err::<SessionId, &'static str>("create failed")
        })
        .await;
        assert_eq!(result.unwrap_err(), "create failed");
        assert!(session_id.is_none());
    }

    #[test]
    fn status_after_prompt_cancel_wins() {
        assert_eq!(status_after_prompt(false, None), AgentRunStatus::Idle);
        assert_eq!(
            status_after_prompt(false, Some("boom".into())),
            AgentRunStatus::Errored("boom".into())
        );
        assert_eq!(status_after_prompt(true, None), AgentRunStatus::Interrupted);
        assert_eq!(
            status_after_prompt(true, Some("boom".into())),
            AgentRunStatus::Interrupted
        );
    }

    #[test]
    fn private_context_wraps_wire_prompt_without_changing_display_text() {
        let wire = compose_agent_prompt("continue here", Some("prior conversation"));

        assert!(wire.starts_with(crate::protocol::PRIVATE_CONTEXT_PREFIX));
        assert!(wire.contains("prior conversation"));
        assert!(wire.ends_with("continue here"));
        assert_eq!(compose_agent_prompt("plain", None), "plain");
    }

    #[test]
    fn claude_acp_disables_native_shell_and_steers_skill_continuation() {
        let meta = session_meta_for_agent("claude").expect("Claude ACP metadata");
        let options = &meta["claudeCode"]["options"];

        assert_eq!(
            options["disallowedTools"],
            serde_json::json!(["Bash", "Monitor", "WebSearch", "WebFetch"])
        );
        assert!(
            options["allowedTools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool == "mcp__vmux__run")
        );
        let prompt = meta["systemPrompt"]["append"].as_str().unwrap();
        assert!(prompt.contains("mcp__vmux__run"));
        assert!(prompt.contains("continue the original user request"));
        assert!(session_meta_for_agent("vibe-acp").is_none());
    }

    #[test]
    fn pick_permission_option_prefers_once_then_first() {
        let opts = vec![
            opt("once", PermissionOptionKind::AllowOnce),
            opt("always", PermissionOptionKind::AllowAlways),
            opt("rej", PermissionOptionKind::RejectOnce),
        ];
        assert_eq!(
            pick_permission_option(&opts, ApprovalDecision::Allow)
                .unwrap()
                .to_string(),
            "once"
        );
        assert_eq!(
            pick_permission_option(&opts, ApprovalDecision::Deny)
                .unwrap()
                .to_string(),
            "rej"
        );

        let always_only = vec![
            opt("aa", PermissionOptionKind::AllowAlways),
            opt("ra", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(
            pick_permission_option(&always_only, ApprovalDecision::Allow)
                .unwrap()
                .to_string(),
            "aa"
        );
    }

    #[test]
    fn resolve_in_cwd_rejects_escape() {
        let cwd = std::path::Path::new("/work");
        assert_eq!(
            resolve_in_cwd(cwd, std::path::Path::new("/work/a.rs")),
            Some(PathBuf::from("/work/a.rs"))
        );
        assert!(resolve_in_cwd(cwd, std::path::Path::new("/etc/passwd")).is_none());
        assert!(resolve_in_cwd(cwd, std::path::Path::new("/work/../etc/passwd")).is_none());
    }

    #[test]
    fn slice_lines_honors_line_and_limit() {
        let text = "a\nb\nc\nd";
        assert_eq!(slice_lines(text, None, None), "a\nb\nc\nd");
        assert_eq!(slice_lines(text, Some(2), None), "b\nc\nd");
        assert_eq!(slice_lines(text, Some(2), Some(2)), "b\nc");
        assert_eq!(slice_lines(text, Some(10), Some(2)), "");
    }

    fn test_shared(
        manager: Arc<tokio::sync::Mutex<ProcessManager>>,
    ) -> (Arc<AcpShared>, broadcast::Receiver<ServiceMessage>) {
        let (stream_tx, stream_rx) = broadcast::channel(64);
        let shared = Arc::new(AcpShared::new(
            "s1".to_string(),
            std::env::temp_dir(),
            ProcessId::new(),
            stream_tx,
            manager,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        ));
        (shared, stream_rx)
    }

    /// End-to-end of the daemon terminal path: `terminal/create` spawns a real PTY + emits
    /// `AcpTerminalCreated`; `wait_for_exit` resolves with the child's code; `output` reads the
    /// completed command's text after exit (kept alive); `release` stops tracking it.
    #[tokio::test]
    async fn acp_terminal_create_wait_output_release() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, mut stream_rx) = test_shared(manager.clone());

        // Drive PTY output + exit detection like the server poll loop (which keeps ACP terminals).
        let poll_mgr = manager.clone();
        let poll = tokio::spawn(async move {
            loop {
                poll_mgr.lock().await.poll_all();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });

        let req = CreateTerminalRequest::new("s1", "/bin/sh").args(vec![
            "-c".to_string(),
            "printf hi; sleep 0.1; exit 7".to_string(),
        ]);
        let created = create_terminal(&shared, req).await.expect("create");
        let tid = created.terminal_id.0.to_string();
        assert!(shared.terminals.lock().unwrap().contains_key(&tid));

        let (emitted_id, emitted_pid) = loop {
            match stream_rx.recv().await.expect("stream open") {
                ServiceMessage::AcpTerminalCreated {
                    terminal_id,
                    process_id,
                    ..
                } => break (terminal_id, process_id),
                _ => continue,
            }
        };
        assert_eq!(emitted_id, tid);
        assert_eq!(emitted_pid.to_string(), tid);

        let wait = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_terminal_exit(
                &shared,
                WaitForTerminalExitRequest::new("s1", TerminalId::new(tid.clone())),
            ),
        )
        .await
        .expect("wait_for_exit timed out")
        .expect("wait_for_exit");
        assert_eq!(wait.exit_status.exit_code, Some(7));

        let out = terminal_output(
            &shared,
            TerminalOutputRequest::new("s1", TerminalId::new(tid.clone())),
        )
        .await
        .expect("output");
        assert!(out.output.contains("hi"), "output was {:?}", out.output);
        assert_eq!(out.exit_status.and_then(|status| status.exit_code), Some(7));

        release_terminal(
            &shared,
            ReleaseTerminalRequest::new("s1", TerminalId::new(tid.clone())),
        )
        .await
        .expect("release");
        assert!(!shared.terminals.lock().unwrap().contains_key(&tid));

        poll.abort();
    }

    #[tokio::test]
    async fn terminal_output_unknown_terminal_errors() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager);
        let result =
            terminal_output(&shared, TerminalOutputRequest::new("s1", "does-not-exist")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_terminal_rejects_nonexistent_cwd() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager.clone());
        let cwd = std::env::temp_dir().join(format!(
            "vmux-acp-missing-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));

        let result = create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh").cwd(cwd),
        )
        .await;

        assert!(result.is_err());
        assert!(manager.lock().await.processes.is_empty());
    }

    #[tokio::test]
    async fn create_terminal_rejects_relative_cwd() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager.clone());

        let result = create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh").cwd("."),
        )
        .await;

        assert!(result.is_err());
        assert!(manager.lock().await.processes.is_empty());
    }

    #[tokio::test]
    async fn removed_terminal_errors_for_wait_and_output() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager.clone());
        let created = create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh")
                .args(vec!["-c".to_string(), "sleep 30".to_string()]),
        )
        .await
        .expect("create");
        let terminal_id = created.terminal_id.0.to_string();
        let process_id = terminal_id.parse().expect("process id");
        manager.lock().await.remove_process(&process_id);

        let wait = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            wait_for_terminal_exit(
                &shared,
                WaitForTerminalExitRequest::new("s1", TerminalId::new(terminal_id.clone())),
            ),
        )
        .await
        .expect("wait timeout");
        assert!(wait.is_err());

        let output = terminal_output(
            &shared,
            TerminalOutputRequest::new("s1", TerminalId::new(terminal_id.clone())),
        )
        .await;
        assert!(output.is_err());
        shared.terminals.lock().unwrap().remove(&terminal_id);
    }

    #[tokio::test]
    async fn release_terminal_kills_running_command() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager.clone());
        let created = create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh")
                .args(vec!["-c".to_string(), "sleep 30".to_string()]),
        )
        .await
        .expect("create");
        let terminal_id = created.terminal_id.0.to_string();
        let process_id = terminal_id.parse().expect("process id");

        release_terminal(
            &shared,
            ReleaseTerminalRequest::new("s1", TerminalId::new(terminal_id)),
        )
        .await
        .expect("release");

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        let exited = loop {
            let exited = {
                let mut manager = manager.lock().await;
                manager.poll_all();
                manager
                    .processes
                    .get(&process_id)
                    .and_then(|process| process.process_exit())
                    .is_some()
            };
            if exited || std::time::Instant::now() >= deadline {
                break exited;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        };
        if !exited {
            manager.lock().await.remove_process(&process_id);
        }

        assert!(exited, "release must kill a running terminal command");
    }

    #[tokio::test]
    async fn terminal_output_respects_byte_limit_at_char_boundary() {
        let manager = Arc::new(tokio::sync::Mutex::new(ProcessManager::default()));
        let (shared, _rx) = test_shared(manager.clone());
        let poll_manager = manager.clone();
        let poll = tokio::spawn(async move {
            loop {
                poll_manager.lock().await.poll_all();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });
        let created = create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh")
                .args(vec!["-c".to_string(), "printf 'abécd'".to_string()])
                .output_byte_limit(3),
        )
        .await
        .expect("create");
        let terminal_id = created.terminal_id.0.to_string();

        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_terminal_exit(
                &shared,
                WaitForTerminalExitRequest::new("s1", TerminalId::new(terminal_id.clone())),
            ),
        )
        .await
        .expect("wait timeout")
        .expect("wait");
        let output = terminal_output(
            &shared,
            TerminalOutputRequest::new("s1", TerminalId::new(terminal_id.clone())),
        )
        .await
        .expect("output");

        assert_eq!(output.output, "cd");
        assert!(output.truncated);

        release_terminal(
            &shared,
            ReleaseTerminalRequest::new("s1", TerminalId::new(terminal_id)),
        )
        .await
        .expect("release");
        poll.abort();
    }
}
