use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite::future},
};
use vmux_core::{PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::{
    CloseRequiresConfirmation,
    settings::ConfirmCloseSettings,
    stack::{
        ActiveTabParam, CloseConfirmed, PendingStackClose, Stack, active_stack_in_pane,
        focused_stack, stack_bundle,
    },
};

#[cfg(test)]
use super::PaneSplitDirection;
use super::{
    CloseRequest, Pane, PaneSplit, first_leaf_descendant, first_stack_in_pane, leaf_pane_bundle,
};
use crate::host::command::LayoutRequestSet;

pub(super) struct ClosePlugin;

impl Plugin for ClosePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CloseRequest>()
            .init_resource::<CloseDialog>()
            .add_message::<PaneCloseRequest>()
            .add_systems(
                Update,
                (request_pane_close, close_panes)
                    .chain()
                    .in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                (process_close_dialogs, process_force_pane_closes)
                    .in_set(LayoutRequestSet::Dispatch),
            );
    }
}

#[derive(Component)]
pub struct PendingPaneClose;

#[derive(Component)]
pub struct ForcePaneClose;

#[derive(Message)]
struct PaneCloseRequest(Entity);

#[derive(Clone, Copy)]
enum CloseTarget {
    Pane(Entity),
    Stack(Entity),
}

