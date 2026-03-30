use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_layout::{Active, VmuxWebview};

fn super_chord(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
}

#[cfg(not(target_os = "macos"))]
fn alt_chord(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight)
}

/// Chrome on macOS: ⌘[ / ⌘] and ⌘← / ⌘→.
/// Chrome on Windows/Linux: Alt+← / Alt+→.
fn chrome_go_back_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    if !keys.just_pressed(KeyCode::ArrowLeft) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        super_chord(keys)
    }
    #[cfg(not(target_os = "macos"))]
    {
        alt_chord(keys)
    }
}

fn chrome_go_forward_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    if !keys.just_pressed(KeyCode::ArrowRight) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        super_chord(keys)
    }
    #[cfg(not(target_os = "macos"))]
    {
        alt_chord(keys)
    }
}

fn shift_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

/// Chrome: ⌘R / Ctrl+R — normal reload (may use cache).
fn chrome_soft_reload_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    if !keys.just_pressed(KeyCode::KeyR) || shift_held(keys) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        super_chord(keys)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    }
}

/// Chrome: ⌘⇧R / Ctrl+Shift+R — hard reload (ignore cache).
fn chrome_hard_reload_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    if !keys.just_pressed(KeyCode::KeyR) || !shift_held(keys) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        super_chord(keys)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    }
}

/// macOS Chrome: ⌘[ and ⌘←; other platforms: Alt+←.
pub fn go_back(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    webviews: Query<Entity, (With<VmuxWebview>, With<Active>)>,
) {
    let bracket =
        cfg!(target_os = "macos") && super_chord(&keys) && keys.just_pressed(KeyCode::BracketLeft);
    if !chrome_go_back_pressed(&keys) && !bracket {
        return;
    }
    if let Ok(webview) = webviews.single() {
        commands.trigger(RequestGoBack { webview });
    }
}

/// macOS Chrome: ⌘] and ⌘→; other platforms: Alt+→.
pub fn go_forward(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    webviews: Query<Entity, (With<VmuxWebview>, With<Active>)>,
) {
    let bracket =
        cfg!(target_os = "macos") && super_chord(&keys) && keys.just_pressed(KeyCode::BracketRight);
    if !chrome_go_forward_pressed(&keys) && !bracket {
        return;
    }
    if let Ok(webview) = webviews.single() {
        commands.trigger(RequestGoForward { webview });
    }
}

pub fn reload(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    webviews: Query<Entity, (With<VmuxWebview>, With<Active>)>,
) {
    let Ok(webview) = webviews.single() else {
        return;
    };
    if chrome_hard_reload_pressed(&keys) {
        commands.trigger(RequestReloadIgnoreCache { webview });
        return;
    }
    if chrome_soft_reload_pressed(&keys) {
        commands.trigger(RequestReload { webview });
    }
}
