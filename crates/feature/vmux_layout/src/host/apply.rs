use std::collections::HashMap;

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use vmux_ecs::{OperationTarget, OperationTargetRevision};
use vmux_ecs::{Order, PageOpenRequest};

use crate::TerminalLayoutSpawnRequest;
use crate::protocol::{LayoutSnapshot, NodeKind};
use crate::snapshot::LayoutSnapshotQuery;

use super::archive::TabArchiveLayout;
use current::CurrentLayout;
use mutation::Topology;

mod current;
mod intent;
mod mutation;

pub(super) struct LayoutApplyPlugin;

impl Plugin for LayoutApplyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LayoutApplyRequest>()
            .add_message::<LayoutApplyResponse>()
            .add_message::<LayoutApplyCancelRequest>()
            .add_message::<LayoutSnapshotRequest>()
            .add_message::<LayoutSnapshotResponse>()
            .add_message::<TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(Update, accept.in_set(Accept))
            .add_systems(Update, cancel.after(Accept).before(Commit))
            .add_systems(Update, commit.in_set(Commit).after(Accept))
            .add_systems(
                Update,
                respond
                    .in_set(Respond)
                    .after(Commit)
                    .after(crate::stack::ComputeFocusSet),
            )
            .add_systems(Update, serve_snapshot_requests);
        app.configure_sets(
            Update,
            Commit.before(crate::active::ActiveSystemSet::Descendants),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Accept;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Commit;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Respond;

#[derive(Message, Clone)]
pub struct LayoutApplyRequest {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

#[derive(Message, Clone)]
pub struct LayoutApplyResponse {
    pub request_id: [u8; 16],
    pub result: Result<LayoutSnapshot, String>,
}

#[derive(Message, Clone, Copy)]
pub struct LayoutApplyCancelRequest {
    pub request_id: [u8; 16],
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotRequest {
    pub request_id: [u8; 16],
    pub anchor: Option<vmux_ecs::ProcessId>,
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotResponse {
    pub request_id: [u8; 16],
    pub snapshot: LayoutSnapshot,
}

#[derive(Component, Clone)]
struct Reconcile {
    request_id: [u8; 16],
    snapshot: LayoutSnapshot,
}

#[derive(Component)]
struct Completed(Result<(), String>);

fn accept(
    mut reader: MessageReader<LayoutApplyRequest>,
    layout: CurrentLayout,
    mut commands: Commands,
) {
    for (index, request) in reader.read().enumerate() {
        let intent = Reconcile {
            request_id: request.request_id,
            snapshot: request.snapshot.clone(),
        };
        let scope = layout.scope();
        let mut operation = commands.spawn((intent, Order(index as u32)));
        match scope {
            Ok(scope) => {
                operation.insert(OperationTargetRevision(layout.revision(scope)));
                if let Some(scope) = scope {
                    operation.insert(OperationTarget::new(scope));
                }
            }
            Err(error) => {
                operation.insert(Completed(Err(error)));
            }
        }
    }
}

fn cancel(
    mut requests: MessageReader<LayoutApplyCancelRequest>,
    operations: Query<(Entity, &Reconcile), Without<Completed>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, intent) in &operations {
            if intent.request_id == request.request_id {
                commands
                    .entity(entity)
                    .insert(Completed(Err("update_layout: canceled".into())));
            }
        }
    }
}

fn commit(
    world: &mut World,
    operations: &mut QueryState<(Entity, &Order), (With<Reconcile>, Without<Completed>)>,
    params: &mut SystemState<(CurrentLayout, TabArchiveLayout, Topology)>,
) {
    let mut pending = Vec::new();
    for (entity, order) in operations.iter(world) {
        pending.push((order.0, entity));
    }
    pending.sort_by_key(|(order, _)| *order);
    for (_, entity) in pending {
        let intent = world.get::<Reconcile>(entity).unwrap().clone();
        let scope = world
            .get::<OperationTarget>(entity)
            .map(OperationTarget::entity);
        let revision = *world.get::<OperationTargetRevision>(entity).unwrap();
        let (validated, previous) = {
            let (layout, _, _) = params.get_mut(world).unwrap();
            (
                layout.preflight(&intent, scope, revision),
                layout.entities(scope),
            )
        };
        let (retained, roots) = match validated {
            Ok(retained) => retained,
            Err(error) => {
                world.entity_mut(entity).insert(Completed(Err(error)));
                continue;
            }
        };
        {
            let (layout, archive, mut topology) = params.get_mut(world).unwrap();
            for &target in &previous {
                if !retained.contains(&target) && layout.is_stack(target) {
                    archive.capture_stack(target, &mut topology.commands);
                }
            }
        }
        params.apply(world);
        world.flush();
        for &target in &retained {
            world.entity_mut(target).remove::<ChildOf>();
        }
        let mut active = None;
        {
            let (_, _, mut topology) = params.get_mut(world).unwrap();
            let window = topology.window(scope);
            let mut tabs = Vec::new();
            for tab in &intent.snapshot.tabs {
                let target = topology.tab(tab, scope);
                let (root, zoomed, hidden) =
                    topology.node(&tab.root, target, roots.get(&target).copied(), true, window);
                topology.children(target, &[root]);
                topology.zoom(target, zoomed, hidden);
                tabs.push(target);
                if tab.is_active {
                    active = Some(target);
                }
            }
            if let Some(scope) = scope {
                topology.children(scope, &tabs);
            }
        }
        params.apply(world);
        world.flush();
        for target in previous.into_iter().rev() {
            if !retained.contains(&target) && world.get_entity(target).is_ok() {
                world.despawn(target);
            }
        }
        if let Some(active) = active {
            world.trigger(vmux_ecs::ActivateRequest { entity: active });
        }
        {
            let (_, _, mut topology) = params.get_mut(world).unwrap();
            topology.focus(&intent.snapshot.focused);
        }
        params.apply(world);
        world.flush();
        world.entity_mut(entity).insert(Completed(Ok(())));
    }
}

fn respond(
    operations: Query<(Entity, &Reconcile, &Completed)>,
    snapshot_query: LayoutSnapshotQuery,
    focused: crate::stack::FocusedStack,
    mut writer: MessageWriter<LayoutApplyResponse>,
    mut commands: Commands,
) {
    for (entity, intent, completed) in &operations {
        let result = match &completed.0 {
            Ok(()) => Ok(snapshot_query.build(&focused, None)),
            Err(error) => Err(error.clone()),
        };
        writer.write(LayoutApplyResponse {
            request_id: intent.request_id,
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn serve_snapshot_requests(
    mut reader: MessageReader<LayoutSnapshotRequest>,
    snapshot_query: LayoutSnapshotQuery,
    focused: crate::stack::FocusedStack,
    process_ids: Query<(&vmux_ecs::ProcessId, &ChildOf)>,
    space_hierarchy: crate::space::SpaceHierarchy,
    active_space_q: Query<Entity, (With<crate::space::Space>, With<vmux_ecs::Active>)>,
    mut writer: MessageWriter<LayoutSnapshotResponse>,
) {
    let pid_by_stack: HashMap<u64, String> = process_ids
        .iter()
        .map(|(pid, co)| (co.parent().to_bits(), pid.to_string()))
        .collect();
    for request in reader.read() {
        let self_stack = request.anchor.and_then(|anchor| {
            process_ids
                .iter()
                .find(|(pid, _)| **pid == anchor)
                .map(|(_, co)| co.parent())
        });
        let target_space = self_stack
            .and_then(|stack| space_hierarchy.get(stack))
            .or_else(|| active_space_q.iter().next());
        let mut snapshot = snapshot_query.build(&focused, self_stack);
        if let Some(target) = target_space {
            snapshot.tabs.retain(|tab| {
                tab.id
                    .as_deref()
                    .and_then(|id| NodeKind::parse_id(id).ok())
                    .map(|(_, bits)| space_hierarchy.get(Entity::from_bits(bits)) == Some(target))
                    .unwrap_or(true)
            });
        }
        for tab in &mut snapshot.tabs {
            snapshot_query.fill_process_ids(&mut tab.root, &pid_by_stack);
        }
        writer.write(LayoutSnapshotResponse {
            request_id: request.request_id,
            snapshot,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Focus, LayoutNode, SplitDirection, Stack as StackDto, Tab as TabDto};
    use bevy_app::{App, Plugin, Update};
    use bevy_ecs::prelude::{ChildOf, Children, Commands, Component, Entity, On, Query};

    use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection};
    use crate::protocol as proto;
    use crate::stack::Stack;
    use crate::tab::Tab as LayoutTab;
    use vmux_flex::prelude::{FlexDirection, Node};
    use vmux_history::LastActivatedAt;

    use bevy::ecs::message::Messages;

    struct ApplyHarness {
        app: App,
        space: Entity,
        tab: Entity,
        pane: Entity,
        stack: Entity,
    }

    impl ApplyHarness {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            Self::install(&mut app);
            let space = app
                .world_mut()
                .spawn((crate::space::Space, vmux_ecs::Active))
                .id();
            let tab = app
                .world_mut()
                .spawn((
                    LayoutTab {
                        name: "Work".into(),
                        startup_dir: None,
                    },
                    ChildOf(space),
                ))
                .id();
            let pane = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();
            let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
            Self {
                app,
                space,
                tab,
                pane,
                stack,
            }
        }

        fn intent(&self) -> LayoutSnapshot {
            LayoutSnapshot {
                tabs: vec![proto::Tab {
                    id: Some(NodeKind::Tab.id(self.tab.to_bits())),
                    name: "Work".into(),
                    is_active: false,
                    root: ApplyHarness::pane(
                        Some(&NodeKind::Pane.id(self.pane.to_bits())),
                        vec![StackDto {
                            id: Some(NodeKind::Stack.id(self.stack.to_bits())),
                            ..default()
                        }],
                    ),
                }],
                focused: default(),
            }
        }

        fn submit(&mut self, request_id: [u8; 16], snapshot: LayoutSnapshot) {
            self.app.world_mut().write_message(LayoutApplyRequest {
                request_id,
                snapshot,
            });
        }

        fn response(&self, request_id: [u8; 16]) -> Result<LayoutSnapshot, String> {
            let messages = self.app.world().resource::<Messages<LayoutApplyResponse>>();
            messages
                .get_cursor()
                .read(messages)
                .find(|response| response.request_id == request_id)
                .expect("correlated layout result")
                .result
                .clone()
        }

        fn pane(id: Option<&str>, stacks: Vec<StackDto>) -> LayoutNode {
            LayoutNode::Pane {
                id: id.map(str::to_string),
                is_zoomed: false,
                stacks,
            }
        }

        fn split(id: Option<&str>, children: Vec<LayoutNode>, weights: Vec<f32>) -> LayoutNode {
            LayoutNode::Split {
                id: id.map(str::to_string),
                direction: SplitDirection::Row,
                flex_weights: weights,
                children,
            }
        }

        fn snapshot(root: LayoutNode, focus: Focus) -> LayoutSnapshot {
            LayoutSnapshot {
                tabs: vec![TabDto {
                    id: Some("tab:1".into()),
                    name: "S".into(),
                    is_active: true,
                    root,
                }],
                focused: focus,
            }
        }

        fn install(app: &mut App) {
            app.add_message::<LayoutApplyRequest>()
                .add_message::<LayoutApplyResponse>()
                .add_message::<LayoutSnapshotRequest>()
                .add_message::<LayoutSnapshotResponse>()
                .add_message::<crate::TerminalLayoutSpawnRequest>()
                .add_message::<PageOpenRequest>()
                .add_plugins(vmux_ecs::PrimitivesPlugin)
                .add_plugins(LayoutApplyPlugin);
            app.world_mut()
                .spawn(crate::active_pane::ActiveStack::default().local_bundle());
        }

        fn apply(app: &mut App, snapshot: LayoutSnapshot) -> Result<LayoutSnapshot, String> {
            if !app
                .world()
                .contains_resource::<Messages<LayoutApplyCancelRequest>>()
            {
                Self::install(app);
            }
            let request_id = [42; 16];
            app.world_mut()
                .resource_mut::<Messages<LayoutApplyRequest>>()
                .write(LayoutApplyRequest {
                    request_id,
                    snapshot,
                });
            app.update();
            let responses = app.world().resource::<Messages<LayoutApplyResponse>>();
            let mut cursor = responses.get_cursor();
            cursor
                .read(responses)
                .find(|response| response.request_id == request_id)
                .expect("layout apply response")
                .result
                .clone()
        }
    }

    #[test]
    fn apply_closes_are_scoped_to_active_space() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space_a = app
            .world_mut()
            .spawn((crate::space::Space, vmux_ecs::Active))
            .id();
        let space_b = app.world_mut().spawn(crate::space::Space).id();
        let tab_a = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), ChildOf(space_a)))
            .id();
        let pane_a = app.world_mut().spawn((Pane, ChildOf(tab_a))).id();
        let tab_b = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), ChildOf(space_b)))
            .id();
        let pane_b = app.world_mut().spawn((Pane, ChildOf(tab_b))).id();
        let snapshot = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab_a.to_bits())),
                name: String::new(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane_a.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snapshot).unwrap();

        assert!(app.world().get_entity(tab_b).is_ok());
        assert!(app.world().get_entity(pane_b).is_ok());
    }

    #[test]
    fn validate_accepts_minimal_existing_layout() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        );
        assert!(
            Reconcile {
                request_id: [0; 16],
                snapshot: snap
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn validate_rejects_duplicate_pane_id() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::split(
                Some("split:4"),
                vec![
                    ApplyHarness::pane(Some("pane:2"), vec![]),
                    ApplyHarness::pane(Some("pane:2"), vec![]),
                ],
                vec![1.0, 1.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("DuplicateId")
        ));
    }

    #[test]
    fn validate_rejects_new_pane_without_tabs() {
        let snap = ApplyHarness::snapshot(ApplyHarness::pane(None, vec![]), Focus::default());
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("NewPaneMissingStacks")
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_url() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::pane(
                None,
                vec![StackDto {
                    id: None,
                    url: String::new(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("NewStackMissingUrl")
        ));
    }

    #[test]
    fn validate_rejects_new_tab_without_kind() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::pane(
                None,
                vec![StackDto {
                    id: None,
                    url: "https://x".into(),
                    kind: String::new(),
                    ..Default::default()
                }],
            ),
            Focus::default(),
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("NewStackMissingKind")
        ));
    }

    #[test]
    fn validate_rejects_focus_to_unknown_id() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::pane(
                Some("pane:2"),
                vec![StackDto {
                    id: Some("stack:3".into()),
                    ..Default::default()
                }],
            ),
            Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:99".into()),
                stack: None,
            },
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("FocusReferencesUnknownId")
        ));
    }

    #[test]
    fn validate_rejects_wrong_kind_in_position() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::pane(Some("stack:2"), vec![]),
            Focus::default(),
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("WrongKindForPosition")
        ));
    }

    #[test]
    fn validate_rejects_flex_weights_length_mismatch() {
        let snap = ApplyHarness::snapshot(
            ApplyHarness::split(
                Some("split:4"),
                vec![ApplyHarness::pane(
                    Some("pane:2"),
                    vec![StackDto {
                        id: Some("stack:3".into()),
                        ..Default::default()
                    }],
                )],
                vec![1.0, 2.0],
            ),
            Focus::default(),
        );
        assert!(matches!(
            Reconcile { request_id: [0; 16], snapshot: snap }.validate(),
            Err(error) if error.contains("FlexWeightsLengthMismatch")
        ));
    }

    #[test]
    fn updating_split_direction_changes_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let _pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let _pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(NodeKind::Split.id(split_e.to_bits())),
                    direction: proto::SplitDirection::Column,
                    flex_weights: vec![],
                    children: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        let updated = app.world().get::<PaneSplit>(split_e).unwrap();
        assert_eq!(updated.direction, PaneSplitDirection::Column);
    }

    #[test]
    fn updating_flex_weights_writes_pane_size_flex_grow() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane, PaneSize { flex_grow: 1.0 }, ChildOf(split_e)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(NodeKind::Split.id(split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![3.0, 1.0],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_a.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_b.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        assert_eq!(app.world().get::<PaneSize>(pane_a).unwrap().flex_grow, 3.0);
        assert_eq!(app.world().get::<PaneSize>(pane_b).unwrap().flex_grow, 1.0);
    }

    #[test]
    fn moves_pane_to_new_parent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let split_a = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let split_b = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let moved = app.world_mut().spawn((Pane, ChildOf(split_a))).id();
        let _filler_b = app.world_mut().spawn((Pane, ChildOf(split_b))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(NodeKind::Split.id(split_a.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![proto::LayoutNode::Split {
                        id: Some(NodeKind::Split.id(split_b.to_bits())),
                        direction: proto::SplitDirection::Row,
                        flex_weights: vec![],
                        children: vec![proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(moved.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        }],
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        let parent = app.world().get::<ChildOf>(moved).map(|p| p.parent());
        assert_eq!(parent, Some(split_b));
    }

    #[test]
    fn moves_stack_to_new_tab_reparents_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(NodeKind::Tab.id(tab.to_bits())),
                    name: "S".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: Some(NodeKind::Pane.id(pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "YouTube".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: Some(NodeKind::Stack.id(stack.to_bits())),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let s_parent = app
            .world()
            .get::<ChildOf>(stack)
            .map(|p| p.parent())
            .expect("moved stack still has a parent");
        assert_ne!(
            s_parent, pane,
            "stack should be reparented out of the original pane"
        );
        let s_grandparent = app.world().get::<ChildOf>(s_parent).map(|p| p.parent());
        assert!(
            s_grandparent.is_some() && s_grandparent != Some(tab),
            "moved stack's pane should live under the new tab, not the original"
        );
    }

    #[test]
    fn snapshot_active_tab_becomes_most_recently_activated() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let active_tab = app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "A".into(),
                    startup_dir: None,
                },
                LastActivatedAt(1),
            ))
            .id();
        let active_pane = app.world_mut().spawn((Pane, ChildOf(active_tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(NodeKind::Tab.id(active_tab.to_bits())),
                    name: "A".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: Some(NodeKind::Pane.id(active_pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "New".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, &LastActivatedAt), With<LayoutTab>>();
        let ts: Vec<(Entity, i64)> = q.iter(app.world()).map(|(e, l)| (e, l.0)).collect();
        let active_ts = ts
            .iter()
            .find(|(e, _)| *e == active_tab)
            .map(|(_, t)| *t)
            .expect("active tab has a timestamp");
        let max_other = ts
            .iter()
            .filter(|(e, _)| *e != active_tab)
            .map(|(_, t)| *t)
            .max()
            .expect("a new tab exists");
        assert!(
            active_ts > max_other,
            "is_active tab ({active_ts}) must out-rank other tabs ({max_other})"
        );
    }

    #[test]
    fn new_tab_parented_as_sibling_of_existing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let main = app.world_mut().spawn_empty().id();
        let tab = app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "A".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![
                proto::Tab {
                    id: Some(NodeKind::Tab.id(tab.to_bits())),
                    name: "A".into(),
                    is_active: true,
                    root: proto::LayoutNode::Pane {
                        id: Some(NodeKind::Pane.id(pane.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                proto::Tab {
                    id: None,
                    name: "New".into(),
                    is_active: false,
                    root: proto::LayoutNode::Pane {
                        id: None,
                        is_zoomed: false,
                        stacks: vec![proto::Stack {
                            id: None,
                            url: "https://example.com".into(),
                            kind: "browser".into(),
                            ..Default::default()
                        }],
                    },
                },
            ],
            focused: proto::Focus::default(),
        };
        ApplyHarness::apply(&mut app, snap).unwrap();

        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, Option<&ChildOf>), With<LayoutTab>>();
        let parent = q
            .iter(app.world())
            .find(|(e, _)| *e != tab)
            .and_then(|(_, c)| c.map(|c| c.parent()))
            .expect("new tab exists with a parent");
        assert_eq!(
            parent, main,
            "new tab should be a sibling of the existing tab (same parent)"
        );
    }

    #[test]
    fn omitting_pane_from_snapshot_closes_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let keep = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let drop_me = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(NodeKind::Split.id(split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![proto::LayoutNode::Pane {
                        id: Some(NodeKind::Pane.id(keep.to_bits())),
                        is_zoomed: false,
                        stacks: vec![],
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        assert!(
            app.world().get_entity(drop_me).is_err(),
            "drop_me should be despawned"
        );
        assert!(app.world().get_entity(keep).is_ok(), "keep should survive");
    }

    #[test]
    fn apply_returns_error_for_stale_tab_id_does_not_panic() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let dead_stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();
        app.world_mut().entity_mut(dead_stack).despawn();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(NodeKind::Stack.id(dead_stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        let result = ApplyHarness::apply(&mut app, snap);
        assert!(matches!(result, Err(error) if error.contains("MissingReferencedEntity")));
    }

    #[test]
    fn submitting_new_tab_id_none_spawns_stack_entity() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: None,
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        let stack_count = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(stack_count, 1, "one new Stack entity should be spawned");
    }

    #[test]
    fn malformed_pane_id_skips_subtree_no_orphan_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();

        let pane_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();

        let bad_node = proto::LayoutNode::Pane {
            id: Some("pane:not_a_number".into()),
            is_zoomed: false,
            stacks: vec![proto::Stack {
                id: None,
                url: "https://example.com".into(),
                kind: "browser".into(),
                ..Default::default()
            }],
        };
        let _ = ApplyHarness::apply(
            &mut app,
            LayoutSnapshot {
                tabs: vec![TabDto {
                    id: Some(NodeKind::Tab.id(tab.to_bits())),
                    name: "S".into(),
                    is_active: true,
                    root: bad_node,
                }],
                focused: Default::default(),
            },
        );

        let pane_count_after = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            pane_count_before, pane_count_after,
            "malformed id must not spawn orphan pane"
        );

        let stack_count = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(stack_count, 0, "stacks under malformed pane must not spawn");
    }

    #[test]
    fn malformed_split_id_skips_subtree_no_orphan_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();

        let split_count_before = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();

        let bad_node = proto::LayoutNode::Split {
            id: Some("split:garbage".into()),
            direction: proto::SplitDirection::Row,
            flex_weights: vec![],
            children: vec![proto::LayoutNode::Pane {
                id: None,
                is_zoomed: false,
                stacks: vec![],
            }],
        };
        let _ = ApplyHarness::apply(
            &mut app,
            LayoutSnapshot {
                tabs: vec![TabDto {
                    id: Some(NodeKind::Tab.id(tab.to_bits())),
                    name: "S".into(),
                    is_active: true,
                    root: bad_node,
                }],
                focused: Default::default(),
            },
        );

        let split_count_after = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            split_count_before, split_count_after,
            "malformed id must not spawn orphan split"
        );

        let pane_count = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            pane_count, 0,
            "children under malformed split must not spawn"
        );
    }

    #[test]
    fn reordering_split_children_swaps_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let split_e = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let pane_a = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let pane_b = app.world_mut().spawn((Pane, ChildOf(split_e))).id();
        let pane_c = app.world_mut().spawn((Pane, ChildOf(split_e))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: Some(NodeKind::Split.id(split_e.to_bits())),
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_c.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_a.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_b.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        let children = app
            .world()
            .get::<Children>(split_e)
            .expect("split has Children");
        let order: Vec<Entity> = children.iter().collect();
        assert_eq!(
            order,
            vec![pane_c, pane_a, pane_b],
            "Children should match submitted order"
        );
    }

    #[test]
    fn focus_change_updates_the_canonical_selection_path() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(NodeKind::Stack.id(stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus {
                tab: Some(NodeKind::Tab.id(tab.to_bits())),
                pane: Some(NodeKind::Pane.id(pane_e.to_bits())),
                stack: Some(NodeKind::Stack.id(stack.to_bits())),
            },
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        let tab_time = app.world().get::<LastActivatedAt>(tab).unwrap();
        let pane_time = app.world().get::<LastActivatedAt>(pane_e).unwrap();
        let stack_time = app.world().get::<LastActivatedAt>(stack).unwrap();
        assert_eq!(tab_time.0, pane_time.0);
        assert_eq!(pane_time.0, stack_time.0);
    }

    #[test]
    fn apply_response_reports_the_canonical_focus_after_reduction() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        ApplyHarness::install(&mut app);
        app.add_message::<crate::tab::CloseTabRequest>()
            .add_plugins((crate::space::SpaceLayoutPlugin, crate::stack::StackPlugin));
        let space = app
            .world_mut()
            .spawn((
                crate::space::Space,
                crate::space::CurrentSpace,
                vmux_ecs::Active,
            ))
            .id();
        let first_tab = app
            .world_mut()
            .spawn((
                LayoutTab::default(),
                LastActivatedAt(2),
                vmux_ecs::Active,
                ChildOf(space),
            ))
            .id();
        let first_pane = app
            .world_mut()
            .spawn((Pane::bundle(), LastActivatedAt(2), ChildOf(first_tab)))
            .id();
        let first_stack = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                LastActivatedAt(2),
                vmux_ecs::Active,
                ChildOf(first_pane),
            ))
            .id();
        let second_tab = app
            .world_mut()
            .spawn((LayoutTab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let second_pane = app
            .world_mut()
            .spawn((Pane::bundle(), LastActivatedAt(1), ChildOf(second_tab)))
            .id();
        let second_stack = app
            .world_mut()
            .spawn((Stack::bundle(), LastActivatedAt(1), ChildOf(second_pane)))
            .id();
        let first = proto::Tab {
            id: Some(NodeKind::Tab.id(first_tab.to_bits())),
            name: String::new(),
            is_active: false,
            root: proto::LayoutNode::Pane {
                id: Some(NodeKind::Pane.id(first_pane.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: Some(NodeKind::Stack.id(first_stack.to_bits())),
                    ..default()
                }],
            },
        };
        let second = proto::Tab {
            id: Some(NodeKind::Tab.id(second_tab.to_bits())),
            name: String::new(),
            is_active: true,
            root: proto::LayoutNode::Pane {
                id: Some(NodeKind::Pane.id(second_pane.to_bits())),
                is_zoomed: false,
                stacks: vec![proto::Stack {
                    id: Some(NodeKind::Stack.id(second_stack.to_bits())),
                    ..default()
                }],
            },
        };
        let requested = proto::Focus {
            tab: Some(NodeKind::Tab.id(second_tab.to_bits())),
            pane: Some(NodeKind::Pane.id(second_pane.to_bits())),
            stack: Some(NodeKind::Stack.id(second_stack.to_bits())),
        };
        let response = ApplyHarness::apply(
            &mut app,
            LayoutSnapshot {
                tabs: vec![first, second],
                focused: requested.clone(),
            },
        )
        .unwrap();

        assert_eq!(response.focused, requested);
        let focused = app
            .world_mut()
            .query_filtered::<
                &crate::active_pane::ActiveStack,
                With<crate::active_pane::ProfileId>,
            >()
            .single(app.world())
            .unwrap();
        assert_eq!(focused.stack, Some(second_stack));
    }

    #[test]
    fn apply_focus_preserves_existing_when_dto_fields_omitted() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        ApplyHarness::install(&mut app);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(pane_e)))
            .id();

        {
            let world = app.world_mut();
            let mut query = world.query_filtered::<
                &mut crate::active_pane::ActiveStack,
                With<crate::active_pane::ProfileId>,
            >();
            let mut active = query.single_mut(world).unwrap();
            active.tab = Some(tab);
            active.pane = Some(pane_e);
            active.stack = Some(stack);
        }

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane_e.to_bits())),
                    is_zoomed: false,
                    stacks: vec![proto::Stack {
                        id: Some(NodeKind::Stack.id(stack.to_bits())),
                        ..Default::default()
                    }],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();
        let f = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<
                &crate::active_pane::ActiveStack,
                With<crate::active_pane::ProfileId>,
            >();
            *query.single(world).unwrap()
        };
        assert_eq!(f.tab, Some(tab), "focused.tab must be preserved");
        assert_eq!(f.pane, Some(pane_e), "focused.pane must be preserved");
        assert_eq!(f.stack, Some(stack), "focused.stack must be preserved");
    }

    #[test]
    fn new_split_inserts_node_with_flex_direction() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();
        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane_e = app.world_mut().spawn((Pane, ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(pane_e.to_bits())),
                            is_zoomed: false,
                            stacks: vec![],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        let split_count = app
            .world_mut()
            .query_filtered::<&Node, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .filter(|node| node.flex_direction == FlexDirection::Row)
            .count();
        assert!(
            split_count >= 1,
            "spawn_split should produce a Pane+PaneSplit with Node{{flex_direction: Row}}"
        );
    }

    #[test]
    fn new_split_wraps_existing_pane_without_converting_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let existing_pane = app
            .world_mut()
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(existing_pane.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(NodeKind::Stack.id(stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        assert!(
            app.world().get::<PaneSplit>(existing_pane).is_none(),
            "existing pane should stay a leaf"
        );

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        assert_eq!(splits.len(), 1, "exactly one new split entity should exist");
        let new_split = splits[0];

        let node = app.world().get::<Node>(new_split).unwrap();
        assert_eq!(node.flex_direction, FlexDirection::Row);

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(new_split)
            .expect("split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2, "split should have two leaf children");
        assert_eq!(
            children[0], existing_pane,
            "existing pane should be first per submitted order"
        );

        let stack_parent = app.world().get::<ChildOf>(stack).map(|p| p.parent());
        assert_eq!(
            stack_parent,
            Some(existing_pane),
            "existing stack should stay under existing pane"
        );
    }

    #[test]
    fn new_root_split_id_none_reuses_existing_root_split_of_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let existing_root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let existing_leaf = app
            .world_mut()
            .spawn((Pane::bundle(), ChildOf(existing_root)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_leaf)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(existing_leaf.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(NodeKind::Stack.id(stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        assert_eq!(
            splits,
            vec![existing_root],
            "should reuse existing root split, not spawn a new one"
        );

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(existing_root)
            .expect("root split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], existing_leaf);
    }

    #[test]
    fn serve_snapshot_requests_emits_response() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        ApplyHarness::install(&mut app);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let _ = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();

        app.world_mut()
            .resource_mut::<Messages<LayoutSnapshotRequest>>()
            .write(LayoutSnapshotRequest {
                request_id: [7; 16],
                anchor: None,
            });
        app.update();

        let responses = app.world().resource::<Messages<LayoutSnapshotResponse>>();
        let mut cursor = responses.get_cursor();
        let response = cursor
            .read(responses)
            .next()
            .expect("expected one response");
        assert_eq!(response.request_id, [7; 16]);
        assert_eq!(response.snapshot.tabs.len(), 1);
    }

    #[test]
    fn apply_layout_requests_emits_response_with_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let pane = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Pane {
                    id: Some(NodeKind::Pane.id(pane.to_bits())),
                    is_zoomed: false,
                    stacks: vec![],
                },
            }],
            focused: proto::Focus::default(),
        };

        let response = ApplyHarness::apply(&mut app, snap).unwrap();
        assert_eq!(response.tabs.len(), 1);
    }

    #[test]
    fn new_split_preserves_submitted_children_order_with_new_pane_first() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>();

        let tab = app
            .world_mut()
            .spawn(LayoutTab {
                name: "S".into(),
                startup_dir: None,
            })
            .id();
        let existing_pane = app
            .world_mut()
            .spawn((Pane::bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(existing_pane)))
            .id();

        let snap = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "S".into(),
                is_active: true,
                root: proto::LayoutNode::Split {
                    id: None,
                    direction: proto::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        proto::LayoutNode::Pane {
                            id: None,
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: None,
                                url: "https://example.com".into(),
                                kind: "browser".into(),
                                ..Default::default()
                            }],
                        },
                        proto::LayoutNode::Pane {
                            id: Some(NodeKind::Pane.id(existing_pane.to_bits())),
                            is_zoomed: false,
                            stacks: vec![proto::Stack {
                                id: Some(NodeKind::Stack.id(stack.to_bits())),
                                ..Default::default()
                            }],
                        },
                    ],
                },
            }],
            focused: proto::Focus::default(),
        };

        ApplyHarness::apply(&mut app, snap).unwrap();

        let splits: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>()
            .iter(app.world())
            .collect();
        let new_split = splits[0];
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(new_split)
            .expect("split has children")
            .iter()
            .collect();
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[1], existing_pane,
            "existing pane should be second per submitted order"
        );
    }
    #[derive(Component)]
    struct UnsavedText(String);

    fn transfer_text(
        request: On<vmux_ecs::page::ArchiveCaptureRequest>,
        children: Query<&Children>,
        buffers: Query<&UnsavedText>,
        archives: Query<&vmux_ecs::ArchivedPage>,
        mut commands: Commands,
    ) {
        let Ok(archive) = archives.get(request.archive) else {
            return;
        };
        if !archive.url.starts_with("file:") {
            return;
        }
        if let Ok(children) = children.get(request.source) {
            for child in children.iter() {
                if buffers.get(child).is_ok() {
                    commands.entity(child).insert(ChildOf(request.archive));
                }
            }
        }
    }

    #[test]
    fn competing_intents_commit_in_admission_order_and_refuse_stale_revision() {
        let mut fixture = ApplyHarness::new();
        let mut first = fixture.intent();
        first.tabs[0].name = "First".into();
        let mut second = fixture.intent();
        second.tabs[0].name = "Second".into();
        fixture.submit([1; 16], first);
        fixture.submit([2; 16], second);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        assert!(fixture.response([2; 16]).unwrap_err().contains("revision"));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "First"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }

    #[test]
    fn cancellation_precedes_mutation_and_does_not_open_content() {
        let mut fixture = ApplyHarness::new();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Canceled".into();
        snapshot.tabs.push(proto::Tab {
            id: None,
            name: "New".into(),
            is_active: false,
            root: ApplyHarness::pane(
                None,
                vec![StackDto {
                    url: "https://example.com".into(),
                    kind: "browser".into(),
                    ..default()
                }],
            ),
        });
        fixture.submit([1; 16], snapshot);
        fixture
            .app
            .world_mut()
            .write_message(LayoutApplyCancelRequest {
                request_id: [1; 16],
            });
        fixture.app.update();
        assert!(fixture.response([1; 16]).unwrap_err().contains("canceled"));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<PageOpenRequest>>()
                .is_empty()
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<LayoutTab>>()
                .iter(fixture.app.world())
                .count(),
            1
        );
    }

    #[test]
    fn wrong_live_entity_kind_refuses_the_whole_intent() {
        let mut fixture = ApplyHarness::new();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Refused".into();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(fixture.stack.to_bits())), vec![]);
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(
            fixture
                .response([1; 16])
                .unwrap_err()
                .contains("WrongKindForPosition")
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }

    #[test]
    fn foreign_space_identity_refuses_without_touching_either_space() {
        let mut fixture = ApplyHarness::new();
        let foreign = fixture.app.world_mut().spawn(crate::space::Space).id();
        let tab = fixture
            .app
            .world_mut()
            .spawn((LayoutTab::default(), ChildOf(foreign)))
            .id();
        let leaf = fixture
            .app
            .world_mut()
            .spawn((Pane::bundle(), ChildOf(tab)))
            .id();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Refused".into();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(leaf.to_bits())), vec![]);
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_err());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture.app.world().get::<ChildOf>(leaf).unwrap().parent(),
            tab
        );
        assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
    }

    #[test]
    fn protected_descendant_refuses_closure_before_rename_or_archive() {
        let mut fixture = ApplyHarness::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.stack)
            .insert(vmux_ecs::PageMetadata {
                url: "file:///work.txt".into(),
                ..default()
            });
        let buffer = fixture
            .app
            .world_mut()
            .spawn((
                crate::CloseRequiresConfirmation,
                UnsavedText("unsaved".into()),
                ChildOf(fixture.stack),
            ))
            .id();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Refused".into();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(fixture.pane.to_bits())), vec![]);
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(
            fixture
                .response([1; 16])
                .unwrap_err()
                .contains("confirmation")
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture.app.world().get::<UnsavedText>(buffer).unwrap().0,
            "unsaved"
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&vmux_ecs::ArchivedPage>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn deleting_an_omitted_page_flushes_archive_transfer_before_despawn() {
        let mut fixture = ApplyHarness::new();
        fixture.app.add_plugins(ReconciliationFixturePlugin);
        let metadata = vmux_ecs::PageMetadata {
            url: "file:///work.txt".into(),
            title: "Work".into(),
            ..default()
        };
        let surface = fixture
            .app
            .world_mut()
            .spawn((crate::Browser, metadata.clone(), ChildOf(fixture.stack)))
            .id();
        let buffer = fixture
            .app
            .world_mut()
            .spawn((UnsavedText("unsaved bytes".into()), ChildOf(surface)))
            .id();
        fixture.app.world_mut().entity_mut(fixture.stack).insert((
            vmux_ecs::PageMetadata {
                url: "file:///stale-stack.txt".into(),
                ..default()
            },
            crate::page_slot::Current::new(Some(surface), metadata),
        ));
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(fixture.pane.to_bits())), vec![]);
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        assert!(fixture.app.world().get_entity(fixture.stack).is_err());
        assert!(fixture.app.world().get_entity(surface).is_err());
        let owner = fixture.app.world().get::<ChildOf>(buffer).unwrap().parent();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::ArchivedPage>(owner)
                .unwrap()
                .url,
            "file:///work.txt"
        );
        assert_eq!(
            fixture.app.world().get::<UnsavedText>(buffer).unwrap().0,
            "unsaved bytes"
        );
    }

    #[test]
    fn moved_stack_preserves_unsaved_child_and_does_not_reopen_or_archive() {
        let mut fixture = ApplyHarness::new();
        let buffer = fixture
            .app
            .world_mut()
            .spawn((UnsavedText("unsaved bytes".into()), ChildOf(fixture.stack)))
            .id();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(fixture.pane.to_bits())), vec![]);
        snapshot.tabs.push(proto::Tab {
            id: None,
            name: "Moved".into(),
            is_active: true,
            root: ApplyHarness::pane(
                None,
                vec![StackDto {
                    id: Some(NodeKind::Stack.id(fixture.stack.to_bits())),
                    ..default()
                }],
            ),
        });
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        let pane = fixture
            .app
            .world()
            .get::<ChildOf>(fixture.stack)
            .unwrap()
            .parent();
        assert_ne!(pane, fixture.pane);
        let tab = fixture.app.world().get::<ChildOf>(pane).unwrap().parent();
        assert_eq!(
            fixture.app.world().get::<ChildOf>(tab).unwrap().parent(),
            fixture.space
        );
        assert_eq!(
            fixture.app.world().get::<ChildOf>(buffer).unwrap().parent(),
            fixture.stack
        );
        assert_eq!(
            fixture.app.world().get::<UnsavedText>(buffer).unwrap().0,
            "unsaved bytes"
        );
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<PageOpenRequest>>()
                .is_empty()
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&vmux_ecs::ArchivedPage>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn all_new_tabs_attach_to_captured_space_after_original_tab_is_removed() {
        let mut fixture = ApplyHarness::new();
        let snapshot = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: None,
                name: "New".into(),
                is_active: true,
                root: ApplyHarness::pane(
                    None,
                    vec![StackDto {
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..default()
                    }],
                ),
            }],
            focused: default(),
        };
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        assert!(fixture.app.world().get_entity(fixture.tab).is_err());
        let tabs: Vec<Entity> = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<LayoutTab>>()
            .iter(fixture.app.world())
            .collect();
        assert_eq!(tabs.len(), 1);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(tabs[0])
                .unwrap()
                .parent(),
            fixture.space
        );
    }

    #[test]
    fn invalid_entity_bits_and_numeric_id_aliases_refuse_without_panicking() {
        for bad in [
            "pane:4294967295".to_string(),
            NodeKind::Pane.id(Entity::PLACEHOLDER.to_bits()),
        ] {
            let mut fixture = ApplyHarness::new();
            let mut snapshot = fixture.intent();
            snapshot.tabs[0].root = ApplyHarness::pane(Some(&bad), vec![]);
            fixture.submit([1; 16], snapshot);
            fixture.app.update();
            assert!(fixture.response([1; 16]).is_err());
            assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
        }
        let mut fixture = ApplyHarness::new();
        let mut snapshot = fixture.intent();
        let id = NodeKind::Pane.id(fixture.pane.to_bits());
        let alias = format!("pane:0{}", fixture.pane.to_bits());
        snapshot.tabs[0].root = ApplyHarness::split(
            None,
            vec![
                ApplyHarness::pane(Some(&id), vec![]),
                ApplyHarness::pane(Some(&alias), vec![]),
            ],
            vec![],
        );
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(
            fixture
                .response([1; 16])
                .unwrap_err()
                .contains("DuplicateId")
        );
        assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
    }

    #[derive(Component)]
    struct ReparentAfterAccept(Entity);

    #[derive(Component)]
    struct RebindWindowAfterAccept(Entity);

    struct ReconciliationFixturePlugin;

    impl Plugin for ReconciliationFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(transfer_text).add_systems(
                Update,
                (reparent_after_accept, rebind_window_after_accept)
                    .after(Accept)
                    .before(Commit),
            );
        }
    }

    fn reparent_after_accept(
        changes: Query<(Entity, &ReparentAfterAccept)>,
        operations: Query<&Reconcile, Without<Completed>>,
        mut commands: Commands,
    ) {
        if operations.is_empty() {
            return;
        }
        for (entity, change) in &changes {
            commands.entity(entity).insert(ChildOf(change.0));
        }
    }

    fn rebind_window_after_accept(
        mut changes: Query<(&RebindWindowAfterAccept, &mut bevy_cef::prelude::HostWindow)>,
        operations: Query<&Reconcile, Without<Completed>>,
    ) {
        if operations.is_empty() {
            return;
        }
        for (change, mut host) in &mut changes {
            host.0 = change.0;
        }
    }

    #[test]
    fn topology_changed_after_acceptance_refuses_before_any_other_effect() {
        let mut fixture = ApplyHarness::new();
        fixture.app.add_plugins(ReconciliationFixturePlugin);
        let foreign = fixture.app.world_mut().spawn(crate::space::Space).id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.pane)
            .insert(ReparentAfterAccept(foreign));
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Refused".into();
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).unwrap_err().contains("revision"));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.pane)
                .unwrap()
                .parent(),
            foreign
        );
        assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<PageOpenRequest>>()
                .is_empty()
        );
    }

    #[test]
    fn reversing_existing_ancestor_order_detaches_retained_nodes_before_reparenting() {
        let mut fixture = ApplyHarness::new();
        let outer = fixture
            .app
            .world_mut()
            .spawn((
                Pane::split_bundle(PaneSplitDirection::Row),
                ChildOf(fixture.tab),
            ))
            .id();
        let inner = fixture
            .app
            .world_mut()
            .spawn((Pane::split_bundle(PaneSplitDirection::Row), ChildOf(outer)))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.pane)
            .insert(ChildOf(inner));
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root = ApplyHarness::split(
            Some(&NodeKind::Split.id(inner.to_bits())),
            vec![ApplyHarness::split(
                Some(&NodeKind::Split.id(outer.to_bits())),
                vec![ApplyHarness::pane(
                    Some(&NodeKind::Pane.id(fixture.pane.to_bits())),
                    vec![StackDto {
                        id: Some(NodeKind::Stack.id(fixture.stack.to_bits())),
                        ..default()
                    }],
                )],
                vec![],
            )],
            vec![],
        );
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        assert_eq!(
            fixture.app.world().get::<ChildOf>(inner).unwrap().parent(),
            fixture.tab
        );
        assert_eq!(
            fixture.app.world().get::<ChildOf>(outer).unwrap().parent(),
            inner
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.pane)
                .unwrap()
                .parent(),
            outer
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }

    #[test]
    fn a_new_pane_receives_its_requested_weight_and_zoom_without_losing_existing_content() {
        let mut fixture = ApplyHarness::new();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root = ApplyHarness::split(
            None,
            vec![
                LayoutNode::Pane {
                    id: None,
                    is_zoomed: true,
                    stacks: vec![StackDto {
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..default()
                    }],
                },
                ApplyHarness::pane(
                    Some(&NodeKind::Pane.id(fixture.pane.to_bits())),
                    vec![StackDto {
                        id: Some(NodeKind::Stack.id(fixture.stack.to_bits())),
                        ..default()
                    }],
                ),
            ],
            vec![3.0, 1.0],
        );
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        let root = fixture
            .app
            .world()
            .get::<Children>(fixture.tab)
            .unwrap()
            .iter()
            .next()
            .unwrap();
        let children: Vec<Entity> = fixture
            .app
            .world()
            .get::<Children>(root)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(children[1], fixture.pane);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::pane::PaneSize>(children[0])
                .unwrap()
                .flex_grow,
            3.0
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<crate::pane::Zoomed>(fixture.tab)
                .unwrap()
                .leaf,
            children[0]
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }
    #[test]
    fn response_reports_requested_tab_order_from_canonical_siblings() {
        let mut fixture = ApplyHarness::new();
        let second = fixture
            .app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "Second".into(),
                    startup_dir: None,
                },
                ChildOf(fixture.space),
            ))
            .id();
        let leaf = fixture
            .app
            .world_mut()
            .spawn((Pane::bundle(), ChildOf(second)))
            .id();
        let mut snapshot = fixture.intent();
        snapshot.tabs.insert(
            0,
            proto::Tab {
                id: Some(NodeKind::Tab.id(second.to_bits())),
                name: "Second".into(),
                is_active: false,
                root: ApplyHarness::pane(Some(&NodeKind::Pane.id(leaf.to_bits())), vec![]),
            },
        );
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        let response = fixture.response([1; 16]).unwrap();
        let expected = vec![
            Some(NodeKind::Tab.id(second.to_bits())),
            Some(NodeKind::Tab.id(fixture.tab.to_bits())),
        ];
        assert_eq!(
            response
                .tabs
                .into_iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn an_unsettled_page_without_a_capture_source_refuses_closure() {
        let mut fixture = ApplyHarness::new();
        let surface = fixture
            .app
            .world_mut()
            .spawn((crate::Browser, ChildOf(fixture.stack)))
            .id();
        let buffer = fixture
            .app
            .world_mut()
            .spawn((UnsavedText("unsaved bytes".into()), ChildOf(surface)))
            .id();
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root =
            ApplyHarness::pane(Some(&NodeKind::Pane.id(fixture.pane.to_bits())), vec![]);
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_err());
        assert_eq!(
            fixture.app.world().get::<UnsavedText>(buffer).unwrap().0,
            "unsaved bytes"
        );
        assert!(fixture.app.world().get_entity(fixture.stack).is_ok());
    }
    #[test]
    fn new_layout_roots_inherit_the_canonical_host_window() {
        let mut fixture = ApplyHarness::new();
        let window = fixture.app.world_mut().spawn_empty().id();
        let host = fixture
            .app
            .world_mut()
            .spawn(bevy_cef::prelude::HostWindow(window))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.space)
            .insert(ChildOf(host));
        let snapshot = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: None,
                name: "New".into(),
                is_active: false,
                root: ApplyHarness::pane(
                    None,
                    vec![StackDto {
                        url: "https://example.com".into(),
                        kind: "browser".into(),
                        ..default()
                    }],
                ),
            }],
            focused: default(),
        };
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        let tab = fixture
            .app
            .world()
            .get::<Children>(fixture.space)
            .unwrap()
            .iter()
            .next()
            .unwrap();
        let root = fixture
            .app
            .world()
            .get::<Children>(tab)
            .unwrap()
            .iter()
            .next()
            .unwrap();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<bevy_cef::prelude::HostWindow>(root)
                .unwrap()
                .0,
            window
        );
    }

    #[test]
    fn window_binding_changed_after_acceptance_refuses_before_mutation() {
        let mut fixture = ApplyHarness::new();
        fixture.app.add_plugins(ReconciliationFixturePlugin);
        let first = fixture.app.world_mut().spawn_empty().id();
        let second = fixture.app.world_mut().spawn_empty().id();
        let host = fixture
            .app
            .world_mut()
            .spawn((
                bevy_cef::prelude::HostWindow(first),
                RebindWindowAfterAccept(second),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.space)
            .insert(ChildOf(host));
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].name = "Refused".into();
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).unwrap_err().contains("revision"));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<bevy_cef::prelude::HostWindow>(host)
                .unwrap()
                .0,
            second
        );
    }
    #[test]
    fn multiple_active_spaces_use_the_canonical_focused_owner() {
        let mut fixture = ApplyHarness::new();
        let other = fixture
            .app
            .world_mut()
            .spawn((crate::space::Space, vmux_ecs::Active))
            .id();
        let tab = fixture
            .app
            .world_mut()
            .spawn((
                LayoutTab {
                    name: "Other".into(),
                    startup_dir: None,
                },
                ChildOf(other),
            ))
            .id();
        let leaf = fixture
            .app
            .world_mut()
            .spawn((Pane::bundle(), ChildOf(tab)))
            .id();
        let profile = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<crate::active_pane::ProfileId>>()
            .single(fixture.app.world())
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(profile)
            .insert(crate::active_pane::ActiveStack {
                tab: Some(tab),
                pane: Some(leaf),
                stack: None,
            });
        let snapshot = LayoutSnapshot {
            tabs: vec![proto::Tab {
                id: Some(NodeKind::Tab.id(tab.to_bits())),
                name: "Selected".into(),
                is_active: false,
                root: ApplyHarness::pane(Some(&NodeKind::Pane.id(leaf.to_bits())), vec![]),
            }],
            focused: default(),
        };
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        assert_eq!(
            fixture.app.world().get::<LayoutTab>(tab).unwrap().name,
            "Selected"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LayoutTab>(fixture.tab)
                .unwrap()
                .name,
            "Work"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }
    #[test]
    fn coupled_split_direction_and_weight_commit_to_the_same_render_node() {
        let mut fixture = ApplyHarness::new();
        let nested = fixture
            .app
            .world_mut()
            .spawn((
                Pane::split_bundle(PaneSplitDirection::Row),
                ChildOf(fixture.tab),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.pane)
            .insert(ChildOf(nested));
        let mut snapshot = fixture.intent();
        snapshot.tabs[0].root = ApplyHarness::split(
            None,
            vec![LayoutNode::Split {
                id: Some(NodeKind::Split.id(nested.to_bits())),
                direction: SplitDirection::Column,
                flex_weights: vec![1.0],
                children: vec![ApplyHarness::pane(
                    Some(&NodeKind::Pane.id(fixture.pane.to_bits())),
                    vec![StackDto {
                        id: Some(NodeKind::Stack.id(fixture.stack.to_bits())),
                        ..default()
                    }],
                )],
            }],
            vec![3.0],
        );
        fixture.submit([1; 16], snapshot);
        fixture.app.update();
        assert!(fixture.response([1; 16]).is_ok());
        let render = fixture.app.world().get::<Node>(nested).unwrap();
        assert_eq!(render.flex_direction, FlexDirection::Column);
        assert_eq!(render.flex_grow, 3.0);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(fixture.stack)
                .unwrap()
                .parent(),
            fixture.pane
        );
    }
}
