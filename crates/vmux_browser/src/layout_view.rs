//! The layout's chrome, rendered by a transparent `WKWebView`.
//!
//! The layout is the one page that has to be see-through, and a windowed CEF browser paints an
//! opaque root on macOS — [CEF #2315], unimplemented since 2017, and the community patches address
//! Windows and Linux because those go through the Views framework while macOS uses the native
//! `NSView` path. So the layout is served by wry instead, and `layout_cef_bundle` no longer carries
//! a `Browser` at all.
//!
//! Almost nothing here is new machinery. `vmux://` already resolved through Bevy's `AssetServer`:
//! CEF's scheme handler only forwards a [`CefRequest`] down a channel and waits for a
//! [`CefResponse`], so resolution was never CEF-specific and [`VmuxProtocol`] sends the same
//! request. The page's channel was already a runtime seam too, so the page half resolves
//! `window.cef` or `window.vmuxWry` — whichever its engine injected — and this supplies the second
//! global rather than a second protocol.
//!
//! Both directions therefore rejoin the existing paths. [`PageMessage`] decodes the envelope out of
//! wry's string IPC and pushes a [`BinIpcEventRaw`] onto the channel CEF's client handler feeds, so
//! every `BinReceive` observer fires unchanged; [`forward_host_emit`] observes [`BinHostEmitEvent`]
//! alongside `bevy_cef`'s own. The layout keeps its entity, which is what makes this a change of
//! engine rather than a rewrite of its host half — `Browsers::can_emit_to` is the one seam that had
//! to exist, because every emit used to be gated on CEF owning a browser.
//!
//! [CEF #2315]: https://bitbucket.org/chromiumembedded/cef/issues/2315

use bevy::prelude::*;
#[cfg(target_os = "macos")]
use bevy::window::PrimaryWindow;
#[cfg(target_os = "macos")]
use bevy::winit::WINIT_WINDOWS;
#[cfg(target_os = "macos")]
use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawSender};
#[cfg(target_os = "macos")]
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};

#[cfg(target_os = "macos")]
use vmux_setting::AppSettings;

#[cfg(target_os = "macos")]
use vmux_layout::LayoutCef;

#[cfg(target_os = "macos")]
use vmux_layout::event::LAYOUT_PAGE_URL;

pub struct LayoutViewPlugin;

impl Plugin for LayoutViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_layout_view);
        #[cfg(target_os = "macos")]
        app.add_systems(
            Update,
            (
                resize_layout_view,
                sync_layout_view_color_scheme.run_if(resource_changed::<AppSettings>),
            )
                .after(spawn_layout_view),
        );
        // After the CEF route has had its say, or it re-focuses the pane in the same frame.
        #[cfg(target_os = "macos")]
        app.add_systems(
            PostUpdate,
            sync_layout_view_focus.after(crate::host_focus::apply_windowed_host_focus),
        );
        #[cfg(target_os = "macos")]
        app.add_observer(forward_host_emit);
    }
}

// wry calls `objc2::exception::catch`, whose C shim ships as a static archive built by
// `objc2-exception-helper`. Cargo puts that archive's directory on the link path but its `-l`
// never reaches this binary, so the reference resolves to nothing. Naming the library here is
// what pulls it in.
#[cfg(target_os = "macos")]
#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}

/// The wry view, and the entity whose page it serves.
///
/// `LayoutCef` no longer carries a CEF browser, but it is still the id every `BinReceive` observer
/// in `vmux_layout` is registered against and every host emit is addressed to. Keeping it is what
/// makes this a change of engine rather than a rewrite of the layout's host half.
#[cfg(target_os = "macos")]
struct LayoutView {
    webview: wry::WebView,
    layout: Entity,
}

