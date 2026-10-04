use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, ProcessId};
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestApplySet, AgentRequestBlocked,
    AgentRequestMessage, AgentRequestRouteSet,
};

use crate::stack::FocusedStack;

#[vmux_api::contract(Copy, Eq)]
pub enum AgentPaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<AgentPaneDirection> for vmux_api::open_target::PaneDirection {
    fn from(direction: AgentPaneDirection) -> Self {
        match direction {
            AgentPaneDirection::Top => Self::Top,
            AgentPaneDirection::Right => Self::Right,
            AgentPaneDirection::Bottom => Self::Bottom,
            AgentPaneDirection::Left => Self::Left,
        }
    }
}

#[vmux_api::agent]
pub struct AgentOpenBeside {
    pub anchor: ProcessId,
    pub direction: Option<AgentPaneDirection>,
    pub url: String,
    pub focus: bool,
}

#[vmux_api::agent]
pub(super) struct AgentFocusPane {
    pub pane: String,
}

#[vmux_api::agent]
pub(super) struct AgentUpdateLayout {
    pub layout: vmux_api::protocol::layout::LayoutSnapshot,
}

#[vmux_api::agent(Copy, Eq)]
pub(super) struct AgentReadLayout {
    pub anchor: Option<ProcessId>,
}

pub(super) struct LayoutAgentPlugin;

impl Plugin for LayoutAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentFocusPane>()
            .add_agent_request::<AgentUpdateLayout>()
            .add_agent_request::<AgentOpenBeside>()
            .add_message::<FocusPaneRequest>()
            .add_systems(
                Update,
                (request_focus, focus_pane, update)
                    .chain()
                    .after(AgentRequestRouteSet),
            )
            .add_systems(Update, open_beside.in_set(AgentRequestApplySet));
    }
}

