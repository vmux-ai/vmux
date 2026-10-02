use crate::ProcessId;
use crate::room::ClientOpId;

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
pub enum FileTouchKind {
    Read,
    Edit,
}

#[vmux_api::contract(Eq)]
pub struct FileSearchMatch {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub preview: String,
}

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
pub struct AgentTurnEnded {
    pub anchor: ProcessId,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentResumeInAcp {
    pub anchor: ProcessId,
}

#[vmux_api::agent]
pub struct AgentFileSearch {
    pub anchor: ProcessId,
    pub root: String,
    pub query: String,
    pub matches: Vec<FileSearchMatch>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentPromptEnvelope<'a>(&'a str);

const PRIVATE_CONTEXT_PREFIX: &str = "<vmux_handoff_context>";
const PRIVATE_CONTEXT_PROMPT_MARKER: &str = "\n\nCurrent user prompt:\n";
const PRIVATE_CONTEXT_LENGTH_PREFIX: &str = "Context bytes: ";
const PRIVATE_CONTEXT_CLOSING_TAG: &str = "\n</vmux_handoff_context>";

impl AgentPromptEnvelope<'static> {
    pub fn compose(display_text: &str, context: Option<&str>) -> String {
        match context {
            Some(context) => format!(
                "{PRIVATE_CONTEXT_PREFIX}\n{PRIVATE_CONTEXT_LENGTH_PREFIX}{}\n{context}{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}{display_text}",
                context.len()
            ),
            None => display_text.to_string(),
        }
    }
}

impl<'a> AgentPromptEnvelope<'a> {
    pub fn new(prompt: &'a str) -> Self {
        Self(prompt)
    }

    pub fn display(self) -> Option<&'a str> {
        self.split().map(|(_, display)| display)
    }

    pub fn split(self) -> Option<(&'a str, &'a str)> {
        self.split_length_delimited().or_else(|| {
            let body = self.body()?;
            let separator = format!("{PRIVATE_CONTEXT_CLOSING_TAG}{PRIVATE_CONTEXT_PROMPT_MARKER}");
            body.rsplit_once(&separator)
        })
    }

    pub fn has_private_context(self) -> bool {
        self.body()
            .is_some_and(|body| body.contains(PRIVATE_CONTEXT_CLOSING_TAG))
    }

    fn body(self) -> Option<&'a str> {
        self.0
            .find(PRIVATE_CONTEXT_PREFIX)
            .and_then(|start| self.0.get(start + PRIVATE_CONTEXT_PREFIX.len()..))?
            .strip_prefix('\n')
    }

    fn split_length_delimited(self) -> Option<(&'a str, &'a str)> {
        let body = self.body()?;
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
}
