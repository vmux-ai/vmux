//! A page whose components run in this process, painted by a `WKWebView` of its own.
//!
//! The layout was the first, and is no longer the only one: a start pane is the same arrangement in
//! a smaller rectangle. What differs between two native pages is a URL, a root component and the
//! document chrome they render into — a [`NativePage`] — so that is all a caller supplies.
//!
//! The surface itself belongs to [`vmux_native`], which knows nothing of this app: it owns the
//! view, the `vmux://` protocol that answers `__events` and serves the shell, the IPC handler that
//! hears the page back, and the `VirtualDom`. What lives here is the half that is this app's —
//! which entity gets a view, where it sits, who has the keyboard — and the three channels a page
//! reaches back through, in [`PageEmbedder`].

use std::collections::HashMap;
use std::rc::Rc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::BinIpcEventRawSender;
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};
use vmux_core::host::page::HostsPage;
use vmux_core::page_metadata::PageMetadata;
use vmux_layout::LayoutCef;
use vmux_native::{AssetReply, Embedding, NativePage, PageSurface};
use vmux_ui::hooks::EventListenerError;

use crate::present::PaneFrames;

/// Every page this build can host in its own process, by the URL that asks for it.
///
/// The layout is absent on purpose: its view is full-window, front-most and transparent, and it is
/// built before any pane exists. It is the same `PageSurface` underneath, driven by its own systems.
static NATIVE_PAGES: &[&NativePage] = &[];

pub(crate) struct PageSurfacePlugin;

