use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::Window;
use vmux_ecs::EntityTarget;

use super::topology::{
    Id, InvalidTopology, Leaf, Origin, Root, Slot, Split, TabDirectory, TabGroup, Weight,
};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RestoreAdmissionSet;

#[derive(Component, Clone, Copy)]
pub(super) struct RestoreIntent {
    pub(super) root: Entity,
    pub(super) space: Entity,
    pub(super) window: Entity,
    pub(super) origin_matches: bool,
    pub(super) window_bound: bool,
}

#[derive(Component)]
pub(super) struct Claimed(pub(super) Entity);

#[derive(Component)]
pub(super) struct RestoreBinding {
    pub(super) operation: Entity,
    pub(super) live: Entity,
}

#[derive(Component)]
pub(super) struct ScaffoldOwner(pub(super) Entity);

#[derive(Component)]
struct Pending;

#[derive(Component)]
struct Prepared;

#[derive(Component)]
struct Allocated;

#[derive(Component)]
struct Transferred;

#[derive(Component)]
struct Promotion {
    operation: Entity,
    live: Entity,
}

#[derive(EntityEvent)]
pub(super) struct RestoreRequest {
    #[event_target]
    pub(super) operation: Entity,
    pub(super) root: Entity,
    pub(super) space: Entity,
    pub(super) window: Entity,
    pub(super) origin_matches: bool,
    pub(super) window_bound: bool,
}

pub(super) struct RestorePlugin;

impl Plugin for RestorePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(admit).add_systems(
            Update,
            (preflight, allocate, transfer, consume, cleanup)
                .chain()
                .after(RestoreAdmissionSet)
                .in_set(super::LayoutRequestSet::Handle),
        );
    }
}

#[derive(SystemParam)]
struct Topology<'w, 's> {
    roots: Query<'w, 's, Option<&'static Claimed>, With<Root>>,
}

fn allocate(world: &mut World, prepared: &mut QueryState<Entity, With<Prepared>>) {
    let mut nodes = world.query::<(
        Option<&Id>,
        Option<&Weight>,
        Has<Split>,
        Has<Leaf>,
        Has<Slot>,
    )>();
    let prepared = prepared.iter(world).collect::<Vec<_>>();
    for operation in prepared {
        let Some(intent) = world.get::<RestoreIntent>(operation).copied() else {
            continue;
        };
        if let Err(error) = Topology::validate(intent, world, operation, &mut nodes) {
            Topology::refuse(world, operation, error);
            continue;
        }
        if world.get::<TabGroup>(intent.root).is_some() {
            let directory = world
                .get::<TabDirectory>(intent.root)
                .and_then(|directory| directory.0.clone());
            Topology::allocate_group(intent, world, operation, directory);
        } else {
            Topology::allocate_single(intent, world, operation);
        }
        world
            .entity_mut(operation)
            .remove::<Prepared>()
            .insert(Allocated);
        world.flush();
    }
}

impl Topology<'_, '_> {
    fn ordered_children(world: &World, parent: Entity) -> Vec<Entity> {
        let mut children = world
            .get::<Children>(parent)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        children.sort_by_key(|child| {
            (
                world
                    .get::<vmux_ecs::Order>(*child)
                    .map_or(u32::MAX, |order| order.0),
                child.to_bits(),
            )
        });
        children
    }

