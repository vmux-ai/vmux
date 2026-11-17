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

#[derive(Message, Clone)]
pub struct AgentRequestInput {
    pub request_id: AgentRequestId,
    pub origin: CommandOrigin,
    pub request: AgentRequest,
}

#[derive(Message, Clone)]
pub struct AgentRequestReady(pub AgentRequestInput);

impl std::ops::Deref for AgentRequestReady {
    type Target = AgentRequestInput;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Component)]
pub struct AgentRequestGate {
    pub anchor: vmux_api::ProcessId,
    pub sid: Option<String>,
    pub request_id: AgentRequestId,
    pub active: bool,
}

#[derive(Message)]
pub struct AgentRequestGateFailed {
    pub anchor: vmux_api::ProcessId,
    pub reason: String,
}

#[derive(Component)]
struct QueuedAgentRequest {
    input: AgentRequestInput,
    anchor: vmux_api::ProcessId,
}

#[derive(Resource, Default)]
struct Sequence(u32);

pub struct AgentRequestDispatchPlugin;

impl Plugin for AgentRequestDispatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sequence>()
            .add_message::<AgentRequestInput>()
            .add_message::<AgentRequestReady>()
            .add_message::<AgentRequestGateFailed>()
            .add_message::<AgentCommandResponse>()
            .configure_sets(
                Update,
                (
                    AgentRequestReleaseSet,
                    AgentRequestDispatchSet,
                    AgentRequestRouteSet,
                    AgentRequestPrerequisiteSet,
                    AgentRequestApplySet,
                )
                    .chain()
                    .after(crate::service::ServiceMessageSet),
            )
            .add_systems(
                Update,
                (fail_queued, release_requests)
                    .chain()
                    .in_set(AgentRequestReleaseSet),
            )
            .add_systems(Update, dispatch_requests.in_set(AgentRequestDispatchSet));
    }
}

