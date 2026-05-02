use crate::chrome_state::WebviewChromeStateSender;
use crate::common::localhost::responser::{InlineHtmlId, InlineHtmlStore};
use crate::common::{
    HostWindow, IpcEventRawSender, ResolvedWebviewUri, WebviewSize, WebviewSource,
    WebviewTransparent,
};
use crate::cursor_icon::SystemCursorIconSender;
use crate::loading_state::WebviewLoadingStateSender;
use crate::popup_state::WebviewPopupSender;
use crate::prelude::PreloadScripts;
use crate::webview::mesh::MeshWebviewPlugin;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use bevy_cef_core::prelude::*;
use bevy_remote::BrpSender;
#[allow(deprecated)]
use raw_window_handle::HasRawWindowHandle;
use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod history_swipe;
mod mesh;
mod pinch_zoom;
mod webview_sprite;

const TEXTURE_WAKE_MIN_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Resource, Clone)]
struct TextureWakeCallback(Option<TextureWake>);

pub mod prelude {
    pub use crate::webview::{
        CefSystems, RequestCloseDevtool, RequestShowDevTool, WebviewPlugin, mesh::*,
    };
}

/// A Trigger event to request showing the developer tools in a webview.
///
/// When you want to close the developer tools, use [`RequestCloseDevtool`].
///
/// ```rust
/// use bevy::prelude::*;
/// use bevy_cef::prelude::*;
///
/// #[derive(Component)]
/// struct DebugWebview;
///
/// fn show_devtool_system(mut commands: Commands, webviews: Query<Entity, With<DebugWebview>>) {
///     let entity = webviews.single().unwrap();
///     commands.entity(entity).trigger(|webview| RequestShowDevTool { webview });
/// }
/// ```
#[derive(Reflect, Debug, Copy, Clone, Serialize, Deserialize, EntityEvent)]
#[reflect(Serialize, Deserialize)]
pub struct RequestShowDevTool {
    #[event_target]
    pub webview: Entity,
}