#[cfg(target_os = "macos")]
impl LayoutView {
    /// Full window, because the chrome this renders *is* the window's chrome — a smaller box
    /// could only ever be sampled over whatever pane happened to be behind it, which is what made
    /// the earlier runs unable to tell a transparent view from a view showing a pane.
    fn bounds_of(window: &Window) -> wry::Rect {
        wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(window.width(), window.height()).into(),
        }
    }

    /// Make `prefers-color-scheme` inside the view answer with the app's setting rather than the
    /// system's.
    ///
    /// The `theme` event alone is not enough. CEF has a colour-scheme override of its own, which
    /// `sync_appearance_to_cef` drives, so a CEF page's media queries already agreed with the
    /// setting; a `WKWebView` has no such thing and inherits its `NSAppearance` from the window.
    /// Left alone it renders the chrome dark on a dark desktop no matter what the setting says.
    /// Hand the view AppKit first responder, so its DOM receives keys.
    ///
    /// A CEF page is focused through `Browsers::set_windowed_focus` and a terminal wants the
    /// keyboard on the winit window; neither route can reach a `WKWebView`, so nothing else in the
    /// app can give this view the responder.
    fn take_first_responder(&self) {
        use objc2_app_kit::NSView;
        use wry::WebViewExtMacOS;

        let wk = self.webview.webview();
        let view: &NSView = &wk;
        let Some(window) = view.window() else {
            return;
        };
        let already_holds_it = window
            .firstResponder()
            .is_some_and(|current| std::ptr::eq(&*current as *const _ as *const NSView, view));
        if already_holds_it {
            return;
        }
        if !window.makeFirstResponder(Some(view)) {
            warn!("layout_view: the window refused first responder, chrome input will not work");
        }
    }

    fn set_color_scheme(&self, mode: vmux_setting::ColorScheme) {
        use objc2_app_kit::{
            NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua,
            NSAppearanceNameDarkAqua, NSView,
        };
        use vmux_setting::ColorScheme;
        use wry::WebViewExtMacOS;

        let name = match mode {
            ColorScheme::Light => Some(unsafe { NSAppearanceNameAqua }),
            ColorScheme::Dark => Some(unsafe { NSAppearanceNameDarkAqua }),
            ColorScheme::Device => None,
        };
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        let wk = self.webview.webview();
        let view: &NSView = &wk;
        view.setAppearance(appearance.as_deref());
        info!("layout_view: color scheme set to {mode:?}");
    }
}

#[cfg(target_os = "macos")]
fn spawn_layout_view(world: &mut World) {
    if world.get_non_send::<LayoutView>().is_some() {
        return;
    }
    let Ok(window_entity) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
    else {
        report_waiting("no primary window entity");
        return;
    };
    let Some(requester) = world.get_resource::<Requester>().cloned() else {
        report_waiting("no Requester resource, cef localhost plugin has not built yet");
        return;
    };
    let Some(bin_ipc) = world.get_resource::<BinIpcEventRawSender>() else {
        report_waiting("no BinIpcEventRawSender resource, cef ipc plugin has not built yet");
        return;
    };
    let bin_ipc = bin_ipc.0.clone();
    let Ok(layout) = world
        .query_filtered::<Entity, With<LayoutCef>>()
        .single(world)
    else {
        report_waiting("no LayoutCef entity to borrow an identity from");
        return;
    };
    let Ok(bounds) = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(world)
        .map(LayoutView::bounds_of)
    else {
        report_waiting("no primary Window component to size against");
        return;
    };
    let page = PageMessage::new(bin_ipc, layout);
    let built = WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let window = winit_windows.get_window(window_entity)?;
        Some(
            wry::WebViewBuilder::new()
                .with_transparent(true)
                .with_initialization_script(WRY_HOST_SHIM)
                .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                    VmuxProtocol::serve(&requester, request, responder);
                })
                .with_ipc_handler(move |request| page.receive(request.body()))
                .with_url(LAYOUT_PAGE_URL)
                .with_bounds(bounds)
                .build_as_child(&**window),
        )
    });
    match built {
        None => report_waiting("primary window has no winit window yet"),
        Some(Ok(webview)) => {
            raise_above_window_layers(&webview);
            world
                .non_send_mut::<Browsers>()
                .set_externally_hosted(layout);
            info!("layout_view: serving layout entity {layout:?}, no cef browser behind it");
            let view = LayoutView { webview, layout };
            view.set_color_scheme(world.resource::<AppSettings>().appearance.mode);
            world.insert_non_send(view);
        }
        Some(Err(error)) => error!("layout_view: build_as_child failed: {error}"),
    }
}

#[cfg(target_os = "macos")]
fn resize_layout_view(
    view: Option<NonSend<LayoutView>>,
    window: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>,
) {
    let Some(view) = view else {
        return;
    };
    let Ok(window) = window.single() else {
        return;
    };
    if let Err(error) = view.webview.set_bounds(LayoutView::bounds_of(window)) {
        error!("layout_view: set_bounds failed: {error}");
    }
}

#[cfg(target_os = "macos")]
fn sync_layout_view_color_scheme(view: Option<NonSend<LayoutView>>, settings: Res<AppSettings>) {
    let Some(view) = view else {
        return;
    };
    view.set_color_scheme(settings.appearance.mode);
}

