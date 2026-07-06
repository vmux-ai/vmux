//! Per-session ACP driver: spawns the agent subprocess, runs the `Client` connection,
//! and pumps prompts/approvals through it while projecting `session/update` to the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, CreateTerminalRequest, InitializeRequest,
    KillTerminalRequest, McpServer, NewSessionRequest, PermissionOption, PermissionOptionId,
    PromptRequest, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, TerminalOutputRequest, TextContent,
    WaitForTerminalExitRequest, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{Client, Responder};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vmux_core::ProcessId;

use super::projector::{AcpProjector, Intent};
use crate::protocol::{
    AgentCommand, AgentRequestId, AgentRunStatus, ApprovalDecision, ServiceMessage,
};

/// A command pushed into a live ACP session from the GUI side.
pub enum AcpInput {
    User(String),
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    Close,
}

/// State shared between the driver's request handlers and its prompt loop.
pub struct AcpShared {
    pub sid: String,
    pub cwd: PathBuf,
    pub anchor: ProcessId,
    pub stream_tx: broadcast::Sender<ServiceMessage>,
    pub projector: Mutex<AcpProjector>,
    pub pending_perms: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    pub terminals: Mutex<HashMap<String, ProcessId>>,
}

impl AcpShared {
    pub fn snapshot_message(&self) -> ServiceMessage {
        let projector = self.projector.lock().unwrap();
        let messages_json =
            serde_json::to_string(projector.messages()).unwrap_or_else(|_| "[]".to_string());
        ServiceMessage::AgentMessagesSnapshot {
            sid: self.sid.clone(),
            messages_json,
        }
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

#[allow(clippy::too_many_arguments)]
pub async fn run(
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    mcp_servers: Vec<McpServer>,
    shared: Arc<AcpShared>,
    mut input_rx: mpsc::UnboundedReceiver<AcpInput>,
) {
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
    let read_cwd = shared.cwd.clone();
    let write_cwd = shared.cwd.clone();

    let result = Client
        .builder()
        .on_receive_request(
            async move |req: RequestPermissionRequest,
                        responder: Responder<RequestPermissionResponse>,
                        _cx| {
                let call_id = req.tool_call.tool_call_id.to_string();
                let name = req.tool_call.fields.title.clone().unwrap_or_default();
                let args_json = req
                    .tool_call
                    .fields
                    .raw_input
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
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
            async move |_req: CreateTerminalRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: terminal/create not yet implemented")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: TerminalOutputRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: terminal/output not yet implemented")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: WaitForTerminalExitRequest, responder: Responder<_>, _cx| {
                responder
                    .respond_with_internal_error("acp: terminal/wait_for_exit not yet implemented")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: KillTerminalRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: terminal/kill not yet implemented")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: ReleaseTerminalRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: terminal/release not yet implemented")
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
            // Terminals are provided via the vmux_mcp `run` tool (real panes), not ACP-native.
            init.client_capabilities.terminal = false;
            cx.send_request(init).block_task().await?;

            let mut new_session = NewSessionRequest::new(main_shared.cwd.clone());
            new_session.mcp_servers = mcp_servers;
            let session = cx.send_request(new_session).block_task().await?;
            main_shared.emit_status(AgentRunStatus::Idle);

            while let Some(input) = input_rx.recv().await {
                match input {
                    AcpInput::User(text) => {
                        main_shared
                            .projector
                            .lock()
                            .unwrap()
                            .push_user(text.clone());
                        main_shared.emit(main_shared.snapshot_message());
                        main_shared.emit_status(AgentRunStatus::Streaming);
                        let cx_prompt = cx.clone();
                        let shared = main_shared.clone();
                        let session_id = session.session_id.clone();
                        cx.spawn(async move {
                            let prompt = PromptRequest::new(
                                session_id,
                                vec![ContentBlock::Text(TextContent::new(text))],
                            );
                            let status = match cx_prompt.send_request(prompt).block_task().await {
                                Ok(_) => AgentRunStatus::Idle,
                                Err(err) => AgentRunStatus::Errored(err.to_string()),
                            };
                            shared.emit(shared.snapshot_message());
                            shared.emit_status(status);
                            Ok(())
                        })?;
                    }
                    AcpInput::Approve { call_id, decision } => {
                        if let Some(tx) = main_shared.pending_perms.lock().unwrap().remove(&call_id)
                        {
                            let _ = tx.send(decision);
                        }
                    }
                    AcpInput::Close => {
                        let _ = cx
                            .send_notification(CancelNotification::new(session.session_id.clone()));
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

async fn drain_stderr(stderr: tokio::process::ChildStderr) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::warn!(target: "acp", "{line}");
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::PermissionOptionKind;

    fn opt(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), id.to_string(), kind)
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
}