impl CloseTarget {
    fn confirm(self, world: &mut World) {
        match self {
            Self::Pane(pane) => {
                let Ok(mut entity_mut) = world.get_entity_mut(pane) else {
                    return;
                };
                entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));

                let mut current = pane;
                for _ in 0..10 {
                    if world
                        .get_entity(current)
                        .is_ok_and(|e| e.contains::<crate::tab::Tab>())
                    {
                        if let Ok(mut entity_mut) = world.get_entity_mut(current) {
                            entity_mut.insert(LastActivatedAt::now());
                        }
                        break;
                    }
                    if let Some(child_of) = world.get::<ChildOf>(current) {
                        current = child_of.get();
                    } else {
                        break;
                    }
                }
                world
                    .resource_mut::<Messages<CloseRequest>>()
                    .write(CloseRequest);
            }
            Self::Stack(stack) => {
                let Some(parent_pane) = world.get::<ChildOf>(stack).map(|child| child.get()) else {
                    return;
                };
                let sibling_stacks: Vec<Entity> = world
                    .get::<Children>(parent_pane)
                    .map(|children| {
                        children
                            .iter()
                            .filter(|&entity| {
                                entity != stack && world.get::<Stack>(entity).is_some()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let was_active = {
                    let mut query = world.query::<(Entity, &LastActivatedAt)>();
                    let stacks_with_timestamp: Vec<(Entity, LastActivatedAt)> = world
                        .get::<Children>(parent_pane)
                        .map(|children| {
                            children
                                .iter()
                                .filter_map(|entity| query.get(world, entity).ok())
                                .filter(|(entity, _)| world.get::<Stack>(*entity).is_some())
                                .map(|(entity, timestamp)| (entity, *timestamp))
                                .collect()
                        })
                        .unwrap_or_default();
                    stacks_with_timestamp
                        .iter()
                        .max_by_key(|(_, timestamp)| timestamp.0)
                        .map(|(entity, _)| *entity)
                        == Some(stack)
                };

                world.despawn(stack);
                if was_active
                    && let Some(&next) = sibling_stacks.first()
                    && let Ok(mut entity_mut) = world.get_entity_mut(next)
                {
                    entity_mut.insert(LastActivatedAt::now());
                }
            }
        }
    }
}

enum CloseDialogTask {
    Pending {
        target: CloseTarget,
        task: Task<bool>,
    },
    #[cfg(test)]
    Ready {
        target: CloseTarget,
        confirmed: bool,
    },
}

impl CloseDialogTask {
    fn target(&self) -> CloseTarget {
        match self {
            Self::Pending { target, .. } => *target,
            #[cfg(test)]
            Self::Ready { target, .. } => *target,
        }
    }

    fn poll(&mut self) -> Option<bool> {
        match self {
            Self::Pending { task, .. } => future::block_on(future::poll_once(task)),
            #[cfg(test)]
            Self::Ready { confirmed, .. } => Some(*confirmed),
        }
    }
}

#[derive(Resource, Default)]
struct CloseDialog(Option<CloseDialogTask>);

impl CloseDialog {
    fn start(
        &mut self,
        target: CloseTarget,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) {
        let task = IoTaskPool::get().spawn(async move {
            let result = rfd::AsyncMessageDialog::new()
                .set_title("Close terminal?")
                .set_description("A process is still running in this terminal. Close anyway?")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await;
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            matches!(result, rfd::MessageDialogResult::Yes)
        });
        self.0 = Some(CloseDialogTask::Pending { target, task });
    }

    fn poll(&mut self) -> Option<(CloseTarget, bool)> {
        let task = self.0.as_mut()?;
        let confirmed = task.poll()?;
        let target = task.target();
        self.0 = None;
        Some((target, confirmed))
    }

    #[cfg(test)]
    fn ready(target: CloseTarget, confirmed: bool) -> Self {
        Self(Some(CloseDialogTask::Ready { target, confirmed }))
    }
}

fn request_pane_close(
    mut reader: MessageReader<CloseRequest>,
    active_tab: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_times: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_times: Query<(Entity, &LastActivatedAt), With<Stack>>,
    close_required: Query<(), With<CloseRequiresConfirmation>>,
    confirmed: Query<(), With<CloseConfirmed>>,
    pending: Query<(), With<PendingPaneClose>>,
    settings: Res<ConfirmCloseSettings>,
    mut requests: MessageWriter<PaneCloseRequest>,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let (_, Some(active), _) = focused_stack(
            active_tab.get(),
            &all_children,
            &leaf_panes,
            &pane_times,
            &pane_children,
            &stack_times,
        ) else {
            continue;
        };
        let needs_confirmation = settings.enabled
            && pane_has_close_confirmation(active, &pane_children, &all_children, &close_required);
        if needs_confirmation {
            if confirmed.contains(active) {
                commands.entity(active).remove::<CloseConfirmed>();
            } else {
                if !pending.contains(active) {
                    commands.entity(active).insert(PendingPaneClose);
                }
                continue;
            }
        }

        requests.write(PaneCloseRequest(active));
    }
}

fn close_panes(
    mut requests: MessageReader<PaneCloseRequest>,
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_times: Query<(Entity, &LastActivatedAt), With<Pane>>,
    stack_times: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    stacks: Query<Entity, With<Stack>>,
    startup: Option<Res<vmux_core::EffectiveStartupUrl>>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let active = request.0;
        let Ok(parent) = child_of.get(active).map(Relationship::get) else {
            continue;
        };
        if !splits.contains(parent) {
            commands.entity(active).despawn();
            let leaf = commands
                .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(parent)))
                .id();
            let stack = commands
                .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(leaf)))
                .id();
            commands.entity(leaf).insert(LastActivatedAt::now());
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url: vmux_core::EffectiveStartupUrl::resolve(startup.as_deref()),
                request_id: None,
            });
            continue;
        }

        let Ok(children) = pane_children.get(parent) else {
            continue;
        };
        let siblings: Vec<Entity> = children
            .iter()
            .filter(|&entity| {
                entity != active && (leaf_panes.contains(entity) || splits.contains(entity))
            })
            .collect();

        if siblings.len() >= 2 {
            commands.entity(active).despawn();
            let newest = siblings
                .iter()
                .copied()
                .max_by_key(|&entity| pane_times.get(entity).map(|(_, time)| time.0).unwrap_or(0))
                .unwrap_or(siblings[0]);
            let leaf = first_leaf_descendant(newest, &pane_children, &leaf_panes);
            commands.entity(leaf).insert(LastActivatedAt::now());
            if let Some(stack) = active_stack_in_pane(leaf, &pane_children, &stack_times)
                .or_else(|| first_stack_in_pane(leaf, &pane_children, &stacks))
            {
                commands.entity(stack).insert(LastActivatedAt::now());
            }
            continue;
        }

        let Some(sibling) = siblings.into_iter().next() else {
            continue;
        };
        let sibling_children: Vec<Entity> = pane_children
            .get(sibling)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        for &child in &sibling_children {
            commands.entity(child).insert(ChildOf(parent));
        }

        let new_active_pane;
        if splits.contains(sibling) {
            let direction = splits
                .get(sibling)
                .map(|split| split.direction)
                .unwrap_or_default();
            new_active_pane = first_leaf_descendant(sibling, &pane_children, &leaf_panes);
            commands.entity(sibling).remove::<ChildOf>();
            commands.queue(move |world: &mut World| {
                world.despawn(sibling);
                PaneSplit::set_direction(world, parent, direction);
            });
        } else {
            new_active_pane = parent;
            commands.entity(parent).remove::<PaneSplit>();
            commands.entity(parent).insert(Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Stretch,
                ..default()
            });
            commands.entity(sibling).despawn();
        }

        commands.entity(active).despawn();
        commands
            .entity(new_active_pane)
            .insert(LastActivatedAt::now());
        let active_stack = active_stack_in_pane(new_active_pane, &pane_children, &stack_times)
            .or_else(|| first_stack_in_pane(new_active_pane, &pane_children, &stacks))
            .or_else(|| {
                sibling_children
                    .iter()
                    .copied()
                    .find(|&entity| stacks.contains(entity))
            });
        if let Some(stack) = active_stack {
            commands.entity(stack).insert(LastActivatedAt::now());
        }
    }
}

