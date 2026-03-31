//! Sync `CefKeyboardTarget` with [`Active`] after splits / focus cycling, and in [`PreUpdate`]
//! before CEF key delivery. Pointer hover focus is set in [`PostUpdate`] by `vmux_layout` (layout
//! rects, before `apply_pane_layout`) so the next frame’s CEF picking matches [`Active`].
//!
//! [`consume_keyboard_for_prefix_routing`] drains [`KeyboardInput`] before CEF so chord keys
//! (e.g. **Ctrl+B** then **r**) are not typed into the focused webview; [`tmux_prefix_commands`]
//! still sees [`ButtonInput`] from [`InputSystems`].
//!
//! [`sync_cef_pointer_suppression_for_prefix`] sets [`CefSuppressPointerInput`] and
//! [`CefSuppressKeyboardInput`] in [`PreUpdate`] (same rules as below). It also suppresses
//! **keyboard** delivery for command-palette chords (⌘T/Ctrl+T, ⌘L/Ctrl+L) on the key-down frame so
//! the focused webview does not handle the shortcut (e.g. “new tab”). Draining [`KeyboardInput`]
//! here does not stop CEF from seeing the same messages (independent readers), so keyboard
//! blocking is enforced via [`CefSuppressKeyboardInput`] in bevy_cef’s key handler.

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, CefSuppressKeyboardInput, CefSuppressPointerInput};
use bevy_cef_core::prelude::Browsers;
use vmux_core::Active;
use vmux_core::VmuxCommandPaletteState;
use vmux_core::input_root::{AppInputRoot, VmuxPrefixState};
use vmux_layout::{History, Pane, WebviewPane};

/// ⌘T/Ctrl+T and ⌘L/Ctrl+L — suppress CEF for this frame so leafwing can drive the palette while a
/// webview has keyboard focus.
fn command_palette_chord_just_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    let key = if keys.just_pressed(KeyCode::KeyT) {
        true
    } else if keys.just_pressed(KeyCode::KeyL) {
        true
    } else {
        false
    };
    if !key {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    }
}

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
    palette: Option<Res<VmuxCommandPaletteState>>,
) {
    let palette_on = palette.map(|p| p.open).unwrap_or(false);
    let palette_hotkey = command_palette_chord_just_pressed(&keys);
    let Ok(prefix) = prefix_q.single() else {
        pointer.0 = palette_on;
        keyboard.0 = palette_on || palette_hotkey;
        return;
    };
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let just_b = keys.just_pressed(KeyCode::KeyB);
    let double_prefix = prefix.awaiting && ctrl && just_b;
    let first_prefix = !prefix.awaiting && ctrl && just_b;
    let chord_continuation = prefix.awaiting && !double_prefix;
    let prefix_on = first_prefix || chord_continuation;
    let on = prefix_on || palette_on;
    pointer.0 = on;
    keyboard.0 = on || palette_hotkey;
}

/// Match CEF OSR focus to the active pane so windowless browsers paint without waiting for a click.
///
/// History panes stay `set_focus(true)` even when the cursor/keyboard active pane is elsewhere, so
/// the WASM list keeps compositing and host-emitted updates stay visible in a split layout.
pub fn sync_cef_osr_focus_with_active_pane(
    browsers: NonSend<Browsers>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    history_panes: Query<Entity, (With<WebviewPane>, With<History>)>,
) {
    let active_ent = active.single().ok();
    let history_focus: Vec<Entity> = history_panes.iter().collect();
    browsers.sync_osr_focus_to_active_pane(active_ent, &history_focus);
}

pub fn sync_cef_keyboard_target(
    mut commands: Commands,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    panes: Query<Entity, With<Pane>>,
    mut last: Local<Option<(Entity, usize)>>,
) {
    let Ok(active_ent) = active.single() else {
        *last = None;
        return;
    };
    let pane_count = panes.iter().count();
    if let Some((e, n)) = *last {
        if e == active_ent && n == pane_count {
            return;
        }
    }
    *last = Some((active_ent, pane_count));
    for e in panes.iter() {
        if e == active_ent {
            commands.entity(e).insert(CefKeyboardTarget);
        } else {
            commands.entity(e).remove::<CefKeyboardTarget>();
        }
    }
}
