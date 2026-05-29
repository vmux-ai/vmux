use crate::cef_state::WebviewCefStateSender;
use crate::common::localhost::responser::{InlineHtmlId, InlineHtmlStore};
use crate::common::{
    BinIpcEventRawSender, HostWindow, IpcEventRawSender, ResolvedWebviewUri, WebviewSize,
    WebviewSource, WebviewTransparent,
};
use crate::cursor_icon::SystemCursorIconSender;
use crate::loading_state::{WebviewCommittedNavigationSender, WebviewLoadingStateSender};
use crate::popup_state::WebviewPopupSender;
use crate::prelude::PreloadScripts;
use crate::webview::mesh::MeshWebviewPlugin;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window};
use bevy::winit::WINIT_WINDOWS;
use bevy_cef_core::prelude::*;
use bevy_remote::BrpSender;
#[allow(deprecated)]
use raw_window_handle::HasRawWindowHandle;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

mod history_swipe;
mod mesh;
mod pinch_zoom;
mod webview_sprite;

#[derive(Resource, Clone)]
struct TextureWakeCallback(Option<TextureWake>);

#[derive(Resource, Clone)]
struct TextureWakeMinInterval(Arc<AtomicU64>);

impl Default for TextureWakeMinInterval {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(duration_nanos(
            windowless_frame_interval_from_frame_rate(DEFAULT_WINDOWLESS_FRAME_RATE),
        ))))
    }
}

impl TextureWakeMinInterval {
    fn set_min_interval(&self, interval: Duration) {
        self.0.store(duration_nanos(interval), Ordering::Relaxed);
    }

    fn min_interval(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::Relaxed))
    }
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap_or(u64::MAX)
}

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
        let texture_wake_policy = TextureWakeMinInterval::default();
        let texture_wake = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| spawn_texture_wake_throttler(wrapper, texture_wake_policy.clone()));
        app.register_type::<RequestShowDevTool>()
            .init_resource::<CefDiskProfileRoot>()
            .init_non_send_resource::<Browsers>()
            .insert_resource(TextureWakeCallback(texture_wake))
            .insert_resource(texture_wake_policy)
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
            .add_systems(
                Update,
                sync_windowless_frame_rate.after(CefSystems::CreateAndResize),
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

fn sync_windowless_frame_rate(
    browsers: NonSend<Browsers>,
    texture_wake_policy: Res<TextureWakeMinInterval>,
    webviews: Query<(Entity, Option<&HostWindow>), With<WebviewSource>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let mut min_interval = None::<Duration>;
        for (entity, host_window) in webviews.iter() {
            let window_entity = host_window
                .map(|h| h.0)
                .or_else(|| primary_window.single().ok());
            let refresh_rate_millihertz = window_entity
                .and_then(|e| winit_windows.get_window(e))
                .and_then(|window| window.current_monitor())
                .and_then(|monitor| monitor.refresh_rate_millihertz());
            let windowless_frame_rate =
                windowless_frame_rate_from_refresh_millihertz(refresh_rate_millihertz);
            let frame_interval = windowless_frame_interval_from_frame_rate(windowless_frame_rate);
            min_interval = Some(
                min_interval
                    .map(|current| current.min(frame_interval))
                    .unwrap_or(frame_interval),
            );
            browsers.set_windowless_frame_rate(&entity, windowless_frame_rate);
        }
        if let Some(interval) = min_interval {
            texture_wake_policy.set_min_interval(interval);
        }
    });
}

fn spawn_texture_wake_throttler(
    wrapper: &bevy::winit::EventLoopProxyWrapper,
    policy: TextureWakeMinInterval,
) -> TextureWake {
    let proxy = (**wrapper).clone();
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("cef-texture-wake-throttle".into())
        .spawn(move || {
            let mut last_fire: Option<Instant> = None;
            while rx.recv().is_ok() {
                if let Some(t) = last_fire {
                    let elapsed = Instant::now().duration_since(t);
                    let min_interval = policy.min_interval();
                    if elapsed < min_interval {
                        std::thread::sleep(min_interval - elapsed);
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
    bin_ipc_event_sender: Res<BinIpcEventRawSender>,
    brp_sender: Res<BrpSender>,
    cursor_icon_sender: Res<SystemCursorIconSender>,
    loading_state_sender: Res<WebviewLoadingStateSender>,
    committed_nav_sender: Res<WebviewCommittedNavigationSender>,
    cef_state_sender: Res<WebviewCefStateSender>,
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

            let winit_window = window_entity.and_then(|e| winit_windows.get_window(e));
            let refresh_rate_millihertz = winit_window
                .and_then(|window| window.current_monitor())
                .and_then(|monitor| monitor.refresh_rate_millihertz());
            let windowless_frame_rate =
                windowless_frame_rate_from_refresh_millihertz(refresh_rate_millihertz);
            let host_window = winit_window
                .and_then(|w| {
                    #[allow(deprecated)]
                    w.raw_window_handle().ok()
                });
            webview_debug_log(format!(
                "create_webview entity={entity:?} uri={} size={:?} scale={device_scale_factor} transparent={transparent} host_window={} fps={windowless_frame_rate}",
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
                bin_ipc_event_sender.0.clone(),
                brp_sender.clone(),
                cursor_icon_sender.clone(),
                loading_state_sender.0.clone(),
                committed_nav_sender.0.clone(),
                cef_state_sender.0.clone(),
                popup_sender.0.clone(),
                texture_wake.0.clone(),
                &initialize_scripts.0,
                host_window,
                disk_profile.0.as_deref(),
                if transparent { Some(0x00000000) } else { None },
                windowless_frame_rate,
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
    fn texture_wake_throttle_defaults_to_120hz_interval() {
        assert_eq!(
            TextureWakeMinInterval::default().min_interval(),
            windowless_frame_interval_from_frame_rate(DEFAULT_WINDOWLESS_FRAME_RATE)
        );
    }

    #[test]
    fn webview_does_not_drive_external_begin_frames_from_bevy_schedule() {
        let implementation = include_str!("webview.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(!implementation.contains("drive_external_begin_frames"));
        assert!(!implementation.contains("send_external_begin_frame"));
    }

    #[test]
    fn webview_uses_current_monitor_refresh_for_initial_cef_frame_rate() {
        let implementation = include_str!("webview.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("current_monitor()"));
        assert!(implementation.contains("refresh_rate_millihertz()"));
        assert!(implementation.contains("windowless_frame_rate_from_refresh_millihertz"));
        assert!(implementation.contains("windowless_frame_rate,"));
    }

    #[test]
    fn webview_keeps_existing_cef_frame_rate_synced_to_monitor() {
        let implementation = include_str!("webview.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("sync_windowless_frame_rate"));
        assert!(implementation.contains("browsers.set_windowless_frame_rate"));
        assert!(implementation.contains("texture_wake_policy.set_min_interval"));
    }
}
