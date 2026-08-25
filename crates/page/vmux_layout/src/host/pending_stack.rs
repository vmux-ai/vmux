use bevy::prelude::*;
use vmux_core::KeyboardOwner;
use vmux_core::launcher::{RestoreKeyboardToStack, StackInPaneChosen};

use crate::cef::Browser;
use crate::stack::Stack;

pub(crate) struct PendingStackPlugin;

impl Plugin for PendingStackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StackInPaneChosen>()
            .add_message::<RestoreKeyboardToStack>()
            .add_systems(
                Update,
                (focus_chosen_stack_in_pane, restore_keyboard_to_stack)
                    .before(crate::stack::ComputeFocusSet),
            );
    }
}

fn restore_keyboard_to_stack(
    mut requests: MessageReader<RestoreKeyboardToStack>,
    all_children: Query<&Children>,
    content_pages: Query<
        Entity,
        (
            With<Browser>,
            Without<crate::Header>,
            Without<crate::side_sheet::SideSheet>,
        ),
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(children) = all_children.get(request.stack) else {
            continue;
        };
        for child in children.iter() {
            if content_pages.contains(child) {
                commands.entity(child).try_insert(KeyboardOwner);
            }
        }
    }
}

fn focus_chosen_stack_in_pane(
    mut chosen: MessageReader<StackInPaneChosen>,
    leaf_panes: Query<Entity, (With<crate::pane::Pane>, Without<crate::pane::PaneSplit>)>,
    pane_children: Query<&Children, With<crate::pane::Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
) {
    for event in chosen.read() {
        let Some(pane) = leaf_panes.iter().find(|e| e.to_bits() == event.pane_bits) else {
            continue;
        };
        let stack = pane_children.get(pane).ok().and_then(|children| {
            children
                .iter()
                .filter(|&e| stack_q.contains(e))
                .nth(event.index)
        });
        vmux_core::focus_pane_entity(stack.unwrap_or(pane), &mut commands, &child_of_q);
    }
}
