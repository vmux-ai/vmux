use bevy::prelude::*;
use bevy_cef_core::prelude::{HOST_CEF, SCHEME_CEF, resolved_cef_embedded_page_config};
use serde::{Deserialize, Serialize};

pub(crate) struct WebviewCoreComponentsPlugin;

impl Plugin for WebviewCoreComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CefSuppressPointerInput::default())
            .insert_resource(CefSuppressKeyboardInput::default())
            .register_type::<WebviewSize>()
            .register_type::<WebviewSource>()
            .register_type::<CefKeyboardTarget>()
            .register_type::<HostWindow>()
            .register_type::<ZoomLevel>()
            .register_type::<AudioMuted>()
            .register_type::<PreloadScripts>();
    }
}

/// A component that specifies the content source of a webview.
///
/// Use [`WebviewSource::new`] for remote URLs, [`WebviewSource::local`] for local files
/// served via `cef://localhost/`, or [`WebviewSource::inline`] for raw HTML content.
///
/// When the value of this component is changed at runtime, the existing browser
/// automatically navigates to the new source without being recreated.
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

/// When `true`, CEF skips forwarding [`KeyboardInput`] and [`Ime`] commits to browsers (pair with
/// [`CefSuppressPointerInput`] for host-managed chords).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CefSuppressKeyboardInput(pub bool);

#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component, Debug)]
#[require(WebviewSize, ZoomLevel, AudioMuted, PreloadScripts)]
pub enum WebviewSource {
    /// A remote or local URL (e.g. `"https://..."` or `"cef://localhost/file.html"`).
    Url(String),
    /// Raw HTML content served via an internal `cef://localhost/__inline__/{id}` scheme.
    InlineHtml(String),
}

impl WebviewSource {
    /// Creates a URL source.
    ///
    /// To specify a local file path, use [`WebviewSource::local`] instead.
    pub fn new(url: impl Into<String>) -> Self {
        Self::Url(url.into())
    }

    /// Creates a local file source.
    ///
    /// The given path is interpreted as `cef://localhost/<path>`.
    pub fn local(path: impl Into<String>) -> Self {
        Self::Url(format!("{SCHEME_CEF}://{HOST_CEF}/{}", path.into()))
    }

    /// Serves a Bevy [`embedded`](https://bevy.org/examples/assets/embedded-asset/) asset.
    ///
    /// Navigates to `cef://localhost/embedded/<path>`, which the CEF localhost handler resolves
    /// to `embedded://<path>` for [`AssetServer::load`]. Use the same logical path you passed to
    /// `EmbeddedAssetRegistry::insert_asset`
    /// (for example `history/index.html`).
    pub fn embedded(path: impl Into<String>) -> Self {
        let p = path.into();
        Self::Url(format!("{SCHEME_CEF}://{HOST_CEF}/embedded/{p}"))
    }

    pub fn custom_scheme_host_root(host: impl Into<String>) -> Self {
        let cfg = resolved_cef_embedded_page_config();
        let h = host.into();
        Self::Url(format!("{}{}/", cfg.scheme_prefix(), h))
    }

    pub fn custom_scheme_host_url(host: impl Into<String>) -> Self {
        let cfg = resolved_cef_embedded_page_config();
        let h = host.into();
        Self::Url(format!("{}{}", cfg.scheme_prefix(), h))
    }

    pub fn custom_scheme_document_url(
        host: impl Into<String>,
        embedded_path: impl Into<String>,
    ) -> Self {
        let cfg = resolved_cef_embedded_page_config();
        let h = host.into();
        let p = embedded_path.into();
        Self::Url(format!("{}{}/{}", cfg.scheme_prefix(), h, p))
    }

    /// Creates an inline HTML source.
    ///
    /// The HTML content is served through the internal `cef://localhost/__inline__/{id}` scheme,
    /// so IPC (`window.cef.emit/listen/brp`) and [`PreloadScripts`] work as expected.
    pub fn inline(html: impl Into<String>) -> Self {
        Self::InlineHtml(html.into())
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

/// Holds the webview surface texture handle for alpha hit-testing.
///
/// This component is automatically inserted and updated by the render systems.
/// It provides material-type-agnostic access to the webview texture.
#[derive(Component, Debug, Clone)]
pub struct WebviewSurface(pub Handle<Image>);

/// Analogous to [`CefKeyboardTarget`] but for pointer events (mouse wheel only, for now).
///
/// When **at least one** [`WebviewSource`] entity has this marker, `on_mouse_wheel` only forwards
/// scroll events to those entities. When **no** entity carries the marker, every webview receives
/// wheel events (default behavior).
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct CefPointerTarget;

/// Marker: CEF renders with a fully transparent background (`0x00000000`).
///
/// Without this marker the default opaque-white background is used.
/// Add to header, side-sheet, or modal entities so their CSS
/// `background-color: transparent` actually produces alpha-0 pixels.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default)]
pub struct WebviewTransparent;