    fn first_leaf(world: &World, root: Entity) -> Option<Entity> {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if world.get::<crate::pane::Pane>(node).is_some()
                && world.get::<crate::pane::PaneSplit>(node).is_none()
            {
                return Some(node);
            }
            if let Some(children) = world.get::<Children>(node) {
                pending.extend(children.iter().rev());
            }
        }
        None
    }

    fn focus(world: &mut World, node: Entity) {
        let mut current = node;
        for _ in 0..128 {
            world
                .entity_mut(current)
                .insert(vmux_ecs::LastActivatedAt::now());
            let Some(parent) = world.get::<ChildOf>(current) else {
                break;
            };
            current = parent.parent();
            if world.get::<crate::space::Space>(current).is_some() {
                break;
            }
        }
    }

    fn allocate_pages(world: &mut World, operation: Entity, leaf: Entity, live: Entity) {
        for archive in Self::ordered_children(world, leaf) {
            let Some(page) = world.get::<vmux_ecs::ArchivedPage>(archive).cloned() else {
                continue;
            };
            let active = world.get::<vmux_ecs::Active>(archive).is_some();
            let stack = world
                .spawn((
                    crate::stack::Stack::bundle(),
                    ChildOf(live),
                    ScaffoldOwner(operation),
                ))
                .id();
            world.entity_mut(stack).insert((
                vmux_ecs::PageMetadata {
                    url: page.url,
                    title: page.title,
                    ..Default::default()
                },
                vmux_ecs::LastActivatedAt(if active {
                    vmux_ecs::UnixMillis::now().0
                } else {
                    0
                }),
            ));
            world.entity_mut(archive).insert(RestoreBinding {
                operation,
                live: stack,
            });
            let index = world
                .get::<vmux_ecs::Order>(archive)
                .map_or(usize::MAX, |order| order.0 as usize);
            Self::insert_at::<crate::stack::Stack>(world, live, stack, index);
        }
    }

    fn insert_at<T: Component>(world: &mut World, parent: Entity, node: Entity, index: usize) {
        let children = world
            .get::<Children>(parent)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| *child != node)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let positions = children
            .iter()
            .enumerate()
            .filter_map(|(position, child)| world.get::<T>(*child).is_some().then_some(position))
            .collect::<Vec<_>>();
        let position = positions.get(index).copied().unwrap_or(children.len());
        world.entity_mut(parent).insert_children(position, &[node]);
    }

    fn allocate_group(
        intent: RestoreIntent,
        world: &mut World,
        operation: Entity,
        directory: Option<String>,
    ) {
        let name = world
            .get::<Name>(intent.root)
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let tab = world
            .spawn((
                crate::tab::Tab::bundle(),
                vmux_ecs::LastActivatedAt::now(),
                ChildOf(intent.space),
                ScaffoldOwner(operation),
            ))
            .id();
        world.entity_mut(tab).insert(crate::tab::Tab {
            name,
            startup_dir: directory,
        });
        if intent.origin_matches
            && let Some(index) = world
                .get::<Origin>(intent.root)
                .and_then(|origin| origin.tab_index)
        {
            Self::insert_at::<crate::tab::Tab>(world, intent.space, tab, index);
        }
        let mut pending = vec![(intent.root, tab)];
        while let Some((archive_parent, live_parent)) = pending.pop() {
            let children = Self::ordered_children(world, archive_parent);
            for archive in children {
                if world.get::<vmux_ecs::ArchivedPage>(archive).is_some() {
                    continue;
                }
                let id = world
                    .get::<Id>(archive)
                    .map(|id| id.0.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let live = if let Some(axis) = world.get::<Split>(archive).map(Split::axis) {
                    let direction = match axis {
                        vmux_ecs::SplitAxis::Row => crate::pane::PaneSplitDirection::Row,
                        vmux_ecs::SplitAxis::Column => crate::pane::PaneSplitDirection::Column,
                    };
                    world
                        .spawn((
                            crate::pane::Pane::split_bundle(direction),
                            ChildOf(live_parent),
                            ScaffoldOwner(operation),
                        ))
                        .id()
                } else {
                    world
                        .spawn((
                            crate::pane::Pane::bundle(),
                            ChildOf(live_parent),
                            ScaffoldOwner(operation),
                        ))
                        .id()
                };
                let weight = world.get::<Weight>(archive).map_or(1.0, |weight| weight.0);
                world
                    .entity_mut(live)
                    .insert((crate::pane::PaneId(id), vmux_ecs::LastActivatedAt(0)));
                Self::apply_weight(world, live, weight);
                if archive_parent == intent.root {
                    world
                        .entity_mut(live)
                        .insert(bevy_cef::prelude::HostWindow(intent.window));
                }
                world
                    .entity_mut(archive)
                    .insert(RestoreBinding { operation, live });
                if world.get::<Leaf>(archive).is_some() || world.get::<Slot>(archive).is_some() {
                    Self::allocate_pages(world, operation, archive, live);
                } else {
                    pending.push((archive, live));
                }
            }
        }
        world.entity_mut(intent.root).insert(RestoreBinding {
            operation,
            live: tab,
        });
    }

    fn allocate_single(intent: RestoreIntent, world: &mut World, operation: Entity) {
        let mut nodes = vec![intent.root];
        while let Some(node) = nodes.pop() {
            if world.get::<Leaf>(node).is_some() {
                let id = world
                    .get::<Id>(node)
                    .map(|id| id.0.clone())
                    .unwrap_or_default();
                let mut leaves = world.query_filtered::<(Entity, &crate::pane::PaneId), (
                    With<crate::pane::Pane>,
                    Without<crate::presentation::Retiring>,
                )>();
                let candidates = leaves
                    .iter(world)
                    .filter_map(|(leaf, value)| (value.0 == id).then_some(leaf))
                    .collect::<Vec<_>>();
                let live = candidates
                    .into_iter()
                    .find(|leaf| Self::in_space(world, *leaf, intent.space))
                    .and_then(|leaf| Self::first_leaf(world, leaf))
                    .or_else(|| Self::reattach(intent, world, operation, node));
                let live = match live {
                    Some(live) => live,
                    None => {
                        let tab = world
                            .spawn((
                                crate::tab::Tab::bundle(),
                                ChildOf(intent.space),
                                vmux_ecs::LastActivatedAt::now(),
                                ScaffoldOwner(operation),
                            ))
                            .id();
                        if intent.origin_matches
                            && let Some(index) = world
                                .get::<Origin>(intent.root)
                                .and_then(|origin| origin.tab_index)
                        {
                            Self::insert_at::<crate::tab::Tab>(world, intent.space, tab, index);
                        }
                        world
                            .spawn((
                                crate::pane::Pane::bundle(),
                                ChildOf(tab),
                                bevy_cef::prelude::HostWindow(intent.window),
                                ScaffoldOwner(operation),
                            ))
                            .id()
                    }
                };
                world
                    .entity_mut(node)
                    .insert(RestoreBinding { operation, live });
                Self::allocate_pages(world, operation, node, live);
                break;
            }
            nodes.extend(Self::ordered_children(world, node));
        }
    }

    fn in_space(world: &World, node: Entity, space: Entity) -> bool {
        let mut current = node;
        for _ in 0..128 {
            if world
                .get::<crate::presentation::Retiring>(current)
                .is_some()
            {
                return false;
            }
            let Some(parent) = world.get::<ChildOf>(current) else {
                return false;
            };
            current = parent.parent();
            if current == space {
                return world.get::<crate::presentation::Retiring>(space).is_none();
            }
        }
        false
    }
}

