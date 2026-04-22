use crate::common::{CefKeyboardTarget, CefSuppressKeyboardInput, WebviewSource};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy_cef_core::prelude::{Browsers, create_cef_key_events, keyboard_modifiers};
use cef;
use serde::{Deserialize, Serialize};

/// [`SystemSet`] for systems that forward keyboard and IME input to CEF (runs in [`PreUpdate`]
/// after [`InputSystems`](bevy::input::InputSystems)). Schedule app logic that must run before
/// delivery with `.before(CefKeyboardInputSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CefKeyboardInputSet;

/// The plugin to handle keyboard inputs.
///
/// To use IME, you need to set [`Window::ime_enabled`](bevy::prelude::Window) to `true`.
pub(super) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IsImeCommiting>().add_systems(
            PreUpdate,
            (
                // Workaround for bevy_winit not calling `set_ime_allowed` on initial window
                // creation when `Window::ime_enabled` is `true` from the start.
                activate_ime,
                ime_event
                    .run_if(on_message::<Ime>)
                    .in_set(CefKeyboardInputSet),
                send_key_event
                    .run_if(on_message::<KeyboardInput>)
                    .in_set(CefKeyboardInputSet),
            )
                .chain()
                .after(InputSystems),
        );
    }
}

/// Workaround: bevy_winit does not call `winit::Window::set_ime_allowed()` during initial window
/// creation when `Window::ime_enabled` is `true`. This means `Ime` events are never generated.
///
/// To trigger bevy_winit's own `changed_windows` system, we temporarily toggle `ime_enabled` off
/// then back on over two frames, which causes the change detection to fire and call
/// `set_ime_allowed(true)` internally.
fn activate_ime(mut windows: Query<&mut Window>, mut state: Local<ImeActivationState>) {
    match *state {
        ImeActivationState::Pending => {
            for mut window in windows.iter_mut() {
                if window.ime_enabled {
                    window.ime_enabled = false;
                    *state = ImeActivationState::Toggled;
                }
            }
        }
        ImeActivationState::Toggled => {
            for mut window in windows.iter_mut() {
                if !window.ime_enabled {
                    window.ime_enabled = true;
                    *state = ImeActivationState::Done;
                }
            }
        }
        ImeActivationState::Done => {}
    }
}

#[derive(Default)]
enum ImeActivationState {
    #[default]
    Pending,
    Toggled,
    Done,
}

#[derive(Resource, Default, Serialize, Deserialize, Reflect)]
#[reflect(Default, Serialize, Deserialize)]
struct IsImeCommiting(bool);

fn send_key_event(
    mut er: MessageReader<KeyboardInput>,
    mut is_ime_commiting: ResMut<IsImeCommiting>,
    input: Res<ButtonInput<KeyCode>>,
    browsers: NonSend<Browsers>,
    webviews_all: Query<Entity, With<WebviewSource>>,
    webviews_targeted: Query<Entity, (With<WebviewSource>, With<CefKeyboardTarget>)>,
    mut targeted_buf: Local<Vec<Entity>>,
    suppress: Res<CefSuppressKeyboardInput>,
) {
    let modifiers = keyboard_modifiers(&input);
    targeted_buf.clear();
    targeted_buf.extend(webviews_targeted.iter());
    let use_targets = !targeted_buf.is_empty();
    // Track non-character keys already sent as Pressed this batch to deduplicate.
    // On macOS, bevy_winit can deliver two KeyboardInput(Pressed) messages for a
    // single physical press of navigation/editing keys (arrows, Delete, etc.).
    let mut non_char_pressed: Vec<KeyCode> = Vec::new();
    for event in er.read() {
        // Drain browser-process work before/after each key so IPC isn't still queued when the next
        // frame's Main pump runs (reduces randomly dropped characters under load).
        cef::do_message_loop_work();
        if suppress.0 {
            continue;
        }
        // Deduplicate non-character pressed keys.
        // On macOS, bevy_winit can deliver two Pressed messages for a single
        // physical press of navigation/editing keys.  The duplicates may arrive
        // in the same frame (caught by `non_char_pressed` vec) **or** across
        // consecutive frames (caught by `ButtonInput::just_pressed`).
        if event.state == ButtonState::Pressed && !event.repeat {
            if is_non_character_key(event.key_code) {
                // Same-frame dedup: skip if we already forwarded this key code
                // in the current batch.
                if non_char_pressed.contains(&event.key_code) {
                    cef::do_message_loop_work();
                    continue;
                }
                non_char_pressed.push(event.key_code);
                // Cross-frame dedup: `ButtonInput::just_pressed` is only true
                // on the first frame a key transitions to pressed.  A stale
                // duplicate arriving one frame later will have
                // `just_pressed == false` because the key is already held.
                if !input.just_pressed(event.key_code) {
                    cef::do_message_loop_work();
                    continue;
                }
            }
        }
        if event.key_code == KeyCode::Enter && is_ime_commiting.0 {
            // If the IME is committing, we don't want to send the Enter key event.
            // This is to prevent sending the Enter key event when the IME is committing.
            is_ime_commiting.0 = false;
            cef::do_message_loop_work();
            continue;
        }
        if event.state == ButtonState::Pressed {
            let mut handled_clipboard = false;
            if use_targets {
                for webview in targeted_buf.iter() {
                    if browsers.try_dispatch_clipboard_shortcut(
                        webview,
                        event.key_code,
                        modifiers,
                        event.state,
                    ) {
                        handled_clipboard = true;
                        break;
                    }
                }
            } else {
                for webview in webviews_all.iter() {
                    if browsers.try_dispatch_clipboard_shortcut(
                        &webview,
                        event.key_code,
                        modifiers,
                        event.state,
                    ) {
                        handled_clipboard = true;
                        break;
                    }
                }
            }
            if handled_clipboard {
                cef::do_message_loop_work();
                continue;
            }
        }
        let key_events = create_cef_key_events(modifiers, &input, &event);
        for key_event in key_events {
            if use_targets {
                for webview in targeted_buf.iter() {
                    browsers.send_key(webview, key_event.clone());
                }
            } else {
                for webview in webviews_all.iter() {
                    browsers.send_key(&webview, key_event.clone());
                }
            }
        }
        cef::do_message_loop_work();
    }
}

/// Returns true for key codes that do not produce character input (navigation,
/// modifiers, function keys, etc.).  Mirrors `is_not_character_key_code` in
/// `bevy_cef_core` but kept local to avoid coupling the dedup logic to the
/// core crate's internal classification.
fn is_non_character_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::ArrowLeft
            | KeyCode::ArrowUp
            | KeyCode::ArrowRight
            | KeyCode::ArrowDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Escape
            | KeyCode::Tab
            | KeyCode::Enter
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Insert
            | KeyCode::CapsLock
            | KeyCode::NumLock
            | KeyCode::ScrollLock
            | KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}

fn ime_event(
    mut er: MessageReader<Ime>,
    mut is_ime_commiting: ResMut<IsImeCommiting>,
    browsers: NonSend<Browsers>,
    suppress: Res<CefSuppressKeyboardInput>,
) {
    for event in er.read() {
        if suppress.0 {
            continue;
        }
        match event {
            Ime::Preedit { value, cursor, .. } => {
                browsers.set_ime_composition(value, cursor.map(|(_, e)| e as u32))
            }
            Ime::Commit { value, .. } => {
                browsers.set_ime_commit_text(value);
                is_ime_commiting.0 = true;
            }
            Ime::Disabled { .. } => {
                browsers.ime_cancel_composition();
            }
            _ => {}
        }
    }
}
