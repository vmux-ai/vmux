use crate::command::{AppCommand, PaneCommand, TabCommand};
use crate::layout::pane::Pane;
use crate::layout::space::Space;
use crate::layout::tab::Tab;
use crate::settings::AppSettings;
use crate::terminal::PtyExited;
use bevy::ecs::message::Messages;
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::ClosingWindow;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use vmux_history::LastActivatedAt;

/// Marker: tab is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingTabClose;

/// Marker: pane is waiting for close confirmation dialog.
#[derive(Component)]
pub struct PendingPaneClose;

/// Marker: close was confirmed, skip dialog next time.
#[derive(Component)]
pub struct CloseConfirmed;

/// Resource: window entity awaiting quit confirmation dialog.
#[derive(Resource, Default)]
pub struct PendingWindowClose {
    pub window: Option<Entity>,
}

pub struct ConfirmClosePlugin;

impl Plugin for ConfirmClosePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingWindowClose>()
            .add_systems(Update, show_pending_close_dialogs);
    }
}

/// Check if confirmation is needed based on settings.
pub fn should_confirm(settings: &AppSettings) -> bool {
    settings
        .terminal
        .as_ref()
        .is_none_or(|t| t.confirm_close)
}

/// Check if a tab entity has any child terminal that is still running.
pub fn has_live_terminal(
    tab: Entity,
    children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(children) = children_q.get(tab) {
        children.iter().any(|child| terminal_q.contains(child))
    } else {
        false
    }
}

/// Check if a pane has any tab with a live terminal.
pub fn pane_has_live_terminal(
    pane: Entity,
    pane_children_q: &Query<&Children, With<Pane>>,
    all_children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(tabs) = pane_children_q.get(pane) {
        tabs.iter()
            .any(|tab| has_live_terminal(tab, all_children_q, terminal_q))
    } else {
        false
    }
}

/// Show confirmation dialog for quitting with N running terminals.
/// Returns `true` if user confirms the quit.
pub fn confirm_quit_dialog(count: usize) -> bool {
    let msg = if count == 1 {
        "A terminal is still running. Quit anyway?".to_string()
    } else {
        format!("{count} terminals are still running. Quit anyway?")
    };
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Quit Vmux?")
        .set_description(&msg)
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

/// Exclusive system: processes pending close confirmations by showing native
/// dialogs on the main thread. Runs each frame but only does work when
/// pending markers exist.
fn show_pending_close_dialogs(world: &mut World) {
    process_pending_tab_closes(world);
    process_pending_pane_closes(world);
    process_pending_window_close(world);
}

fn process_pending_tab_closes(world: &mut World) {
    let pending: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &ChildOf), (With<PendingTabClose>, With<Tab>)>()
        .iter(world)
        .map(|(tab, co)| (tab, co.get()))
        .collect();

    if pending.is_empty() {
        return;
    }

    for (tab, pane) in pending {
        let confirmed = show_close_dialog();

        // Remove pending marker regardless of result
        if let Ok(mut entity_mut) = world.get_entity_mut(tab) {
            entity_mut.remove::<PendingTabClose>();
        }

        if confirmed {
            // Mark as confirmed and activate so TabCommand::Close targets it
            if let Ok(mut entity_mut) = world.get_entity_mut(tab) {
                entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));
            }
            if let Ok(mut entity_mut) = world.get_entity_mut(pane) {
                entity_mut.insert(LastActivatedAt::now());
            }
            // Activate the space containing this pane
            activate_space_for(world, pane);
            // Re-send close command
            world
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Tab(TabCommand::Close));
        }
    }
}

fn process_pending_pane_closes(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, (With<PendingPaneClose>, With<Pane>)>()
        .iter(world)
        .collect();

    if pending.is_empty() {
        return;
    }

    for pane in pending {
        let confirmed = show_close_dialog();

        if let Ok(mut entity_mut) = world.get_entity_mut(pane) {
            entity_mut.remove::<PendingPaneClose>();
        }

        if confirmed {
            if let Ok(mut entity_mut) = world.get_entity_mut(pane) {
                entity_mut.insert((CloseConfirmed, LastActivatedAt::now()));
            }
            activate_space_for(world, pane);
            world
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Pane(PaneCommand::Close));
        }
    }
}

fn process_pending_window_close(world: &mut World) {
    let window = world.resource::<PendingWindowClose>().window;
    let Some(window) = window else {
        return;
    };

    // Clear pending immediately
    world.resource_mut::<PendingWindowClose>().window = None;

    let mut query = world.query_filtered::<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>();
    let count = query.iter(world).count();

    if (count == 0 || confirm_quit_dialog(count))
        && let Ok(mut entity_mut) = world.get_entity_mut(window)
    {
        entity_mut.insert(ClosingWindow);
    }
}

fn show_close_dialog() -> bool {
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Close Terminal?")
        .set_description("A process is still running in this terminal. Close anyway?")
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

/// Walk up the entity hierarchy from `entity` to find and activate its Space.
fn activate_space_for(world: &mut World, entity: Entity) {
    let mut current = entity;
    for _ in 0..10 {
        if world
            .get_entity(current)
            .is_ok_and(|e| e.contains::<Space>())
        {
            if let Ok(mut entity_mut) = world.get_entity_mut(current) {
                entity_mut.insert(LastActivatedAt::now());
            }
            return;
        }
        if let Some(co) = world.get::<ChildOf>(current) {
            current = co.get();
        } else {
            return;
        }
    }
}
