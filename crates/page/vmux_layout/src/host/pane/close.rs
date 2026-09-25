#[cfg(test)]
use bevy::ecs::message::Messages;
use bevy::{
    ecs::relationship::Relationship,
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
            .add_message::<PaneCloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(
                Update,
                (request_pane_close, close_panes)
                    .chain()
                    .in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                Update,
                (
                    poll_close_dialogs,
                    process_force_pane_closes,
                    apply_close_dialog_results,
                    start_close_dialogs,
                )
                    .chain()
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

#[derive(Message)]
struct CloseDialogResult {
    target: CloseTarget,
    confirmed: bool,
}

#[derive(Component)]
struct CloseDialogOperation {
    target: CloseTarget,
    task: Task<bool>,
}

impl CloseDialogOperation {
    fn new(
        target: CloseTarget,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> Self {
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
        Self { target, task }
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
                super::set_split_direction(world, parent, direction);
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

fn poll_close_dialogs(
    mut dialogs: Query<(Entity, &mut CloseDialogOperation)>,
    mut results: MessageWriter<CloseDialogResult>,
    mut commands: Commands,
) {
    for (entity, mut dialog) in &mut dialogs {
        let Some(confirmed) = future::block_on(future::poll_once(&mut dialog.task)) else {
            continue;
        };
        results.write(CloseDialogResult {
            target: dialog.target,
            confirmed,
        });
        commands.entity(entity).despawn();
    }
}

fn process_force_pane_closes(
    pending: Query<Entity, (With<ForcePaneClose>, With<Pane>)>,
    mut results: MessageWriter<CloseDialogResult>,
    mut commands: Commands,
) {
    for pane in &pending {
        commands.entity(pane).remove::<ForcePaneClose>();
        results.write(CloseDialogResult {
            target: CloseTarget::Pane(pane),
            confirmed: true,
        });
    }
}

fn apply_close_dialog_results(
    mut results: MessageReader<CloseDialogResult>,
    child_of: Query<&ChildOf>,
    tabs: Query<(), With<crate::tab::Tab>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<(), With<Stack>>,
    stack_times: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut close_requests: MessageWriter<CloseRequest>,
    mut commands: Commands,
) {
    for result in results.read() {
        if !result.confirmed {
            continue;
        }
        match result.target {
            CloseTarget::Pane(pane) => {
                commands
                    .entity(pane)
                    .insert((CloseConfirmed, LastActivatedAt::now()));
                let mut current = pane;
                for _ in 0..10 {
                    if tabs.contains(current) {
                        commands.entity(current).insert(LastActivatedAt::now());
                        break;
                    }
                    let Ok(parent) = child_of.get(current) else {
                        break;
                    };
                    current = parent.get();
                }
                close_requests.write(CloseRequest);
            }
            CloseTarget::Stack(stack) => {
                let Ok(parent) = child_of.get(stack) else {
                    continue;
                };
                let parent_pane = parent.get();
                let sibling_stacks: Vec<Entity> = pane_children
                    .get(parent_pane)
                    .map(|children| {
                        children
                            .iter()
                            .filter(|entity| *entity != stack && stacks.contains(*entity))
                            .collect()
                    })
                    .unwrap_or_default();
                let was_active = pane_children.get(parent_pane).is_ok_and(|children| {
                    children
                        .iter()
                        .filter_map(|entity| stack_times.get(entity).ok())
                        .max_by_key(|(_, timestamp)| timestamp.0)
                        .map(|(entity, _)| entity)
                        == Some(stack)
                });
                commands.entity(stack).despawn();
                if was_active && let Some(next) = sibling_stacks.first() {
                    commands.entity(*next).insert(LastActivatedAt::now());
                }
            }
        }
    }
}

fn start_close_dialogs(
    dialogs: Query<(), With<CloseDialogOperation>>,
    pending_panes: Query<Entity, (With<PendingPaneClose>, With<Pane>)>,
    pending_stacks: Query<Entity, (With<PendingStackClose>, With<Stack>)>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !dialogs.is_empty() {
        return;
    }
    let target = pending_panes
        .iter()
        .next()
        .map(CloseTarget::Pane)
        .or_else(|| pending_stacks.iter().next().map(CloseTarget::Stack));
    let Some(target) = target else {
        return;
    };
    let entity = match target {
        CloseTarget::Pane(entity) | CloseTarget::Stack(entity) => entity,
    };
    commands
        .entity(entity)
        .remove::<PendingPaneClose>()
        .remove::<PendingStackClose>();
    let wake = wake.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(CloseDialogOperation::new(target, wake));
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
            .add_message::<CloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(
                Update,
                (process_force_pane_closes, apply_close_dialog_results).chain(),
            );
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab), ForcePaneClose))
            .id();

        app.update();

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
            .add_message::<CloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(Update, apply_close_dialog_results);
        let tab = app
            .world_mut()
            .spawn((crate::tab::Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CloseDialogResult>>()
            .write(CloseDialogResult {
                target: CloseTarget::Pane(pane),
                confirmed: true,
            });
        app.update();

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
