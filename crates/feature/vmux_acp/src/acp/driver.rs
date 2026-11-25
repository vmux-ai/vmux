use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use agent_client_protocol::schema::ProtocolVersion;
#[cfg(test)]
use agent_client_protocol::schema::v1::SessionConfigOptionCategory;
use agent_client_protocol::schema::v1::{
    AudioContent, CancelNotification, ContentBlock, CreateTerminalRequest, CreateTerminalResponse,
    EnvVariable, HttpHeader, ImageContent, Implementation, InitializeRequest, KillTerminalRequest,
    KillTerminalResponse, LoadSessionRequest, McpCapabilities, McpServer, McpServerHttp,
    McpServerSse, McpServerStdio, NewSessionRequest, PermissionOption, PermissionOptionId,
    PromptCapabilities, PromptRequest, ReadTextFileRequest, ReadTextFileResponse,
    ReleaseTerminalRequest, ReleaseTerminalResponse, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, ResourceLink, SelectedPermissionOutcome,
    SessionConfigOption, SessionId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionModeRequest, TerminalExitStatus, TerminalId,
    TerminalOutputRequest, TerminalOutputResponse, TextContent, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder};
use base64::Engine;
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vmux_ecs::ProcessId;
use vmux_ecs::channel::{AsyncChannel, ChannelSender};

use super::agent_process::AgentProcess;
#[cfg(test)]
use super::projection::AcpTranscripts;
use super::projection::{AcpToolCallId, ApprovalDetailsQuery, ApprovalDetailsRequest};
use super::workspace_driver::WorkspaceLocation;
use super::{AcpProjectionEvent, AcpSelectionSnapshot};
#[cfg(test)]
use bevy::prelude::{MinimalPlugins, Name};
use vmux_api::protocol::{
    AgentAttachment, AgentPromptEnvelope, AgentRunStatus, ApprovalDecision, ManagedMcpServer,
    ManagedMcpTransport, ServiceMessage, SharedEvent,
};
#[cfg(test)]
use vmux_api::session::AssistantBlock;
use vmux_api::session::{Message, RemoteApproval};
#[cfg(test)]
use vmux_process::ProcessPlugin;
use vmux_process::{ProcessLaunch, ProcessRuntime, ProcessUpdate};

use agent_client_protocol::schema::v1::PermissionOptionKind as Kind;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Default)]
pub(super) struct AcpMcpServers {
    builtin: Option<McpServer>,
    managed: Vec<ManagedMcpServer>,
    proxy_command: Option<String>,
    proxy_prefix_args: Vec<String>,
    profile: Option<String>,
    cwd: Option<PathBuf>,
}

impl AcpMcpServers {
    pub(super) fn from_sources(
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        managed_mcp_servers: Vec<ManagedMcpServer>,
        cwd: PathBuf,
    ) -> Self {
        let profile = mcp_args
            .windows(2)
            .find_map(|pair| (pair[0] == "--profile").then(|| pair[1].clone()));
        let proxy_prefix_args = mcp_args
            .iter()
            .position(|arg| arg == "mcp")
            .map(|index| mcp_args[..index].to_vec())
            .unwrap_or_default();
        let proxy_command = mcp_command.clone();
        let builtin = mcp_command.map(|command| {
            McpServer::Stdio(McpServerStdio::new("vmux", PathBuf::from(command)).args(mcp_args))
        });
        let managed = managed_mcp_servers
            .into_iter()
            .filter(|server| !server.name.trim().is_empty() && server.name != "vmux")
            .collect();
        Self {
            builtin,
            managed,
            proxy_command,
            proxy_prefix_args,
            profile,
            cwd: Some(cwd),
        }
    }

    fn proxy_server(&self, name: &str) -> Option<McpServer> {
        let command = self.proxy_command.as_ref()?;
        let mut args = self.proxy_prefix_args.clone();
        args.push("mcp-proxy".to_string());
        args.push("--server".to_string());
        args.push(name.to_string());
        if let Some(profile) = &self.profile {
            args.push("--profile".to_string());
            args.push(profile.clone());
        }
        if let Some(cwd) = &self.cwd {
            args.push("--cwd".to_string());
            args.push(cwd.to_string_lossy().into_owned());
        }
        Some(McpServer::Stdio(
            McpServerStdio::new(name.to_string(), PathBuf::from(command)).args(args),
        ))
    }

    pub(super) fn for_capabilities(&self, capabilities: &McpCapabilities) -> Vec<McpServer> {
        let mut servers = Vec::new();
        if let Some(builtin) = &self.builtin {
            servers.push(builtin.clone());
        }
        for server in &self.managed {
            let needs_proxy = server.oauth
                || (server.transport == ManagedMcpTransport::Stdio && server.cwd.is_some())
                || (server.transport == ManagedMcpTransport::Http && !capabilities.http)
                || (server.transport == ManagedMcpTransport::Sse && !capabilities.sse);
            if needs_proxy {
                if let Some(proxied) = self.proxy_server(&server.name) {
                    servers.push(proxied);
                } else {
                    tracing::warn!(
                        target: "acp",
                        server = %server.name,
                        "skipping MCP server requiring vmux mcp-proxy; sidecar unavailable"
                    );
                }
                continue;
            }
            match server.transport {
                ManagedMcpTransport::Stdio => {
                    let Some(command) = &server.command else {
                        continue;
                    };
                    let env = server
                        .env
                        .iter()
                        .map(|(name, value)| EnvVariable::new(name.clone(), value.clone()))
                        .collect();
                    servers.push(McpServer::Stdio(
                        McpServerStdio::new(server.name.clone(), PathBuf::from(command))
                            .args(server.args.clone())
                            .env(env),
                    ));
                }
                ManagedMcpTransport::Http => {
                    let Some(url) = &server.url else {
                        continue;
                    };
                    let headers = server
                        .headers
                        .iter()
                        .map(|(name, value)| HttpHeader::new(name.clone(), value.clone()))
                        .collect();
                    servers.push(McpServer::Http(
                        McpServerHttp::new(server.name.clone(), url.clone()).headers(headers),
                    ));
                }
                ManagedMcpTransport::Sse => {
                    let Some(url) = &server.url else {
                        continue;
                    };
                    let headers = server
                        .headers
                        .iter()
                        .map(|(name, value)| HttpHeader::new(name.clone(), value.clone()))
                        .collect();
                    servers.push(McpServer::Sse(
                        McpServerSse::new(server.name.clone(), url.clone()).headers(headers),
                    ));
                }
            }
        }
        servers
    }
}

pub(super) struct AcpConfigStateInput {
    pub(super) config_options: Vec<SessionConfigOption>,
    pub(super) modes: Option<SessionModeState>,
}

pub(super) struct AcpSelectedConfigInput {
    pub(super) config_id: Option<String>,
    pub(super) value: String,
    pub(super) config_options: Vec<SessionConfigOption>,
}

pub(super) struct AcpProjectionSenders {
    events: ChannelSender<AcpProjectionEvent>,
}

impl AcpProjectionSenders {
    pub(super) fn open_with_waker(waker: Waker) -> (Self, AsyncChannel<AcpProjectionEvent>) {
        let (events, channel) = AsyncChannel::unbounded_with_waker(waker);
        (Self { events }, channel)
    }

    pub(super) fn begin_history_replay(&self) {
        let _ = self.events.send(AcpProjectionEvent::BeginHistoryReplay);
    }

    pub(super) fn session_update(&self, update: SessionUpdate) {
        let _ = self
            .events
            .send(AcpProjectionEvent::SessionUpdate(Box::new(update)));
    }

    pub(super) fn finish_history_replay(&self, loaded: bool) {
        let _ = self
            .events
            .send(AcpProjectionEvent::FinishHistoryReplay { loaded });
    }

    pub(super) fn request_snapshot(&self) {
        let _ = self.events.send(AcpProjectionEvent::RequestSnapshot);
    }

    fn agent_info(&self, name: String) {
        let _ = self.events.send(AcpProjectionEvent::AgentInfo(name));
    }

    fn config_state(&self, input: AcpConfigStateInput) {
        let _ = self.events.send(AcpProjectionEvent::ConfigState(input));
    }

    fn selected_config(&self, input: AcpSelectedConfigInput) {
        let _ = self.events.send(AcpProjectionEvent::SelectedConfig(input));
    }

    fn status(&self, status: AgentRunStatus) {
        let _ = self.events.send(AcpProjectionEvent::Status(status));
    }

    fn approval_requested(&self, approval: RemoteApproval) {
        let _ = self
            .events
            .send(AcpProjectionEvent::ApprovalRequested(approval));
    }

    fn approval_resolved(&self, call_id: String) {
        let _ = self
            .events
            .send(AcpProjectionEvent::ApprovalResolved(call_id));
    }

    pub(super) fn approval_details(
        &self,
        query: ApprovalDetailsQuery,
        response: oneshot::Sender<Option<(String, String)>>,
    ) {
        let _ = self.events.send(AcpProjectionEvent::ApprovalDetails(
            ApprovalDetailsRequest { query, response },
        ));
    }

    fn startup_ready(&self) {
        let _ = self.events.send(AcpProjectionEvent::Startup(true));
    }

    fn startup_failed(&self) {
        let _ = self.events.send(AcpProjectionEvent::Startup(false));
    }

    async fn selection_snapshot(&self) -> AcpSelectionSnapshot {
        let (response, receiver) = oneshot::channel();
        let _ = self
            .events
            .send(AcpProjectionEvent::SelectionSnapshot(response));
        receiver.await.unwrap_or_default()
    }
}

pub(super) struct AcpPromptCommand {
    pub(super) text: String,
    pub(super) context: Option<String>,
    pub(super) attachments: Vec<AgentAttachment>,
    pub(super) preferred_mode: Option<String>,
}

pub(super) struct AcpApprovalCommand {
    pub(super) call_id: String,
    pub(super) decision: ApprovalDecision,
}

pub(super) struct AcpConfigCommand {
    pub(super) request_id: u64,
    pub(super) config_id: Option<String>,
    pub(super) value: String,
}

#[derive(Clone, Copy)]
enum AcpTerminalExit {
    Pending,
    Exited(Option<i32>),
    Removed,
}

impl AcpTerminalExit {
    fn recovered(recorded: Option<Option<i32>>) -> Option<Self> {
        match recorded {
            None => Some(Self::Removed),
            Some(None) => None,
            Some(code) => Some(Self::Exited(code)),
        }
    }

    fn status(code: Option<i32>) -> TerminalExitStatus {
        let status = TerminalExitStatus::new();
        match code {
            Some(code) => status.exit_code(code as u32),
            None => status,
        }
    }
}

struct AcpTerminal {
    process_id: ProcessId,
    exit_rx: watch::Receiver<AcpTerminalExit>,
    output_byte_limit: Option<u64>,
}

