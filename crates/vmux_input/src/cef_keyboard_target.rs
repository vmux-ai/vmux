//! Sync `CefKeyboardTarget` with [`Active`] after splits / focus cycling, and in [`PreUpdate`]
//! before CEF key delivery. Pointer handlers in `pane_spawn` set both immediately on hover/press.
//!
//! [`consume_keyboard_for_prefix_routing`] drains [`KeyboardInput`] before CEF so chord keys
//! (e.g. **Ctrl+B** then **r**) are not typed into the focused webview; [`tmux_prefix_commands`]
//! still sees [`ButtonInput`] from [`InputSystems`].
//!
//! [`sync_cef_pointer_suppression_for_prefix`] sets [`CefSuppressPointerInput`] and
//! [`CefSuppressKeyboardInput`] in [`PreUpdate`] (same rules as below). Draining [`KeyboardInput`]
//! here does not stop CEF from seeing the same messages (independent readers), so keyboard
//! blocking is enforced via [`CefSuppressKeyboardInput`] in bevy_cef’s key handler.

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, CefSuppressKeyboardInput, CefSuppressPointerInput};
use vmux_core::Active;
use vmux_core::input_root::{AppInputRoot, VmuxPrefixState};
use vmux_layout::Pane;

/// Drop keyboard messages before they reach CEF while a tmux-style prefix chord is in progress.
pub fn consume_keyboard_for_prefix_routing(
    mut reader: MessageReader<KeyboardInput>,
    prefix_q: Query<&VmuxPrefixState, With<AppInputRoot>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let Ok(prefix) = prefix_q.single() else {
        return;
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let just_b = keys.just_pressed(KeyCode::KeyB);
    // Second Ctrl+B while armed: let keys through so the page can receive a literal prefix.
    let double_prefix = prefix.awaiting && ctrl && just_b;
    // First Ctrl+B of a chord: swallow so "b" is not typed into the webview.
    let first_prefix = !prefix.awaiting && ctrl && just_b;
    // After prefix, swallow command keys (r, m, o, …) but not the double-prefix escape.
    let chord_continuation = prefix.awaiting && !double_prefix;
    if !(first_prefix || chord_continuation) {
        return;
    }
    for _ in reader.read() {}
}

/// Skip CEF pointer forwarding while a tmux-style prefix chord is active (including the frame
/// of the first **Ctrl+B**).
pub fn sync_cef_pointer_suppression_for_prefix(
    mut pointer: ResMut<CefSuppressPointerInput>,
    mut keyboard: ResMut<CefSuppressKeyboardInput>,
    prefix_q: Query<&VmuxPrefixState, With<AppInputRoot>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let Ok(prefix) = prefix_q.single() else {
        pointer.0 = false;
        keyboard.0 = false;
        return;
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let just_b = keys.just_pressed(KeyCode::KeyB);
    let double_prefix = prefix.awaiting && ctrl && just_b;
    let first_prefix = !prefix.awaiting && ctrl && just_b;
    let chord_continuation = prefix.awaiting && !double_prefix;
    let on = first_prefix || chord_continuation;
    pointer.0 = on;
    keyboard.0 = on;
}

pub fn sync_cef_keyboard_target(
    mut commands: Commands,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
) {
    let Ok(active_ent) = active.single() else {
        return;
    };
    for e in panes.iter() {
        if e == active_ent {
            commands.entity(e).insert(CefKeyboardTarget);
        } else {
            commands.entity(e).remove::<CefKeyboardTarget>();
        }
    }
}