fn transfer(world: &mut World, allocated: &mut QueryState<Entity, With<Allocated>>) {
    let mut nodes = world.query::<(
        Option<&Id>,
        Option<&Weight>,
        Has<Split>,
        Has<Leaf>,
        Has<Slot>,
    )>();
    let mut promotions = world.query::<(Entity, &Promotion, &Split)>();
    let operations = allocated.iter(world).collect::<Vec<_>>();
    for operation in operations {
        let Some(intent) = world.get::<RestoreIntent>(operation).copied() else {
            continue;
        };
        let validation = Topology::validate(intent, world, operation, &mut nodes)
            .and_then(|()| Topology::validate_bindings(intent, world, operation));
        if let Err(error) = validation {
            Topology::refuse(world, operation, error);
            continue;
        }
        Topology::promote(world, operation, &mut promotions);
        for archive in Topology::pages(intent.root, world) {
            let stack = world.get::<RestoreBinding>(archive).unwrap().live;
            world.trigger(vmux_ecs::page::ArchiveRestoreRequest { archive, stack });
            world.entity_mut(archive).insert(Transferred);
        }
        world.flush();
        world
            .entity_mut(operation)
            .remove::<Allocated>()
            .insert(Transferred);
    }
}

fn consume(
    world: &mut World,
    pages: &mut QueryState<
        (Entity, &RestoreBinding),
        (With<vmux_ecs::ArchivedPage>, With<Transferred>),
    >,
) {
    let pending = pages
        .iter(world)
        .map(|(archive, binding)| (archive, binding.operation, binding.live))
        .collect::<Vec<_>>();
    for (archive, operation, stack) in pending {
        if world.get::<Transferred>(operation).is_none() || world.get_entity(stack).is_err() {
            continue;
        }
        let Some(page) = world.get::<vmux_ecs::ArchivedPage>(archive).cloned() else {
            continue;
        };
        world
            .entity_mut(stack)
            .remove::<ScaffoldOwner>()
            .insert(vmux_ecs::store::PageRestore);
        if world.get::<vmux_ecs::Active>(archive).is_some() {
            Topology::focus(world, stack);
        }
        if let Some(launch) = page.launch {
            world.entity_mut(stack).insert(launch);
        }
        if let Some(workspace) = world.get::<vmux_ecs::WorkspaceKind>(archive).copied() {
            world.entity_mut(stack).insert(workspace);
        }
        if !page.url.is_empty() {
            world.write_message(vmux_ecs::PageOpenRequest {
                target: vmux_ecs::PageOpenTarget::Stack(stack),
                url: page.url,
                request_id: None,
            });
        }
        world.despawn(archive);
        let mut owned = world.query::<(Entity, &ScaffoldOwner)>();
        let owners = owned
            .iter(world)
            .filter_map(|(node, owner)| (owner.0 == operation).then_some(node))
            .collect::<Vec<_>>();
        for node in owners {
            world.entity_mut(node).remove::<ScaffoldOwner>();
        }
    }
    let mut done = world.query_filtered::<(Entity, &RestoreIntent), With<Transferred>>();
    let done = done
        .iter(world)
        .map(|(operation, intent)| (operation, intent.root))
        .collect::<Vec<_>>();
    for (operation, root) in done {
        let Some(intent) = world.get::<RestoreIntent>(operation).copied() else {
            continue;
        };
        if !Topology::pages(intent.root, world).is_empty() {
            Topology::refuse(world, operation, "archive transfer target retired".into());
            world.entity_mut(operation).remove::<Transferred>();
            continue;
        }
        world.despawn(root);
        world.despawn(operation);
    }
}