impl AcpTerminal {
    fn snapshot(&self) -> Result<AcpTerminalSnapshot, String> {
        let exit = *self.exit_rx.borrow();
        if matches!(exit, AcpTerminalExit::Removed) {
            return Err("process no longer exists".into());
        }
        Ok(AcpTerminalSnapshot {
            process_id: self.process_id,
            exit,
            output_byte_limit: self.output_byte_limit,
        })
    }
}

struct AcpTerminalSnapshot {
    process_id: ProcessId,
    exit: AcpTerminalExit,
    output_byte_limit: Option<u64>,
}

impl AcpTerminalSnapshot {
    fn truncate(&self, output: String) -> (String, bool) {
        let Some(limit) = self.output_byte_limit else {
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
}

#[derive(Default)]
struct AcpTerminals(Mutex<HashMap<String, AcpTerminal>>);

impl AcpTerminals {
    fn insert(&self, id: String, terminal: AcpTerminal) {
        self.0.lock().unwrap().insert(id, terminal);
    }

    fn snapshot(&self, terminal_id: &TerminalId) -> Result<AcpTerminalSnapshot, String> {
        let key = terminal_id.0.to_string();
        let terminals = self.0.lock().unwrap();
        let terminal = terminals
            .get(&key)
            .ok_or_else(|| format!("acp: unknown terminal {key}"))?;
        terminal
            .snapshot()
            .map_err(|error| format!("acp: terminal {key} {error}"))
    }

    fn exit_receiver(
        &self,
        terminal_id: &TerminalId,
    ) -> Result<watch::Receiver<AcpTerminalExit>, String> {
        let key = terminal_id.0.to_string();
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|terminal| terminal.exit_rx.clone())
            .ok_or_else(|| format!("acp: unknown terminal {key}"))
    }

    fn remove(&self, terminal_id: &TerminalId) -> Result<AcpTerminal, String> {
        let key = terminal_id.0.to_string();
        self.0
            .lock()
            .unwrap()
            .remove(&key)
            .ok_or_else(|| format!("acp: unknown terminal {key}"))
    }

    #[cfg(test)]
    fn contains(&self, terminal_id: &str) -> bool {
        self.0.lock().unwrap().contains_key(terminal_id)
    }
}

#[derive(Clone)]
struct AcpFsScope {
    cwd: PathBuf,
}

impl AcpFsScope {
    fn resolve(&self, path: &std::path::Path) -> Option<PathBuf> {
        vmux_path::ScopedPath::resolve(&self.cwd, path)
            .ok()
            .map(vmux_path::ScopedPath::into_path_buf)
    }

    fn read(&self, request: &ReadTextFileRequest) -> Result<String, String> {
        let path = self
            .resolve(&request.path)
            .ok_or("path outside session cwd")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        Ok(AcpFileContents(text).slice(request.line, request.limit))
    }

    fn write(&self, request: &WriteTextFileRequest) -> Result<(), String> {
        let path = self
            .resolve(&request.path)
            .ok_or("path outside session cwd")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, &request.content)
            .map_err(|error| format!("write {}: {error}", path.display()))
    }
}

struct AcpFileContents(String);

impl AcpFileContents {
    fn slice(&self, line: Option<u32>, limit: Option<u32>) -> String {
        if line.is_none() && limit.is_none() {
            return self.0.clone();
        }
        let start = line.unwrap_or(1).saturating_sub(1) as usize;
        let lines = self.0.lines().collect::<Vec<_>>();
        let end = limit
            .map(|limit| start.saturating_add(limit as usize).min(lines.len()))
            .unwrap_or(lines.len());
        lines.get(start..end).unwrap_or(&[]).join("\n")
    }
}

struct AcpAttachment<'a>(&'a AgentAttachment);

impl AcpAttachment<'_> {
    fn uri(&self) -> String {
        let path = &self.0.path;
        url::Url::from_file_path(path)
            .map(|url| url.to_string())
            .unwrap_or_else(|_| format!("file://{path}"))
    }
}

struct AcpAgentInfo<'a>(Option<&'a Implementation>);

impl AcpAgentInfo<'_> {
    fn display_name(&self) -> Option<String> {
        let info = self.0?;
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
}

struct AcpPermissionOptions<'a>(&'a [PermissionOption]);

impl AcpPermissionOptions<'_> {
    fn select(&self, decision: ApprovalDecision) -> Option<PermissionOptionId> {
        let preferred: &[Kind] = match decision {
            ApprovalDecision::Allow => &[Kind::AllowOnce, Kind::AllowAlways],
            ApprovalDecision::Deny => &[Kind::RejectOnce, Kind::RejectAlways],
            ApprovalDecision::AllowAlways => &[Kind::AllowAlways],
        };
        preferred
            .iter()
            .find_map(|kind| self.0.iter().find(|option| &option.kind == kind))
            .map(|option| option.option_id.clone())
    }
}

struct PromptCompletion {
    cancelled: bool,
    error: Option<String>,
}

impl PromptCompletion {
    fn status(self) -> AgentRunStatus {
        if self.cancelled {
            AgentRunStatus::Interrupted
        } else if let Some(error) = self.error {
            AgentRunStatus::Errored(error)
        } else {
            AgentRunStatus::Idle
        }
    }
}

#[derive(Default)]
struct AcpStderrTail(Mutex<VecDeque<String>>);

impl AcpStderrTail {
    const CAPACITY: usize = 50;
    const SHOWN: usize = 8;

    fn push(&self, line: String) {
        let mut tail = self.0.lock().unwrap();
        if tail.len() >= Self::CAPACITY {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    fn detail(&self, shown: usize) -> String {
        let tail = self.0.lock().unwrap();
        let lines = tail
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        if lines.is_empty() {
            return String::new();
        }
        let start = lines.len().saturating_sub(shown);
        format!("\n\n{}", lines[start..].join("\n"))
    }
}

pub(super) type AcpProcesses = ProcessRuntime;

pub(super) struct AcpShared {
    pub sid: String,
    cwd: Mutex<PathBuf>,
    pub anchor: ProcessId,
    pub stream_tx: broadcast::Sender<ServiceMessage>,
    projection: AcpProjectionSenders,
    pub(super) projector_updates: watch::Sender<u64>,
    pub pending_perms: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    terminals: AcpTerminals,
    processes: AcpProcesses,
    pub cancel_requested: AtomicBool,
    stderr_tail: AcpStderrTail,
    startup_ready: AtomicBool,
    startup_error: Mutex<Option<String>>,
}

impl AcpShared {
    #[cfg(test)]
    pub(super) fn new(
        sid: String,
        cwd: PathBuf,
        anchor: ProcessId,
        stream_tx: broadcast::Sender<ServiceMessage>,
        processes: impl Into<AcpProcesses>,
    ) -> Self {
        let (projection, _) = AcpProjectionSenders::open_with_waker(Waker::noop().clone());
        Self::with_projection(sid, cwd, anchor, stream_tx, processes, projection)
    }

    pub(super) fn with_projection(
        sid: String,
        cwd: PathBuf,
        anchor: ProcessId,
        stream_tx: broadcast::Sender<ServiceMessage>,
        processes: impl Into<AcpProcesses>,
        projection: AcpProjectionSenders,
    ) -> Self {
        Self {
            sid,
            cwd: Mutex::new(cwd),
            anchor,
            stream_tx,
            projection,
            projector_updates: watch::channel(0).0,
            pending_perms: Mutex::new(HashMap::new()),
            terminals: AcpTerminals::default(),
            processes: processes.into(),
            cancel_requested: AtomicBool::new(false),
            stderr_tail: AcpStderrTail::default(),
            startup_ready: AtomicBool::new(false),
            startup_error: Mutex::new(None),
        }
    }

    fn mark_startup_ready(&self) {
        tracing::info!(target: "acp", sid = %self.sid, "session established");
        self.startup_ready.store(true, Ordering::SeqCst);
        self.projection.startup_ready();
    }

    fn startup_ready(&self) -> bool {
        self.startup_ready.load(Ordering::SeqCst)
    }

    fn fail_startup(&self, error: String) {
        let mut failure = self.startup_error.lock().unwrap();
        *failure = Some(error.clone());
        self.projection.startup_failed();
        self.projection.status(AgentRunStatus::Errored(error));
    }

    fn report_startup_exit(&self) {
        if self.startup_ready() {
            return;
        }
        let mut failure = self.startup_error.lock().unwrap();
        if failure.is_some() {
            return;
        }
        let error = format!("agent exited during startup{}", self.stderr_detail());
        *failure = Some(error.clone());
        self.projection.startup_failed();
        self.projection.status(AgentRunStatus::Errored(error));
    }

    pub(super) fn snapshot_message(&self, messages: &[Message]) -> ServiceMessage {
        ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot {
            sid: self.sid.clone(),
            messages: messages.to_vec(),
        })
    }

    pub(super) fn cwd(&self) -> PathBuf {
        self.cwd.lock().unwrap().clone()
    }

    pub(super) fn set_cwd(&self, cwd: PathBuf) {
        *self.cwd.lock().unwrap() = cwd;
    }

    pub(super) fn publish_workspace_change(&self, workspace: &WorkspaceLocation) {
        *self.cwd.lock().unwrap() = workspace.working_directory.clone();
        self.emit(ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged {
            sid: self.sid.clone(),
            name: workspace.name.clone(),
            branch: workspace.revision.clone(),
            cwd: workspace.working_directory.to_string_lossy().into_owned(),
            workspace_cwd: workspace.project_directory.to_string_lossy().into_owned(),
        }));
    }

    pub(super) fn emit(&self, msg: ServiceMessage) {
        let _ = self.stream_tx.send(msg);
    }

    fn publish_config_selection_result(
        &self,
        request_id: u64,
        config_id: Option<&str>,
        value: &str,
        succeeded: bool,
    ) {
        self.emit(ServiceMessage::AcpSessionConfigSelectionResult {
            sid: self.sid.clone(),
            request_id,
            config_id: config_id.map(str::to_string),
            value: value.to_string(),
            succeeded,
        });
    }

    fn stderr_detail(&self) -> String {
        self.stderr_tail.detail(AcpStderrTail::SHOWN)
    }
}

async fn resolve_approval_details(
    request: &RequestPermissionRequest,
    shared: &AcpShared,
) -> Option<(String, String)> {
    let deadline = tokio::time::Instant::now() + AcpDriver::APPROVAL_DETAILS_WAIT;
    let mut updates = shared.projector_updates.subscribe();
    let query = ApprovalDetailsQuery::from_request(request);
    let fallback = query.fallback();
    loop {
        let (response, receiver) = oneshot::channel();
        shared.projection.approval_details(query.clone(), response);
        match tokio::time::timeout_at(deadline, receiver).await {
            Ok(Ok(Some(details))) => return Some(details),
            Ok(Ok(None)) => {}
            _ => return Some(fallback),
        }
        if tokio::time::timeout_at(deadline, updates.changed())
            .await
            .is_err()
        {
            return Some(fallback);
        }
    }
}