fn open_beside(
    mut requests: MessageReader<AgentRequestMessage<AgentOpenBeside>>,
    mut blocked: MessageReader<AgentRequestBlocked>,
    anchors: Query<(&ProcessId, &ChildOf)>,
    child_of: Query<&ChildOf>,
    mut open: MessageWriter<crate::OpenBesideRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let blocked = blocked
        .read()
        .map(|blocked| (blocked.anchor, blocked.reason.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    for request in requests.read() {
        let result = if let Some(reason) = blocked.get(&request.payload.anchor) {
            AgentCommandResult::Error(reason.clone())
        } else {
            let pane = anchors
                .iter()
                .find_map(|(process_id, child_of)| {
                    (*process_id == request.payload.anchor).then_some(child_of.parent())
                })
                .and_then(|stack| child_of.get(stack).ok())
                .map(ChildOf::parent);
            match pane {
                None => AgentCommandResult::Error("self process not found".to_string()),
                Some(pane) => {
                    open.write(crate::OpenBesideRequest {
                        pane,
                        direction: request.payload.direction.map(Into::into),
                        url: request.payload.url.clone(),
                        request_id: request.reply.request_id.0,
                        focus: request.origin.allows_focus(request.payload.focus),
                    });
                    AgentCommandResult::Ok
                }
            }
        };
        responses.write(request.reply.response(result));
    }
}

#[derive(Message, Clone)]
struct FocusPaneRequest {
    pane: String,
}

#[derive(Clone, Copy)]
struct CurrentFocus {
    tab: Option<Entity>,
    pane: Option<Entity>,
    stack: Option<Entity>,
}

impl CurrentFocus {
    fn of(focus: &crate::active_pane::ActiveStack) -> Self {
        Self {
            tab: focus.tab,
            pane: focus.pane,
            stack: focus.stack,
        }
    }

    fn preserve_in(self, snapshot: &mut vmux_api::protocol::layout::LayoutSnapshot) {
        snapshot.focused = vmux_api::protocol::layout::Focus {
            tab: self.id(crate::protocol::NodeKind::Tab, self.tab),
            pane: self.id(crate::protocol::NodeKind::Pane, self.pane),
            stack: self.id(crate::protocol::NodeKind::Stack, self.stack),
        };
        if let Some(tab) = snapshot.focused.tab.as_deref() {
            for item in &mut snapshot.tabs {
                item.is_active = item.id.as_deref() == Some(tab);
            }
        }
    }

    fn id(self, kind: crate::protocol::NodeKind, entity: Option<Entity>) -> Option<String> {
        entity.map(|entity| kind.id(entity.to_bits()))
    }
}

fn request_focus(
    mut requests: MessageReader<AgentRequestMessage<AgentFocusPane>>,
    mut focus: MessageWriter<FocusPaneRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let result = if !request.origin.is_agent() {
            focus.write(FocusPaneRequest {
                pane: request.payload.pane.clone(),
            });
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error("focus_pane is disabled for agents".to_string())
        };
        responses.write(request.reply.response(result));
    }
}

fn focus_pane(mut requests: MessageReader<FocusPaneRequest>, mut commands: Commands) {
    for request in requests.read() {
        let Ok((_, bits)) = crate::protocol::NodeKind::parse_id(&request.pane) else {
            continue;
        };
        commands.trigger(vmux_ecs::ActivateRequest {
            entity: Entity::from_bits(bits),
        });
    }
}

fn update(
    mut requests: MessageReader<AgentRequestMessage<AgentUpdateLayout>>,
    focus: FocusedStack,
    mut apply: MessageWriter<crate::apply::LayoutApplyRequest>,
) {
    for request in requests.read() {
        let mut snapshot = request.payload.layout.clone();
        if request.origin.is_agent() {
            CurrentFocus::of(&focus).preserve_in(&mut snapshot);
        }
        apply.write(crate::apply::LayoutApplyRequest {
            request_id: request.reply.request_id.0,
            snapshot,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::agent::{AgentReply, CommandOrigin};

    struct OpenBesideFixture {
        app: App,
        anchor: ProcessId,
    }

    impl OpenBesideFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_message::<AgentRequestMessage<AgentOpenBeside>>()
                .add_message::<AgentRequestBlocked>()
                .add_message::<crate::OpenBesideRequest>()
                .add_message::<AgentCommandResponse>()
                .add_systems(Update, open_beside);
            let pane = app.world_mut().spawn_empty().id();
            let stack = app.world_mut().spawn(ChildOf(pane)).id();
            let anchor = ProcessId::new();
            app.world_mut().spawn((anchor, ChildOf(stack)));
            Self { app, anchor }
        }

        fn request(&mut self) -> vmux_api::protocol::AgentRequestId {
            let request_id = vmux_api::protocol::AgentRequestId::new();
            self.app.world_mut().write_message(AgentRequestMessage {
                reply: AgentReply::new(request_id),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(self.anchor),
                },
                payload: AgentOpenBeside {
                    anchor: self.anchor,
                    direction: Some(AgentPaneDirection::Left),
                    url: "https://example.com".to_string(),
                    focus: true,
                },
            });
            request_id
        }
    }

    #[test]
    fn open_beside_routes_from_the_layout_feature() {
        let mut fixture = OpenBesideFixture::new();
        let request_id = fixture.request();
        fixture.app.update();

        let requests = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<crate::OpenBesideRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].request_id, request_id.0);
        assert_eq!(
            requests[0].direction,
            Some(vmux_api::open_target::PaneDirection::Left)
        );
        assert!(!requests[0].focus);
    }

    #[test]
    fn prerequisite_failure_blocks_layout_agent_requests() {
        let mut fixture = OpenBesideFixture::new();
        let request_id = fixture.request();
        fixture.app.world_mut().write_message(AgentRequestBlocked {
            anchor: fixture.anchor,
            reason: "worktree failed".to_string(),
        });
        fixture.app.update();

        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<crate::OpenBesideRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let responses = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<AgentCommandResponse>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, request_id);
        assert_eq!(
            responses[0].result,
            AgentCommandResult::Error("worktree failed".to_string())
        );
    }
}