fn cleanup(
    world: &mut World,
    operations: &mut QueryState<(Entity, &RestoreIntent, &vmux_ecs::OperationState)>,
) {
    let retired = operations
        .iter(world)
        .filter_map(|(operation, intent, state)| {
            (!matches!(
                state,
                vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
            ))
            .then_some((operation, *intent))
        })
        .collect::<Vec<_>>();
    for (operation, intent) in retired {
        let mut owned = world.query::<(Entity, &ScaffoldOwner)>();
        let owners = owned
            .iter(world)
            .filter_map(|(node, owner)| (owner.0 == operation).then_some(node))
            .collect::<Vec<_>>();
        for node in owners.into_iter().rev() {
            if world.get_entity(node).is_err() {
                continue;
            }
            if Topology::empty_scaffold(world, operation, node) {
                world.despawn(node);
            } else {
                world.entity_mut(node).remove::<ScaffoldOwner>();
            }
        }
        let mut bindings = world.query::<(Entity, &RestoreBinding)>();
        let bound = bindings
            .iter(world)
            .filter_map(|(node, binding)| (binding.operation == operation).then_some(node))
            .collect::<Vec<_>>();
        for node in bound {
            world
                .entity_mut(node)
                .remove::<(RestoreBinding, Promotion)>();
        }
        if world
            .get::<Claimed>(intent.root)
            .is_some_and(|claim| claim.0 == operation)
        {
            world.entity_mut(intent.root).remove::<Claimed>();
        }
        world.despawn(operation);
    }
}

fn admit(request: On<RestoreRequest>, topology: Topology, mut commands: Commands) {
    if topology
        .roots
        .get(request.root)
        .map_or(true, |claimed| claimed.is_some())
    {
        commands
            .entity(request.operation)
            .remove::<vmux_ecs::OperationRetained>()
            .insert((
                vmux_ecs::OperationState::Failed,
                vmux_ecs::OperationError::new("archive is unavailable or already claimed").unwrap(),
            ));
        return;
    }
    commands.entity(request.operation).insert((
        RestoreIntent {
            root: request.root,
            space: request.space,
            window: request.window,
            origin_matches: request.origin_matches,
            window_bound: request.window_bound,
        },
        EntityTarget::<Root>::new(request.root),
        Pending,
    ));
    commands
        .entity(request.root)
        .insert(Claimed(request.operation));
}

fn preflight(world: &mut World, pending: &mut QueryState<Entity, With<Pending>>) {
    let mut nodes = world.query::<(
        Option<&Id>,
        Option<&Weight>,
        Has<Split>,
        Has<Leaf>,
        Has<Slot>,
    )>();
    let pending = pending.iter(world).collect::<Vec<_>>();
    for operation in pending {
        let Some(intent) = world.get::<RestoreIntent>(operation).copied() else {
            continue;
        };
        let active = world
            .get::<vmux_ecs::OperationState>(operation)
            .is_some_and(|state| {
                matches!(
                    state,
                    vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
                )
            });
        let error = if active {
            Topology::validate(intent, world, operation, &mut nodes).err()
        } else {
            Some("archive restore was canceled".into())
        };
        if let Some(error) = error {
            world.entity_mut(operation).remove::<Pending>().insert((
                vmux_ecs::OperationState::Failed,
                vmux_ecs::OperationError::new(error).unwrap(),
            ));
            if world
                .get::<Claimed>(intent.root)
                .is_some_and(|claim| claim.0 == operation)
            {
                world.entity_mut(intent.root).remove::<Claimed>();
            }
            continue;
        }
        world
            .entity_mut(operation)
            .remove::<Pending>()
            .insert(Prepared);
    }
}

