//! A page whose components run in this process, painted by a `WKWebView` of its own.
//!
//! The layout was the first, and is no longer the only one: a start pane is the same arrangement in
//! a smaller rectangle. What differs between two native pages is a URL, a root component and the
//! document chrome they render into — [`SurfacePage`] — so that is all a caller supplies.
//!
//! What does *not* differ, and lives here: the view, the `vmux://` protocol that answers `__events`
//! and serves the shell, the IPC handler that hears the page back, and the `VirtualDom` in
//! [`dom`](self::dom).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use bevy_cef::prelude::BinIpcEventRawSender;
use bevy_cef_core::prelude::{BinIpcEventRaw, Browsers, Requester, embedded_page_host_of};
use vmux_core::host::page::HostsPage;
use vmux_core::page_metadata::PageMetadata;
use vmux_layout::LayoutCef;

use self::dom::{PageWaker, SurfaceDom};
use self::protocol::{PageMessage, VmuxProtocol, WRY_HOST_SHIM};
use crate::present::PaneFrames;

pub(crate) mod dom;
mod protocol;

/// Every page this build can host in its own process, by the URL that asks for it.
///
/// The layout is absent on purpose: its view is full-window, front-most and transparent, and it is
/// built before any pane exists. It is the same `PageSurface` underneath, driven by its own systems.
static NATIVE_PAGES: &[&SurfacePage] = &[];

pub(crate) struct PageSurfacePlugin;

impl Plugin for PageSurfacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open_native_surfaces).add_systems(
            PostUpdate,
            (place_native_surfaces, render_native_surfaces)
                .chain()
                .after(crate::present::sync_windowed_frames),
        );
    }
}

/// The surfaces this process is painting, one per page entity.
///
/// A `NonSend` resource rather than a component, which is where this would rather live: a component
/// must be `Send + Sync` and a `wry::WebView` is neither. The entity is the key instead, so the two
/// stay in step by lookup rather than by storage.
#[derive(Default)]
pub(crate) struct PageSurfaces(HashMap<Entity, PageSurface>);

/// Build a view for any page that asks for one and has not got one yet.
///
/// Exclusive because building needs the winit window, the asset requester and the IPC channel at
/// once, and because the surfaces are `NonSend`.
fn open_native_surfaces(world: &mut World) {
    let wanted: Vec<(Entity, &'static SurfacePage)> = world
        .query_filtered::<(Entity, &PageMetadata), (With<HostsPage>, Without<LayoutCef>)>()
        .iter(world)
        .filter_map(|(entity, meta)| {
            let page = NATIVE_PAGES.iter().find(|page| page.url == meta.url)?;
            Some((entity, *page))
        })
        .collect();
    if wanted.is_empty() {
        return;
    }

    let Some(window_entity) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .ok()
    else {
        return;
    };
    let Some(requester) = world.get_resource::<Requester>().cloned() else {
        return;
    };
    let Some(bin_ipc) = world
        .get_resource::<BinIpcEventRawSender>()
        .map(|s| s.0.clone())
    else {
        return;
    };
    let waker = dom::PageWaker::of(world.get_resource::<bevy::winit::EventLoopProxyWrapper>());
    if world.get_non_send::<PageSurfaces>().is_none() {
        world.insert_non_send(PageSurfaces::default());
    }

    for (entity, page) in wanted {
        if world
            .get_non_send::<PageSurfaces>()
            .is_some_and(|surfaces| surfaces.0.contains_key(&entity))
        {
            continue;
        }
        // Off screen until the frame sync says where it goes, rather than briefly at the origin.
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(1.0, 1.0).into(),
        };
        let built = WINIT_WINDOWS.with(|winit_windows| {
            let winit_windows = winit_windows.borrow();
            let window = winit_windows.get_window(window_entity)?;
            Some(PageSurface::build(
                page,
                &**window,
                entity,
                bounds,
                bin_ipc.clone(),
                requester.clone(),
                waker.clone(),
            ))
        });
        match built {
            None => return,
            Some(Ok(surface)) => {
                surface.set_visible(false);
                world
                    .non_send_mut::<Browsers>()
                    .set_externally_hosted(entity);
                info!("page_surface: hosting {} for {entity:?}", page.url);
                world
                    .non_send_mut::<PageSurfaces>()
                    .0
                    .insert(entity, surface);
            }
            Some(Err(error)) => {
                error!(
                    "page_surface: build_as_child failed for {}: {error}",
                    page.url
                )
            }
        }
    }
}

