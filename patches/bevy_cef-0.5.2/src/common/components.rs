use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) struct WebviewCoreComponentsPlugin;

impl Plugin for WebviewCoreComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CefSuppressPointerInput::default())
            .register_type::<WebviewSize>()
            .register_type::<WebviewSource>()
            .register_type::<CefKeyboardTarget>()
            .register_type::<CefIgnorePinchZoom>()
            .register_type::<WebviewWindowed>()
            .register_type::<WebviewNativeLiquidGlass>()
            .register_type::<WebviewOpaqueWindowedBackground>()
            .register_type::<WebviewWindowedNativeFocus>()
            .register_type::<WebviewMaxFrameRate>()
            .register_type::<WebviewNativeOverlay>()
            .register_type::<HistorySwipeVisualOffset>()
            .register_type::<HostWindow>()
            .register_type::<ZoomLevel>()
            .register_type::<AudioMuted>()
            .register_type::<PreloadScripts>();
    }
}

/// Marker: restrict forwarded keyboard events to the webviews carrying it.
///
/// When present on **at least one** [`WebviewSource`] entity, only those entities receive
/// forwarded keyboard events. When **no** webview has this marker, every webview receives keys
/// (legacy single- or multi-webview behavior).
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct CefKeyboardTarget;

/// When `true`, mesh/sprite pointer observers skip forwarding mouse move / click / wheel to CEF.
///
/// Host apps (e.g. tmux-style prefix chords) can set this so pointer input stays with the shell
/// while a key chord is in progress.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CefSuppressPointerInput(pub bool);

/// Marker: this webview should ignore pinch-to-zoom gestures.
///
/// Useful for CEF / UI webviews where the host doesn't want the user to
/// inadvertently zoom the layout.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct CefIgnorePinchZoom;

/// Marker: create this webview as a **windowed** native CEF child view (Chromium renders,
/// composites, scrolls, and handles input itself) instead of off-screen rendering into a Bevy
/// mesh. macOS browse mode. Read at browser creation; toggling at runtime requires recreating the
/// browser.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewWindowed;

#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewNativeLiquidGlass;

#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewOpaqueWindowedBackground;

#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewWindowedNativeFocus;

/// Caps an OSR webview's windowless frame rate (fps). `sync_windowless_frame_rate` clamps the
/// monitor-derived rate to this value, so a mostly-static surface (e.g. the layout) repaints —
/// and forces a Bevy re-render — far less often. No effect on windowed webviews.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct WebviewMaxFrameRate(pub i32);

/// Marker: route this OSR webview's accelerated IOSurface frames into `NativeOverlayFrames` (for a
/// native overlay layer) instead of uploading them to its Bevy texture. Lets the surface be shown
/// in a native view composited *above* windowed pages.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewNativeOverlay;

/// The URL a webview points at.
///
/// When the value is changed at runtime, the existing browser navigates to the new URL
/// without being recreated.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component, Debug)]
#[require(WebviewSize, ZoomLevel, AudioMuted, PreloadScripts)]
pub struct WebviewSource(pub String);

impl WebviewSource {
    pub fn new(url: impl Into<String>) -> Self {
        Self(url.into())
    }
}

#[derive(Component, Debug, Clone)]
pub struct ResolvedWebviewUri(pub String);

/// Specifies the view size of the webview.
///
/// This does not affect the actual object size.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq)]
#[reflect(Component, Debug, Default)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub struct WebviewSize(pub Vec2);

impl Default for WebviewSize {
    fn default() -> Self {
        Self(Vec2::splat(800.0))
    }
}

/// An optional component to specify the parent window of the webview.
/// The window handle of [Window] specified by this component is passed to `parent_view` of [`WindowInfo`](cef::WindowInfo).
///
/// If this component is not inserted, the handle of [PrimaryWindow](bevy::window::PrimaryWindow) is passed instead.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq)]
#[reflect(Component, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub struct HostWindow(pub Entity);

/// This component is used to specify the zoom level of the webview.
///
/// Specify 0.0 to reset the zoom level to the default.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
#[reflect(Component, Debug, Serialize, Deserialize, Default)]
pub struct ZoomLevel(pub f64);

/// This component is used to specify whether the audio of the webview is muted or not.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq, Default, Serialize, Deserialize)]
#[reflect(Component, Debug, Default, Serialize, Deserialize)]
pub struct AudioMuted(pub bool);

/// This component is used to preload scripts in the webview.
///
/// Scripts specified in this component are executed before the scripts in the HTML.
#[derive(Reflect, Component, Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[reflect(Component, Debug, Default, Serialize, Deserialize)]
pub struct PreloadScripts(pub Vec<String>);

impl<L, S> From<L> for PreloadScripts
where
    L: IntoIterator<Item = S>,
    S: Into<String>,
{
    fn from(scripts: L) -> Self {
        Self(scripts.into_iter().map(Into::into).collect())
    }
}

/// Native-only preload scripts excluded from Bevy reflection and remote inspection.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct PrivatePreloadScripts(pub Vec<String>);

impl<L, S> From<L> for PrivatePreloadScripts
where
    L: IntoIterator<Item = S>,
    S: Into<String>,
{
    fn from(scripts: L) -> Self {
        Self(scripts.into_iter().map(Into::into).collect())
    }
}

/// Analogous to [`CefKeyboardTarget`] but for pointer events (mouse wheel only, for now).
///
/// When **at least one** [`WebviewSource`] entity has this marker, `on_mouse_wheel` only forwards
/// scroll events to those entities. When **no** entity carries the marker, every webview receives
/// wheel events (default behavior).
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct CefPointerTarget;

/// Transient browser-surface offset used while a horizontal history swipe is in progress.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq, Default)]
#[reflect(Component, Debug, Default)]
pub struct HistorySwipeVisualOffset {
    pub offset_px: f32,
    pub progress: f32,
}

/// Marker: CEF renders with a fully transparent background (`0x00000000`).
///
/// Without this marker the default opaque-white background is used.
/// Add to header, side-sheet, or modal entities so their CSS
/// `background-color: transparent` actually produces alpha-0 pixels.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewTransparent;
