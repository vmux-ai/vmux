use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;

use crate::terminal::TerminalKind;
pub use vmux_api::agent::AgentKind;
use vmux_api::protocol::{AgentCommandResult, AgentRequest, AgentRequestId};

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AgentPromptContribution(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AgentDisabledSkillRoot(pub PathBuf);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentSessionRoot;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AgentContinuationRequest {
    pub session: Entity,
    pub context: String,
}

#[derive(Clone, Debug, Default)]
pub enum CommandOrigin {
    #[default]
    User,
    Agent {
        sid: Option<String>,
        anchor: Option<vmux_api::ProcessId>,
    },
}

impl CommandOrigin {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    pub fn allows_focus(&self, requested: bool) -> bool {
        requested && !self.is_agent()
    }
}

#[derive(Message)]
pub struct AgentRequestInput {
    pub request_id: AgentRequestId,
    pub origin: CommandOrigin,
    pub request: AgentRequest,
}

impl AgentRequestInput {
    pub fn decode<T>(&self) -> Result<Option<T>, String>
    where
        T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned,
    {
        self.request.decode()
    }
}

#[derive(Message)]
pub struct AgentRequestMessage<T> {
    pub reply: AgentReply,
    pub origin: CommandOrigin,
    pub payload: T,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentRequestRouteSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentRequestPrerequisiteSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentRequestApplySet;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AgentRequestBlocked {
    pub anchor: vmux_api::ProcessId,
    pub reason: String,
}

pub trait AgentRequestAppExt {
    fn add_agent_request<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync;

    fn add_agent_message<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract
            + serde::de::DeserializeOwned
            + Message
            + Clone
            + Send
            + Sync;
}

impl AgentRequestAppExt for App {
    fn add_agent_request<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync,
    {
        self.add_message::<AgentRequestInput>()
            .add_message::<AgentRequestMessage<T>>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentRequestBlocked>()
            .configure_sets(
                Update,
                AgentRequestRouteSet.after(crate::service::ServiceMessageSet),
            )
            .configure_sets(
                Update,
                (
                    AgentRequestRouteSet,
                    AgentRequestPrerequisiteSet,
                    AgentRequestApplySet,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                route_agent_requests::<T>.in_set(AgentRequestRouteSet),
            )
    }

    fn add_agent_message<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract
            + serde::de::DeserializeOwned
            + Message
            + Clone
            + Send
            + Sync,
    {
        self.add_agent_request::<T>()
            .add_message::<T>()
            .add_systems(
                Update,
                forward_agent_messages::<T>.in_set(AgentRequestApplySet),
            )
    }
}

fn route_agent_requests<T>(
    mut requests: MessageReader<AgentRequestInput>,
    mut routed: MessageWriter<AgentRequestMessage<T>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) where
    T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync,
{
    for request in requests.read() {
        match request.decode::<T>() {
            Ok(Some(payload)) => {
                routed.write(AgentRequestMessage {
                    reply: AgentReply::new(request.request_id),
                    origin: request.origin.clone(),
                    payload,
                });
            }
            Ok(None) => {}
            Err(message) => {
                responses.write(
                    AgentReply::new(request.request_id)
                        .response(AgentCommandResult::Error(message)),
                );
            }
        }
    }
}

fn forward_agent_messages<T>(
    mut requests: MessageReader<AgentRequestMessage<T>>,
    mut messages: MessageWriter<T>,
    mut responses: MessageWriter<AgentCommandResponse>,
) where
    T: Message + Clone,
{
    for request in requests.read() {
        messages.write(request.payload.clone());
        responses.write(request.reply.ok());
    }
}

#[derive(Clone, Message)]
pub struct AgentCommandResponse {
    pub request_id: AgentRequestId,
    pub result: AgentCommandResult,
}

#[derive(Clone, Copy)]
pub struct AgentReply {
    pub request_id: AgentRequestId,
}

impl AgentReply {
    pub fn new(request_id: AgentRequestId) -> Self {
        Self { request_id }
    }

    pub fn response(self, result: AgentCommandResult) -> AgentCommandResponse {
        AgentCommandResponse {
            request_id: self.request_id,
            result,
        }
    }

    pub fn ok(self) -> AgentCommandResponse {
        self.response(AgentCommandResult::Ok)
    }
}

pub fn effort_levels(agent_key: &str) -> &'static [&'static str] {
    match agent_key {
        "claude" | "cli:claude" => &["low", "medium", "high", "max"],
        "cli:codex" => &["minimal", "low", "medium", "high"],
        _ => &[],
    }
}

pub fn default_effort(agent_key: &str) -> &'static str {
    match agent_key {
        "claude" | "cli:claude" | "cli:codex" => "medium",
        _ => "",
    }
}

impl From<AgentKind> for TerminalKind {
    fn from(kind: AgentKind) -> Self {
        match kind {
            AgentKind::Vibe => TerminalKind::Vibe,
            AgentKind::Claude => TerminalKind::Claude,
            AgentKind::Codex => TerminalKind::Codex,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct AgentCliKind(pub AgentKind);

#[derive(Component, Debug, Clone)]
#[require(AgentSessionRoot)]
pub struct AgentSession {
    pub kind: AgentKind,
}

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug, Clone)]
pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Message, Debug, Clone)]
pub struct SpawnAgentInStackRequest {
    pub kind: AgentKind,
    pub cwd: PathBuf,
    pub session_id: Option<String>,
    pub stack: Entity,
    pub initial_prompt: Option<String>,
    pub initial_attachments: Vec<vmux_api::protocol::AgentAttachment>,
}

