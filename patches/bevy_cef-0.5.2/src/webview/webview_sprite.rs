use crate::common::{
    CefIgnorePinchZoom, CefPointerTarget, CefSuppressPointerInput, HistorySwipeVisualOffset,
    WebviewSize, WebviewSource, ZoomLevel,
};
use crate::webview::history_swipe::{HistorySwipeAction, HistorySwipeOutcome, HistorySwipeState};
use crate::webview::pinch_zoom::zoom_level_after_pinch;
use crate::webview::texture_upload::{WebviewTextureUploads, apply_webview_texture};
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_cef_core::prelude::{Browsers, RenderTextureMessage};
use std::fmt::Debug;
use std::time::Instant;

pub(in crate::webview) struct WebviewSpritePlugin;

impl Plugin for WebviewSpritePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<SpritePickingPlugin>() {
            app.add_plugins(SpritePickingPlugin);
        }

        app.add_systems(
            Update,
            (
                setup_observers,
                on_mouse_wheel.run_if(on_message::<MouseWheel>),
                on_pinch_zoom.run_if(on_message::<PinchGesture>),
            ),
        )
        .add_systems(
            PostUpdate,
            render.run_if(on_message::<RenderTextureMessage>),
        );
    }
}

fn render(
    mut er: MessageReader<RenderTextureMessage>,
    mut images: ResMut<Assets<bevy::prelude::Image>>,
    mut uploads: ResMut<WebviewTextureUploads>,
    webviews: Query<&Sprite, With<WebviewSource>>,
) {
    for texture in er.read() {
        if let Ok(sprite) = webviews.get(texture.webview) {
            apply_webview_texture(texture, &mut images, &sprite.image, &mut uploads);
        }
    }
}

fn setup_observers(
    mut commands: Commands,
    webviews: Query<Entity, (Added<WebviewSource>, With<Sprite>)>,
) {
    for entity in webviews.iter() {
        commands
            .entity(entity)
            .observe(apply_on_pointer_move)
            .observe(apply_on_pointer_pressed)
            .observe(apply_on_pointer_released);
    }
}

fn apply_on_pointer_move(
    trigger: On<Pointer<Move>>,
    input: Res<ButtonInput<MouseButton>>,
    browsers: NonSend<Browsers>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    webviews: Query<(&Sprite, &WebviewSize, &GlobalTransform)>,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some(pos) = obtain_relative_pos_from_trigger(&trigger, &webviews, &cameras) else {
        return;
    };
    browsers.send_mouse_move(&trigger.entity, input.get_pressed(), pos, false);
}

fn apply_on_pointer_pressed(
    trigger: On<Pointer<Press>>,
    browsers: NonSend<Browsers>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    webviews: Query<(&Sprite, &WebviewSize, &GlobalTransform)>,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some(pos) = obtain_relative_pos_from_trigger(&trigger, &webviews, &cameras) else {
        return;
    };
    browsers.send_mouse_click(&trigger.entity, pos, trigger.button, false);
}

fn apply_on_pointer_released(
    trigger: On<Pointer<Release>>,
    browsers: NonSend<Browsers>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    webviews: Query<(&Sprite, &WebviewSize, &GlobalTransform)>,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        return;
    }
    let Some(pos) = obtain_relative_pos_from_trigger(&trigger, &webviews, &cameras) else {
        return;
    };
    browsers.send_mouse_click(&trigger.entity, pos, trigger.button, true);
}

fn on_mouse_wheel(
    mut commands: Commands,
    mut er: MessageReader<MouseWheel>,
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &Sprite, &WebviewSize, &GlobalTransform)>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
    suppress: Res<CefSuppressPointerInput>,
    mut history_swipe: Local<HistorySwipeState>,
) {
    if suppress.0 {
        for _ in er.read() {}
        return;
    }
    for event in er.read() {
        let Ok(window) = windows.get(event.window) else {
            continue;
        };
        let Some(cursor_pos) = window.cursor_position() else {
            continue;
        };
        for (webview, sprite, webview_size, gtf) in webviews.iter() {
            let Some(pos) = obtain_relative_pos(sprite, webview_size, gtf, &cameras, cursor_pos)
            else {
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
    mut webviews: Query<
        (
            Entity,
            &Sprite,
            &WebviewSize,
            &GlobalTransform,
            &mut ZoomLevel,
            Has<CefPointerTarget>,
        ),
        Without<CefIgnorePinchZoom>,
    >,
    cameras: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
    suppress: Res<CefSuppressPointerInput>,
) {
    if suppress.0 {
        for _ in er.read() {}
        return;
    }

    let use_targets = webviews.iter().any(|(_, _, _, _, _, targeted)| targeted);
    for event in er.read() {
        let Some(cursor_pos) = windows.iter().find_map(Window::cursor_position) else {
            continue;
        };
        for (_, sprite, webview_size, gtf, mut zoom_level, targeted) in webviews.iter_mut() {
            if use_targets && !targeted {
                continue;
            }
            if obtain_relative_pos(sprite, webview_size, gtf, &cameras, cursor_pos).is_none() {
                continue;
            }
            zoom_level.0 = zoom_level_after_pinch(zoom_level.0, event.0 as f64);
            break;
        }
    }
}

fn obtain_relative_pos_from_trigger<E: Debug + Clone + Reflect>(
    trigger: &On<Pointer<E>>,
    webviews: &Query<(&Sprite, &WebviewSize, &GlobalTransform)>,
    cameras: &Query<(&Camera, &GlobalTransform)>,
) -> Option<Vec2> {
    let (sprite, webview_size, gtf) = webviews.get(trigger.entity).ok()?;
    obtain_relative_pos(
        sprite,
        webview_size,
        gtf,
        cameras,
        trigger.pointer_location.position,
    )
}

fn obtain_relative_pos(
    sprite: &Sprite,
    webview_size: &WebviewSize,
    transform: &GlobalTransform,
    cameras: &Query<(&Camera, &GlobalTransform)>,
    cursor_pos: Vec2,
) -> Option<Vec2> {
    let size = sprite.custom_size?;
    let viewport_pos = cameras.iter().find_map(|(camera, camera_gtf)| {
        camera
            .world_to_viewport(camera_gtf, transform.translation())
            .ok()
    })?;
    let relative_pos = (cursor_pos - viewport_pos + size / 2.0) / size;
    let px = webview_size.0;
    Some(Vec2::new(relative_pos.x * px.x, relative_pos.y * px.y))
}
