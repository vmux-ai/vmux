use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use vmux_core::{CAMERA_DISTANCE, VmuxWorldCamera};

use crate::{CEF_PAGE_ZOOM_LEVEL, VmuxWebview, WEBVIEW_URL};

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
    webviews: Query<Entity, With<VmuxWebview>>,
) {
    let bracket = cfg!(target_os = "macos")
        && super_chord(&keys)
        && keys.just_pressed(KeyCode::BracketLeft);
    if !chrome_go_back_pressed(&keys) && !bracket {
        return;
    }
    for webview in webviews.iter() {
        commands.trigger(RequestGoBack { webview });
    }
}

/// macOS Chrome: ⌘] and ⌘→; other platforms: Alt+→.
pub fn go_forward(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    webviews: Query<Entity, With<VmuxWebview>>,
) {
    let bracket = cfg!(target_os = "macos")
        && super_chord(&keys)
        && keys.just_pressed(KeyCode::BracketRight);
    if !chrome_go_forward_pressed(&keys) && !bracket {
        return;
    }
    for webview in webviews.iter() {
        commands.trigger(RequestGoForward { webview });
    }
}

pub fn reload(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    webviews: Query<Entity, With<VmuxWebview>>,
) {
    if !chrome_reload_pressed(&keys) {
        return;
    }
    for webview in webviews.iter() {
        commands.trigger(RequestReload { webview });
    }
}

pub(crate) fn spawn_webview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.spawn((
        VmuxWebview,
        WebviewSource::new(WEBVIEW_URL),
        ZoomLevel(CEF_PAGE_ZOOM_LEVEL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
        MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                ..default()
            },
            extension: WebviewMaterial::default(),
        })),
    ));
}

pub fn sync_webview_layout_size_to_window(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<&Camera, With<VmuxWorldCamera>>,
    mut webview: Query<&mut WebviewSize, With<VmuxWebview>>,
) {
    let Ok(mut webview_size) = webview.single_mut() else {
        return;
    };

    let from_camera = camera.single().ok().and_then(|cam| {
        let sz = cam.logical_viewport_size()?;
        (sz.x > 0.0 && sz.y > 0.0 && sz.x.is_finite() && sz.y.is_finite()).then_some(sz)
    });

    let next = match from_camera {
        Some(sz) => sz,
        None => {
            let Ok(window) = window.single() else {
                return;
            };
            let w = window.width().max(1.0e-3);
            let h = window.height().max(1.0e-3);
            if !(w.is_finite() && h.is_finite()) {
                return;
            }
            Vec2::new(w, h)
        }
    };

    if webview_size.0 != next {
        webview_size.0 = next;
    }
}

pub fn fit_webview_plane_to_window(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &Projection), (With<Camera3d>, With<VmuxWorldCamera>)>,
    mut webview: Query<&mut Transform, With<VmuxWebview>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, projection)) = camera.single() else {
        return;
    };
    let Ok(mut transform) = webview.single_mut() else {
        return;
    };

    let Projection::Perspective(perspective) = projection else {
        return;
    };

    let w = window.width();
    let h = window.height();
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return;
    }

    let aspect = camera
        .logical_viewport_size()
        .filter(|s| s.x > 0.0 && s.y > 0.0 && s.x.is_finite() && s.y.is_finite())
        .map(|s| s.x / s.y)
        .unwrap_or(w / h);

    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;
    let new_scale = Vec3::new(half_w, half_h, 1.0);
    if (transform.scale.x - new_scale.x).abs() > 1e-4
        || (transform.scale.y - new_scale.y).abs() > 1e-4
    {
        transform.scale = new_scale;
    }
}