fn pane_has_close_confirmation(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    all_children: &Query<&Children>,
    close_required: &Query<(), With<CloseRequiresConfirmation>>,
) -> bool {
    pane_children.get(pane).is_ok_and(|stacks| {
        stacks
            .iter()
            .any(|stack| entity_tree_has_close_confirmation(stack, all_children, close_required))
    })
}

fn entity_tree_has_close_confirmation(
    entity: Entity,
    children: &Query<&Children>,
    close_required: &Query<(), With<CloseRequiresConfirmation>>,
) -> bool {
    close_required.contains(entity)
        || children.get(entity).is_ok_and(|descendants| {
            descendants
                .iter()
                .any(|child| entity_tree_has_close_confirmation(child, children, close_required))
        })
}

fn process_close_dialogs(world: &mut World) {
    let resolved = world.resource_mut::<CloseDialog>().poll();
    if let Some((target, confirmed)) = resolved
        && confirmed
    {
        target.confirm(world);
    }
    if world.resource::<CloseDialog>().0.is_some() {
        return;
    }

    let target = world
        .query_filtered::<Entity, (With<PendingPaneClose>, With<Pane>)>()
        .iter(world)
        .next()
        .map(CloseTarget::Pane)
        .or_else(|| {
            world
                .query_filtered::<Entity, (With<PendingStackClose>, With<Stack>)>()
                .iter(world)
                .next()
                .map(CloseTarget::Stack)
        });
    let Some(target) = target else {
        return;
    };
    let entity = match target {
        CloseTarget::Pane(entity) | CloseTarget::Stack(entity) => entity,
    };
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut
            .remove::<PendingPaneClose>()
            .remove::<PendingStackClose>();
    }
    let wake = world
        .get_resource::<bevy::winit::EventLoopProxyWrapper>()
        .map(|proxy| (**proxy).clone());
    world.resource_mut::<CloseDialog>().start(target, wake);
}