/// Move each surface to its pane, and retire the ones whose page is gone.
///
/// Retirement is by sweep rather than by hook: a stack is cleared with `try_despawn`, which no
/// observer here hears, so the only reliable question is whether the entity still exists.
fn place_native_surfaces(
    surfaces: Option<NonSendMut<PageSurfaces>>,
    frames: Res<PaneFrames>,
    pages: Query<(), With<HostsPage>>,
) {
    let Some(mut surfaces) = surfaces else {
        return;
    };
    surfaces.0.retain(|entity, _| pages.contains(*entity));
    for (entity, surface) in surfaces.0.iter() {
        let Some(frame) = frames.of(*entity) else {
            surface.set_visible(false);
            continue;
        };
        surface.set_bounds(wry::Rect {
            position: wry::dpi::LogicalPosition::new(frame.left, frame.top).into(),
            size: wry::dpi::LogicalSize::new(frame.width, frame.height).into(),
        });
        surface.set_visible(true);
    }
}

fn render_native_surfaces(surfaces: Option<NonSend<PageSurfaces>>) {
    let Some(surfaces) = surfaces else {
        return;
    };
    for surface in surfaces.0.values() {
        surface.render();
    }
}

/// Everything that distinguishes one natively-hosted page from another.
///
/// A `const` per page, because a page names itself: the alternative is a registry the pages have to
/// be looked up in, which is one more thing to keep in agreement with them.
pub(crate) struct SurfacePage {
    pub(crate) url: &'static str,
    pub(crate) component: vmux_dioxus::PageComponent,
    /// The element the interpreter renders into, and its classes.
    pub(crate) root_id: &'static str,
    pub(crate) root_class: &'static str,
    /// Everything inside `<head>` — stylesheets, `<base>`, inline rules.
    pub(crate) head: &'static str,
    pub(crate) html_attributes: &'static str,
    pub(crate) body_class: &'static str,
    /// A page drawn over other content wants to see through itself; one filling a pane does not.
    pub(crate) transparent: bool,
}

/// One page's view and the dom that fills it.
pub(crate) struct PageSurface {
    webview: wry::WebView,
    dom: SurfaceDom,
}

impl PageSurface {
    /// Build the view for a page, as a child of the app's window.
    ///
    /// Returns `None` when the window is not up yet, which is a state the caller retries out of
    /// rather than an error.
    pub(crate) fn build(
        page: &'static SurfacePage,
        window: &impl wry::raw_window_handle::HasWindowHandle,
        entity: Entity,
        bounds: wry::Rect,
        bin_ipc: async_channel::Sender<BinIpcEventRaw>,
        requester: Requester,
        waker: PageWaker,
    ) -> Result<Self, wry::Error> {
        let dom = SurfaceDom::mount(
            page.component,
            bin_ipc.clone(),
            entity,
            embedded_page_host_of(page.url).unwrap_or_default(),
            waker,
        );
        let message = PageMessage::new(page, bin_ipc, entity, dom.clone());
        let serve = dom.clone();
        let webview = wry::WebViewBuilder::new()
            .with_transparent(page.transparent)
            .with_initialization_script(WRY_HOST_SHIM)
            .with_asynchronous_custom_protocol("vmux".into(), move |_id, request, responder| {
                VmuxProtocol::serve(page, &serve, &requester, request, responder);
            })
            .with_ipc_handler(move |request| message.receive(request.body()))
            .with_url(page.url)
            .with_bounds(bounds)
            .build_as_child(window)?;

        Ok(Self { webview, dom })
    }

    pub(crate) fn dom(&self) -> &SurfaceDom {
        &self.dom
    }

    pub(crate) fn set_bounds(&self, bounds: wry::Rect) {
        if let Err(error) = self.webview.set_bounds(bounds) {
            error!("page_surface: set_bounds failed: {error}");
        }
    }

    /// A pane the frame sync skipped is one that is not on screen.
    ///
    /// It has to be said explicitly: the sync leaves a hidden pane out rather than giving it an
    /// empty rectangle, so a surface left alone would sit at whatever rectangle it last had.
    pub(crate) fn set_visible(&self, visible: bool) {
        if let Err(error) = self.webview.set_visible(visible) {
            error!("page_surface: set_visible failed: {error}");
        }
    }

    /// Evaluate the next batch of edits, then whatever scripts the page asked for.
    ///
    /// The scripts go after the batch, so an element a component just asked to focus exists to be
    /// found.
    pub(crate) fn render(&self) {
        if let Some(script) = self.dom.next_batch()
            && let Err(error) = self.webview.evaluate_script(script.as_str())
        {
            error!("page_surface: applying an edit batch failed: {error}");
        }
        for script in self.dom.take_pending_scripts() {
            if let Err(error) = self.webview.evaluate_script(&script) {
                error!("page_surface: a page script failed: {error}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn webview(&self) -> &wry::WebView {
        &self.webview
    }
}
