//! Systems registered by [`crate::VmuxPlugin`](super::VmuxPlugin) and sub-plugins.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::component::{AppAction, AppInputRoot, VmuxWebview, VmuxWorldCamera};
use crate::{CAMERA_DISTANCE, CEF_PAGE_ZOOM_LEVEL, WEBVIEW_URL};

pub(crate) fn spawn_app_input(mut commands: Commands) {
    let mut input_map = InputMap::<AppAction>::default();
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Super, KeyCode::KeyQ),
    );
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Control, KeyCode::KeyQ),
    );
    commands.spawn((AppInputRoot, input_map, ActionState::<AppAction>::default()));
}

pub(crate) fn exit_on_quit_action(
    query: Query<&ActionState<AppAction>, With<AppInputRoot>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok(state) = query.single() else {
        return;
    };
    if state.just_pressed(&AppAction::Quit) {
        app_exit.write(AppExit::Success);
    }
}

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        VmuxWorldCamera,
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0., 0., CAMERA_DISTANCE))
            .looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

pub(crate) fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
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

pub(crate) fn sync_webview_layout_size_to_window(
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

pub(crate) fn fit_webview_plane_to_window(
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
