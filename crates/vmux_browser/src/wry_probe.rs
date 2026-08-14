//! Does a `WKWebView` render transparent inside our Bevy window?
//!
//! CEF cannot: a windowed CEF browser paints an opaque root on macOS regardless of an alpha-0
//! `background_color`, which is [CEF #2315](https://bitbucket.org/chromiumembedded/cef/issues/2315)
//! and unimplemented there since 2017. That is the only thing keeping the layout off-screen, so
//! before committing to a second engine this asks the one question that decides it.
//!
//! A panel over the pane area, transparent, with one translucent card. If the pane shows through
//! the panel and only the card is tinted, `WKWebView` composites with alpha in this window and the
//! layout can move to wry. If the panel is a white or black rectangle, it cannot, and the chrome
//! goes opaque instead.
//!
//! Deliberately not wired to anything: no `vmux://`, no IPC, no sizing to the layout. Delete with
//! the branch.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;

pub struct WryProbePlugin;

impl Plugin for WryProbePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_probe);
    }
}

// wry calls `objc2::exception::catch`, whose C shim ships as a static archive built by
// `objc2-exception-helper`. Cargo puts that archive's directory on the link path but its `-l`
// never reaches this binary, so the reference resolves to nothing. Naming the library here is
// what pulls it in.
#[cfg(target_os = "macos")]
#[link(name = "objc2_exception_helper_0_1", kind = "static")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
struct WryProbe(#[allow(dead_code)] wry::WebView);

#[cfg(target_os = "macos")]
fn spawn_probe(world: &mut World) {
    if world.get_non_send::<WryProbe>().is_some() {
        return;
    }
    let Ok(window_entity) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
    else {
        report_waiting("no primary window entity");
        return;
    };
    let built = WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let window = winit_windows.get_window(window_entity)?;
        Some(
            wry::WebViewBuilder::new()
                .with_transparent(true)
                .with_html(PROBE_HTML)
                .with_bounds(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(420.0, 160.0).into(),
                    size: wry::dpi::LogicalSize::new(420.0, 320.0).into(),
                })
                .build_as_child(&**window),
        )
    });
    match built {
        None => report_waiting("primary window has no winit window yet"),
        Some(Ok(webview)) => {
            lift_above_layout_overlay(&webview);
            world.insert_non_send(WryProbe(webview));
        }
        Some(Err(error)) => error!("wry_probe: build_as_child failed: {error}"),
    }
}

/// The layout composites as a full-window `CALayer` at `zPosition` 100, so a freshly parented
/// sibling is covered by it no matter where it sits in subview order — which is why the first two
/// runs showed nothing at all, transparent or solid. Subview order cannot beat a `zPosition`;
/// another `zPosition` can.
#[cfg(target_os = "macos")]
fn lift_above_layout_overlay(webview: &wry::WebView) {
    use objc2_app_kit::NSView;
    use wry::WebViewExtMacOS;

    let wk = webview.webview();
    let view: &NSView = &wk;
    view.setWantsLayer(true);
    let Some(layer) = view.layer() else {
        error!("wry_probe: WKWebView has no layer, cannot lift it above the layout overlay");
        return;
    };
    layer.setZPosition(500.0);
    let frame = view.frame();
    info!(
        "wry_probe: WKWebView built as child, transparent=true, zPosition=500, frame={}x{} at ({}, {}), has_superview={}",
        frame.size.width,
        frame.size.height,
        frame.origin.x,
        frame.origin.y,
        unsafe { view.superview() }.is_some(),
    );
}

/// A probe that silently does nothing looks exactly like a probe that answered "no", which is how
/// the first run of this was misread. Say why it has not built yet, once.
#[cfg(target_os = "macos")]
fn report_waiting(reason: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        info!("wry_probe: waiting, {reason}");
    }
}

#[cfg(not(target_os = "macos"))]
fn spawn_probe() {}

/// The actual question, now that the solid-red run proved the view reaches the screen.
///
/// A transparent body with one translucent card. If the pane shows through everywhere except the
/// card, `WKWebView` composites with alpha inside this window and the layout can move to wry. A
/// white or black rectangle means it cannot, and the chrome goes opaque instead.
#[cfg(target_os = "macos")]
const PROBE_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { height: 100%; margin: 0; background: transparent; }
  body { display: grid; place-items: center; font: 600 15px -apple-system, sans-serif; }
  .card {
    padding: 22px 26px; border-radius: 18px; text-align: center; color: #111;
    background: rgba(255,255,255,0.34);
    backdrop-filter: blur(24px) saturate(180%);
    border: 1px solid rgba(255,255,255,0.5);
    box-shadow: 0 12px 40px rgba(0,0,0,0.18);
  }
  .hint { margin-top: 6px; font-weight: 400; font-size: 12px; opacity: 0.65; }
</style></head>
<body><div class="card">wry / WKWebView
<div class="hint">the area around this card should not be a rectangle</div>
</div></body></html>"#;
