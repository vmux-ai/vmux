use crate::common::{CefKeyboardTarget, WebviewSource};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy_cef_core::prelude::{Browsers, create_cef_key_events, keyboard_modifiers};
use cef;
use serde::{Deserialize, Serialize};

/// [`SystemSet`] for systems that forward raw key events to CEF (runs in [`PreUpdate`] after
/// [`InputSystems`](bevy::input::InputSystems)). Schedule app logic that must run before key
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
                ime_event.run_if(on_message::<Ime>),
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
) {
    let modifiers = keyboard_modifiers(&input);
    targeted_buf.clear();
    targeted_buf.extend(webviews_targeted.iter());
    let use_targets = !targeted_buf.is_empty();
    for event in er.read() {
        // Drain browser-process work before/after each key so IPC isn't still queued when the next
        // frame's Main pump runs (reduces randomly dropped characters under load).
        cef::do_message_loop_work();
        if event.key_code == KeyCode::Enter && is_ime_commiting.0 {
            // If the IME is committing, we don't want to send the Enter key event.
            // This is to prevent sending the Enter key event when the IME is committing.
            is_ime_commiting.0 = false;
            cef::do_message_loop_work();
            continue;
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

fn ime_event(
    mut er: MessageReader<Ime>,
    mut is_ime_commiting: ResMut<IsImeCommiting>,
    browsers: NonSend<Browsers>,
) {
    for event in er.read() {
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
