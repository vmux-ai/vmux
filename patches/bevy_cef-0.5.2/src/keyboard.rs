use crate::common::{CefKeyboardTarget, CefSuppressKeyboardInput, WebviewSource};
use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_cef_core::prelude::{Browsers, create_cef_key_events, keyboard_modifiers};
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
    mut forwarded_presses: Local<Vec<KeyCode>>,
) {
    let modifiers = keyboard_modifiers(&input);
    targeted_buf.clear();
    targeted_buf.extend(webviews_targeted.iter());
    let use_targets = !targeted_buf.is_empty();
    for event in er.read() {
        cef::do_message_loop_work();
        if suppress.0 {
            continue;
        }
        if event.state == ButtonState::Released {
            let was_tracked = forwarded_presses.contains(&event.key_code);
            forwarded_presses.retain(|k| *k != event.key_code);
            if is_non_character_key(event.key_code) || was_tracked {
                cef::do_message_loop_work();
                continue;
            }
        }
        let needs_dedup =
            is_non_character_key(event.key_code) || is_emacs_nav_key(event.key_code, &input);
        if event.state == ButtonState::Pressed && !event.repeat && needs_dedup {
            if forwarded_presses.contains(&event.key_code) {
                cef::do_message_loop_work();
                continue;
            }
            forwarded_presses.push(event.key_code);
        }
        if event.key_code == KeyCode::Enter && is_ime_commiting.0 {
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
        let key_events = create_cef_key_events(modifiers, &input, event);
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

#[cfg(target_os = "macos")]
fn is_emacs_nav_key(key: KeyCode, input: &ButtonInput<KeyCode>) -> bool {
    let ctrl = input.pressed(KeyCode::ControlLeft) || input.pressed(KeyCode::ControlRight);
    ctrl && matches!(
        key,
        KeyCode::KeyA
            | KeyCode::KeyE
            | KeyCode::KeyF
            | KeyCode::KeyB
            | KeyCode::KeyN
            | KeyCode::KeyP
            | KeyCode::KeyJ
            | KeyCode::KeyK
            | KeyCode::KeyD
            | KeyCode::KeyH
    )
}

#[cfg(not(target_os = "macos"))]
fn is_emacs_nav_key(_key: KeyCode, _input: &ButtonInput<KeyCode>) -> bool {
    false
}

fn ime_event(
    mut er: MessageReader<Ime>,
    mut is_ime_commiting: ResMut<IsImeCommiting>,
    browsers: NonSend<Browsers>,
    webviews_targeted: Query<Entity, (With<WebviewSource>, With<CefKeyboardTarget>)>,
    mut targeted_buf: Local<Vec<Entity>>,
    suppress: Res<CefSuppressKeyboardInput>,
) {
    targeted_buf.clear();
    targeted_buf.extend(webviews_targeted.iter());
    let use_targets = !targeted_buf.is_empty();
    for event in er.read() {
        if suppress.0 {
            continue;
        }
        match event {
            Ime::Preedit { value, cursor, .. } => {
                let cursor = cursor.map(|(_, e)| e as u32);
                if use_targets {
                    for webview in targeted_buf.iter() {
                        browsers.set_ime_composition_for(webview, value, cursor);
                    }
                } else {
                    browsers.set_ime_composition(value, cursor);
                }
            }
            Ime::Commit { value, .. } => {
                if use_targets {
                    for webview in targeted_buf.iter() {
                        browsers.set_ime_commit_text_for(webview, value);
                    }
                } else {
                    browsers.set_ime_commit_text(value);
                }
                is_ime_commiting.0 = true;
            }
            Ime::Disabled { .. } => {
                if use_targets {
                    for webview in targeted_buf.iter() {
                        browsers.ime_cancel_composition_for(webview);
                    }
                } else {
                    browsers.ime_cancel_composition();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use bevy::prelude::{ButtonInput, KeyCode};

    #[test]
    fn ime_events_route_to_keyboard_targets() {
        let implementation = include_str!("keyboard.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("webviews_targeted"));
        assert!(implementation.contains("set_ime_composition_for"));
        assert!(implementation.contains("set_ime_commit_text_for"));
        assert!(implementation.contains("ime_cancel_composition_for"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ctrl_j_k_are_deduped_as_macos_nav_keys() {
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::ControlLeft);

        assert!(super::is_emacs_nav_key(KeyCode::KeyJ, &input));
        assert!(super::is_emacs_nav_key(KeyCode::KeyK, &input));
    }
}