impl Topology<'_, '_> {
    fn reattach(
        intent: RestoreIntent,
        world: &mut World,
        operation: Entity,
        leaf: Entity,
    ) -> Option<Entity> {
        let mut path = vec![leaf];
        let mut current = leaf;
        while let Some(parent) = world.get::<ChildOf>(current) {
            current = parent.parent();
            if current == intent.root {
                break;
            }
            path.push(current);
        }
        path.reverse();
        let root = *path.first()?;
        world.get::<Split>(root)?;
        let id = world.get::<Id>(root)?.0.clone();
        let mut panes = world.query_filtered::<(Entity, &crate::pane::PaneId), (
            With<crate::pane::Pane>,
            Without<crate::presentation::Retiring>,
        )>();
        let candidates = panes
            .iter(world)
            .filter_map(|(pane, value)| (value.0 == id).then_some(pane))
            .collect::<Vec<_>>();
        let mut live = candidates
            .into_iter()
            .find(|pane| Self::in_space(world, *pane, intent.space))?;
        world
            .entity_mut(root)
            .insert(RestoreBinding { operation, live });
        for nodes in path.windows(2) {
            let parent = nodes[0];
            let archive = nodes[1];
            let id = world.get::<Id>(archive)?.0.clone();
            let existing = world.get::<Children>(live).and_then(|children| {
                children.iter().find(|child| {
                    world
                        .get::<crate::pane::PaneId>(*child)
                        .is_some_and(|value| value.0 == id)
                })
            });
            if let Some(existing) = existing {
                live = existing;
            } else {
                if Self::needs_promotion(world, parent, live, operation) {
                    world
                        .entity_mut(parent)
                        .insert(Promotion { operation, live });
                }
                let axis = world.get::<Split>(archive).map(Split::axis);
                let weight = world.get::<Weight>(archive).map_or(1.0, |weight| weight.0);
                let child = if let Some(axis) = axis {
                    world
                        .spawn((
                            crate::pane::Pane::split_bundle(Self::direction(axis)),
                            crate::pane::PaneId(id),
                            ChildOf(live),
                            ScaffoldOwner(operation),
                        ))
                        .id()
                } else {
                    world
                        .spawn((
                            crate::pane::Pane::bundle(),
                            crate::pane::PaneId(id),
                            ChildOf(live),
                            ScaffoldOwner(operation),
                        ))
                        .id()
                };
                Self::apply_weight(world, child, weight);
                let index = world
                    .get::<vmux_ecs::Order>(archive)
                    .map_or(usize::MAX, |order| order.0 as usize);
                Self::insert_at::<crate::pane::Pane>(world, live, child, index);
                live = child;
            }
            world
                .entity_mut(archive)
                .insert(RestoreBinding { operation, live });
        }
        Self::first_leaf(world, live)
    }

    fn apply_weight(world: &mut World, entity: Entity, weight: f32) {
        let mut entity = world.entity_mut(entity);
        entity.insert(crate::pane::PaneSize { flex_grow: weight });
        if let Some(mut node) = entity.get_mut::<vmux_flex::Node>() {
            node.flex_grow = weight;
        }
    }

    fn direction(axis: vmux_ecs::SplitAxis) -> crate::pane::PaneSplitDirection {
        match axis {
            vmux_ecs::SplitAxis::Row => crate::pane::PaneSplitDirection::Row,
            vmux_ecs::SplitAxis::Column => crate::pane::PaneSplitDirection::Column,
        }
    }

