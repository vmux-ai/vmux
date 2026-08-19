//! Hosting native pages where there is a `WKWebView` to host them in.

use std::collections::HashMap;
use std::rc::Rc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawSender};
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};
use vmux_core::host::page::HostsPage;
use vmux_core::page_metadata::PageMetadata;
use vmux_layout::LayoutCef;
use vmux_native::{Appearance, AssetReply, Embedding, NativePage, PageSurface};
use vmux_setting::{AppSettings, ColorScheme};
use vmux_ui::hooks::EventListenerError;

use super::{NativePages, Placement};
use crate::present::PaneFrames;

pub(super) struct NativePagesMacosPlugin;

impl Plugin for NativePagesMacosPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                open_native_pages,
                sync_native_appearance.run_if(resource_changed::<AppSettings>),
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (place_native_pages, render_native_pages)
                .chain()
                .after(crate::present::sync_windowed_frames),
        )
        // After the CEF route has had its say, or it re-focuses a pane in the same frame.
        .add_systems(
            PostUpdate,
            focus_native_page.after(crate::host_focus::apply_windowed_host_focus),
        )
        .add_observer(forward_host_emit);
    }
}

/// The pages this process is painting, one per entity.
///
/// A `NonSend` resource rather than a component, which is where this would rather live: a component
/// must be `Send + Sync` and a `wry::WebView` is neither. The entity is the key instead, so the two
/// stay in step by lookup rather than by storage.
#[derive(Default)]
struct HostedPages(HashMap<Entity, HostedPage>);

/// One page's view, and where it was told to put it.
struct HostedPage {
    surface: PageSurface,
    placement: Placement,
}

impl HostedPages {
    fn get(&self, page: Entity) -> Option<&HostedPage> {
        self.0.get(&page)
    }

    /// The entity of the page placed as the window's chrome, if one is running.
    fn layout(&self) -> Option<Entity> {
        for (entity, hosted) in self.0.iter() {
            if hosted.placement == Placement::Layout {
                return Some(*entity);
            }
        }

        None
    }
}

/// Build a view for every registered page that asks for one and has not got one yet.
///
/// Exclusive because building needs the winit window and the app's channels at once, and because
/// the views are `NonSend`.
fn open_native_pages(world: &mut World) {
    let registered = world.resource::<NativePages>().0.clone();
    let mut wanted = Vec::new();
    for (page, placement, instance) in registered {
        for entity in placement.claim(world, page) {
            wanted.push((entity, page, placement, instance));
        }
    }
    wanted.retain(|(entity, _, _, _)| {
        !world
            .get_non_send::<HostedPages>()
            .is_some_and(|hosted| hosted.0.contains_key(entity))
    });
    if wanted.is_empty() {
        return;
    }

    let Ok(window_entity) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
    else {
        report_waiting("no primary window entity");
        return;
    };
    let embedder = match PageEmbedder::of(world) {
        Ok(embedder) => embedder,
        Err(reason) => {
            report_waiting(reason);
            return;
        }
    };
    let appearance = appearance_of(world.resource::<AppSettings>().appearance.mode);
    if world.get_non_send::<HostedPages>().is_none() {
        world.insert_non_send(HostedPages::default());
    }

    for (entity, page, placement, read_instance) in wanted {
        // Off screen until the placement pass says where it goes, rather than briefly at the
        // origin.
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(1.0, 1.0).into(),
        };
        let instance = match read_instance {
            Some(read) => read(world, entity),
            None => vmux_native::Instance::default(),
        };
        let built = WINIT_WINDOWS.with(|winit_windows| {
            let winit_windows = winit_windows.borrow();
            let window = winit_windows.get_window(window_entity)?;
            Some(PageSurface::build(
                page,
                &**window,
                bounds,
                embedder.embed(entity, page.url),
                instance,
            ))
        });
        match built {
            None => {
                report_waiting("primary window has no winit window yet");
                return;
            }
            Some(Ok(surface)) => {
                surface.set_visible(false);
                surface.set_appearance(appearance);
                if placement.is_frontmost() {
                    surface.raise_above_layers();
                }
                world
                    .non_send_mut::<Browsers>()
                    .set_externally_hosted(entity);
                info!(
                    "native_page: hosting {} for {entity:?} as {placement:?}, {appearance:?}",
                    page.url
                );
                world
                    .non_send_mut::<HostedPages>()
                    .0
                    .insert(entity, HostedPage { surface, placement });
            }
            Some(Err(error)) => {
                error!(
                    "native_page: build_as_child failed for {}: {error}",
                    page.url
                )
            }
        }
    }
}

