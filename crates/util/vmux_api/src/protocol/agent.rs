use crate::room::ClientOpId;
use crate::{ProcessId, json::JsonValue};

#[vmux_api::contract(Eq)]
pub struct AgentRequest {
    pub id: String,
    pub body: Vec<u8>,
}

impl AgentRequest {
    pub fn encode<T>(payload: &T) -> Result<Self, String>
    where
        T: crate::AgentRequestContract + serde::Serialize,
    {
        let body = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
        Ok(Self {
            id: T::id().to_string(),
            body,
        })
    }

    pub fn decode<T>(&self) -> Result<Option<T>, String>
    where
        T: crate::AgentRequestContract + serde::de::DeserializeOwned,
    {
        if self.id != T::id() {
            return Ok(None);
        }
        serde_json::from_slice(&self.body)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

#[vmux_api::contract(Copy, Eq, Hash)]
pub struct AgentRequestId(pub [u8; 16]);

impl Default for AgentRequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRequestId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
}

#[vmux_api::contract(Copy, Eq)]
pub enum AgentShellMode {
    NewTab,
    Active,
}

#[vmux_api::contract(Copy, Eq)]
pub enum AgentPaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

#[vmux_api::contract(Copy, Eq)]
pub enum ManagedMcpTransport {
    Stdio,
    Http,
    Sse,
}

#[vmux_api::contract(Eq)]
pub struct ManagedMcpServer {
    pub name: String,
    pub transport: ManagedMcpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
    pub url: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[vmux_api::contract(Copy, Eq)]
pub enum PlacementMode {
    Auto,
    Split,
    Stack,
}

#[vmux_api::contract(Copy, Eq)]
pub enum FileTouchKind {
    Read,
    Edit,
}

#[vmux_api::contract(Copy, Eq)]
pub enum SimulatorButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::contract(Eq)]
pub struct FileSearchMatch {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

#[vmux_api::contract(Eq)]
pub struct AgentBookmarkPage {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
}

#[vmux_api::agent]
pub struct AgentInvokeCommand {
    pub id: String,
    #[rkyv(attr(allow(dead_code)))]
    pub args: JsonValue,
}

#[vmux_api::agent]
pub struct AgentNewTerminalTab {
    pub cwd: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[vmux_api::agent]
pub struct AgentRunShell {
    pub command: String,
    pub cwd: String,
    pub mode: AgentShellMode,
}

#[vmux_api::agent]
pub struct AgentBrowserNavigate {
    pub url: String,
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub struct AgentBrowserInstallExtension {
    pub source: String,
}

#[vmux_api::agent]
pub struct AgentTerminalSend {
    pub text: String,
    pub terminal: Option<String>,
}

#[vmux_api::agent]
pub struct AgentFocusPane {
    pub pane: String,
}

#[vmux_api::agent]
pub struct AgentRenameProfile {
    pub name: String,
}

#[vmux_api::agent]
pub struct AgentUpdateSettings {
    pub path: String,
    pub value: JsonValue,
}

#[vmux_api::agent]
pub struct AgentUpdateLayout {
    pub layout: crate::protocol::layout::LayoutSnapshot,
}

#[vmux_api::agent]
pub struct AgentBrowserGoBack {
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub struct AgentBrowserGoForward {
    pub pane: Option<String>,
}

#[vmux_api::agent]
pub struct AgentBrowserHistorySearch {
    pub query: String,
    pub limit: u32,
}

#[vmux_api::agent]
pub struct AgentOpenInNewStack {
    pub url: String,
}

#[vmux_api::agent]
pub struct AgentSpaceCreate {
    pub name: Option<String>,
}

#[vmux_api::agent]
pub struct AgentSpaceRename {
    pub space_id: String,
    pub name: String,
}

#[vmux_api::agent]
pub struct AgentSpaceDelete {
    pub space_id: String,
}

#[vmux_api::agent]
pub struct AgentOpenBeside {
    pub anchor: ProcessId,
    pub direction: Option<AgentPaneDirection>,
    pub url: String,
    pub focus: bool,
}

#[vmux_api::agent]
pub struct AgentRun {
    pub anchor: ProcessId,
    pub command: String,
    pub direction: AgentPaneDirection,
    pub focus: bool,
    pub beside: Option<ProcessId>,
    pub mode: PlacementMode,
    pub terminal: Option<ProcessId>,
    pub done_marker: Option<String>,
}

#[vmux_api::agent]
pub struct AgentRunWithPlacementOverride(pub AgentRun);

#[vmux_api::agent]
pub struct AgentNotify {
    pub title: Option<String>,
    pub body: Option<String>,
}

#[vmux_api::agent]
pub struct AgentFileTouched {
    pub anchor: ProcessId,
    pub path: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub end_col: Option<u32>,
    pub kind: FileTouchKind,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentCreateWorktree {
    pub anchor: ProcessId,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentTurnEnded {
    pub anchor: ProcessId,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentResumeInAcp {
    pub anchor: ProcessId,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentChooseWorkspace {
    pub anchor: ProcessId,
}

#[vmux_api::agent]
pub struct AgentCreateWorktreeOnBranch {
    pub anchor: ProcessId,
    pub branch: String,
    pub project: Option<String>,
}

#[vmux_api::agent]
pub struct AgentBookmarkAdd {
    pub page: AgentBookmarkPage,
    pub folder: Option<String>,
}

#[vmux_api::agent]
pub struct AgentBookmarkRemove {
    pub uuid: String,
}

#[vmux_api::agent]
pub struct AgentBookmarkPin {
    pub uuid: String,
}

#[vmux_api::agent]
pub struct AgentBookmarkUnpin {
    pub uuid: String,
}

#[vmux_api::agent]
pub struct AgentBookmarkPinUrl {
    pub page: AgentBookmarkPage,
}

#[vmux_api::agent]
pub struct AgentBookmarkFolderCreate {
    pub name: String,
}

#[vmux_api::agent]
pub struct AgentRequestUserChoice {
    pub anchor: ProcessId,
    pub question: String,
    pub options: Vec<String>,
}

#[vmux_api::agent]
pub struct AgentChooseWorkspaceAtPath {
    pub anchor: ProcessId,
    pub path: String,
}

#[vmux_api::agent]
pub struct AgentPrepareWorktree {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub task: Option<String>,
    pub create: bool,
}

#[vmux_api::agent]
pub struct AgentFileSearch {
    pub anchor: ProcessId,
    pub root: String,
    pub query: String,
    pub matches: Vec<FileSearchMatch>,
}

#[vmux_api::agent]
pub struct AgentSetConversationTitle {
    pub anchor: ProcessId,
    pub title: String,
}

#[vmux_api::agent]
pub struct AgentWriteKnowledge {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub title: String,
    pub content: String,
}

#[vmux_api::agent]
pub struct AgentSearchKnowledge {
    pub anchor: ProcessId,
    pub query: String,
    pub limit: u16,
}

#[vmux_api::agent]
pub struct AgentReadKnowledge {
    pub anchor: ProcessId,
    pub path: String,
    pub line: u32,
    pub limit: u32,
}

#[vmux_api::agent]
pub struct AgentNewChat {
    pub client_op_id: ClientOpId,
    pub prompt: String,
    pub agent_url: Option<String>,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentListAgents;

#[vmux_api::agent(Copy, Eq)]
pub struct AgentListTeam;

#[vmux_api::agent]
pub struct AgentListModels {
    pub sid: String,
}

#[vmux_api::agent]
pub struct AgentSelectModel {
    pub sid: String,
    pub model_id: String,
}

#[vmux_api::agent]
pub struct AgentSetEffort {
    pub sid: String,
    pub level: String,
}

pub const AGENT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub const AGENT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub const AGENT_TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[vmux_api::contract]
pub enum AgentCommandResult {
    Ok,
    Text(String),
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum ApprovalDecision {
    Allow,
    #[default]
    Deny,
    AllowAlways,
}

#[vmux_api::contract]
pub enum AgentRunStatus {
    Streaming,
    Idle,
    Interrupted,
    Errored(String),
}

#[vmux_api::contract(Eq)]
pub struct AgentAttachment {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

pub const PRIVATE_CONTEXT_PREFIX: &str = "<vmux_handoff_context>";
pub const PRIVATE_CONTEXT_PROMPT_MARKER: &str = "\n\nCurrent user prompt:\n";
const PRIVATE_CONTEXT_LENGTH_PREFIX: &str = "Context bytes: ";
const PRIVATE_CONTEXT_CLOSING_TAG: &str = "\n</vmux_handoff_context>";

pub fn compose_agent_prompt(display_text: &str, context: Option<&str>) -> String {
    match context {
        Some(context) => format!(
            "{PRIVATE_CONTEXT_PREFIX}\n{PRIVATE_CONTEXT_LENGTH_PREFIX}{}\n{context}{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}{display_text}",
            context.len()
        ),
        None => display_text.to_string(),
    }
}

pub fn extract_display_prompt(prompt: &str) -> Option<&str> {
    split_private_context_prompt(prompt).map(|(_, display)| display)
}

pub fn split_private_context_prompt(prompt: &str) -> Option<(&str, &str)> {
    split_length_delimited_private_context(prompt).or_else(|| {
        let body = private_context_body(prompt)?;
        let separator = format!("{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}");
        body.rsplit_once(&separator)
    })
}

pub fn has_private_context_envelope(prompt: &str) -> bool {
    private_context_body(prompt).is_some_and(|body| body.contains(PRIVATE_CONTEXT_CLOSING_TAG))
}

fn private_context_body(prompt: &str) -> Option<&str> {
    prompt
        .find(PRIVATE_CONTEXT_PREFIX)
        .and_then(|start| prompt.get(start + PRIVATE_CONTEXT_PREFIX.len()..))?
        .strip_prefix('\n')
}

fn split_length_delimited_private_context(prompt: &str) -> Option<(&str, &str)> {
    let body = private_context_body(prompt)?;
    let (length, body) = body.split_once('\n')?;
    let context_len = length
        .strip_prefix(PRIVATE_CONTEXT_LENGTH_PREFIX)?
        .parse::<usize>()
        .ok()?;
    let context = body.get(..context_len)?;
    let display = body
        .get(context_len..)?
        .strip_prefix(PRIVATE_CONTEXT_CLOSING_TAG)?
        .strip_prefix(PRIVATE_CONTEXT_PROMPT_MARKER)?;
    Some((context, display))
}