impl Plugin for PageSurfacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open_native_surfaces)
            .add_systems(
                PostUpdate,
                (place_native_surfaces, render_native_surfaces)
                    .chain()
                    .after(crate::present::sync_windowed_frames),
            )
            .add_systems(
                PostUpdate,
                focus_native_surface.after(crate::host_focus::apply_windowed_host_focus),
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

impl PageSurfaces {
    fn get(&self, page: Entity) -> Option<&PageSurface> {
        self.0.get(&page)
    }
}

/// Build a view for any page that asks for one and has not got one yet.
///
/// Exclusive because building needs the winit window and the app's channels at once, and because
/// the surfaces are `NonSend`.
fn open_native_surfaces(world: &mut World) {
    let wanted: Vec<(Entity, &'static NativePage)> = world
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
    let Ok(embedder) = PageEmbedder::of(world) else {
        return;
    };
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
                bounds,
                embedder.embed(entity, page.url),
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

/// Hand first responder to the pane the focus intent named.
///
/// Every frame rather than on the edge, for the same reason the layout view does it: focus is taken
/// away by routes with their own schedules, and having lost it looks exactly like never having had
/// it.
fn focus_native_surface(
    surfaces: Option<NonSend<PageSurfaces>>,
    intent: Res<crate::host_focus::HostFocusIntent>,
) {
    let crate::host_focus::HostFocusIntent::NativePane(page) = *intent else {
        return;
    };
    let Some(surface) = surfaces.as_ref().and_then(|surfaces| surfaces.get(page)) else {
        return;
    };
    surface.take_first_responder();
}

fn render_native_surfaces(surfaces: Option<NonSend<PageSurfaces>>) {
    let Some(surfaces) = surfaces else {
        return;
    };
    for surface in surfaces.0.values() {
        surface.render();
    }
}

/// The app's half of every native page: the channels a surface reaches back through.
///
/// Gathered once and asked for an [`Embedding`] per page, because all three belong to the app
/// rather than to any one page — what makes an embedding a page's own is the entity it addresses.
#[derive(Clone)]
pub(crate) struct PageEmbedder {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    requester: Requester,
    waker: PageWaker,
}

impl PageEmbedder {
    /// Read the app's channels out of the world.
    ///
    /// The error names the one that is missing, because a surface that silently never builds looks
    /// exactly like one that built and rendered nothing.
    pub(crate) fn of(world: &mut World) -> Result<Self, &'static str> {
        let Some(requester) = world.get_resource::<Requester>().cloned() else {
            return Err("no Requester resource, the CEF custom scheme plugin has not built yet");
        };
        let Some(bin_ipc) = world.get_resource::<BinIpcEventRawSender>() else {
            return Err("no BinIpcEventRawSender resource, the cef ipc plugin has not built yet");
        };

        Ok(Self {
            bin_ipc: bin_ipc.0.clone(),
            requester,
            waker: PageWaker::of(world.get_resource::<EventLoopProxyWrapper>()),
        })
    }

    /// What one page's surface is handed when it is built.
    pub(crate) fn embed(&self, entity: Entity, url: &str) -> Embedding {
        Embedding {
            outbox: Rc::new(PageOutbox {
                bin_ipc: self.bin_ipc.clone(),
                webview: entity,
                host: embedded_page_host_of(url).unwrap_or_default(),
            }),
            assets: Rc::new(PageAssets(self.requester.clone())),
            waker: Rc::new(self.waker.clone()),
        }
    }
}

/// Asks winit for a frame, because the page just gave itself something to render.
///
/// The app renders on demand — `UpdateMode::Reactive` with a one-second wait — so every source of
/// work has to say so. A page hosted here has three winit cannot see: an IPC ack, a DOM event
/// answered on the protocol thread, and a host emit running a listener.
#[derive(Clone)]
pub(crate) struct PageWaker(Option<EventLoopProxy<WinitUserEvent>>);

impl PageWaker {
    fn of(proxy: Option<&EventLoopProxyWrapper>) -> Self {
        Self(proxy.map(|proxy| (*proxy).clone()))
    }
}

impl vmux_native::Wake for PageWaker {
    fn wake(&self) {
        let Some(proxy) = self.0.as_ref() else {
            return;
        };
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

/// Where a natively-hosted page's emitted bytes go: onto the channel every existing `BinReceive`
/// observer already reads.
///
/// A page in the wasm bundle reaches the host by base64-ing an envelope through `window.ipc`,
/// which the IPC handler decodes back into a [`BinIpcEventRaw`]. Running in this process the
/// payload is already bytes and the entity is already known, so it goes straight on.
struct PageOutbox {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: String,
}

impl vmux_native::Outbox for PageOutbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        // Unbounded, so this never blocks — which is what lets an event handler call it while the
        // page waits on a synchronous reply.
        self.bin_ipc
            .send_blocking(BinIpcEventRaw {
                webview: self.webview,
                host: self.host.clone(),
                id: id.to_string(),
                payload: bytes.to_vec(),
            })
            .map_err(|_| EventListenerError::Unsupported)
    }
}

/// `vmux://` assets, resolved by the same Bevy systems that answer them for CEF.
///
/// CEF's scheme handler only forwards a [`CefRequest`] down a channel and waits for a
/// [`CefResponse`], so resolution was never CEF-specific and this sends the same request.
struct PageAssets(Requester);

impl vmux_native::Assets for PageAssets {
    /// The reply is handed to a thread rather than answered here: it comes from a Bevy system, and
    /// this runs on the main thread, so blocking would stop the schedule that produces it.
    fn fetch(&self, url: &str, reply: AssetReply) {
        let uri = asset_load_path_from_request_url(url);
        if uri.is_empty() {
            error!("page_surface: vmux:// url maps to no asset path, url={url}");
            reply.fail("no asset path for url");
            return;
        }
        let (tx, rx) = async_channel::bounded::<CefResponse>(1);
        if self
            .0
            .send_blocking(CefRequest {
                uri: uri.clone(),
                responser: Responser(tx),
            })
            .is_err()
        {
            error!("page_surface: vmux:// request channel closed, uri={uri}");
            reply.fail("request channel closed");
            return;
        }
        std::thread::spawn(move || match rx.recv_blocking() {
            Ok(response) => reply.respond(
                response.status_code as u16,
                &response.mime_type,
                response.data,
            ),
            Err(_) => {
                error!("page_surface: vmux:// responder dropped, uri={uri}");
                reply.fail("responder dropped");
            }
        });
    }
}