/// Move each view to where its placement puts it, and retire the ones whose page is gone.
///
/// Retirement is by sweep rather than by hook: a stack is cleared with `try_despawn`, which no
/// observer here hears, so the only reliable question is whether the entity still exists.
fn place_native_pages(
    hosted: Option<NonSendMut<HostedPages>>,
    frames: Res<PaneFrames>,
    window: Query<&Window, With<PrimaryWindow>>,
    pages: Query<(), With<HostsPage>>,
) {
    let Some(mut hosted) = hosted else {
        return;
    };
    hosted.0.retain(|entity, _| pages.contains(*entity));
    let window = window.single().ok();
    for (entity, page) in hosted.0.iter() {
        let Some(bounds) = page.placement.bounds(*entity, window, &frames) else {
            page.surface.set_visible(false);
            continue;
        };
        page.surface.set_bounds(bounds);
        page.surface.set_visible(true);
        if page.placement.is_frontmost() {
            page.surface.raise_above_siblings();
        }
    }
}

fn render_native_pages(hosted: Option<NonSend<HostedPages>>) {
    let Some(hosted) = hosted else {
        return;
    };
    for page in hosted.0.values() {
        page.surface.render();
    }
}

/// Hand first responder to the page the focus intent named.
///
/// Every frame rather than on the edge, because focus is taken away by routes with their own
/// schedules, and having lost it looks exactly like never having had it.
///
/// Nothing resigns it here: leaving it to nobody is a state the app is never otherwise in, and CEF
/// then declines to reclaim. A pane takes it back through `set_windowed_focus` instead, which
/// `apply_windowed_host_focus` forces on the way out of these intents.
fn focus_native_page(
    hosted: Option<NonSend<HostedPages>>,
    intent: Res<crate::host_focus::HostFocusIntent>,
) {
    let Some(hosted) = hosted else {
        return;
    };
    let wanted = match *intent {
        crate::host_focus::HostFocusIntent::LayoutView => hosted.layout(),
        crate::host_focus::HostFocusIntent::NativePane(page) => Some(page),
        _ => return,
    };
    let Some(page) = wanted.and_then(|entity| hosted.get(entity)) else {
        return;
    };
    page.surface.take_first_responder();
}

/// Deliver every host→page event aimed at a page running here.
///
/// `bevy_cef`'s own observer still runs and finds no browser for these entities, so it is this that
/// carries the payload the rest of the way — straight to the listener the page registered, with no
/// base64 and no JS shim, because the page is in this process.
fn forward_host_emit(host_emit: On<BinHostEmitEvent>, hosted: Option<NonSend<HostedPages>>) {
    let Some(hosted) = hosted else {
        return;
    };
    let Some(page) = hosted.get(host_emit.webview) else {
        return;
    };
    page.surface.deliver(&host_emit.id, &host_emit.payload);
}

/// Make `prefers-color-scheme` inside every view answer with the app's setting.
///
/// The `theme` event alone is not enough. CEF has a colour-scheme override of its own, which
/// `sync_appearance_to_cef` drives, so a CEF page's media queries already agreed with the setting;
/// a `WKWebView` inherits its `NSAppearance` from the window and has no such thing.
fn sync_native_appearance(hosted: Option<NonSend<HostedPages>>, settings: Res<AppSettings>) {
    let Some(hosted) = hosted else {
        return;
    };
    let appearance = appearance_of(settings.appearance.mode);
    info!("native_page: colour scheme set to {appearance:?}");
    for page in hosted.0.values() {
        page.surface.set_appearance(appearance);
    }
}

fn appearance_of(mode: ColorScheme) -> Appearance {
    match mode {
        ColorScheme::Light => Appearance::Light,
        ColorScheme::Dark => Appearance::Dark,
        ColorScheme::Device => Appearance::System,
    }
}

impl Placement {
    /// Whether the view is put back in front of its siblings every frame.
    ///
    /// A pane opening puts its view last in the parent's subview array, which is where clicks are
    /// resolved from — so a page drawn over the panes has to say so again after every one.
    fn is_frontmost(self) -> bool {
        matches!(self, Self::Layout | Self::Modal)
    }

    /// The entities whose page this is, and which therefore want a view.
    fn claim(self, world: &mut World, page: &NativePage) -> Vec<Entity> {
        match self {
            // The layout has no `PageMetadata` to be matched on. Its entity is spawned by the
            // shell before any page exists and named by a marker instead, and it is the id every
            // `BinReceive` observer in `vmux_layout` is already registered against.
            Self::Layout => world
                .query_filtered::<Entity, With<LayoutCef>>()
                .iter(world)
                .collect(),
            Self::Pane | Self::Modal => {
                let mut claimed = Vec::new();
                let mut pages = world
                    .query_filtered::<(Entity, &PageMetadata), (With<HostsPage>, Without<LayoutCef>)>(
                    );
                for (entity, meta) in pages.iter(world) {
                    if page.answers_for(&meta.url) {
                        claimed.push(entity);
                    }
                }

                claimed
            }
        }
    }