async fn encoded_media(path: String, limit: u64) -> Option<(String, u64)> {
    tokio::task::spawn_blocking(move || {
        let metadata = std::fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > limit {
            return None;
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .ok()?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .ok()?;
        let size = u64::try_from(bytes.len()).ok()?;
        (size <= limit).then(|| {
            (
                base64::engine::general_purpose::STANDARD.encode(bytes),
                size,
            )
        })
    })
    .await
    .ok()
    .flatten()
}

async fn prompt_content_blocks(
    text: &str,
    context: Option<&str>,
    attachments: &[AgentAttachment],
    capabilities: &PromptCapabilities,
) -> Vec<ContentBlock> {
    let mut blocks = Vec::with_capacity(attachments.len() + 1);
    let mut remaining_media_bytes = AcpDriver::PROMPT_MEDIA_TOTAL_LIMIT;
    let text = AgentPromptEnvelope::compose(text, context);
    if !text.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(text)));
    }
    for attachment in attachments {
        let uri = AcpAttachment(attachment).uri();
        let media_supported = (capabilities.image && attachment.mime_type.starts_with("image/"))
            || (capabilities.audio && attachment.mime_type.starts_with("audio/"));
        let limit = AcpDriver::PROMPT_MEDIA_FILE_LIMIT.min(remaining_media_bytes);
        let encoded = if media_supported && attachment.size <= limit && limit > 0 {
            encoded_media(attachment.path.clone(), limit).await
        } else {
            None
        };
        if let Some((data, size)) = encoded {
            remaining_media_bytes = remaining_media_bytes.saturating_sub(size);
            if capabilities.image && attachment.mime_type.starts_with("image/") {
                blocks.push(ContentBlock::Image(
                    ImageContent::new(data, attachment.mime_type.clone()).uri(uri),
                ));
                continue;
            }
            if capabilities.audio && attachment.mime_type.starts_with("audio/") {
                blocks.push(ContentBlock::Audio(AudioContent::new(
                    data,
                    attachment.mime_type.clone(),
                )));
                continue;
            }
        }
        blocks.push(ContentBlock::ResourceLink(
            ResourceLink::new(attachment.name.clone(), uri)
                .mime_type(Some(attachment.mime_type.clone()))
                .size(i64::try_from(attachment.size).ok()),
        ));
    }
    blocks
}

pub(super) struct AcpDriver;

impl AcpDriver {
    const PROMPT_MEDIA_FILE_LIMIT: u64 = 8 * 1024 * 1024;
    const PROMPT_MEDIA_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
    const APPROVAL_DETAILS_WAIT: std::time::Duration = std::time::Duration::from_millis(250);
    const STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
    const TERMINAL_COLS: u16 = 80;
    const TERMINAL_ROWS: u16 = 24;