/// Deliver every host→page event aimed at the layout.
///
/// `bevy_cef`'s own observer still runs and finds no browser for this entity, so it is this that
/// carries the payload the rest of the way.
/// Holds first responder for the layout view while its chrome owns the keyboard.
///
/// Runs every frame rather than on the edge, because `apply_winit_host_focus` reclaims for winit on
/// its own schedule and losing the responder silently looks exactly like never having had it.
///
/// Nothing resigns it here: leaving it to nobody is a state the app is never otherwise in, and CEF
/// then declines to reclaim. The pane takes it back through `set_windowed_focus` instead, which
/// `apply_windowed_host_focus` forces on the way out of this intent.
#[cfg(target_os = "macos")]
fn sync_layout_view_focus(
    view: Option<NonSend<LayoutView>>,
    intent: Res<crate::host_focus::HostFocusIntent>,
) {
    if *intent != crate::host_focus::HostFocusIntent::LayoutView {
        return;
    }
    let Some(view) = view else {
        return;
    };
    view.take_first_responder();
}

#[cfg(target_os = "macos")]
fn forward_host_emit(host_emit: On<BinHostEmitEvent>, view: Option<NonSend<LayoutView>>) {
    use base64::Engine;

    let Some(view) = view else {
        return;
    };
    if host_emit.webview != view.layout {
        return;
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(&host_emit.payload);
    let Ok(id) = serde_json::to_string(&host_emit.id) else {
        return;
    };
    let Ok(payload) = serde_json::to_string(&payload) else {
        return;
    };
    let script = format!("window.vmuxWry && window.vmuxWry._dispatch({id}, {payload})");
    if let Err(error) = view.webview.evaluate_script(&script) {
        error!("layout_view: host emit '{}' failed: {error}", host_emit.id);
    }
}

/// Nothing renders the layout off macOS.
///
/// `layout_cef_bundle` dropped its `Browser` on every platform, and only macOS has a replacement,
/// so this says so once rather than leaving a blank window to be diagnosed. Extending wry here is
/// plausible — WebKitGTK is its Linux backend — but the transparency this exists for is a macOS
/// question, and nobody is running the desktop app there today.
#[cfg(not(target_os = "macos"))]
fn spawn_layout_view() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        warn!("layout_view: the layout has no renderer on this platform, chrome will be missing");
    }
}

/// `vmux://` for the wry view, answered by the same Bevy systems that answer it for CEF.
#[cfg(target_os = "macos")]
struct VmuxProtocol;

#[cfg(target_os = "macos")]
impl VmuxProtocol {
    /// The responder is handed to a thread rather than awaited here: the reply comes from a Bevy
    /// system, and this runs on the main thread, so blocking would stop the schedule that produces
    /// it and deadlock.
    fn serve(
        requester: &Requester,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();
        let uri = asset_load_path_from_request_url(&url);
        if uri.is_empty() {
            error!("layout_view: vmux:// url maps to no asset path, url={url}");
            responder.respond(Self::error_response("no asset path for url"));
            return;
        }
        let (tx, rx) = async_channel::bounded::<CefResponse>(1);
        if requester
            .send_blocking(CefRequest {
                uri: uri.clone(),
                responser: Responser(tx),
            })
            .is_err()
        {
            error!("layout_view: vmux:// request channel closed, uri={uri}");
            responder.respond(Self::error_response("request channel closed"));
            return;
        }
        std::thread::spawn(move || match rx.recv_blocking() {
            Ok(response) => {
                let built = wry::http::Response::builder()
                    .status(response.status_code as u16)
                    .header(wry::http::header::CONTENT_TYPE, response.mime_type)
                    .body(response.data);
                match built {
                    Ok(built) => responder.respond(built),
                    Err(error) => {
                        error!("layout_view: vmux:// response invalid uri={uri}: {error}");
                        responder.respond(Self::error_response("response invalid"));
                    }
                }
            }
            Err(_) => {
                error!("layout_view: vmux:// responder dropped, uri={uri}");
                responder.respond(Self::error_response("responder dropped"));
            }
        });
    }

    fn error_response(reason: &str) -> wry::http::Response<Vec<u8>> {
        wry::http::Response::builder()
            .status(500)
            .header(wry::http::header::CONTENT_TYPE, "text/plain")
            .body(reason.as_bytes().to_vec())
            .expect("a literal status and body always build")
    }
}