    fn needs_promotion(world: &World, parent: Entity, live: Entity, operation: Entity) -> bool {
        let Some(live_split) = world.get::<crate::pane::PaneSplit>(live) else {
            return true;
        };
        let Some(archived_split) = world.get::<Split>(parent) else {
            return false;
        };
        if live_split.direction != Self::direction(archived_split.axis()) {
            return true;
        }
        let archived_children = Self::ordered_children(world, parent).len();
        if archived_children != 2 {
            return false;
        }
        let existing_panes = world
            .get::<Children>(live)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| {
                        world.get::<crate::pane::Pane>(*child).is_some()
                            && world
                                .get::<ScaffoldOwner>(*child)
                                .is_none_or(|owner| owner.0 != operation)
                    })
                    .count()
            })
            .unwrap_or(0);
        existing_panes >= 2
    }

    fn promote_survivor(world: &mut World, live: Entity, operation: Entity) -> Entity {
        if let Some(direction) = world
            .get::<crate::pane::PaneSplit>(live)
            .map(|split| split.direction)
        {
            let panes = world
                .get::<Children>(live)
                .map(|children| {
                    children
                        .iter()
                        .filter(|child| {
                            world.get::<crate::pane::Pane>(*child).is_some()
                                && world
                                    .get::<ScaffoldOwner>(*child)
                                    .is_none_or(|owner| owner.0 != operation)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let survivor = world
                .spawn((
                    crate::pane::Pane::split_bundle(direction),
                    ChildOf(live),
                    vmux_ecs::LastActivatedAt(0),
                ))
                .id();
            for pane in panes {
                world.entity_mut(pane).insert(ChildOf(survivor));
            }
            return survivor;
        }
        let stacks = world
            .get::<Children>(live)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| world.get::<crate::stack::Stack>(*child).is_some())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let survivor = world
            .spawn((
                crate::pane::Pane::bundle(),
                ChildOf(live),
                vmux_ecs::LastActivatedAt(0),
            ))
            .id();
        for stack in stacks {
            world.entity_mut(stack).insert(ChildOf(survivor));
        }
        survivor
    }

    fn promote(
        world: &mut World,
        operation: Entity,
        promotions: &mut QueryState<(Entity, &Promotion, &Split)>,
    ) {
        let pending = promotions
            .iter(world)
            .filter_map(|(archive, promotion, split)| {
                (promotion.operation == operation).then_some((
                    archive,
                    promotion.live,
                    split.axis(),
                ))
            })
            .collect::<Vec<_>>();
        for (archive, live, axis) in pending {
            let survivor = Self::promote_survivor(world, live, operation);
            let target_order = Self::ordered_children(world, archive)
                .into_iter()
                .find_map(|child| {
                    world
                        .get::<RestoreBinding>(child)
                        .filter(|binding| binding.operation == operation)
                        .and_then(|_| world.get::<vmux_ecs::Order>(child))
                        .map(|order| order.0)
                })
                .unwrap_or(1);
            let slot = Self::ordered_children(world, archive)
                .into_iter()
                .find(|child| {
                    world
                        .get::<vmux_ecs::Order>(*child)
                        .is_some_and(|order| order.0 != target_order)
                });
            let weight = slot
                .and_then(|slot| world.get::<Weight>(slot))
                .map_or(1.0, |weight| weight.0);
            Self::apply_weight(world, survivor, weight);
            Self::insert_at::<crate::pane::Pane>(
                world,
                live,
                survivor,
                if target_order == 0 { 1 } else { 0 },
            );
            let live_weight = world
                .get::<crate::pane::PaneSize>(live)
                .map_or(1.0, |size| size.flex_grow);
            world
                .entity_mut(live)
                .insert(crate::pane::Pane::split_bundle(Self::direction(axis)));
            Self::apply_weight(world, live, live_weight);
            world.entity_mut(archive).remove::<Promotion>();
        }
    }

    fn refuse(world: &mut World, operation: Entity, error: String) {
        world
            .entity_mut(operation)
            .remove::<(Pending, Prepared, Allocated)>()
            .insert((
                vmux_ecs::OperationState::Failed,
                vmux_ecs::OperationError::new(error).unwrap(),
            ));
    }

    fn empty_scaffold(world: &World, operation: Entity, node: Entity) -> bool {
        let mut nodes = vec![node];
        while let Some(node) = nodes.pop() {
            if world.get::<vmux_ecs::PageNavigation>(node).is_some() {
                return false;
            }
            if world
                .get::<ScaffoldOwner>(node)
                .is_none_or(|owner| owner.0 != operation)
            {
                return false;
            }
            if let Some(children) = world.get::<Children>(node) {
                nodes.extend(children.iter());
            }
        }
        true
    }

    fn pages(root: Entity, world: &World) -> Vec<Entity> {
        let mut nodes = vec![root];
        let mut pages = Vec::new();
        while let Some(node) = nodes.pop() {
            if world.get::<vmux_ecs::ArchivedPage>(node).is_some() {
                pages.push(node);
            }
            nodes.extend(Self::ordered_children(world, node).into_iter().rev());
        }
        pages
    }

    fn validate_bindings(
        intent: RestoreIntent,
        world: &World,
        operation: Entity,
    ) -> Result<(), String> {
        let pages = Self::pages(intent.root, world);
        if pages.is_empty() {
            return Err("archive has no pages".into());
        }
        let mut targets = std::collections::BTreeSet::new();
        for page in pages {
            let Some(binding) = world.get::<RestoreBinding>(page) else {
                return Err("archive page mapping is missing".into());
            };
            if binding.operation != operation
                || !targets.insert(binding.live)
                || world.get::<crate::stack::Stack>(binding.live).is_none()
                || !Self::in_space(world, binding.live, intent.space)
                || world
                    .get::<ScaffoldOwner>(binding.live)
                    .is_none_or(|owner| owner.0 != operation)
                || world
                    .get::<Children>(binding.live)
                    .is_some_and(|children| !children.is_empty())
                || world
                    .get::<vmux_ecs::PageNavigation>(binding.live)
                    .is_some()
            {
                return Err("archive page mapping changed owner or scope".into());
            }
        }
        Ok(())
    }

    fn validate(
        intent: RestoreIntent,
        world: &mut World,
        operation: Entity,
        nodes: &mut QueryState<(
            Option<&Id>,
            Option<&Weight>,
            Has<Split>,
            Has<Leaf>,
            Has<Slot>,
        )>,
    ) -> Result<(), String> {
        if world
            .get::<vmux_ecs::OperationState>(operation)
            .is_none_or(|state| {
                !matches!(
                    state,
                    vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
                )
            })
        {
            return Err("archive restore was canceled".into());
        }
        if world.get::<Root>(intent.root).is_none()
            || world
                .get::<Claimed>(intent.root)
                .is_none_or(|claim| claim.0 != operation)
        {
            return Err("archive identity or claim changed".into());
        }
        if world.get::<crate::space::Space>(intent.space).is_none()
            || world.get::<Window>(intent.window).is_none()
        {
            return Err("archive restore Space or Window retired".into());
        }
        let mut current = intent.space;
        let mut window = None;
        for _ in 0..128 {
            if let Some(host) = world.get::<bevy_cef::prelude::HostWindow>(current) {
                window = Some(host.0);
                break;
            }
            let Some(parent) = world.get::<ChildOf>(current) else {
                break;
            };
            current = parent.parent();
        }
        if window.is_some_and(|window| window != intent.window)
            || (intent.window_bound && window.is_none())
        {
            return Err("archive restore Space changed windows".into());
        }
        Self::validate_topology(intent.root, world, nodes)
    }

    fn validate_topology(
        root: Entity,
        world: &World,
        topology: &mut QueryState<(
            Option<&Id>,
            Option<&Weight>,
            Has<Split>,
            Has<Leaf>,
            Has<Slot>,
        )>,
    ) -> Result<(), String> {
        let mut nodes = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        let mut pages = 0;
        while let Some(node) = nodes.pop() {
            let Ok((id, weight, split, leaf, slot)) = topology.get(world, node) else {
                return Err("archive node is missing".into());
            };
            if !visited.insert(node) || world.get::<InvalidTopology>(node).is_some() {
                return Err("archive topology is invalid or cyclic".into());
            }
            if let Some(weight) = weight
                && (!weight.0.is_finite() || weight.0 <= 0.0)
            {
                return Err("archive node weight is invalid".into());
            }
            if node != root && !slot && world.get::<vmux_ecs::ArchivedPage>(node).is_none() {
                let Some(id) = id else {
                    return Err("archive node identity is missing".into());
                };
                if id.0.is_empty() || !ids.insert(id.0.as_str()) {
                    return Err("archive node identity conflicts".into());
                }
                if split == leaf {
                    return Err("archive node kind is invalid".into());
                }
            }
            if world.get::<vmux_ecs::ArchivedPage>(node).is_some() {
                pages += 1;
                if world
                    .get::<ChildOf>(node)
                    .is_none_or(|parent| world.get::<Leaf>(parent.parent()).is_none())
                {
                    return Err("archive page has no leaf owner".into());
                }
            }
            let mut orders = std::collections::BTreeSet::new();
            if let Some(children) = world.get::<Children>(node) {
                for child in children.iter() {
                    if world.get::<vmux_ecs::ArchivedPage>(node).is_some() {
                        continue;
                    }
                    let Some(order) = world.get::<vmux_ecs::Order>(child) else {
                        return Err("archive node order is missing".into());
                    };
                    if !orders.insert(order.0) {
                        return Err("archive sibling order conflicts".into());
                    }
                    nodes.push(child);
                }
            }
        }
        if pages == 0 {
            return Err("archive has no pages".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::lifecycle::Add;
    use bevy::prelude::{App, Commands, Entity, Name, On, Plugin, Query, With};

    struct RestoreFixture {
        app: App,
        archive: Entity,
        window: Entity,
        space: Entity,
    }

    impl RestoreFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(super::super::ArchivePlugin);
            let window = app
                .world_mut()
                .spawn((Window::default(), bevy::window::PrimaryWindow))
                .id();
            let space = app
                .world_mut()
                .spawn((
                    crate::space::Space,
                    crate::space::SpaceId("restore-test".into()),
                ))
                .id();
            let archive = app
                .world_mut()
                .spawn(vmux_ecs::ArchivedPage {
                    url: "https://retained.example".into(),
                    space_id: "restore-test".into(),
                    closed_at: vmux_ecs::UnixMillis::now().0,
                    ..Default::default()
                })
                .id();
            app.update();
            Self {
                app,
                archive,
                window,
                space,
            }
        }

        fn restore(&mut self) {
            let leaf = self
                .app
                .world()
                .get::<ChildOf>(self.archive)
                .unwrap()
                .parent();
            let root = self.app.world().get::<ChildOf>(leaf).unwrap().parent();
            let operation = self
                .app
                .world_mut()
                .spawn(vmux_ecs::Operation::bundle("archive.restore").unwrap())
                .id();
            let window_bound = self
                .app
                .world()
                .get::<bevy_cef::prelude::HostWindow>(self.space)
                .is_some();
            self.app.world_mut().trigger(RestoreRequest {
                operation,
                root,
                space: self.space,
                window: self.window,
                origin_matches: true,
                window_bound,
            });
            self.app.update();
        }

        fn assert_retained(&mut self) {
            assert_eq!(
                self.app
                    .world()
                    .get::<vmux_ecs::ArchivedPage>(self.archive)
                    .unwrap()
                    .url,
                "https://retained.example"
            );
            assert_eq!(
                self.app
                    .world_mut()
                    .query_filtered::<Entity, With<crate::stack::Stack>>()
                    .iter(self.app.world())
                    .count(),
                0
            );
            assert_eq!(
                self.app
                    .world_mut()
                    .query_filtered::<Entity, With<Claimed>>()
                    .iter(self.app.world())
                    .count(),
                0
            );
            assert_eq!(
                self.app
                    .world_mut()
                    .query_filtered::<Entity, With<ScaffoldOwner>>()
                    .iter(self.app.world())
                    .count(),
                0
            );
        }
    }

    #[derive(Resource)]
    struct CancelOnAllocated;

    #[derive(Resource)]
    struct DisconnectOnAllocated;

    #[derive(Resource)]
    struct OccupyOnAllocated;

    struct RestoreInterferencePlugin;

    impl Plugin for RestoreInterferencePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(cancel)
                .add_observer(disconnect)
                .add_observer(occupy);
        }
    }

    fn cancel(
        added: On<Add, Allocated>,
        enabled: Option<Res<CancelOnAllocated>>,
        mut commands: Commands,
    ) {
        if enabled.is_none() {
            return;
        }
        commands
            .entity(added.entity)
            .insert(vmux_ecs::OperationState::Canceled);
    }

    fn disconnect(
        added: On<Add, Allocated>,
        enabled: Option<Res<DisconnectOnAllocated>>,
        pages: Query<(Entity, &RestoreBinding), With<vmux_ecs::ArchivedPage>>,
        mut commands: Commands,
    ) {
        if enabled.is_none() {
            return;
        }
        for (page, binding) in &pages {
            if binding.operation == added.entity {
                commands.entity(page).remove::<RestoreBinding>();
                break;
            }
        }
    }

    fn occupy(
        added: On<Add, Allocated>,
        enabled: Option<Res<OccupyOnAllocated>>,
        pages: Query<&RestoreBinding, With<vmux_ecs::ArchivedPage>>,
        mut commands: Commands,
    ) {
        if enabled.is_none() {
            return;
        }
        for binding in &pages {
            if binding.operation == added.entity {
                commands.spawn((
                    Name::new("authored during allocation"),
                    ChildOf(binding.live),
                ));
                break;
            }
        }
    }

    #[test]
    fn a_restore_target_adopted_by_another_owner_is_retained_without_overwriting_it() {
        let mut fixture = RestoreFixture::new();
        fixture
            .app
            .insert_resource(OccupyOnAllocated)
            .add_plugins(RestoreInterferencePlugin);
        fixture.restore();
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::ArchivedPage>(fixture.archive)
                .is_some()
        );
        let (_, parent) = fixture
            .app
            .world_mut()
            .query::<(&Name, &ChildOf)>()
            .iter(fixture.app.world())
            .find(|(name, _)| name.as_str() == "authored during allocation")
            .unwrap();
        let stack = parent.parent();
        assert!(
            fixture
                .app
                .world()
                .get::<crate::stack::Stack>(stack)
                .is_some()
        );
        assert!(fixture.app.world().get::<ScaffoldOwner>(stack).is_none());
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&Claimed>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn canceled_allocated_restore_retains_archive_and_removes_only_empty_scaffolding() {
        let mut fixture = RestoreFixture::new();
        fixture
            .app
            .insert_resource(CancelOnAllocated)
            .add_plugins(RestoreInterferencePlugin);
        fixture.restore();
        fixture.assert_retained();
    }

    #[test]
    fn incomplete_page_mapping_refuses_without_consuming_the_archive() {
        let mut fixture = RestoreFixture::new();
        fixture
            .app
            .insert_resource(DisconnectOnAllocated)
            .add_plugins(RestoreInterferencePlugin);
        fixture.restore();
        fixture.assert_retained();
    }
}
