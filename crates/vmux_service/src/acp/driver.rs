//! Per-session ACP driver: spawns the agent subprocess, runs the `Client` connection,
//! and pumps prompts/approvals through it while projecting `session/update` to the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, CreateTerminalRequest, InitializeRequest,
    KillTerminalRequest, McpServer, NewSessionRequest, PromptRequest, ReadTextFileRequest,
    ReleaseTerminalRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, TerminalOutputRequest, TextContent,
    WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::{Client, Responder};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vmux_core::ProcessId;

use super::projector::{AcpProjector, Intent};
use crate::protocol::{AgentRunStatus, ApprovalDecision, ServiceMessage};

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

    let result = Client
        .builder()
        .on_receive_request(
            async move |req: RequestPermissionRequest,
                        responder: Responder<RequestPermissionResponse>,
                        _cx| {
                // STUB — real allow/deny round-trip lands in the permission task.
                let _ = (&perm_shared, req);
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: ReadTextFileRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: fs/read_text_file not yet implemented")
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: WriteTextFileRequest, responder: Responder<_>, _cx| {
                responder.respond_with_internal_error("acp: fs/write_text_file not yet implemented")
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
            init.client_capabilities.terminal = true;
            cx.send_request(init).block_task().await?;

            let mut new_session = NewSessionRequest::new(main_shared.cwd.clone());
            new_session.mcp_servers = mcp_servers;
            let session = cx.send_request(new_session).block_task().await?;
            main_shared.emit_status(AgentRunStatus::Idle);

            while let Some(input) = input_rx.recv().await {
                match input {
                    AcpInput::User(text) => {
                        main_shared.emit_status(AgentRunStatus::Streaming);
                        let prompt = PromptRequest::new(
                            session.session_id.clone(),
                            vec![ContentBlock::Text(TextContent::new(text))],
                        );
                        let status = match cx.send_request(prompt).block_task().await {
                            Ok(_) => AgentRunStatus::Idle,
                            Err(err) => AgentRunStatus::Errored(err.to_string()),
                        };
                        main_shared.emit(main_shared.snapshot_message());
                        main_shared.emit_status(status);
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
