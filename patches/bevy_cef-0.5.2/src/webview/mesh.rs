mod webview_extend_material;
mod webview_extend_standard_material;
mod webview_material;

pub use crate::common::*;
use crate::system_param::pointer::WebviewPointer;
use crate::webview::history_swipe::{
    HistorySwipeAction, HistorySwipeOutcome, HistorySwipeState, return_history_swipe_visual,
};
use crate::webview::pinch_zoom::zoom_level_after_pinch;
use crate::webview::webview_sprite::WebviewSpritePlugin;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use std::time::Instant;
pub use webview_extend_material::*;
pub use webview_extend_standard_material::*;
pub use webview_material::*;

pub struct MeshWebviewPlugin;

impl Plugin for MeshWebviewPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MeshPickingPlugin>() {
            app.add_plugins(MeshPickingPlugin);
        }

        app.add_plugins((
            WebviewMaterialPlugin,
            WebviewExtendStandardMaterialPlugin,
            WebviewSpritePlugin,
            crate::webview::texture_upload::WebviewTextureUploadPlugin,
        ))
        .add_systems(
            Update,
            (
                setup_observers,
                on_mouse_wheel.run_if(on_message::<MouseWheel>),
                on_pinch_zoom.run_if(on_message::<PinchGesture>),
                return_history_swipe_visual,
            ),
        );
    }
}

fn setup_observers(
    mut commands: Commands,
    webviews: Query<Entity, (Added<WebviewSource>, Or<(With<Mesh3d>, With<Mesh2d>)>)>,
) {
    for entity in webviews.iter() {
        commands
            .entity(entity)
            .observe(on_pointer_move)
            .observe(on_pointer_pressed)
            .observe(on_pointer_released);
    }
}

fn on_pointer_move(
    trigger: On<Pointer<Move>>,
    input: Res<ButtonInput<MouseButton>>,
    pointer: WebviewPointer,
    browsers: NonSend<Browsers>,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some((webview, pos)) = pointer.pos_from_trigger(&trigger) else {
        return;
    };

    browsers.send_mouse_move(&webview, input.get_pressed(), pos, false);
}

fn on_pointer_pressed(
    trigger: On<Pointer<Press>>,
    browsers: NonSend<Browsers>,
    pointer: WebviewPointer,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some((webview, pos)) = pointer.pos_from_trigger(&trigger) else {
        return;
    };
    browsers.send_mouse_click(&webview, pos, trigger.button, false);
}

fn on_pointer_released(
    trigger: On<Pointer<Release>>,
    browsers: NonSend<Browsers>,
    pointer: WebviewPointer,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some((webview, pos)) = pointer.pos_from_trigger(&trigger) else {
        return;
    };
    browsers.send_mouse_click(&webview, pos, trigger.button, true);
}

fn on_mouse_wheel(
    mut commands: Commands,
    mut er: MessageReader<MouseWheel>,
    browsers: NonSend<Browsers>,
    pointer: WebviewPointer,
    windows: Query<&Window>,
    webviews_all: Query<
        (Entity, Option<&Pickable>),
        (With<WebviewSource>, Or<(With<Mesh3d>, With<Mesh2d>)>),
    >,
    webviews_targeted: Query<Entity, (With<WebviewSource>, With<CefPointerTarget>)>,
    suppress: Res<CefSuppressPointerInput>,
    mut history_swipe: Local<HistorySwipeState>,
) {
    if suppress.0 {
        for _ in er.read() {}
        return;
    }
    let use_targets = webviews_targeted.iter().next().is_some();
    for event in er.read() {
        let Ok(window) = windows.get(event.window) else {
            continue;
        };
        let Some(cursor_pos) = window.cursor_position() else {
            continue;
        };
        let iter: Box<dyn Iterator<Item = Entity>> = if use_targets {
            Box::new(webviews_targeted.iter())
        } else {
            Box::new(webviews_all.iter().filter_map(|(entity, pickable)| {
                accepts_untargeted_pointer(pickable).then_some(entity)
            }))
        };
        for webview in iter {
            let Some(pos) = pointer.pointer_pos(webview, cursor_pos) else {
                continue;
            };
            let delta = match event.unit {
                MouseScrollUnit::Line => {
                    // CEF expects pixel deltas; Chromium default: 3 lines × 40px = 120px per notch
                    Vec2::new(event.x * 120.0, event.y * 120.0)
                }
                MouseScrollUnit::Pixel => Vec2::new(event.x, event.y),
            };
            match history_swipe.record(webview, event.unit, delta, Instant::now()) {
                HistorySwipeOutcome::PassThrough => {
                    commands
                        .entity(webview)
                        .insert(HistorySwipeVisualOffset::default());
                    browsers.send_mouse_wheel(&webview, pos, delta);
                }
                HistorySwipeOutcome::Consumed { visual } => {
                    commands
                        .entity(webview)
                        .insert(HistorySwipeVisualOffset::from(visual));
                }
                HistorySwipeOutcome::Navigate { action, visual } => {
                    commands
                        .entity(webview)
                        .insert(HistorySwipeVisualOffset::from(visual));
                    match action {
                        HistorySwipeAction::Back => browsers.go_back(&webview),
                        HistorySwipeAction::Forward => browsers.go_forward(&webview),
                    }
                }
            }
        }
    }
}

fn on_pinch_zoom(
    mut er: MessageReader<PinchGesture>,
    pointer: WebviewPointer,
    windows: Query<&Window>,
    mut webviews_all: Query<
        (Entity, &mut ZoomLevel, Option<&Pickable>),
        (
            With<WebviewSource>,
            Or<(With<Mesh3d>, With<Mesh2d>)>,
            Without<CefIgnorePinchZoom>,
        ),
    >,
    webviews_targeted: Query<
        Entity,
        (
            With<WebviewSource>,
            With<CefPointerTarget>,
            Without<CefIgnorePinchZoom>,
        ),
    >,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        for _ in er.read() {}
        return;
    }

    let use_targets = webviews_targeted.iter().next().is_some();
    for event in er.read() {
        let Some(cursor_pos) = windows.iter().find_map(Window::cursor_position) else {
            continue;
        };
        let candidates: Vec<Entity> = if use_targets {
            webviews_targeted.iter().collect()
        } else {
            webviews_all
                .iter()
                .filter_map(|(entity, _, pickable)| {
                    accepts_untargeted_pointer(pickable).then_some(entity)
                })
                .collect()
        };
        for webview in candidates {
            if pointer.pointer_pos(webview, cursor_pos).is_none() {
                continue;
            }
            let Ok((_, mut zoom_level, _)) = webviews_all.get_mut(webview) else {
                continue;
            };
            zoom_level.0 = zoom_level_after_pinch(zoom_level.0, event.0 as f64);
            break;
        }
    }
}

fn accepts_untargeted_pointer(pickable: Option<&Pickable>) -> bool {
    pickable.is_none_or(|p| p != &Pickable::IGNORE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untargeted_pointer_candidates_skip_ignored_pickables() {
        assert!(!accepts_untargeted_pointer(Some(&Pickable::IGNORE)));
        assert!(accepts_untargeted_pointer(Some(&Pickable::default())));
        assert!(accepts_untargeted_pointer(None));
    }
}
