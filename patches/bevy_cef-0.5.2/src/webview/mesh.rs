#[cfg(feature = "pbr")]
mod webview_extend_material;
mod webview_extend_standard_material;
mod webview_material;

pub use crate::common::*;
#[cfg(feature = "pbr")]
use crate::system_param::pointer::WebviewPointer;
#[cfg(feature = "pbr")]
use crate::webview::history_swipe::{
    HistorySwipeAction, HistorySwipeOutcome, HistorySwipeState, return_history_swipe_visual,
};
#[cfg(feature = "pbr")]
use crate::webview::pinch_zoom::zoom_level_after_pinch;
use crate::webview::webview_sprite::WebviewSpritePlugin;
#[cfg(not(feature = "pbr"))]
use bevy::asset::{AsAssetId, Asset, AssetApp, AssetId};
#[cfg(feature = "pbr")]
use bevy::input::gestures::PinchGesture;
#[cfg(feature = "pbr")]
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
#[cfg(feature = "pbr")]
use bevy::picking::Pickable;
#[cfg(feature = "pbr")]
use bevy::platform::collections::HashMap;
#[cfg(not(feature = "pbr"))]
use bevy::prelude::Deref;
#[cfg(feature = "pbr")]
pub use bevy::prelude::MeshMaterial3d as WebviewMaterialHandle;
use bevy::prelude::*;
#[cfg(feature = "pbr")]
use bevy_cef_core::prelude::*;
#[cfg(feature = "pbr")]
use std::time::Instant;
#[cfg(feature = "pbr")]
pub use webview_extend_material::*;
pub use webview_extend_standard_material::*;
pub use webview_material::*;

#[cfg(not(feature = "pbr"))]
#[derive(Component, Clone, Debug, Deref)]
#[require(Sprite = webview_sprite())]
pub struct WebviewMaterialHandle<M: Asset>(pub Handle<M>);

#[cfg(not(feature = "pbr"))]
fn webview_sprite() -> Sprite {
    Sprite {
        custom_size: Some(Vec2::ONE),
        ..default()
    }
}

#[cfg(not(feature = "pbr"))]
impl<M: Asset> From<&WebviewMaterialHandle<M>> for AssetId<M> {
    fn from(material: &WebviewMaterialHandle<M>) -> Self {
        material.id()
    }
}

#[cfg(not(feature = "pbr"))]
impl<M: Asset> AsAssetId for WebviewMaterialHandle<M> {
    type Asset = M;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.id()
    }
}

pub struct MeshWebviewPlugin;

impl Plugin for MeshWebviewPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "pbr")]
        if !app.is_plugin_added::<MeshPickingPlugin>() {
            app.add_plugins(MeshPickingPlugin);
        }

        app.add_plugins((
            WebviewMaterialPlugin,
            WebviewSpritePlugin,
            crate::webview::texture_upload::WebviewTextureUploadPlugin,
            crate::webview::accelerated_upload::WebviewAcceleratedUploadPlugin,
        ));

        #[cfg(not(feature = "pbr"))]
        app.init_asset::<WebviewExtendStandardMaterial>();

        #[cfg(feature = "pbr")]
        app.add_plugins(WebviewExtendStandardMaterialPlugin)
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

#[cfg(all(test, not(feature = "pbr")))]
mod user_mode_tests {
    use super::*;
    use bevy::asset::AssetPlugin;

    #[test]
    fn initializes_webview_material_assets() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshWebviewPlugin));

        assert!(
            app.world()
                .contains_resource::<Assets<WebviewExtendStandardMaterial>>()
        );
    }
}

#[cfg(feature = "pbr")]
fn setup_observers(
    mut commands: Commands,
    webviews: Query<
        Entity,
        (
            Added<WebviewSource>,
            Or<(With<Mesh3d>, With<Mesh2d>)>,
            Without<WebviewWindowed>,
        ),
    >,
) {
    for entity in webviews.iter() {
        commands
            .entity(entity)
            .observe(on_pointer_move)
            .observe(on_pointer_pressed)
            .observe(on_pointer_released);
    }
}

#[cfg(feature = "pbr")]
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

#[cfg(feature = "pbr")]
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

#[cfg(feature = "pbr")]
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

#[cfg(feature = "pbr")]
fn on_mouse_wheel(
    mut commands: Commands,
    mut er: MessageReader<MouseWheel>,
    browsers: NonSend<Browsers>,
    pointer: WebviewPointer,
    windows: Query<&Window>,
    webviews_all: Query<
        (Entity, Option<&Pickable>),
        (
            With<WebviewSource>,
            Or<(With<Mesh3d>, With<Mesh2d>)>,
            Without<WebviewWindowed>,
        ),
    >,
    webviews_targeted: Query<
        Entity,
        (
            With<WebviewSource>,
            With<CefPointerTarget>,
            Without<WebviewWindowed>,
        ),
    >,
    suppress: Res<CefSuppressPointerInput>,
    mut history_swipe: Local<HistorySwipeState>,
) {
    if suppress.0 {
        for _ in er.read() {}
        return;
    }
    let mut pending = HashMap::<Entity, (Vec2, Vec2)>::default();
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
                    pending
                        .entry(webview)
                        .and_modify(|(position, accumulated)| {
                            *position = pos;
                            *accumulated += delta;
                        })
                        .or_insert((pos, delta));
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
    for (webview, (position, delta)) in pending {
        browsers.send_mouse_wheel(&webview, position, delta);
    }
}

#[cfg(feature = "pbr")]
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

#[cfg(feature = "pbr")]
fn accepts_untargeted_pointer(pickable: Option<&Pickable>) -> bool {
    pickable.is_none_or(|p| p != &Pickable::IGNORE)
}

#[cfg(all(test, feature = "pbr"))]
mod tests {
    use super::*;

    #[test]
    fn untargeted_pointer_candidates_skip_ignored_pickables() {
        assert!(!accepts_untargeted_pointer(Some(&Pickable::IGNORE)));
        assert!(accepts_untargeted_pointer(Some(&Pickable::default())));
        assert!(accepts_untargeted_pointer(None));
    }
}
