#[cfg(test)]
use bevy::ecs::message::Messages;
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite::future},
};
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use crate::{
    CloseRequiresConfirmation,
    settings::ConfirmCloseSettings,
    stack::{ActiveTabParam, CloseStackRequest, LayoutFocus, Stack},
};

#[cfg(test)]
use super::PaneSplitDirection;
use super::{CloseRequest, Pane, PaneHierarchy, PaneSplit, PaneStacks};
use crate::host::command::LayoutRequestSet;

pub(super) struct ClosePlugin;

impl Plugin for ClosePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CloseRequest>()
            .add_message::<PaneCloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(
                Update,
                (request, close)
                    .chain()
                    .in_set(LayoutRequestSet::Handle)
                    .before(crate::stack::CloseStackSet),
            )
            .add_systems(
                Update,
                (
                    poll_dialogs,
                    process_force_closes,
                    apply_dialog_results,
                    start_dialogs,
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

#[derive(Message)]
struct CloseDialogResult {
    target: Entity,
    confirmed: bool,
}

#[derive(Component)]
struct CloseDialogOperation {
    target: Entity,
    task: Task<bool>,
}

fn request(
    mut reader: MessageReader<CloseRequest>,
    active_tab: ActiveTabParam,
    focus: LayoutFocus,
    all_children: Query<&Children>,
    pane_children: Query<&Children, With<Pane>>,
    close_required: Query<(), With<CloseRequiresConfirmation>>,
    pending: Query<(), With<PendingPaneClose>>,
    settings: Res<ConfirmCloseSettings>,
    mut requests: MessageWriter<PaneCloseRequest>,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let (_, Some(active), _) = focus.resolve(active_tab.get()) else {
            continue;
        };
        let needs_confirmation = settings.enabled
            && pane_has_close_confirmation(active, &pane_children, &all_children, &close_required);
        if needs_confirmation {
            if !pending.contains(active) {
                commands.entity(active).insert(PendingPaneClose);
            }
            continue;
        }

        requests.write(PaneCloseRequest(active));
    }
}

fn close(
    mut requests: MessageReader<PaneCloseRequest>,
    panes: PaneHierarchy,
    pane_times: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_stacks: PaneStacks,
    focus: LayoutFocus,
    child_of: Query<&ChildOf>,
    splits: Query<&PaneSplit>,
    stacks: Query<Entity, With<Stack>>,
    mut nodes: Query<&mut Node, With<Pane>>,
    focused_space: crate::space::FocusedSpace,
    mut stack_close_requests: MessageWriter<CloseStackRequest>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let active = request.0;
        let Ok(parent) = child_of.get(active).map(Relationship::get) else {
            continue;
        };
        if !splits.contains(parent) {
            if let Some(current) = focus.stack(active).or_else(|| pane_stacks.first(active)) {
                stack_close_requests.write(CloseStackRequest::reset_pane(current));
                continue;
            }
            let stack = commands
                .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(active)))
                .id();
            commands.entity(active).insert(LastActivatedAt::now());
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url: focused_space.resolved_startup_url(),
                request_id: None,
            });
            continue;
        }

        let Ok(children) = panes.children.get(parent) else {
            continue;
        };
        let siblings: Vec<Entity> = children
            .iter()
            .filter(|&entity| {
                entity != active && (panes.leaves.contains(entity) || splits.contains(entity))
            })
            .collect();

        if siblings.len() >= 2 {
            commands.entity(active).despawn();
            let newest = siblings
                .iter()
                .copied()
                .max_by_key(|&entity| pane_times.get(entity).map(|(_, time)| time.0).unwrap_or(0))
                .unwrap_or(siblings[0]);
            let leaf = panes.first_leaf(newest);
            commands.entity(leaf).insert(LastActivatedAt::now());
            if let Some(stack) = focus.stack(leaf).or_else(|| pane_stacks.first(leaf)) {
                commands.entity(stack).insert(LastActivatedAt::now());
            }
            continue;
        }

        let Some(sibling) = siblings.into_iter().next() else {
            continue;
        };
        let sibling_children: Vec<Entity> = panes
            .children
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
            new_active_pane = panes.first_leaf(sibling);
            commands.entity(sibling).remove::<ChildOf>();
            commands.entity(sibling).despawn();
            commands.entity(parent).insert(PaneSplit { direction });
            if let Ok(mut node) = nodes.get_mut(parent) {
                node.flex_direction = direction.flex_direction();
                let gaps = direction.gaps(crate::event::PANE_GAP_PX);
                node.column_gap = gaps.column_gap;
                node.row_gap = gaps.row_gap;
            }
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
        let active_stack = focus
            .stack(new_active_pane)
            .or_else(|| pane_stacks.first(new_active_pane))
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

fn poll_dialogs(
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

fn process_force_closes(
    pending: Query<Entity, (With<ForcePaneClose>, With<Pane>)>,
    mut results: MessageWriter<CloseDialogResult>,
    mut commands: Commands,
) {
    for pane in &pending {
        commands.entity(pane).remove::<ForcePaneClose>();
        results.write(CloseDialogResult {
            target: pane,
            confirmed: true,
        });
    }
}

fn apply_dialog_results(
    mut results: MessageReader<CloseDialogResult>,
    panes: Query<(), With<Pane>>,
    mut close_requests: MessageWriter<PaneCloseRequest>,
) {
    for result in results.read() {
        if !result.confirmed || !panes.contains(result.target) {
            continue;
        }
        close_requests.write(PaneCloseRequest(result.target));
    }
}

fn start_dialogs(
    dialogs: Query<(), With<CloseDialogOperation>>,
    pending_panes: Query<Entity, (With<PendingPaneClose>, With<Pane>)>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    if !dialogs.is_empty() {
        return;
    }
    let target = pending_panes.iter().next();
    let Some(target) = target else {
        return;
    };
    commands.entity(target).remove::<PendingPaneClose>();
    let wake = wake.as_deref().map(|proxy| (**proxy).clone());
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
    commands.spawn(CloseDialogOperation { target, task });
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
            .add_message::<CloseStackRequest>()
            .init_resource::<ConfirmCloseSettings>()
            .add_message::<PageOpenRequest>();
        app
    }

    #[test]
    fn force_pane_close_dispatches_pane_close_without_dialog() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<PaneCloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(Update, (process_force_closes, apply_dialog_results).chain());
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
        let closes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneCloseRequest>>()
            .drain()
            .collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].0, pane);
    }

    #[test]
    fn confirmed_close_dialog_dispatches_pane_close() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<PaneCloseRequest>()
            .add_message::<CloseDialogResult>()
            .add_systems(Update, apply_dialog_results);
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
                target: pane,
                confirmed: true,
            });
        app.update();

        let closes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneCloseRequest>>()
            .drain()
            .collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].0, pane);
    }

    #[test]
    fn closing_last_pane_delegates_to_the_stack_transition() {
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
        let stack =
            app.world_mut()
                .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));
        let stack = stack.id();
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
        let requests = app
            .world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].stack, stack);
        assert_eq!(
            requests[0].reason,
            crate::stack::CloseStackReason::ResetPane
        );
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