#[derive(Debug, Clone)]
pub struct StackSessionHandoff {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages: Vec<vmux_api::room::Message>,
    pub context: String,
    pub truncated: bool,
}

#[derive(Message, Debug, Clone)]
pub struct SwapStackSession {
    pub stack: Entity,
    pub target_url: String,
    pub cwd: PathBuf,
    pub handoff: Option<StackSessionHandoff>,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct RestartAgentPty {
    pub entity: Entity,
}

pub fn parse_acp_agent_url(url: &str) -> Option<String> {
    let route = vmux_api::VmuxRoute::parse(url)?;
    if !route.is_agent() {
        return None;
    }
    let segs: Vec<&str> = route.path_segments().collect();
    match segs.as_slice() {
        [id] => Some((*id).to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_kind_into_terminal_kind() {
        assert_eq!(TerminalKind::from(AgentKind::Vibe), TerminalKind::Vibe);
        assert_eq!(TerminalKind::from(AgentKind::Claude), TerminalKind::Claude);
        assert_eq!(TerminalKind::from(AgentKind::Codex), TerminalKind::Codex);
    }

    #[test]
    fn parse_page_agent_url_provider_model_only() {
        let (provider, model, sid) =
            parse_page_agent_url("vmux://sessions/openai/gpt-5.5").unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.5");
        assert!(sid.is_none());
    }

    #[test]
    fn parse_page_agent_url_with_sid() {
        let (provider, model, sid) =
            parse_page_agent_url("vmux://sessions/anthropic/claude-opus-4.7/xHigh").unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4.7");
        assert_eq!(sid.as_deref(), Some("xHigh"));
    }

    #[test]
    fn parse_page_agent_url_rejects_single_segment() {
        assert!(parse_page_agent_url("vmux://sessions/vibe").is_none());
    }

    #[test]
    fn parse_acp_agent_url_single_segment() {
        assert_eq!(
            parse_acp_agent_url("vmux://sessions/vibe-acp"),
            Some("vibe-acp".to_string())
        );
        assert!(parse_acp_agent_url("vmux://sessions/openai/gpt-5.5").is_none());
        assert!(parse_acp_agent_url("https://google.com").is_none());
    }

    #[test]
    fn parse_page_agent_url_rejects_too_many_segments() {
        assert!(parse_page_agent_url("vmux://sessions/openai/gpt/sid/extra").is_none());
    }

    #[test]
    fn parse_page_agent_url_rejects_non_agent_host() {
        assert!(parse_page_agent_url("https://google.com").is_none());
    }

    #[test]
    fn legacy_agent_urls_still_parse() {
        assert_eq!(
            parse_acp_agent_url("vmux://agent/claude"),
            Some("claude".to_string())
        );
        assert_eq!(
            parse_page_agent_url("vmux://agent/openai/gpt-5.5"),
            Some(("openai".to_string(), "gpt-5.5".to_string(), None))
        );
    }

    #[test]
    fn effort_levels_exposed_only_for_wired_agents() {
        assert_eq!(effort_levels("claude"), ["low", "medium", "high", "max"]);
        assert_eq!(
            effort_levels("cli:claude"),
            ["low", "medium", "high", "max"]
        );
        assert_eq!(
            effort_levels("cli:codex"),
            ["minimal", "low", "medium", "high"]
        );
        assert!(effort_levels("codex").is_empty());
        assert!(effort_levels("gemini").is_empty());
        assert!(effort_levels("vibe").is_empty());
        assert!(effort_levels("cli:vibe").is_empty());
    }
}