/// One page-to-host message, as it arrives over wry's string IPC.
///
/// `window.cef.binEmit` takes an `ArrayBuffer`; wry's IPC carries text, so the shim base64s the
/// same `BinIpcEnvelope` bytes and this undoes it. The envelope framing is left alone, because
/// the Bevy side matches its id with `bin_ipc_event_id::<E>()` and that has to keep agreeing.
#[cfg(target_os = "macos")]
struct PageMessage {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: String,
}

#[cfg(target_os = "macos")]
impl PageMessage {
    fn new(bin_ipc: async_channel::Sender<BinIpcEventRaw>, webview: Entity) -> Self {
        let host = embedded_page_host_of(LAYOUT_PAGE_URL).unwrap_or_default();
        Self {
            bin_ipc,
            webview,
            host,
        }
    }

    fn receive(&self, body: &str) {
        use base64::Engine;
        use vmux_ui::transport::bin_ipc_envelope::BinIpcEnvelope;

        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            match level {
                "error" | "reject" => error!("layout page: {text}"),
                "warn" => warn!("layout page: {text}"),
                _ => info!("layout page: {text}"),
            }
            return;
        }
        let bytes = match base64::engine::general_purpose::STANDARD.decode(body) {
            Ok(bytes) => bytes,
            Err(error) => {
                error!("layout_view: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "layout_view: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        let sent = self.bin_ipc.send_blocking(BinIpcEventRaw {
            webview: self.webview,
            host: self.host.clone(),
            id,
            payload,
        });
        if sent.is_err() {
            error!("layout_view: bin ipc channel closed");
        }
    }
}

/// What a wasm page finds on `window` instead of `window.cef`.
///
/// Deliberately the same two verbs. `vmux_ui::transport` picks its `PageHost` at runtime, so the
/// page half of this is a matter of answering to whichever object is present, not of teaching the
/// pages a second protocol.
#[cfg(target_os = "macos")]
const WRY_HOST_SHIM: &str = r#"
(function () {
  const report = (kind, text) => {
    try { window.ipc.postMessage('log:' + kind + ':' + text); } catch (e) {}
  };
  window.addEventListener('error', (e) => {
    report('error', (e.message || 'error') + ' @ ' + (e.filename || '?') + ':' + (e.lineno || 0));
  });
  window.addEventListener('unhandledrejection', (e) => {
    report('reject', String((e.reason && e.reason.stack) || e.reason));
  });
  for (const level of ['error', 'warn', 'log']) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      report(level, args.map((a) => {
        if (a instanceof Error) return a.stack || a.message;
        if (typeof a === 'object') { try { return JSON.stringify(a); } catch (e) { return String(a); } }
        return String(a);
      }).join(' '));
      original(...args);
    };
  }
  const listeners = new Map();
  function toBase64(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
    return btoa(binary);
  }
  function fromBase64(text) {
    const binary = atob(text);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return bytes.buffer;
  }
  window.vmuxWry = {
    binEmit(buffer) { window.ipc.postMessage(toBase64(buffer)); },
    binListen(id, callback) {
      const existing = listeners.get(id) || [];
      existing.push(callback);
      listeners.set(id, existing);
    },
    _dispatch(id, base64) {
      const buffer = fromBase64(base64);
      for (const callback of listeners.get(id) || []) callback(buffer);
    },
  };
})();
"#;

/// Keep the chrome above the other layers in this window.
///
/// Not the panes — those are windowed CEF browsers living in child `NSWindow`s of their own, which
/// no ordering here can reach and none is wanted: a pane sits above the chrome and takes the clicks
/// that land on it, which is the arrangement. What this beats is inside the main window, where
/// `sync_layout_overlay` still parents a `CALayer` at `zPosition` 100 and subview order cannot
/// outrank a `zPosition` — only another one can.
#[cfg(target_os = "macos")]
fn raise_above_window_layers(webview: &wry::WebView) {
    use objc2_app_kit::NSView;
    use wry::WebViewExtMacOS;

    let wk = webview.webview();
    let view: &NSView = &wk;
    view.setWantsLayer(true);
    let Some(layer) = view.layer() else {
        error!("layout_view: WKWebView has no layer, the chrome will paint under the panes");
        return;
    };
    layer.setZPosition(500.0);
}

/// A view that silently never builds looks exactly like a view that built and rendered nothing,
/// which is how an early run of this was misread. Say why it has not built yet, once.
#[cfg(target_os = "macos")]
fn report_waiting(reason: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        info!("layout_view: waiting, {reason}");
    }
}
