use bevy::prelude::*;
use vmux_api::open_target::PaneDirection;
use vmux_ecs::{EntityTarget, Operation, OperationRetained, OperationState, Order};

use crate::pane::{OpenBesideRequest, Pane};
use crate::stack::Stack;

pub(crate) use query::PlacementQuery;

mod commit;
mod query;

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sequence>()
            .add_message::<OpenBesideRequest>()
            .add_message::<vmux_ecs::PageOpenRequest>()
            .add_message::<vmux_ecs::agent::AgentCommandResponse>()
            .add_observer(admit_stack)
            .add_systems(Startup, ensure_counter.after(crate::LayoutStartupSet::Post))
            .add_systems(
                Update,
                admit_open
                    .in_set(PlacementAdmissionSet)
                    .in_set(crate::host::command::LayoutRequestSet::Handle)
                    .after(vmux_ecs::agent::AgentRequestApplySet),
            )
            .add_plugins(commit::PlacementCommitPlugin);
    }
}

fn ensure_counter(counters: Query<(), With<crate::pane::SpawnCounter>>, mut commands: Commands) {
    if counters.is_empty() {
        commands.spawn((
            Name::new("Pane spawn counter"),
            crate::pane::SpawnCounter::default(),
        ));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlacementAdmissionSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlacementCommitSet;

#[derive(EntityEvent, Clone)]
pub struct PlaceStackRequest {
    #[event_target]
    pub operation: Entity,
    pub stack: Entity,
    pub anchor: Entity,
    pub url: String,
    pub direction: Option<PaneDirection>,
    pub automatic: bool,
    pub primary_anchor: bool,
    pub focus: bool,
}

#[derive(EntityEvent)]
pub struct StackPlaced {
    #[event_target]
    pub operation: Entity,
    pub stack: Entity,
    pub result: Result<Entity, String>,
}

#[derive(EntityEvent)]
pub struct PlacementStarting {
    #[event_target]
    pub operation: Entity,
}

#[derive(Component, Clone)]
struct OpenIntent {
    url: String,
    direction: Option<PaneDirection>,
    request_id: [u8; 16],
    focus: bool,
}

#[derive(Component, Clone)]
struct StackIntent {
    url: String,
    direction: Option<PaneDirection>,
    automatic: bool,
    primary_anchor: bool,
    focus: bool,
}

#[derive(Component)]
struct Pending;

#[derive(Component)]
struct Routed;

#[derive(Component)]
struct CreatedStack;

#[derive(Component)]
struct PreviousNavigation(Option<vmux_ecs::PageOpenId>);

#[derive(Component)]
struct Unplaced;

#[derive(Resource, Default)]
struct Sequence(u32);

impl Sequence {
    fn next(&mut self) -> Order {
        self.0 = self
            .0
            .checked_add(1)
            .expect("placement admission sequence exhausted");
        Order(self.0)
    }
}

fn admit_open(
    mut requests: MessageReader<OpenBesideRequest>,
    layout: PlacementQuery,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
    mut responses: MessageWriter<vmux_ecs::agent::AgentCommandResponse>,
) {
    for request in requests.read() {
        let Some(scope) = layout.scope(request.pane) else {
            if !layout.registered(&request.url) {
                responses.write(
                    vmux_ecs::agent::AgentReply::new(vmux_api::protocol::AgentRequestId(
                        request.request_id,
                    ))
                    .response(vmux_api::protocol::AgentCommandResult::Error(
                        "placement source is unavailable".into(),
                    )),
                );
            }
            continue;
        };
        commands.spawn((
            Name::new("Place opened page"),
            Operation::bundle("layout.place-page").expect("fixed placement operation"),
            OperationRetained,
            EntityTarget::<Pane>::new(request.pane),
            scope,
            sequence.next(),
            OpenIntent {
                url: request.url.clone(),
                direction: request.direction,
                request_id: request.request_id,
                focus: request.focus,
            },
            Pending,
        ));
    }
}

fn admit_stack(
    request: On<PlaceStackRequest>,
    layout: PlacementQuery,
    states: Query<&OperationState>,
    mut sequence: ResMut<Sequence>,
    mut commands: Commands,
) {
    let request = request.event();
    let valid = states
        .get(request.operation)
        .is_ok_and(|state| matches!(state, OperationState::Pending | OperationState::Running));
    let scope = layout.scope(request.anchor);
    if !valid
        || scope.is_none()
        || !layout.stack_exists(request.stack)
        || layout.parent(request.stack).is_some()
    {
        commands.trigger(StackPlaced {
            operation: request.operation,
            stack: request.stack,
            result: Err("placement source is unavailable".into()),
        });
        return;
    }
    commands.entity(request.operation).insert((
        sequence.next(),
        EntityTarget::<Pane>::new(request.anchor),
        EntityTarget::<Stack>::new(request.stack),
        scope.unwrap(),
        StackIntent {
            url: request.url.clone(),
            direction: request.direction,
            automatic: request.automatic,
            primary_anchor: request.primary_anchor,
            focus: request.focus,
        },
        Pending,
    ));
    commands.entity(request.stack).insert(Unplaced);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::HostWindow;
    use vmux_ecs::PageMetadata;
    use vmux_flex::ComputedNode;
    use vmux_history::LastActivatedAt;

    #[derive(Resource, Default)]
    struct Results(Vec<(Entity, Entity, Result<Entity, String>)>);

    struct Fixture {
        app: App,
        pane: Entity,
        space: Entity,
        window: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PlacementPlugin))
                .init_resource::<Results>()
                .add_observer(Self::receive);
            let window = app.world_mut().spawn(Window::default()).id();
            let space = app
                .world_mut()
                .spawn((crate::space::Space, HostWindow(window), ChildOf(window)))
                .id();
            let tab = app
                .world_mut()
                .spawn((crate::tab::Tab::default(), ChildOf(space)))
                .id();
            let pane = app
                .world_mut()
                .spawn((
                    Pane::bundle(),
                    crate::pane::SpawnSeq(1),
                    ComputedNode {
                        size: Vec2::new(1200.0, 800.0),
                        ..Default::default()
                    },
                    ChildOf(tab),
                ))
                .id();
            app.update();
            Self {
                app,
                pane,
                space,
                window,
            }
        }

        fn receive(event: On<StackPlaced>, mut results: ResMut<Results>) {
            results
                .0
                .push((event.operation, event.stack, event.result.clone()));
        }

        fn place(
            &mut self,
            anchor: Entity,
            direction: Option<PaneDirection>,
            focus: bool,
        ) -> (Entity, Entity) {
            let stack = self
                .app
                .world_mut()
                .spawn((Stack::bundle(),))
                .insert(PageMetadata {
                    url: "https://example.com".into(),
                    ..Default::default()
                })
                .id();
            let operation = self
                .app
                .world_mut()
                .spawn(Operation::bundle("test.place-stack").unwrap())
                .id();
            self.app.world_mut().trigger(PlaceStackRequest {
                operation,
                stack,
                anchor,
                url: "https://example.com".into(),
                direction,
                automatic: false,
                primary_anchor: false,
                focus,
            });
            self.app.world_mut().flush();
            (operation, stack)
        }
    }

    #[test]
    fn same_frame_splits_read_the_preceding_canonical_children() {
        let mut fixture = Fixture::new();
        let original = fixture
            .app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(fixture.pane)))
            .insert(PageMetadata {
                url: "https://original.example".into(),
                ..Default::default()
            })
            .id();
        let (_, first) = fixture.place(fixture.pane, Some(PaneDirection::Right), false);
        let (_, second) = fixture.place(fixture.pane, Some(PaneDirection::Right), false);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<crate::pane::PaneSplit>(fixture.pane)
                .is_some()
        );
        let original_pane = fixture
            .app
            .world()
            .get::<ChildOf>(original)
            .unwrap()
            .parent();
        let first_pane = fixture.app.world().get::<ChildOf>(first).unwrap().parent();
        let second_pane = fixture.app.world().get::<ChildOf>(second).unwrap().parent();
        assert_ne!(original_pane, first_pane);
        assert_ne!(first_pane, second_pane);
        for pane in [original_pane, first_pane, second_pane] {
            assert_eq!(
                fixture.app.world().get::<ChildOf>(pane).unwrap().parent(),
                fixture.pane
            );
        }
        assert_eq!(fixture.app.world().resource::<Results>().0.len(), 2);
    }

    #[test]
    fn canceled_or_replaced_scope_refuses_before_topology_mutation() {
        for replace in [false, true] {
            let mut fixture = Fixture::new();
            let (operation, stack) = fixture.place(fixture.pane, Some(PaneDirection::Right), true);
            if replace {
                let other = fixture.app.world_mut().spawn(crate::space::Space).id();
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.pane)
                    .insert(ChildOf(other));
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(operation)
                    .insert(OperationState::Canceled);
            }
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<crate::pane::PaneSplit>(fixture.pane)
                    .is_none()
            );
            assert!(fixture.app.world().get::<ChildOf>(stack).is_none());
            assert!(fixture.app.world().resource::<Results>().0[0].2.is_err());
        }
    }

    #[test]
    fn focus_false_preserves_existing_activity_on_current_stack_and_pane() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.pane)
            .insert(LastActivatedAt(7));
        let (_, stack) = fixture.place(fixture.pane, None, false);
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<ChildOf>(stack).unwrap().parent(),
            fixture.pane
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LastActivatedAt>(fixture.pane)
                .unwrap()
                .0,
            7
        );
    }

    #[test]
    fn late_window_retirement_does_not_allocate_a_split() {
        let mut fixture = Fixture::new();
        fixture.place(fixture.pane, Some(PaneDirection::Right), false);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.space)
            .remove::<ChildOf>();
        fixture.app.world_mut().despawn(fixture.window);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<crate::pane::PaneSplit>(fixture.pane)
                .is_none()
        );
        assert!(fixture.app.world().resource::<Results>().0[0].2.is_err());
    }

    #[test]
    fn a_transferred_stack_is_preserved_and_refused_before_split() {
        let mut fixture = Fixture::new();
        let (_, stack) = fixture.place(fixture.pane, Some(PaneDirection::Right), false);
        let tab = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.pane)
            .unwrap()
            .parent();
        let destination = fixture
            .app
            .world_mut()
            .spawn((Pane::bundle(), ChildOf(tab)))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(stack)
            .insert(ChildOf(destination));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<crate::pane::PaneSplit>(fixture.pane)
                .is_none()
        );
        assert_eq!(
            fixture.app.world().get::<ChildOf>(stack).unwrap().parent(),
            destination
        );
        assert!(fixture.app.world().resource::<Results>().0[0].2.is_err());
        fixture.app.world_mut().write_message(OpenBesideRequest {
            pane: fixture.pane,
            direction: None,
            url: "https://example.com".into(),
            request_id: [0; 16],
            focus: false,
        });
        fixture.app.update();
        let world = fixture.app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<Stack>>()
                .iter(world)
                .count(),
            1
        );
    }

    #[test]
    fn a_focused_new_split_activates_the_owning_tab_in_the_input_frame() {
        let mut fixture = Fixture::new();
        fixture.app.add_plugins(crate::host::active::ActivePlugin);
        let tab = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.pane)
            .unwrap()
            .parent();
        let (_, stack) = fixture.place(fixture.pane, Some(PaneDirection::Right), true);
        fixture.app.update();
        let pane = fixture.app.world().get::<ChildOf>(stack).unwrap().parent();
        for entity in [stack, pane, tab] {
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::Active>(entity)
                    .is_some()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<LastActivatedAt>(entity)
                    .is_some_and(|activated| activated.0 > 0)
            );
        }
    }
}