/// A Trigger event to request closing the developer tools in a webview.
///
/// When showing the devtool, use [`RequestShowDevTool`] instead.
///
/// ```rust
/// use bevy::prelude::*;
/// use bevy_cef::prelude::*;
///
/// #[derive(Component)]
/// struct DebugWebview;
///
/// fn close_devtool_system(mut commands: Commands, webviews: Query<Entity, With<DebugWebview>>) {
///     let entity = webviews.single().unwrap();
///     commands.entity(entity).trigger(|webview| RequestCloseDevtool { webview });
/// }
/// ```
#[derive(Reflect, Debug, Copy, Clone, Serialize, Deserialize, EntityEvent)]
#[reflect(Serialize, Deserialize)]
pub struct RequestCloseDevtool {
    #[event_target]
    pub webview: Entity,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CefSystems {
    /// Runs [`create_webview`], [`resize`], and [`navigate_on_source_change`].
    /// Order your spawn systems *before* this set so newly created browser entities
    /// are picked up in the same frame.
    CreateAndResize,
}

pub struct WebviewPlugin;

impl Plugin for WebviewPlugin {
    fn build(&self, app: &mut App) {
        let texture_wake = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(spawn_texture_wake_throttler);
        app.register_type::<RequestShowDevTool>()
            .init_resource::<CefDiskProfileRoot>()
            .init_non_send_resource::<Browsers>()
            .insert_resource(TextureWakeCallback(texture_wake))
            .add_plugins((MeshWebviewPlugin,))
            .add_systems(
                Update,
                (
                    resize.run_if(any_resized),
                    create_webview,
                    navigate_on_source_change,
                )
                    .in_set(CefSystems::CreateAndResize),
            )
            .add_observer(apply_request_show_devtool)
            .add_observer(apply_request_close_devtool);

        app.world_mut()
            .register_component_hooks::<WebviewSource>()
            .on_despawn(|mut world: DeferredWorld, ctx: HookContext| {
                world.non_send_resource_mut::<Browsers>().close(&ctx.entity);
            });

        app.world_mut()
            .register_component_hooks::<InlineHtmlId>()
            .on_remove(|mut world: DeferredWorld, ctx: HookContext| {
                // `on_remove` runs before the component is dropped; `get` should succeed. If it does
                // not (e.g. despawn edge cases), skip rather than panic — stale store entries are
                // bounded and harmless compared to crashing the host.
                if let Some(id) = world.get::<InlineHtmlId>(ctx.entity).map(|c| c.0.clone()) {
                    world.resource_mut::<InlineHtmlStore>().remove(&id);
                }
            });
    }
}

fn any_resized(webviews: Query<Entity, Changed<WebviewSize>>) -> bool {
    !webviews.is_empty()
}

fn spawn_texture_wake_throttler(wrapper: &bevy::winit::EventLoopProxyWrapper) -> TextureWake {
    let proxy = (**wrapper).clone();
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("cef-texture-wake-throttle".into())
        .spawn(move || {
            let mut last_fire: Option<Instant> = None;
            while rx.recv().is_ok() {
                if let Some(t) = last_fire {
                    let elapsed = Instant::now().duration_since(t);
                    if elapsed < TEXTURE_WAKE_MIN_INTERVAL {
                        std::thread::sleep(TEXTURE_WAKE_MIN_INTERVAL - elapsed);
                    }
                }
                while rx.try_recv().is_ok() {}
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
                last_fire = Some(Instant::now());
            }
        })
        .expect("failed to spawn cef-texture-wake-throttle thread");

    std::sync::Arc::new(move || {
        let _ = tx.send(());
    }) as TextureWake
}

#[allow(clippy::too_many_arguments)]
fn create_webview(
    mut browsers: NonSendMut<Browsers>,
    disk_profile: Res<CefDiskProfileRoot>,
    requester: Res<Requester>,
    ipc_event_sender: Res<IpcEventRawSender>,
    brp_sender: Res<BrpSender>,
    cursor_icon_sender: Res<SystemCursorIconSender>,
    loading_state_sender: Res<WebviewLoadingStateSender>,
    chrome_state_sender: Res<WebviewChromeStateSender>,
    popup_sender: Res<WebviewPopupSender>,
    texture_wake: Res<TextureWakeCallback>,
    webviews: Query<
        (
            Entity,
            &ResolvedWebviewUri,
            &WebviewSize,
            &PreloadScripts,
            Option<&HostWindow>,
            Has<WebviewTransparent>,
        ),
        Added<ResolvedWebviewUri>,
    >,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        for (entity, uri, size, initialize_scripts, host_window, transparent) in webviews.iter() {
            let window_entity = host_window
                .map(|h| h.0)
                .or_else(|| primary_window.single().ok());
            let device_scale_factor = window_entity
                .and_then(|e| windows.get(e).ok())
                .map(|w| w.resolution.scale_factor())
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);

            let host_window = host_window
                .and_then(|w| winit_windows.get_window(w.0))
                .or_else(|| winit_windows.get_window(primary_window.single().ok()?))
                .and_then(|w| {
                    #[allow(deprecated)]
                    w.raw_window_handle().ok()
                });
            webview_debug_log(format!(
                "create_webview entity={entity:?} uri={} size={:?} scale={device_scale_factor} transparent={transparent} host_window={}",
                uri.0,
                size.0,
                host_window.is_some()
            ));
            browsers.create_browser(
                entity,
                &uri.0,
                size.0,
                device_scale_factor,
                requester.clone(),
                ipc_event_sender.0.clone(),
                brp_sender.clone(),
                cursor_icon_sender.clone(),
                loading_state_sender.0.clone(),
                chrome_state_sender.0.clone(),
                popup_sender.0.clone(),
                texture_wake.0.clone(),
                &initialize_scripts.0,
                host_window,
                disk_profile.0.as_deref(),
                if transparent { Some(0x00000000) } else { None },
            );
        }
    });
}

fn navigate_on_source_change(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &ResolvedWebviewUri), Changed<ResolvedWebviewUri>>,
    added: Query<Entity, Added<ResolvedWebviewUri>>,
) {
    for (entity, uri) in webviews.iter() {
        if added.contains(entity) {
            continue;
        }
        browsers.navigate(&entity, &uri.0);
    }
}

fn resize(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), Changed<WebviewSize>>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    for (webview, size) in webviews.iter() {
        let window_entity = host_window
            .get(webview)
            .ok()
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let device_scale_factor = window_entity
            .and_then(|e| windows.get(e).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        browsers.resize(&webview, size.0, device_scale_factor);
    }
}

fn apply_request_show_devtool(trigger: On<RequestShowDevTool>, browsers: NonSend<Browsers>) {
    browsers.show_devtool(&trigger.webview);
}

fn apply_request_close_devtool(trigger: On<RequestCloseDevtool>, browsers: NonSend<Browsers>) {
    browsers.close_devtools(&trigger.webview);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_wake_throttle_caps_to_60hz() {
        assert!(TEXTURE_WAKE_MIN_INTERVAL >= Duration::from_millis(16));
        assert!(TEXTURE_WAKE_MIN_INTERVAL <= Duration::from_millis(17));
    }
}
