use std::path::PathBuf;

use bevy::prelude::*;

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
            .add_systems(Update, route_requests::<T>.in_set(AgentRequestRouteSet))
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
            .add_systems(Update, forward_messages::<T>.in_set(AgentRequestApplySet))
    }
}

fn route_requests<T>(
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

fn forward_messages<T>(
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct StackSessionHandoff {
    pub source_agent: String,
    pub source_sid: String,
    pub messages: Vec<vmux_api::conversation::Message>,
    pub context: String,
    pub truncated: bool,
}

#[derive(Message, Debug, Clone)]
pub struct SwapStackSession {
    pub stack: Entity,
    pub target_agent: String,
    pub cwd: PathBuf,
    pub handoff: Option<StackSessionHandoff>,
}