fn process_force_pane_closes(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, (With<ForcePaneClose>, With<Pane>)>()
        .iter(world)
        .collect();
    for pane in pending {
        let Ok(mut entity_mut) = world.get_entity_mut(pane) else {
            continue;
        };
        entity_mut.remove::<ForcePaneClose>();
        entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));

        let mut current = pane;
        for _ in 0..10 {
            if world
                .get_entity(current)
                .is_ok_and(|entity| entity.contains::<crate::tab::Tab>())
            {
                if let Ok(mut entity_mut) = world.get_entity_mut(current) {
                    entity_mut.insert(LastActivatedAt::now());
                }
                break;
            }
            if let Some(child_of) = world.get::<ChildOf>(current) {
                current = child_of.get();
            } else {
                break;
            }
        }
        world
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::{ClosingWindow, PrimaryWindow};

    fn place_pane(app: &mut App, parent: Entity, center: Vec2, size: Vec2) -> Entity {
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(parent),
                ComputedNode {
                    size,
                    center,
                    ..default()
                },
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
        pane
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ClosePlugin))
            .add_message::<CloseRequest>()
            .init_resource::<ConfirmCloseSettings>()
            .add_message::<PageOpenRequest>();
        app
    }

    #[test]
    fn force_pane_close_dispatches_pane_close_without_dialog() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseRequest>();
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab), ForcePaneClose))
            .id();

        process_force_pane_closes(app.world_mut());

        assert!(app.world().get::<ForcePaneClose>(pane).is_none());
        assert!(app.world().get::<CloseConfirmed>(pane).is_some());
        let closes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .drain()
            .filter(|request| matches!(request, CloseRequest))
            .collect();
        assert_eq!(closes.len(), 1);
    }

    #[test]
    fn confirmed_close_dialog_dispatches_pane_close() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseRequest>();
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .insert_resource(CloseDialog::ready(CloseTarget::Pane(pane), true));

        process_close_dialogs(app.world_mut());

        assert!(app.world().get::<CloseConfirmed>(pane).is_some());
        let closes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .drain()
            .filter(|request| matches!(request, CloseRequest))
            .collect();
        assert_eq!(closes.len(), 1);
    }

    #[test]
    fn closing_last_pane_keeps_window_with_fresh_stack() {
        let mut app = app();
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(!app.world().entity(window).contains::<ClosingWindow>());
        let mut panes = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
        assert_eq!(panes.iter(app.world()).count(), 1);
        let mut stacks = app.world_mut().query_filtered::<Entity, With<Stack>>();
        assert_eq!(stacks.iter(app.world()).count(), 1);
    }

    #[test]
    fn closing_pane_preserves_surviving_split_direction() {
        let mut app = app();
        app.world_mut().spawn(PrimaryWindow);
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        let left_top = place_pane(
            &mut app,
            left,
            Vec2::new(200.0, 200.0),
            Vec2::new(400.0, 400.0),
        );
        let left_bottom = place_pane(
            &mut app,
            left,
            Vec2::new(200.0, 600.0),
            Vec2::new(400.0, 400.0),
        );
        let right = place_pane(
            &mut app,
            root,
            Vec2::new(600.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        app.world_mut()
            .entity_mut(right)
            .insert(LastActivatedAt::now());
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(right).is_err());
        let split = app.world().get::<PaneSplit>(root).unwrap();
        assert_eq!(split.direction, PaneSplitDirection::Column);
        assert_eq!(
            app.world().get::<Node>(root).unwrap().flex_direction,
            FlexDirection::Column
        );
        let leaves: Vec<Entity> = app.world().get::<Children>(root).unwrap().iter().collect();
        assert_eq!(leaves.len(), 2);
        assert!(leaves.contains(&left_top) && leaves.contains(&left_bottom));
    }

    #[test]
    fn closing_one_of_three_siblings_keeps_split_intact() {
        let mut app = app();
        app.world_mut().spawn(PrimaryWindow);
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab),
            ))
            .id();
        let a = place_pane(
            &mut app,
            root,
            Vec2::new(200.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        let b = place_pane(
            &mut app,
            root,
            Vec2::new(600.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        let c = place_pane(
            &mut app,
            root,
            Vec2::new(1000.0, 400.0),
            Vec2::new(400.0, 800.0),
        );
        app.world_mut().entity_mut(a).insert(LastActivatedAt(10));
        app.world_mut().entity_mut(c).insert(LastActivatedAt(20));
        app.world_mut().entity_mut(b).insert(LastActivatedAt(30));
        app.world_mut()
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

        app.update();

        assert!(app.world().get_entity(b).is_err());
        let split = app.world().get::<PaneSplit>(root).unwrap();
        assert_eq!(split.direction, PaneSplitDirection::Row);
        let children: Vec<Entity> = app.world().get::<Children>(root).unwrap().iter().collect();
        assert_eq!(children.len(), 2);
        assert!(children.contains(&a) && children.contains(&c));
        for survivor in [a, c] {
            assert!(
                app.world()
                    .get::<Children>(survivor)
                    .is_some_and(|children| {
                        children
                            .iter()
                            .any(|entity| app.world().get::<Stack>(entity).is_some())
                    })
            );
        }
    }
}
