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
use bevy_cef::prelude::BinHostEmitEvent;
#[cfg(target_os = "macos")]
use bevy_cef_core::prelude::Browsers;

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
                keep_layout_view_in_front,
                render_layout_dom,
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
    surface: vmux_native::PageSurface,
    layout: Entity,
}

/// The chrome page: transparent, full window, and drawn over every pane.
///
/// The document below is the wasm bundle's own `index.html` with the wasm removed. It is not
/// decoration: without `index.css` nothing has a Tailwind rule, and without the height and flex
/// rules on `html`, `body` and the root, a flex child has no box to fill — which renders as one
/// icon at its intrinsic size filling the window.
#[cfg(target_os = "macos")]
static LAYOUT_SURFACE: vmux_native::NativePage = vmux_native::NativePage {
    url: LAYOUT_PAGE_URL,
    component: vmux_layout::page::Page,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<title>vmux</title>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
    body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 \
                 text-foreground antialiased",
    transparent: true,
};

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

    /// Put the view last in its parent's subview array, so clicks land on the chrome.
    ///
    /// `hitTest:` walks siblings back to front and knows nothing of `zPosition`, so the chrome
    /// painting above a pane is not the same as the chrome receiving the pointer. A windowed CEF
    /// browser is created with `set_as_child` against this same parent view, and every pane opened
    /// after this view was built lands after it in that array — visibly on top, and taking every
    /// click aimed at the command bar drawn over it.
    ///
    /// Reasserted rather than done once, because the next pane to open undoes it again.
    fn raise_above_panes(&self) {
        use objc2_app_kit::{NSView, NSWindowOrderingMode};
        use wry::WebViewExtMacOS;

        let wk = self.surface.webview().webview();
        let view: &NSView = &wk;
        // `superview` is unsafe only because it hands out a reference AppKit could invalidate; it
        // is read and dropped inside this call, on the thread that owns the hierarchy.
        let Some(parent) = (unsafe { view.superview() }) else {
            return;
        };
        let subviews = parent.subviews();
        let frontmost = subviews.lastObject();
        if frontmost.is_some_and(|front| std::ptr::eq(&*front, view)) {
            return;
        }

        parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Above, None);
    }

    /// Make `prefers-color-scheme` inside the view answer with the app's setting rather than the
    /// system's.
    ///
    /// The `theme` event alone is not enough. CEF has a colour-scheme override of its own, which
    /// `sync_appearance_to_cef` drives, so a CEF page's media queries already agreed with the
    /// setting; a `WKWebView` has no such thing and inherits its `NSAppearance` from the window.
    /// Left alone it renders the chrome dark on a dark desktop no matter what the setting says.
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
        let wk = self.surface.webview().webview();
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
    let embedder = match crate::page_surface::PageEmbedder::of(world) {
        Ok(embedder) => embedder,
        Err(reason) => {
            report_waiting(reason);
            return;
        }
    };
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
    let built = WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let window = winit_windows.get_window(window_entity)?;
        Some(vmux_native::PageSurface::build(
            &LAYOUT_SURFACE,
            &**window,
            bounds,
            embedder.embed(layout, LAYOUT_PAGE_URL),
        ))
    });
    match built {
        None => report_waiting("primary window has no winit window yet"),
        Some(Ok(surface)) => {
            raise_above_window_layers(surface.webview());
            world
                .non_send_mut::<Browsers>()
                .set_externally_hosted(layout);
            info!("layout_view: serving layout entity {layout:?}, no cef browser behind it");
            let view = LayoutView { surface, layout };
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
    view.surface.set_bounds(LayoutView::bounds_of(window));
}

/// A pane can open on any frame, and opening one puts its view in front of the chrome.
#[cfg(target_os = "macos")]
fn keep_layout_view_in_front(view: Option<NonSend<LayoutView>>) {
    let Some(view) = view else {
        return;
    };
    view.raise_above_panes();
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
    view.surface.take_first_responder();
}

#[cfg(target_os = "macos")]
fn forward_host_emit(host_emit: On<BinHostEmitEvent>, view: Option<NonSend<LayoutView>>) {
    let Some(view) = view else {
        return;
    };
    if host_emit.webview != view.layout {
        return;
    }

    // Straight to the listener the page registered. The wasm bundle needed this base64'd through
    // a JS shim because the page was on the other side of a browser; it is in this process now.
    view.surface.deliver(&host_emit.id, &host_emit.payload);
}

/// Evaluate whatever the page's components rendered.
#[cfg(target_os = "macos")]
fn render_layout_dom(view: Option<NonSend<LayoutView>>) {
    let Some(view) = view else {
        return;
    };
    view.surface.render();
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

/// Keep the chrome above the other layers in this window.
///
/// `sync_layout_overlay` parents a `CALayer` at `zPosition` 100, and subview order cannot outrank a
/// `zPosition` — only another one can. This is that other one.
///
/// It buys painting and nothing else. A layer's `zPosition` is invisible to `hitTest:`, which walks
/// the subview array back to front, so raising it here does not move a single click.
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