    /// Where the view goes this frame, or `None` while it is not on screen.
    ///
    /// A pane the frame sync skipped is one that is not showing: the sync leaves it out rather
    /// than giving it an empty rectangle, so a view left alone would sit at whatever rectangle it
    /// last had.
    fn bounds(
        self,
        entity: Entity,
        window: Option<&Window>,
        frames: &PaneFrames,
    ) -> Option<wry::Rect> {
        match self {
            // Full window, because the chrome this renders *is* the window's chrome — a smaller
            // box could only ever be sampled over whatever pane happened to be behind it.
            Self::Layout => {
                let window = window?;
                Some(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
                    size: wry::dpi::LogicalSize::new(window.width(), window.height()).into(),
                })
            }
            Self::Pane | Self::Modal => {
                let frame = frames.of(entity)?;
                Some(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(frame.left, frame.top).into(),
                    size: wry::dpi::LogicalSize::new(frame.width, frame.height).into(),
                })
            }
        }
    }
}

/// The app's half of every native page: the channels a view reaches back through.
///
/// Gathered once and asked for an [`Embedding`] per page, because all three belong to the app
/// rather than to any one page — what makes an embedding a page's own is the entity it addresses.
#[derive(Clone)]
struct PageEmbedder {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    requester: Requester,
    waker: PageWaker,
}

impl PageEmbedder {
    /// Read the app's channels out of the world.
    ///
    /// The error names the one that is missing, because a view that silently never builds looks
    /// exactly like one that built and rendered nothing.
    fn of(world: &mut World) -> Result<Self, &'static str> {
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

    /// What one page's view is handed when it is built.
    fn embed(&self, entity: Entity, url: &str) -> Embedding {
        Embedding {
            outbox: Rc::new(PageOutbox {
                bin_ipc: self.bin_ipc.clone(),
                webview: entity,
                host: embedded_page_host_of(url).unwrap_or_default(),
            }),
            assets: Rc::new(PageAssets {
                requester: self.requester.clone(),
                waker: self.waker.clone(),
            }),
            waker: Rc::new(self.waker.clone()),
        }
    }
}

/// Asks winit for a frame, because a page just gave itself something to render.
///
/// The app renders on demand — `UpdateMode::Reactive` with a one-second wait — so every source of
/// work has to say so. A page hosted here has three winit cannot see: an IPC ack, a DOM event
/// answered on the protocol thread, and a host emit running a listener.
#[derive(Clone)]
struct PageWaker(Option<EventLoopProxy<WinitUserEvent>>);

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
struct PageAssets {
    requester: Requester,
    /// The reply is produced by a Bevy system, and the app renders on demand — so a request that
    /// does not ask for a frame waits out the reactive timeout before anything looks at it. A
    /// page opening from idle asks for its shell, its stylesheets and its fonts in a burst, and
    /// each one paid that wait in turn.
    waker: PageWaker,
}

impl vmux_native::Assets for PageAssets {
    /// The reply is handed to a thread rather than answered here: it comes from a Bevy system, and
    /// this runs on the main thread, so blocking would stop the schedule that produces it.
    fn fetch(&self, url: &str, reply: AssetReply) {
        let uri = asset_load_path_from_request_url(url);
        if uri.is_empty() {
            error!("native_page: vmux:// url maps to no asset path, url={url}");
            reply.fail("no asset path for url");
            return;
        }
        let (tx, rx) = async_channel::bounded::<CefResponse>(1);
        if self
            .requester
            .send_blocking(CefRequest {
                uri: uri.clone(),
                responser: Responser(tx),
            })
            .is_err()
        {
            error!("native_page: vmux:// request channel closed, uri={uri}");
            reply.fail("request channel closed");
            return;
        }
        vmux_native::Wake::wake(&self.waker);
        std::thread::spawn(move || match rx.recv_blocking() {
            Ok(response) => reply.respond(
                response.status_code as u16,
                &response.mime_type,
                response.data,
            ),
            Err(_) => {
                error!("native_page: vmux:// responder dropped, uri={uri}");
                reply.fail("responder dropped");
            }
        });
    }
}

/// A view that silently never builds looks exactly like a view that built and rendered nothing,
/// which is how an early run of this was misread. Say why it has not built yet, once.
fn report_waiting(reason: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        info!("native_page: waiting, {reason}");
    }
}