fn fail_queued(
    mut failures: MessageReader<AgentRequestGateFailed>,
    queued: Query<(Entity, &QueuedAgentRequest)>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        for (entity, request) in &queued {
            if request.anchor != failure.anchor {
                continue;
            }
            responses.write(
                AgentReply::new(request.input.request_id)
                    .response(AgentCommandResult::Error(failure.reason.clone())),
            );
            commands.entity(entity).try_despawn();
        }
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentRequestReleaseSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentRequestDispatchSet;

#[derive(serde::Deserialize)]
struct RequestAnchor {
    anchor: Option<vmux_api::ProcessId>,
}

impl RequestAnchor {
    fn decode(input: &AgentRequestInput) -> Option<vmux_api::ProcessId> {
        serde_json::from_slice::<Self>(&input.request.body)
            .ok()
            .and_then(|context| context.anchor)
    }
}

fn dispatch_requests(
    mut inputs: MessageReader<AgentRequestInput>,
    mut gates: Query<&mut AgentRequestGate>,
    mut ready: MessageWriter<AgentRequestReady>,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    for input in inputs.read() {
        let (sid, explicit) = match &input.origin {
            CommandOrigin::Agent { sid, anchor } => (sid.as_deref(), *anchor),
            CommandOrigin::User => (None, None),
        };
        let anchor = explicit.or_else(|| RequestAnchor::decode(input));
        let gate = gates.iter().find(|gate| {
            gate.active
                && gate.request_id != input.request_id
                && (anchor == Some(gate.anchor)
                    || sid.is_some_and(|sid| gate.sid.as_deref() == Some(sid)))
        });
        if let Some(gate) = gate {
            sequence.0 = sequence.0.wrapping_add(1);
            commands.spawn((
                Name::new("Queued agent request"),
                crate::Order(sequence.0),
                QueuedAgentRequest {
                    input: input.clone(),
                    anchor: gate.anchor,
                },
            ));
        } else {
            for mut gate in &mut gates {
                if gate.request_id == input.request_id {
                    gate.active = true;
                }
            }
            ready.write(AgentRequestReady(input.clone()));
        }
    }
}

fn release_requests(
    queued: Query<(Entity, &crate::Order, &QueuedAgentRequest)>,
    gates: Query<&AgentRequestGate>,
    mut inputs: MessageWriter<AgentRequestInput>,
    mut commands: Commands,
) {
    let mut available = Vec::new();
    for (entity, order, request) in &queued {
        if gates.iter().any(|gate| gate.anchor == request.anchor) {
            continue;
        }
        available.push((order.0, entity, &request.input));
    }
    available.sort_by_key(|(order, _, _)| *order);
    for (_, entity, input) in available {
        inputs.write(input.clone());
        commands.entity(entity).try_despawn();
    }
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
        if !self.is_plugin_added::<AgentRequestRoutePlugin<T>>() {
            self.add_plugins(AgentRequestRoutePlugin::<T>(std::marker::PhantomData));
        }
        self
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

struct AgentRequestRoutePlugin<T>(std::marker::PhantomData<fn() -> T>);

impl<T> Plugin for AgentRequestRoutePlugin<T>
where
    T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync,
{
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AgentRequestDispatchPlugin>() {
            app.add_plugins(AgentRequestDispatchPlugin);
        }
        app.add_message::<AgentRequestInput>()
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
            .add_systems(Update, route_requests::<T>.in_set(AgentRequestRouteSet));
    }
}

fn route_requests<T>(
    mut requests: MessageReader<AgentRequestReady>,
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
    pub messages: Vec<vmux_api::session::Message>,
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

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    struct DispatchFixture {
        app: App,
        ready: bevy::ecs::message::MessageCursor<AgentRequestReady>,
    }

    impl DispatchFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AgentRequestDispatchPlugin));
            Self {
                app,
                ready: Default::default(),
            }
        }

        fn request(
            &mut self,
            anchor: Option<vmux_api::ProcessId>,
            sid: Option<&str>,
        ) -> AgentRequestId {
            let request_id = AgentRequestId::new();
            self.app.world_mut().write_message(AgentRequestInput {
                request_id,
                origin: CommandOrigin::Agent {
                    sid: sid.map(str::to_string),
                    anchor,
                },
                request: AgentRequest::encode(&vmux_api::protocol::AgentNotify {
                    title: None,
                    body: None,
                })
                .unwrap(),
            });
            request_id
        }

        fn delivered(&mut self) -> Vec<AgentRequestId> {
            self.ready
                .read(self.app.world().resource::<Messages<AgentRequestReady>>())
                .map(|request| request.request_id)
                .collect()
        }
    }

    #[test]
    fn prerequisite_holds_following_requests_and_releases_them_in_arrival_order() {
        let mut fixture = DispatchFixture::new();
        let anchor = vmux_api::ProcessId::new();
        let before = fixture.request(Some(anchor), None);
        let first = fixture.request(Some(anchor), None);
        let after = fixture.request(Some(anchor), None);
        let last = fixture.request(Some(anchor), None);
        let gate = fixture
            .app
            .world_mut()
            .spawn(AgentRequestGate {
                anchor,
                sid: None,
                request_id: first,
                active: false,
            })
            .id();
        fixture.app.update();
        assert_eq!(fixture.delivered(), [before, first]);
        fixture
            .app
            .world_mut()
            .entity_mut(gate)
            .remove::<AgentRequestGate>();
        fixture.app.update();
        assert_eq!(fixture.delivered(), [after, last]);
    }

    #[test]
    fn session_gate_holds_anchorless_requests_without_blocking_another_session() {
        let mut fixture = DispatchFixture::new();
        let anchor = vmux_api::ProcessId::new();
        let gate = fixture
            .app
            .world_mut()
            .spawn(AgentRequestGate {
                anchor,
                sid: Some("held".into()),
                request_id: AgentRequestId::new(),
                active: true,
            })
            .id();
        let held = fixture.request(None, Some("held"));
        let other = fixture.request(None, Some("other"));
        fixture.app.update();
        assert_eq!(fixture.delivered(), [other]);
        fixture
            .app
            .world_mut()
            .entity_mut(gate)
            .remove::<AgentRequestGate>();
        fixture.app.update();
        assert_eq!(fixture.delivered(), [held]);
    }

    #[test]
    fn failed_prerequisite_returns_errors_instead_of_running_queued_commands() {
        let mut fixture = DispatchFixture::new();
        let anchor = vmux_api::ProcessId::new();
        let gate = fixture
            .app
            .world_mut()
            .spawn(AgentRequestGate {
                anchor,
                sid: None,
                request_id: AgentRequestId::new(),
                active: true,
            })
            .id();
        let held = fixture.request(Some(anchor), None);
        fixture.app.update();
        assert!(fixture.delivered().is_empty());
        fixture
            .app
            .world_mut()
            .write_message(AgentRequestGateFailed {
                anchor,
                reason: "workspace failed".into(),
            });
        fixture
            .app
            .world_mut()
            .entity_mut(gate)
            .remove::<AgentRequestGate>();
        fixture.app.update();
        assert!(fixture.delivered().is_empty());
        let mut cursor = bevy::ecs::message::MessageCursor::<AgentCommandResponse>::default();
        let replies = fixture
            .app
            .world()
            .resource::<Messages<AgentCommandResponse>>();
        let reply = cursor
            .read(replies)
            .find(|reply| reply.request_id == held)
            .unwrap();
        assert!(
            matches!(&reply.result, AgentCommandResult::Error(reason) if reason == "workspace failed")
        );
    }
}
