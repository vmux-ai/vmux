use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_layout::Active;

use crate::VmuxWebview;

/// Reports `location.href` to Bevy via `window.cef.emit({ url })` (pageshow, SPA history, retry until `cef` exists).
pub(crate) const URL_TRACK_PRELOAD: &str = r#"(function(){function e(){try{if(typeof window!=="undefined"&&window.cef&&typeof window.cef.emit==="function")window.cef.emit({url:location.href});}catch(_){}}function t(){e()}var n=history.pushState,r=history.replaceState;history.pushState=function(){n.apply(history,arguments);setTimeout(t,0)};history.replaceState=function(){r.apply(history,arguments);setTimeout(t,0)};window.addEventListener("popstate",function(){setTimeout(t,0)});window.addEventListener("pageshow",function(){setTimeout(t,0)});var i=0,o=setInterval(function(){e();(window.cef&&window.cef.emit||++i>200)&&clearInterval(o)},50)})();"#;

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

/// Chrome: ⌘R on macOS, Ctrl+R on Windows/Linux.
fn chrome_reload_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    if !keys.just_pressed(KeyCode::KeyR) {
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
    if !chrome_reload_pressed(&keys) {
        return;
    }
    if let Ok(webview) = webviews.single() {
        commands.trigger(RequestReload { webview });
    }
}