    fn is_permissionless_tool(name: &str) -> bool {
        if AcpToolCallId::is_conversation_title(name) {
            return true;
        }
        let normalized = name.trim().to_ascii_lowercase();
        let parts = normalized
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '-' | '.' | ':' | '_')
            })
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        matches!(
            parts.as_slice(),
            ["mcp", "vmux", "request", "user", "choice"]
                | ["vmux", "request", "user", "choice"]
                | ["request", "user", "choice"]
                | ["mcp", "vmux", "search" | "read", "knowledge"]
                | ["vmux", "search" | "read", "knowledge"]
                | ["search" | "read", "knowledge"]
        )
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub(super) async fn run(
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        mcp_servers: AcpMcpServers,
        resume: Option<String>,
        shared: Arc<AcpShared>,
        inputs: (
            mpsc::UnboundedReceiver<AcpPromptCommand>,
            mpsc::UnboundedReceiver<AcpApprovalCommand>,
            mpsc::UnboundedReceiver<AcpConfigCommand>,
            mpsc::UnboundedReceiver<()>,
            mpsc::UnboundedReceiver<()>,
        ),
    ) {
        let agent_cwd = shared.cwd();
        let mut launch = Command::new(&command);
        let mut child = match AgentProcess::spawn(
            launch
                .args(&args)
                .envs(env)
                .current_dir(agent_cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        ) {
            Ok(child) => child,
            Err(err) => {
                shared.fail_startup(format!("acp spawn failed: {err}"));
                return;
            }
        };
        let stdin = child
            .child
            .stdin
            .take()
            .expect("piped stdin")
            .compat_write();
        let stdout = child.child.stdout.take().expect("piped stdout").compat();
        if let Some(stderr) = child.child.stderr.take() {
            tokio::spawn(drain_stderr(stderr, shared.clone()));
        }
        let transport = agent_client_protocol::ByteStreams::new(stdin, stdout);
        let result = Self::connect(transport, mcp_servers, resume, shared.clone(), inputs).await;
        if let Err(err) = result {
            if !shared.startup_ready() {
                shared.fail_startup(format!(
                    "acp connection ended: {err}{}",
                    shared.stderr_detail()
                ));
            } else {
                shared.projection.status(AgentRunStatus::Errored(format!(
                    "acp connection ended: {err}{}",
                    shared.stderr_detail()
                )));
            }
        }
        child.stop().await;
    }

    #[allow(clippy::type_complexity)]
    async fn connect(
        transport: agent_client_protocol::ByteStreams<
            tokio_util::compat::Compat<tokio::process::ChildStdin>,
            tokio_util::compat::Compat<tokio::process::ChildStdout>,
        >,
        mcp_servers: AcpMcpServers,
        resume: Option<String>,
        shared: Arc<AcpShared>,
        inputs: (
            mpsc::UnboundedReceiver<AcpPromptCommand>,
            mpsc::UnboundedReceiver<AcpApprovalCommand>,
            mpsc::UnboundedReceiver<AcpConfigCommand>,
            mpsc::UnboundedReceiver<()>,
            mpsc::UnboundedReceiver<()>,
        ),
    ) -> Result<(), agent_client_protocol::Error> {
        let perm_shared = shared.clone();
        let update_shared = shared.clone();
        let main_shared = shared.clone();
        let read_shared = shared.clone();
        let write_shared = shared.clone();
        let create_shared = shared.clone();
        let output_shared = shared.clone();
        let wait_shared = shared.clone();
        let kill_shared = shared.clone();
        let release_shared = shared;
        Client
            .builder()
            .on_receive_request(
                async move |req: RequestPermissionRequest,
                            responder: Responder<RequestPermissionResponse>,
                            _cx| {
                    Self::handle_permission(req, responder, &perm_shared).await
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: ReadTextFileRequest,
                            responder: Responder<ReadTextFileResponse>,
                            _cx| {
                    Self::handle_read_file(req, responder, &read_shared)
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: WriteTextFileRequest,
                            responder: Responder<WriteTextFileResponse>,
                            _cx| {
                    Self::handle_write_file(req, responder, &write_shared)
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: CreateTerminalRequest,
                            responder: Responder<CreateTerminalResponse>,
                            _cx| match create_terminal(&create_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: TerminalOutputRequest,
                            responder: Responder<TerminalOutputResponse>,
                            _cx| match terminal_output(&output_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: WaitForTerminalExitRequest,
                            responder: Responder<WaitForTerminalExitResponse>,
                            _cx| match wait_for_terminal_exit(&wait_shared, req)
                    .await
                {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: KillTerminalRequest,
                            responder: Responder<KillTerminalResponse>,
                            _cx| match kill_terminal(&kill_shared, req).await {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |req: ReleaseTerminalRequest,
                            responder: Responder<ReleaseTerminalResponse>,
                            _cx| match release_terminal(&release_shared, req).await
                {
                    Ok(resp) => responder.respond(resp),
                    Err(err) => responder.respond_with_internal_error(err),
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |note: SessionNotification, _cx| {
                    update_shared.projection.session_update(note.update);
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(transport, async move |cx| {
                Self::drive_session(cx, mcp_servers, resume, main_shared, inputs).await
            })
            .await
    }

    fn handle_read_file(
        req: ReadTextFileRequest,
        responder: Responder<ReadTextFileResponse>,
        shared: &Arc<AcpShared>,
    ) -> Result<(), agent_client_protocol::Error> {
        let scope = AcpFsScope { cwd: shared.cwd() };
        match scope.read(&req) {
            Ok(content) => responder.respond(ReadTextFileResponse::new(content)),
            Err(err) => responder.respond_with_internal_error(err),
        }
    }

    fn handle_write_file(
        req: WriteTextFileRequest,
        responder: Responder<WriteTextFileResponse>,
        shared: &Arc<AcpShared>,
    ) -> Result<(), agent_client_protocol::Error> {
        let scope = AcpFsScope { cwd: shared.cwd() };
        match scope.write(&req) {
            Ok(()) => responder.respond(WriteTextFileResponse::new()),
            Err(err) => responder.respond_with_internal_error(err),
        }
    }

    async fn handle_permission(
        req: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
        perm_shared: &Arc<AcpShared>,
    ) -> Result<(), agent_client_protocol::Error> {
        let call_id = req.tool_call.tool_call_id.to_string();
        let Some((name, args_json)) = resolve_approval_details(&req, perm_shared).await else {
            tracing::warn!(
                target: "acp",
                sid = %perm_shared.sid,
                call_id = %call_id,
                "ACP permission request omitted tool identity; cancelling"
            );
            return responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        };
        if Self::is_permissionless_tool(&name) {
            let outcome = match AcpPermissionOptions(&req.options).select(ApprovalDecision::Allow) {
                Some(id) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                None => RequestPermissionOutcome::Cancelled,
            };
            return responder.respond(RequestPermissionResponse::new(outcome));
        }
        let (tx, rx) = oneshot::channel();
        perm_shared
            .pending_perms
            .lock()
            .unwrap()
            .insert(call_id.clone(), tx);
        perm_shared.projection.approval_requested(RemoteApproval {
            call_id: call_id.clone(),
            name,
            args: vmux_api::json::JsonValue::parse_or_string(&args_json),
        });
        let decision = rx.await.unwrap_or(ApprovalDecision::Deny);
        perm_shared.projection.approval_resolved(call_id);
        let outcome = match AcpPermissionOptions(&req.options).select(decision) {
            Some(id) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
            None => RequestPermissionOutcome::Cancelled,
        };
        responder.respond(RequestPermissionResponse::new(outcome))
    }

    #[allow(clippy::type_complexity)]
    async fn drive_session(
        cx: ConnectionTo<Agent>,
        mcp_servers: AcpMcpServers,
        resume: Option<String>,
        shared: Arc<AcpShared>,
        inputs: (
            mpsc::UnboundedReceiver<AcpPromptCommand>,
            mpsc::UnboundedReceiver<AcpApprovalCommand>,
            mpsc::UnboundedReceiver<AcpConfigCommand>,
            mpsc::UnboundedReceiver<()>,
            mpsc::UnboundedReceiver<()>,
        ),
    ) -> Result<(), agent_client_protocol::Error> {
        let (mut prompts, mut approvals, mut configs, mut cancels, mut closes) = inputs;
        let Some((mut session_id, prompt_capabilities, mcp_servers)) =
            Self::initialize_session(&cx, &mcp_servers, resume, &shared).await
        else {
            return Ok(());
        };
        shared.mark_startup_ready();
        loop {
            tokio::select! {
                biased;
                close = closes.recv() => {
                    if let Some(sid) = &session_id {
                        let _ = cx.send_notification(CancelNotification::new(sid.clone()));
                    }
                    let _ = close;
                    break;
                }
                cancel = cancels.recv() => {
                    if cancel.is_none() {
                        break;
                    }
                    shared.cancel_requested.store(true, Ordering::SeqCst);
                    for (_id, tx) in shared.pending_perms.lock().unwrap().drain() {
                        let _ = tx.send(ApprovalDecision::Deny);
                    }
                    if let Some(sid) = &session_id {
                        let _ = cx.send_notification(CancelNotification::new(sid.clone()));
                    }
                }
                approval = approvals.recv() => {
                    let Some(approval) = approval else {
                        break;
                    };
                    if let Some(tx) = shared.pending_perms.lock().unwrap().remove(&approval.call_id) {
                        let _ = tx.send(approval.decision);
                    }
                }
                config = configs.recv() => {
                    let Some(config) = config else {
                        break;
                    };
                    Self::handle_set_config(
                        &cx,
                        session_id.as_ref(),
                        &shared,
                        config.request_id,
                        config.config_id,
                        config.value,
                    )
                    .await;
                }
                prompt = prompts.recv() => {
                    let Some(prompt) = prompt else {
                        break;
                    };
                    Self::handle_user_input(
                        &cx,
                        &mut session_id,
                        &mcp_servers,
                        &prompt_capabilities,
                        &shared,
                        prompt.text,
                        prompt.context,
                        prompt.attachments,
                        prompt.preferred_mode,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn initialize_session(
        cx: &ConnectionTo<Agent>,
        mcp_servers: &AcpMcpServers,
        resume: Option<String>,
        shared: &Arc<AcpShared>,
    ) -> Option<(Option<SessionId>, PromptCapabilities, Vec<McpServer>)> {
        let mut init = InitializeRequest::new(ProtocolVersion::V1);
        init.client_capabilities.fs.read_text_file = true;
        init.client_capabilities.fs.write_text_file = true;
        init.client_capabilities.terminal = true;
        let init_resp = match tokio::time::timeout(
            AcpDriver::STARTUP_TIMEOUT,
            cx.send_request(init).block_task(),
        )
        .await
        {
            Ok(Ok(resp)) => resp,
            Ok(Err(err)) => {
                shared.fail_startup(format!(
                    "agent failed to start: {err}{}",
                    shared.stderr_detail()
                ));
                return None;
            }
            Err(_) => {
                shared.fail_startup(format!(
                    "agent did not start within {}s{}",
                    AcpDriver::STARTUP_TIMEOUT.as_secs(),
                    shared.stderr_detail()
                ));
                return None;
            }
        };
        tracing::info!(target: "acp", sid = %shared.sid, "initialize answered");
        let prompt_capabilities = init_resp.agent_capabilities.prompt_capabilities.clone();
        let mcp_servers =
            mcp_servers.for_capabilities(&init_resp.agent_capabilities.mcp_capabilities);
        if let Some(name) = AcpAgentInfo(init_resp.agent_info.as_ref()).display_name() {
            shared.projection.agent_info(name);
        }
        let session_id = Self::load_or_create_session(
            cx,
            resume,
            init_resp.agent_capabilities.load_session,
            &mcp_servers,
            shared,
        )
        .await?;
        Some((session_id, prompt_capabilities, mcp_servers))
    }

    async fn load_or_create_session(
        cx: &ConnectionTo<Agent>,
        resume: Option<String>,
        load_supported: bool,
        mcp_servers: &[McpServer],
        shared: &Arc<AcpShared>,
    ) -> Option<Option<SessionId>> {
        let resume_requested = resume.is_some();
        if resume_requested {
            shared.projection.begin_history_replay();
        }
        let loaded_session = load_requested_session(resume, load_supported, |sid| {
            let mut load = LoadSessionRequest::new(sid, shared.cwd());
            load.mcp_servers = mcp_servers.to_vec();
            let shared = shared.clone();
            let cx = cx.clone();
            async move {
                let answered = tokio::time::timeout(
                    AcpDriver::STARTUP_TIMEOUT,
                    cx.send_request(load).block_task(),
                )
                .await;
                let Ok(loaded) = answered else {
                    tracing::warn!(
                        target: "acp",
                        "session/load did not answer within {}s",
                        AcpDriver::STARTUP_TIMEOUT.as_secs()
                    );
                    return Err("session/load timed out".to_string());
                };
                let response = loaded.map_err(|err| err.to_string())?;
                let config_options = response.config_options.as_deref().unwrap_or_default();
                shared.projection.config_state(AcpConfigStateInput {
                    config_options: config_options.to_vec(),
                    modes: response.modes.clone(),
                });
                Ok(())
            }
        })
        .await;
        let mut session_id = match loaded_session {
            Ok(session_id) => session_id,
            Err(error) => {
                tracing::warn!(target: "acp", sid = %shared.sid, %error, "session resume failed");
                if resume_requested {
                    shared.projection.finish_history_replay(false);
                }
                shared.fail_startup(error);
                return None;
            }
        };
        if resume_requested {
            shared
                .projection
                .finish_history_replay(session_id.is_some());
        }
        if let Some(sid) = &session_id {
            shared.emit(ServiceMessage::AcpSessionCreated {
                sid: shared.sid.clone(),
                acp_session_id: sid.to_string(),
            });
            return Some(session_id);
        }
        Self::create_initial_session(cx, &mut session_id, mcp_servers, shared).await
    }

    async fn create_initial_session(
        cx: &ConnectionTo<Agent>,
        session_id: &mut Option<SessionId>,
        mcp_servers: &[McpServer],
        shared: &Arc<AcpShared>,
    ) -> Option<Option<SessionId>> {
        let ensured = ensure_session(session_id, || {
            let mut new_session = NewSessionRequest::new(shared.cwd());
            new_session.mcp_servers = mcp_servers.to_vec();
            let shared = shared.clone();
            let cx = cx.clone();
            async move {
                match tokio::time::timeout(
                    AcpDriver::STARTUP_TIMEOUT,
                    cx.send_request(new_session).block_task(),
                )
                .await
                {
                    Ok(Ok(response)) => {
                        let config_options = response.config_options.as_deref().unwrap_or_default();
                        shared.projection.config_state(AcpConfigStateInput {
                            config_options: config_options.to_vec(),
                            modes: response.modes.clone(),
                        });
                        Ok(response.session_id)
                    }
                    Ok(Err(err)) => Err(err.to_string()),
                    Err(_) => Err(format!(
                        "no answer within {}s{}",
                        AcpDriver::STARTUP_TIMEOUT.as_secs(),
                        shared.stderr_detail()
                    )),
                }
            }
        })
        .await;
        match ensured {
            Ok((sid, created)) => {
                if created {
                    shared.emit(ServiceMessage::AcpSessionCreated {
                        sid: shared.sid.clone(),
                        acp_session_id: sid.to_string(),
                    });
                }
                Some(session_id.clone())
            }
            Err(err) => {
                shared.projection.status(AgentRunStatus::Errored(format!(
                    "acp session/new failed: {err}"
                )));
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_user_input(
        cx: &ConnectionTo<Agent>,
        session_id: &mut Option<SessionId>,
        mcp_servers: &[McpServer],
        prompt_capabilities: &PromptCapabilities,
        shared: &Arc<AcpShared>,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> Result<(), agent_client_protocol::Error> {
        shared.cancel_requested.store(false, Ordering::SeqCst);
        shared.projection.status(AgentRunStatus::Streaming);
        let ensured = ensure_session(session_id, || {
            let mut new_session = NewSessionRequest::new(shared.cwd());
            new_session.mcp_servers = mcp_servers.to_vec();
            let shared = shared.clone();
            let cx = cx.clone();
            async move {
                cx.send_request(new_session)
                    .block_task()
                    .await
                    .map(|response| {
                        let config_options = response.config_options.as_deref().unwrap_or_default();
                        shared.projection.config_state(AcpConfigStateInput {
                            config_options: config_options.to_vec(),
                            modes: response.modes.clone(),
                        });
                        response.session_id
                    })
            }
        })
        .await;
        let (active_session_id, created) = match ensured {
            Ok(value) => value,
            Err(err) => {
                shared.projection.status(AgentRunStatus::Errored(format!(
                    "acp session/new failed: {err}"
                )));
                return Ok(());
            }
        };
        if created {
            shared.emit(ServiceMessage::AcpSessionCreated {
                sid: shared.sid.clone(),
                acp_session_id: active_session_id.to_string(),
            });
        }
        Self::apply_preferred_mode(cx, &active_session_id, preferred_mode, shared).await;
        let cx_prompt = cx.clone();
        let shared = shared.clone();
        let prompt_capabilities = prompt_capabilities.clone();
        cx.spawn(async move {
            let prompt = PromptRequest::new(
                active_session_id,
                prompt_content_blocks(
                    &text,
                    context.as_deref(),
                    &attachments,
                    &prompt_capabilities,
                )
                .await,
            );
            let errored = match cx_prompt.send_request(prompt).block_task().await {
                Ok(_) => None,
                Err(err) => Some(err.to_string()),
            };
            let cancelled = shared.cancel_requested.swap(false, Ordering::SeqCst);
            shared.projection.request_snapshot();
            shared.projection.status(
                PromptCompletion {
                    cancelled,
                    error: errored,
                }
                .status(),
            );
            Ok(())
        })
    }

    async fn apply_preferred_mode(
        cx: &ConnectionTo<Agent>,
        active_session_id: &SessionId,
        preferred_mode: Option<String>,
        shared: &Arc<AcpShared>,
    ) {
        let Some(mode_id) = preferred_mode else {
            return;
        };
        let available_mode = shared
            .projection
            .selection_snapshot()
            .await
            .configs
            .into_iter()
            .find(|config| config.category.as_deref() == Some("mode"));
        let Some(mode) = available_mode else {
            return;
        };
        if mode.current_value == mode_id
            || !mode.values.iter().any(|option| option.value == mode_id)
        {
            return;
        }
        if let Some(config_id) = mode.config_id.as_ref() {
            match cx
                .send_request(SetSessionConfigOptionRequest::new(
                    active_session_id.clone(),
                    config_id.clone(),
                    mode_id.clone(),
                ))
                .block_task()
                .await
            {
                Ok(response) => shared.projection.selected_config(AcpSelectedConfigInput {
                    config_id: Some(config_id.clone()),
                    value: mode_id,
                    config_options: response.config_options,
                }),
                Err(error) => {
                    tracing::warn!(target: "acp", sid = %shared.sid, "initial mode selection failed: {error}");
                }
            }
        } else {
            match cx
                .send_request(SetSessionModeRequest::new(
                    active_session_id.clone(),
                    mode_id.clone(),
                ))
                .block_task()
                .await
            {
                Ok(_) => shared.projection.selected_config(AcpSelectedConfigInput {
                    config_id: None,
                    value: mode_id,
                    config_options: Vec::new(),
                }),
                Err(error) => {
                    tracing::warn!(target: "acp", sid = %shared.sid, "initial mode selection failed: {error}");
                }
            }
        }
    }

    async fn handle_set_config(
        cx: &ConnectionTo<Agent>,
        session_id: Option<&SessionId>,
        shared: &Arc<AcpShared>,
        request_id: u64,
        config_id: Option<String>,
        value: String,
    ) {
        let Some(sid) = session_id.cloned() else {
            shared.publish_config_selection_result(request_id, config_id.as_deref(), &value, false);
            return;
        };
        let result = if let Some(config_id) = config_id.as_ref() {
            cx.send_request(SetSessionConfigOptionRequest::new(
                sid,
                config_id.clone(),
                value.clone(),
            ))
            .block_task()
            .await
            .map(|response| response.config_options)
        } else {
            cx.send_request(SetSessionModeRequest::new(sid, value.clone()))
                .block_task()
                .await
                .map(|_| Vec::new())
        };
        match result {
            Ok(config_options) => {
                shared.projection.selected_config(AcpSelectedConfigInput {
                    config_id: config_id.clone(),
                    value: value.clone(),
                    config_options,
                });
                shared.publish_config_selection_result(
                    request_id,
                    config_id.as_deref(),
                    &value,
                    true,
                );
            }
            Err(err) => {
                shared.publish_config_selection_result(
                    request_id,
                    config_id.as_deref(),
                    &value,
                    false,
                );
                tracing::warn!(target: "acp", sid = %shared.sid, "session config selection failed: {err}");
            }
        }
    }
}

async fn load_requested_session<F, Fut, E>(
    resume: Option<String>,
    load_supported: bool,
    load: F,
) -> Result<Option<SessionId>, String>
where
    F: FnOnce(SessionId) -> Fut,
    Fut: Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    let Some(resume) = resume else {
        return Ok(None);
    };
    if !load_supported {
        return Err("agent does not support resuming saved sessions".to_string());
    }
    let sid = SessionId::new(resume);
    load(sid.clone())
        .await
        .map_err(|error| format!("session/load failed for {sid}: {error}"))?;
    Ok(Some(sid))
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

async fn drain_stderr(stderr: tokio::process::ChildStderr, shared: Arc<AcpShared>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::warn!(target: "acp", "{line}");
        shared.stderr_tail.push(line);
    }
    shared.report_startup_exit();
}

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
    let session_cwd = shared.cwd();
    let cwd = cwd.unwrap_or_else(|| session_cwd.clone());
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
    let cwd = AcpFsScope { cwd: session_cwd }
        .resolve(&cwd)
        .ok_or_else(|| {
            format!(
                "acp: terminal cwd is outside session cwd: {}; select the project and wait for user approval first",
                cwd.display()
            )
        })?;
    let cwd = cwd.to_string_lossy().into_owned();
    let created = shared
        .processes
        .create(ProcessLaunch {
            id: ProcessId::new(),
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env,
            cols: AcpDriver::TERMINAL_COLS,
            rows: AcpDriver::TERMINAL_ROWS,
            keep_after_exit: true,
        })
        .await?;
    let id = created.id;
    let mut exit_stream = created.updates;

    let (exit_tx, exit_rx) = watch::channel(AcpTerminalExit::Pending);
    let processes = shared.processes.clone();
    tokio::spawn(async move {
        loop {
            match exit_stream.recv().await {
                Ok(ProcessUpdate::Exited { exit_code }) => {
                    let _ = exit_tx.send(AcpTerminalExit::Exited(exit_code));
                    break;
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let recorded = processes.exit_code(id).await.ok();
                    if let Some(exit) = AcpTerminalExit::recovered(recorded) {
                        let _ = exit_tx.send(exit);
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    let _ = exit_tx.send(AcpTerminalExit::Removed);
                    break;
                }
            }
        }
    });

    let terminal_id = id.to_string();
    shared.terminals.insert(
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

async fn terminal_output(
    shared: &AcpShared,
    req: TerminalOutputRequest,
) -> Result<TerminalOutputResponse, String> {
    let terminal = shared.terminals.snapshot(&req.terminal_id)?;
    let output = shared
        .processes
        .transcript(terminal.process_id)
        .await
        .map_err(|_| {
            format!(
                "acp: terminal {} process no longer exists",
                req.terminal_id.0
            )
        })?;
    let (output, truncated) = terminal.truncate(output);
    let mut resp = TerminalOutputResponse::new(output, truncated);
    if let AcpTerminalExit::Exited(code) = terminal.exit {
        resp = resp.exit_status(AcpTerminalExit::status(code));
    }
    Ok(resp)
}

async fn wait_for_terminal_exit(
    shared: &AcpShared,
    req: WaitForTerminalExitRequest,
) -> Result<WaitForTerminalExitResponse, String> {
    let key = req.terminal_id.0.to_string();
    let mut exit_rx = shared.terminals.exit_receiver(&req.terminal_id)?;
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
    Ok(WaitForTerminalExitResponse::new(AcpTerminalExit::status(
        code,
    )))
}

async fn kill_terminal(
    shared: &AcpShared,
    req: KillTerminalRequest,
) -> Result<KillTerminalResponse, String> {
    let terminal = shared.terminals.snapshot(&req.terminal_id)?;
    shared.processes.kill(terminal.process_id).await?;
    Ok(KillTerminalResponse::new())
}

async fn release_terminal(
    shared: &AcpShared,
    req: ReleaseTerminalRequest,
) -> Result<ReleaseTerminalResponse, String> {
    let terminal = shared.terminals.remove(&req.terminal_id)?;
    shared.processes.remove(terminal.process_id).await?;
    Ok(ReleaseTerminalResponse::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ContentChunk, Implementation, PermissionOptionKind, SessionConfigSelectGroup,
        SessionConfigSelectOption, SessionMode, ToolCall, ToolCallUpdateFields, ToolKind,
    };
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::{App, Entity};

    struct ProjectionHarness {
        app: App,
        entity: Entity,
        shared: Arc<AcpShared>,
        stream: broadcast::Receiver<ServiceMessage>,
    }

    #[test]
    fn startup_exit_preserves_the_specific_resume_failure() {
        let mut harness = ProjectionHarness::new(4);
        harness.shared.fail_startup("thread_active_writer".into());
        harness.shared.report_startup_exit();
        harness.app.update();
        assert!(
            matches!(harness.stream.try_recv(), Ok(ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged { status: AgentRunStatus::Errored(error), .. })) if error == "thread_active_writer")
        );
        assert!(harness.stream.try_recv().is_err());
    }

    impl ProjectionHarness {
        fn new(capacity: usize) -> Self {
            let (stream_tx, stream) = broadcast::channel(capacity);
            let (wake, _) = mpsc::unbounded_channel();
            let (processes, _process_runtime) = ProcessRuntime::new(wake.clone());
            let (projection, projection_inboxes) =
                AcpProjectionSenders::open_with_waker(super::super::AcpWake::waker(wake));
            let shared = Arc::new(AcpShared::with_projection(
                "s1".into(),
                PathBuf::from("/tmp"),
                ProcessId::new(),
                stream_tx,
                processes,
                projection,
            ));
            let mut app = App::new();
            app.add_plugins(super::super::AcpRuntimePlugin);
            let entity = app
                .world_mut()
                .spawn((
                    vmux_session::SessionId("s1".into()),
                    super::super::AcpSessionShared(Arc::clone(&shared)),
                    super::super::AcpSessionCwd(PathBuf::from("/tmp")),
                    super::super::projection::AcpProjectionOrder::default(),
                    projection_inboxes,
                    super::super::AcpAgentName::default(),
                    super::super::AcpSessionConfigs::default(),
                    super::super::AcpRunState(AgentRunStatus::Idle),
                    super::super::AcpApprovalState::default(),
                    super::super::AcpHistoryReplay::default(),
                ))
                .id();
            Self {
                app,
                entity,
                shared,
                stream,
            }
        }

        fn update(&mut self) {
            self.app.update();
        }

        fn messages(&mut self) -> Vec<Message> {
            let entity = self.entity;
            self.app
                .world_mut()
                .run_system_once(move |transcripts: AcpTranscripts| transcripts.messages(entity))
                .unwrap()
        }

        fn approval_details(&mut self, query: ApprovalDetailsQuery) -> Option<(String, String)> {
            let (response, mut receiver) = oneshot::channel();
            self.shared.projection.approval_details(query, response);
            self.update();
            receiver.try_recv().unwrap()
        }
    }

    #[test]
    fn stderr_tail_shows_last_lines_and_skips_blanks() {
        let tail: VecDeque<String> = [
            "npm warn old",
            "",
            "npm error 403 Forbidden",
            "   ",
            "Blocked by Security Policy",
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        let tail = AcpStderrTail(Mutex::new(tail));
        assert_eq!(
            tail.detail(2),
            "\n\nnpm error 403 Forbidden\nBlocked by Security Policy"
        );
    }

    #[test]
    fn stderr_tail_is_empty_without_output() {
        let blanks: VecDeque<String> = ["", "   "].iter().map(|line| line.to_string()).collect();
        assert!(AcpStderrTail(Mutex::new(blanks)).detail(8).is_empty());
        assert!(AcpStderrTail::default().detail(8).is_empty());
    }

    #[tokio::test]
    async fn prompt_content_embeds_supported_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        std::fs::write(&path, b"png").unwrap();
        let attachment = AgentAttachment {
            path: path.to_string_lossy().into_owned(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size: 3,
        };
        let mut capabilities = PromptCapabilities::default();
        capabilities.image = true;

        let blocks = prompt_content_blocks("inspect", None, &[attachment], &capabilities).await;

        assert!(matches!(&blocks[0], ContentBlock::Text(text) if text.text == "inspect"));
        assert!(matches!(
            &blocks[1],
            ContentBlock::Image(image)
                if image.data == base64::engine::general_purpose::STANDARD.encode(b"png")
                    && image.mime_type == "image/png"
        ));
    }

    #[tokio::test]
    async fn prompt_content_links_files_without_media_capability() {
        let attachment = AgentAttachment {
            path: "/tmp/report.txt".into(),
            name: "report.txt".into(),
            mime_type: "text/plain".into(),
            size: 12,
        };

        let blocks =
            prompt_content_blocks("", None, &[attachment], &PromptCapabilities::default()).await;

        assert!(matches!(
            &blocks[0],
            ContentBlock::ResourceLink(link)
                if link.name == "report.txt" && link.uri == "file:///tmp/report.txt"
        ));
    }

    #[tokio::test]
    async fn prompt_content_links_supported_media_above_embed_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.png");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(AcpDriver::PROMPT_MEDIA_FILE_LIMIT + 1)
            .unwrap();
        let attachment = AgentAttachment {
            path: path.to_string_lossy().into_owned(),
            name: "large.png".into(),
            mime_type: "image/png".into(),
            size: AcpDriver::PROMPT_MEDIA_FILE_LIMIT + 1,
        };
        let mut capabilities = PromptCapabilities::default();
        capabilities.image = true;

        let blocks = prompt_content_blocks("", None, &[attachment], &capabilities).await;

        assert!(matches!(
            &blocks[0],
            ContentBlock::ResourceLink(link)
                if link.name == "large.png"
                    && link.size == i64::try_from(AcpDriver::PROMPT_MEDIA_FILE_LIMIT + 1).ok()
        ));
    }

    #[test]
    fn acp_display_name_prefers_title_then_name() {
        let titled = Implementation::new("antigravity", "1.0").title("Antigravity");
        assert_eq!(
            AcpAgentInfo(Some(&titled)).display_name().as_deref(),
            Some("Antigravity")
        );

        let named = Implementation::new("claude-code-acp", "1.0");
        assert_eq!(
            AcpAgentInfo(Some(&named)).display_name().as_deref(),
            Some("claude-code-acp")
        );
    }

    #[test]
    fn acp_display_name_ignores_blank_metadata() {
        let blank_title = Implementation::new("codex-acp", "1.0").title("   ");
        assert_eq!(
            AcpAgentInfo(Some(&blank_title)).display_name().as_deref(),
            Some("codex-acp")
        );

        let blank = Implementation::new("   ", "1.0");
        assert_eq!(AcpAgentInfo(Some(&blank)).display_name(), None);
        assert_eq!(AcpAgentInfo(None).display_name(), None);
    }

    #[test]
    fn acp_agent_info_is_replayable_without_a_subscriber() {
        let mut harness = ProjectionHarness::new(1);
        harness.shared.projection.agent_info("Antigravity".into());
        harness.update();

        let name = harness
            .app
            .world()
            .entity(harness.entity)
            .get::<super::super::AcpAgentName>()
            .unwrap();
        assert_eq!(name.0.as_deref(), Some("Antigravity"));
    }

    #[test]
    fn session_configs_preserve_categorized_grouped_selector() {
        let config = SessionConfigOption::select(
            "llm",
            "Language model",
            "opus",
            vec![SessionConfigSelectGroup::new(
                "anthropic",
                "Anthropic",
                vec![
                    SessionConfigSelectOption::new("sonnet", "Claude Sonnet"),
                    SessionConfigSelectOption::new("opus", "Claude Opus")
                        .description("Most capable"),
                ],
            )],
        )
        .category(SessionConfigOptionCategory::Model);

        let configs = super::super::AcpSessionConfigs::from_acp(&[config], None);
        let info = &configs.0[0];

        assert_eq!(info.config_id.as_deref(), Some("llm"));
        assert_eq!(info.category.as_deref(), Some("model"));
        assert_eq!(info.current_value, "opus");
        assert_eq!(info.values.len(), 2);
        assert_eq!(info.values[1].name, "Claude Opus");
        assert_eq!(info.values[1].group.as_deref(), Some("Anthropic"));
        assert_eq!(info.values[1].description.as_deref(), Some("Most capable"));
    }

    #[test]
    fn session_configs_do_not_infer_category_from_id() {
        let config = SessionConfigOption::select(
            "model",
            "Runtime",
            "gpt-5",
            vec![SessionConfigSelectOption::new("gpt-5", "GPT-5")],
        );

        let configs = super::super::AcpSessionConfigs::from_acp(&[config], None);
        let info = &configs.0[0];

        assert_eq!(info.config_id.as_deref(), Some("model"));
        assert_eq!(info.category, None);
        assert_eq!(info.current_value, "gpt-5");
    }

    #[test]
    fn session_configs_include_legacy_session_modes() {
        let modes = SessionModeState::new(
            "ask",
            vec![
                SessionMode::new("ask", "Ask"),
                SessionMode::new("auto", "Auto Allow").description("Approve tool calls"),
            ],
        );

        let configs = super::super::AcpSessionConfigs::from_acp(&[], Some(&modes));
        let info = &configs.0[0];

        assert_eq!(info.config_id, None);
        assert_eq!(info.category.as_deref(), Some("mode"));
        assert_eq!(info.current_value, "ask");
        assert_eq!(info.values.len(), 2);
        assert_eq!(info.values[1].name, "Auto Allow");
        assert_eq!(
            info.values[1].description.as_deref(),
            Some("Approve tool calls")
        );
    }

    #[test]
    fn categorized_mode_suppresses_legacy_duplicate() {
        let legacy = SessionModeState::new("ask", vec![SessionMode::new("ask", "Ask")]);
        let config = SessionConfigOption::select(
            "approval",
            "Permissions",
            "auto",
            vec![
                SessionConfigSelectOption::new("ask", "Ask"),
                SessionConfigSelectOption::new("auto", "Auto Allow"),
            ],
        )
        .category(SessionConfigOptionCategory::Mode);

        let configs = super::super::AcpSessionConfigs::from_acp(&[config], Some(&legacy));
        let info = &configs.0[0];

        assert_eq!(configs.0.len(), 1);
        assert_eq!(info.config_id.as_deref(), Some("approval"));
        assert_eq!(info.current_value, "auto");
        assert_eq!(info.values.len(), 2);
    }

    #[test]
    fn config_selection_result_publishes_request_identity() {
        let (stream_tx, mut stream_rx) = broadcast::channel(2);
        let (wake, _) = mpsc::unbounded_channel();
        let (processes, _process_runtime) = ProcessRuntime::new(wake);
        let shared = AcpShared::new(
            "s1".into(),
            PathBuf::from("/tmp"),
            ProcessId::new(),
            stream_tx,
            processes,
        );
        shared.publish_config_selection_result(9, Some("approval"), "auto", true);

        match stream_rx.try_recv().expect("selection result") {
            ServiceMessage::AcpSessionConfigSelectionResult {
                sid,
                request_id,
                config_id,
                value,
                succeeded,
            } => {
                assert_eq!(sid, "s1");
                assert_eq!(request_id, 9);
                assert_eq!(config_id.as_deref(), Some("approval"));
                assert_eq!(value, "auto");
                assert!(succeeded);
            }
            other => panic!("expected ACP config selection result, got {other:?}"),
        }
    }

    #[test]
    fn selected_config_uses_cached_options_when_set_response_is_empty() {
        let mut harness = ProjectionHarness::new(4);
        let config = SessionConfigOption::select(
            "approval",
            "Permissions",
            "ask",
            vec![
                SessionConfigSelectOption::new("ask", "Ask"),
                SessionConfigSelectOption::new("auto", "Auto Allow"),
            ],
        )
        .category(SessionConfigOptionCategory::Mode);
        harness.shared.projection.config_state(AcpConfigStateInput {
            config_options: vec![config],
            modes: None,
        });
        harness.update();
        let _ = harness.stream.try_recv();

        harness
            .shared
            .projection
            .selected_config(AcpSelectedConfigInput {
                config_id: Some("approval".into()),
                value: "auto".into(),
                config_options: Vec::new(),
            });
        harness.update();

        match harness.stream.try_recv().expect("selected config update") {
            ServiceMessage::AcpSessionConfigState { configs, .. } => {
                assert_eq!(configs[0].current_value, "auto");
                assert_eq!(configs[0].values.len(), 2);
            }
            other => panic!("expected ACP config state, got {other:?}"),
        }
    }

    #[test]
    fn acp_config_state_is_replayable_without_a_subscriber() {
        let mut harness = ProjectionHarness::new(1);
        let config = SessionConfigOption::select(
            "model",
            "Model",
            "sonnet",
            vec![SessionConfigSelectOption::new("sonnet", "Claude Sonnet")],
        )
        .category(SessionConfigOptionCategory::Model);

        harness.shared.projection.config_state(AcpConfigStateInput {
            config_options: vec![config],
            modes: None,
        });
        harness.update();

        let configs = harness
            .app
            .world()
            .entity(harness.entity)
            .get::<super::super::AcpSessionConfigs>()
            .unwrap();
        assert_eq!(configs.0[0].current_value, "sonnet");
        assert_eq!(configs.0[0].values[0].name, "Claude Sonnet");
    }

    #[test]
    fn history_replay_emits_only_the_final_snapshot() {
        let mut harness = ProjectionHarness::new(64);
        harness.shared.projection.begin_history_replay();
        harness.update();

        harness
            .shared
            .projection
            .session_update(SessionUpdate::UserMessageChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new("hello")),
            )));
        harness.update();
        assert!(matches!(
            harness.stream.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        for _ in 0..300 {
            harness
                .shared
                .projection
                .session_update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::Text(TextContent::new("x")),
                )));
        }
        harness.update();

        assert!(matches!(
            harness.stream.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        harness.shared.projection.finish_history_replay(true);
        harness.update();

        let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages, .. }) =
            harness.stream.try_recv().expect("final snapshot")
        else {
            panic!("expected snapshot");
        };
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            &messages[1],
            Message::Assistant { blocks }
                if matches!(blocks.as_slice(), [AssistantBlock::Text(text)] if text.len() == 300)
        ));
        assert!(matches!(
            harness.stream.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn failed_history_replay_discards_partial_transcript() {
        let mut harness = ProjectionHarness::new(64);
        harness.shared.projection.begin_history_replay();
        harness.update();
        harness
            .shared
            .projection
            .session_update(SessionUpdate::UserMessageChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new("partial")),
            )));
        harness.update();
        assert!(matches!(
            harness.stream.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));

        harness.shared.projection.finish_history_replay(false);
        harness.update();

        assert!(harness.messages().is_empty());
        let ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { messages, .. }) =
            harness.stream.try_recv().expect("clearing snapshot")
        else {
            panic!("expected snapshot");
        };
        assert!(messages.is_empty());
        assert!(matches!(
            harness.stream.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn approval_details_fall_back_to_projected_tool_call() {
        let mut harness = ProjectionHarness::new(4);
        harness.shared.projection.session_update(
            agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
                ToolCall::new("call-1", "vmux.run")
                    .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
            ),
        );
        harness.update();
        let request = RequestPermissionRequest::new(
            "session-1",
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new(),
            ),
            Vec::new(),
        );

        assert_eq!(
            harness.approval_details(ApprovalDetailsQuery::from_request(&request)),
            Some((
                "vmux.run".to_string(),
                r#"{"command":"echo hi","focus":true}"#.to_string(),
            ))
        );
    }

    #[test]
    fn approval_details_prefer_permission_request_fields() {
        let mut harness = ProjectionHarness::new(4);
        harness.shared.projection.session_update(
            agent_client_protocol::schema::v1::SessionUpdate::ToolCall(
                ToolCall::new("call-1", "old").raw_input(serde_json::json!({"command": "old"})),
            ),
        );
        harness.update();
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
            harness.approval_details(ApprovalDetailsQuery::from_request(&request)),
            Some(("new".to_string(), r#"{"command":"new"}"#.to_string(),))
        );
    }

    #[test]
    fn approval_details_reject_missing_tool_identity() {
        let mut harness = ProjectionHarness::new(4);
        let request = RequestPermissionRequest::new(
            "session-1",
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new(),
            ),
            Vec::new(),
        );

        let query = ApprovalDetailsQuery::from_request(&request);
        assert_eq!(harness.approval_details(query.clone()), None);
        assert_eq!(query.fallback(), ("Use tool".to_string(), "{}".to_string()));
    }

    #[test]
    fn approval_details_use_kind_when_request_has_arguments_but_no_title() {
        let request = RequestPermissionRequest::new(
            "session-1",
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new()
                    .kind(ToolKind::Execute)
                    .raw_input(serde_json::json!({"command": "echo hi"})),
            ),
            Vec::new(),
        );

        assert_eq!(
            ApprovalDetailsQuery::from_request(&request).fallback(),
            (
                "Execute command".to_string(),
                r#"{"command":"echo hi"}"#.to_string(),
            )
        );
    }

    #[test]
    fn approval_details_wait_for_preceding_tool_call_projection() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut harness = ProjectionHarness::new(2);
            let request = RequestPermissionRequest::new(
                "session-1",
                agent_client_protocol::schema::v1::ToolCallUpdate::new(
                    "call-1",
                    ToolCallUpdateFields::new()
                        .kind(ToolKind::Execute)
                        .raw_input(serde_json::json!({"command": "echo hi"})),
                ),
                Vec::new(),
            );
            let waiting = {
                let shared = Arc::clone(&harness.shared);
                tokio::spawn(async move { resolve_approval_details(&request, &shared).await })
            };

            tokio::task::yield_now().await;
            harness.update();
            harness
                .shared
                .projection
                .session_update(SessionUpdate::ToolCall(
                    ToolCall::new("call-1", "vmux.run")
                        .raw_input(serde_json::json!({"command": "echo hi"})),
                ));
            harness.update();
            tokio::task::yield_now().await;
            harness.update();

            assert_eq!(
                waiting.await.unwrap(),
                Some((
                    "vmux.run".to_string(),
                    r#"{"command":"echo hi"}"#.to_string(),
                ))
            );
        });
    }

    #[test]
    fn conversation_title_permission_resolves_as_host_owned_tool() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut harness = ProjectionHarness::new(2);
            harness
                .shared
                .projection
                .session_update(SessionUpdate::ToolCall(
                    ToolCall::new("title-1", "mcp__vmux__set_conversation_title")
                        .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
                ));
            harness.update();
            let request = RequestPermissionRequest::new(
                "session-1",
                agent_client_protocol::schema::v1::ToolCallUpdate::new(
                    "title-1",
                    ToolCallUpdateFields::new()
                        .kind(ToolKind::Execute)
                        .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
                ),
                Vec::new(),
            );

            let waiting = {
                let shared = Arc::clone(&harness.shared);
                tokio::spawn(async move { resolve_approval_details(&request, &shared).await })
            };
            tokio::task::yield_now().await;
            harness.update();
            let (name, _) = waiting.await.unwrap().unwrap();
            assert!(AcpToolCallId::is_conversation_title(&name));
        });
    }

    #[test]
    fn native_choice_tool_is_always_permissionless() {
        for name in [
            "mcp__vmux__request_user_choice",
            "mcp.vmux.request_user_choice",
            "vmux:request-user-choice",
            "request_user_choice",
        ] {
            assert!(AcpDriver::is_permissionless_tool(name), "{name}");
        }
        assert!(!AcpDriver::is_permissionless_tool(
            "other_request_user_choice"
        ));
    }

    #[test]
    fn knowledge_read_tools_are_always_permissionless() {
        for name in [
            "mcp__vmux__search_knowledge",
            "mcp.vmux.read_knowledge",
            "vmux:search-knowledge",
            "read_knowledge",
        ] {
            assert!(AcpDriver::is_permissionless_tool(name), "{name}");
        }
        assert!(!AcpDriver::is_permissionless_tool("write_knowledge"));
        assert!(!AcpDriver::is_permissionless_tool("other_search_knowledge"));
    }

    #[tokio::test]
    async fn requested_resume_loads_only_when_supported() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let loaded = load_requested_session(Some("resume-1".into()), true, |sid| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                assert_eq!(sid.to_string(), "resume-1");
                Ok::<(), &'static str>(())
            }
        })
        .await;
        assert_eq!(loaded.unwrap().unwrap().to_string(), "resume-1");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        let skipped = load_requested_session(Some("resume-2".into()), false, |_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Ok::<(), &'static str>(()) }
        })
        .await;
        assert!(skipped.is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_requested_resume_reports_the_provider_error() {
        let loaded = load_requested_session(Some("stale".into()), true, |_| async {
            Err::<(), &'static str>("missing")
        })
        .await;
        assert_eq!(
            loaded.unwrap_err(),
            "session/load failed for stale: missing"
        );
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
    fn prompt_completion_cancel_wins() {
        assert_eq!(
            PromptCompletion {
                cancelled: false,
                error: None,
            }
            .status(),
            AgentRunStatus::Idle
        );
        assert_eq!(
            PromptCompletion {
                cancelled: false,
                error: Some("boom".into()),
            }
            .status(),
            AgentRunStatus::Errored("boom".into())
        );
        assert_eq!(
            PromptCompletion {
                cancelled: true,
                error: Some("boom".into()),
            }
            .status(),
            AgentRunStatus::Interrupted
        );
    }

    #[test]
    fn private_context_wraps_wire_prompt_without_changing_display_text() {
        let wire = AgentPromptEnvelope::compose("continue here", Some("prior conversation"));

        assert!(AgentPromptEnvelope::new(&wire).has_private_context());
        assert!(wire.contains("prior conversation"));
        assert!(wire.ends_with("continue here"));
        assert_eq!(AgentPromptEnvelope::compose("plain", None), "plain");
    }

    #[test]
    fn pick_permission_option_preserves_decision_scope() {
        let opt = |id: &str, kind: PermissionOptionKind| {
            PermissionOption::new(id.to_string(), id.to_string(), kind)
        };
        let opts = vec![
            opt("once", PermissionOptionKind::AllowOnce),
            opt("always", PermissionOptionKind::AllowAlways),
            opt("rej", PermissionOptionKind::RejectOnce),
        ];
        assert_eq!(
            AcpPermissionOptions(&opts)
                .select(ApprovalDecision::Allow)
                .unwrap()
                .to_string(),
            "once"
        );
        assert_eq!(
            AcpPermissionOptions(&opts)
                .select(ApprovalDecision::AllowAlways)
                .unwrap()
                .to_string(),
            "always"
        );
        assert_eq!(
            AcpPermissionOptions(&opts)
                .select(ApprovalDecision::Deny)
                .unwrap()
                .to_string(),
            "rej"
        );

        let always_only = vec![
            opt("aa", PermissionOptionKind::AllowAlways),
            opt("ra", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(
            AcpPermissionOptions(&always_only)
                .select(ApprovalDecision::Allow)
                .unwrap()
                .to_string(),
            "aa"
        );
        assert_eq!(
            AcpPermissionOptions(&[opt("once", PermissionOptionKind::AllowOnce)])
                .select(ApprovalDecision::AllowAlways),
            None
        );
    }

    #[test]
    fn file_scope_rejects_escape() {
        let scope = AcpFsScope {
            cwd: PathBuf::from("/work"),
        };
        assert_eq!(
            scope.resolve(std::path::Path::new("/work/a.rs")),
            Some(PathBuf::from("/work/a.rs"))
        );
        assert!(scope.resolve(std::path::Path::new("/etc/passwd")).is_none());
        assert!(
            scope
                .resolve(std::path::Path::new("/work/../etc/passwd"))
                .is_none()
        );
    }

    #[test]
    fn file_contents_honor_line_and_limit() {
        let text = AcpFileContents("a\nb\nc\nd".into());
        assert_eq!(text.slice(None, None), "a\nb\nc\nd");
        assert_eq!(text.slice(Some(2), None), "b\nc\nd");
        assert_eq!(text.slice(Some(2), Some(2)), "b\nc");
        assert_eq!(text.slice(Some(10), Some(2)), "");
    }

    struct ProcessHarness {
        runtime: ProcessRuntime,
        rt: tokio::runtime::Runtime,
        stop: Option<std::sync::mpsc::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl ProcessHarness {
        fn start() -> Self {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio test runtime");
            let (wake, wake_rx) = mpsc::unbounded_channel();
            let (runtime, inboxes) = ProcessRuntime::new(wake);
            let (stop, stop_rx) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let mut wake_rx = wake_rx;
                let mut app = App::new();
                app.add_plugins((MinimalPlugins, ProcessPlugin));
                app.world_mut()
                    .spawn((Name::new("vmux acp process test runtime"), inboxes));
                loop {
                    while wake_rx.try_recv().is_ok() {}
                    app.update();
                    match stop_rx.recv_timeout(std::time::Duration::from_millis(2)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                app.world_mut()
                    .run_schedule(vmux_ecs::service::ShutdownComplete);
                app.update();
            });
            Self {
                runtime,
                rt,
                stop: Some(stop),
                thread: Some(thread),
            }
        }

        fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
            self.rt.block_on(future)
        }

        fn inactive() -> ProcessRuntime {
            let (wake, _) = mpsc::unbounded_channel();
            ProcessRuntime::new(wake).0
        }

        fn shared_at(&self, cwd: PathBuf) -> (Arc<AcpShared>, broadcast::Receiver<ServiceMessage>) {
            let (stream_tx, stream_rx) = broadcast::channel(64);
            let shared = Arc::new(AcpShared::new(
                "s1".to_string(),
                cwd,
                ProcessId::new(),
                stream_tx,
                self.runtime.clone(),
            ));
            (shared, stream_rx)
        }

        fn shared(&self) -> (Arc<AcpShared>, broadcast::Receiver<ServiceMessage>) {
            self.shared_at(std::env::temp_dir())
        }
    }

    impl Drop for ProcessHarness {
        fn drop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    #[test]
    fn explicit_workspace_rebind_updates_host_file_scope() {
        let target = tempfile::tempdir().unwrap();
        let (stream_tx, _stream_rx) = broadcast::channel(4);
        let shared = Arc::new(AcpShared::new(
            "s1".to_string(),
            std::env::temp_dir(),
            ProcessId::new(),
            stream_tx,
            ProcessHarness::inactive(),
        ));

        shared.set_cwd(target.path().canonicalize().unwrap());

        assert_eq!(shared.cwd(), target.path().canonicalize().unwrap());
    }

    #[test]
    fn approval_resolution_is_broadcast_immediately() {
        let mut harness = ProjectionHarness::new(4);
        harness
            .shared
            .projection
            .approval_requested(RemoteApproval {
                call_id: "call-1".into(),
                name: "run".into(),
                args: vmux_api::json::JsonValue::Object(Vec::new()),
            });
        harness.update();
        let _ = harness.stream.try_recv();

        harness.shared.projection.approval_resolved("call-1".into());
        harness.update();
        assert!(matches!(
            harness.stream.try_recv(),
            Ok(ServiceMessage::Shared(SharedEvent::AgentApprovalResolved { sid, call_id }))
                if sid == "s1" && call_id == "call-1"
        ));
        assert!(
            harness
                .app
                .world()
                .entity(harness.entity)
                .get::<super::super::AcpApprovalState>()
                .unwrap()
                .0
                .is_none()
        );
    }

    #[test]
    fn workspace_change_rebinds_runtime_file_operations() {
        let original = tempfile::tempdir().unwrap();
        let original_file = original.path().join("original.txt");
        std::fs::write(&original_file, "original").unwrap();
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree = worktree_parent.path().join("quiet-amber-wolf");
        std::fs::create_dir(&worktree).unwrap();
        let worktree_file = worktree.join("worktree.txt");
        std::fs::write(&worktree_file, "worktree").unwrap();
        let original_file = original_file.canonicalize().unwrap();
        let worktree_file = worktree_file.canonicalize().unwrap();
        let (stream_tx, _stream_rx) = broadcast::channel(4);
        let shared = AcpShared::new(
            "s1".into(),
            original.path().canonicalize().unwrap(),
            ProcessId::new(),
            stream_tx,
            ProcessHarness::inactive(),
        );

        let workspace = WorkspaceLocation::new(
            "quiet-amber-wolf".to_string(),
            "vibe/quiet-amber-wolf".to_string(),
            &worktree,
            original.path(),
        )
        .unwrap();
        shared.publish_workspace_change(&workspace);

        let worktree = worktree.canonicalize().unwrap();
        assert_eq!(shared.cwd(), worktree);
        let scope = AcpFsScope { cwd: shared.cwd() };
        assert_eq!(
            scope.read(&ReadTextFileRequest::new("s1", &worktree_file)),
            Ok("worktree".into())
        );
        assert_eq!(
            scope.read(&ReadTextFileRequest::new("s1", &original_file)),
            Err("path outside session cwd".into())
        );
    }

    #[test]
    fn a_dropped_exit_is_recovered_from_the_manager() {
        assert!(matches!(
            AcpTerminalExit::recovered(Some(Some(7))),
            Some(AcpTerminalExit::Exited(Some(7)))
        ));
        assert!(matches!(
            AcpTerminalExit::recovered(None),
            Some(AcpTerminalExit::Removed)
        ));

        assert!(
            AcpTerminalExit::recovered(Some(None)).is_none(),
            "a child that is still running has an exit still to come, so waiting is correct"
        );
    }

    #[test]
    fn acp_terminal_create_wait_output_release() {
        let harness = ProcessHarness::start();
        let (shared, mut stream_rx) = harness.shared();
        harness.block_on(async move {
            let req = CreateTerminalRequest::new("s1", "/bin/sh").args(vec![
                "-c".to_string(),
                "printf hi; sleep 0.1; exit 7".to_string(),
            ]);
            let created = create_terminal(&shared, req).await.expect("create");
            let tid = created.terminal_id.0.to_string();
            assert!(shared.terminals.contains(&tid));

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
            assert!(!shared.terminals.contains(&tid));
        });
    }

    #[test]
    fn terminal_output_unknown_terminal_errors() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();
        let result = harness.block_on(terminal_output(
            &shared,
            TerminalOutputRequest::new("s1", "does-not-exist"),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn create_terminal_rejects_nonexistent_cwd() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();
        let cwd = std::env::temp_dir().join(format!(
            "vmux-acp-missing-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));

        let result = harness.block_on(create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh").cwd(cwd),
        ));

        assert!(result.is_err());
        assert_eq!(harness.block_on(harness.runtime.count()), Ok(0));
    }

    #[test]
    fn create_terminal_rejects_relative_cwd() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();

        let result = harness.block_on(create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh").cwd("."),
        ));

        assert!(result.is_err());
        assert_eq!(harness.block_on(harness.runtime.count()), Ok(0));
    }

    #[test]
    fn create_terminal_rejects_cwd_outside_session() {
        let session = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared_at(session.path().to_path_buf());

        let result = harness.block_on(create_terminal(
            &shared,
            CreateTerminalRequest::new("s1", "/bin/sh").cwd(outside.path()),
        ));

        assert!(matches!(result, Err(message) if message.contains("outside session cwd")));
        assert_eq!(harness.block_on(harness.runtime.count()), Ok(0));
    }

    #[test]
    fn removed_terminal_errors_for_wait_and_output() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();
        let runtime = harness.runtime.clone();
        harness.block_on(async move {
            let created = create_terminal(
                &shared,
                CreateTerminalRequest::new("s1", "/bin/sh")
                    .args(vec!["-c".to_string(), "sleep 30".to_string()]),
            )
            .await
            .expect("create");
            let terminal_id = created.terminal_id.0.to_string();
            let process_id = terminal_id.parse().expect("process id");
            runtime.remove(process_id).await.expect("remove");

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
            shared
                .terminals
                .remove(&TerminalId::new(terminal_id))
                .unwrap();
        });
    }

    #[test]
    fn release_terminal_removes_running_command() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();
        let runtime = harness.runtime.clone();
        harness.block_on(async move {
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

            assert!(runtime.exit_code(process_id).await.is_err());
            assert_eq!(runtime.count().await, Ok(0));
        });
    }

    #[test]
    fn terminal_output_respects_byte_limit_at_char_boundary() {
        let harness = ProcessHarness::start();
        let (shared, _rx) = harness.shared();
        harness.block_on(async move {
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
        });
    }

    #[test]
    fn capability_filtering_reserves_vmux_and_routes_oauth_cwd_and_fallbacks_through_proxy() {
        let servers = AcpMcpServers::from_sources(
            Some("/bin/vmux".into()),
            vec!["mcp".into(), "--profile".into(), "work".into()],
            vec![
                ManagedMcpServer {
                    name: "vmux".into(),
                    transport: ManagedMcpTransport::Stdio,
                    command: Some("evil".into()),
                    ..Default::default()
                },
                ManagedMcpServer {
                    name: "stdio-ok".into(),
                    transport: ManagedMcpTransport::Stdio,
                    command: Some("npx".into()),
                    args: vec!["-y".into(), "server".into()],
                    env: vec![("K".into(), "V".into())],
                    ..Default::default()
                },
                ManagedMcpServer {
                    name: "stdio-cwd".into(),
                    transport: ManagedMcpTransport::Stdio,
                    command: Some("npx".into()),
                    cwd: Some("/tmp".into()),
                    ..Default::default()
                },
                ManagedMcpServer {
                    name: "linear-vmx".into(),
                    transport: ManagedMcpTransport::Http,
                    url: Some("https://mcp.linear.app/mcp".into()),
                    oauth: true,
                    ..Default::default()
                },
                ManagedMcpServer {
                    name: "static-http".into(),
                    transport: ManagedMcpTransport::Http,
                    url: Some("https://docs.example.com/mcp".into()),
                    headers: vec![("Authorization".into(), "Bearer tok".into())],
                    ..Default::default()
                },
                ManagedMcpServer {
                    name: "legacy-sse".into(),
                    transport: ManagedMcpTransport::Sse,
                    url: Some("https://example.com/sse".into()),
                    ..Default::default()
                },
            ],
            PathBuf::from("/repo"),
        );

        let stdio_only = servers.for_capabilities(&McpCapabilities::default());
        assert_eq!(stdio_only.len(), 6);
        assert!(matches!(&stdio_only[0], McpServer::Stdio(s) if s.name == "vmux"));
        assert!(
            matches!(&stdio_only[1], McpServer::Stdio(s) if s.name == "stdio-ok" && s.command == PathBuf::from("npx"))
        );
        assert!(
            matches!(&stdio_only[2], McpServer::Stdio(s) if s.name == "stdio-cwd" && s.command == PathBuf::from("/bin/vmux") && s.args[0] == "mcp-proxy")
        );
        assert!(
            matches!(&stdio_only[3], McpServer::Stdio(s) if s.name == "linear-vmx" && s.args.contains(&"work".to_string()))
        );
        assert!(
            matches!(&stdio_only[4], McpServer::Stdio(s) if s.name == "static-http" && s.args[0] == "mcp-proxy")
        );
        assert!(
            matches!(&stdio_only[5], McpServer::Stdio(s) if s.name == "legacy-sse" && s.args[0] == "mcp-proxy")
        );

        let http_and_sse = servers.for_capabilities(&McpCapabilities::new().http(true).sse(true));
        assert_eq!(http_and_sse.len(), 6);
        assert!(
            matches!(&http_and_sse[3], McpServer::Stdio(s) if s.name == "linear-vmx" && s.args[0] == "mcp-proxy")
        );
        assert!(matches!(&http_and_sse[4], McpServer::Http(s) if s.name == "static-http"));
        assert!(matches!(&http_and_sse[5], McpServer::Sse(s) if s.name == "legacy-sse"));
    }
}
