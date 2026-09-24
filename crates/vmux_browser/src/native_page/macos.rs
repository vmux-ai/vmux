use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::rc::Rc;
use std::sync::{Mutex, mpsc};

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawSender, HostWindow, ZoomLevel};
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};
use vmux_core::PageOpenSet;
use vmux_core::host::page::HostsPage;
use vmux_core::page_metadata::PageMetadata;
use vmux_layout::LayoutCef;
use vmux_native::{
    Appearance, AssetReply, Embedding, NativePage, NativePagePlacement, NativePageRegistration,
    SiblingOrder, WebView,
};
use vmux_setting::{AppSettings, ColorScheme};
use vmux_ui::hooks::EventListenerError;

use crate::LayoutPointerCapture;
use crate::present::PaneFrames;

pub(super) struct NativePageMacosPlugin;

impl Plugin for NativePageMacosPlugin {
    fn build(&self, app: &mut App) {
        let (metadata_tx, metadata_rx) = async_channel::unbounded();
        app.insert_resource(NativePageMetadataSender(metadata_tx))
            .insert_resource(NativePageMetadataReceiver(metadata_rx))
            .add_systems(First, accept_page_wakes)
            .add_systems(
                Update,
                (
                    apply_native_page_metadata,
                    open_native_pages.after(PageOpenSet::HandleKnownPages),
                    sync_native_appearance.run_if(resource_changed::<AppSettings>),
                    sync_native_page_scale,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (place_native_pages, render_native_pages)
                    .chain()
                    .after(crate::present::sync_windowed_frames),
            )
            .add_systems(
                PostUpdate,
                focus_native_page.after(crate::host_focus::apply_windowed_host_focus),
            )
            .add_observer(forward_host_emit);
    }
}

#[derive(Default)]
struct HostedPages(HashMap<Entity, HostedPage>);

struct HostedPage {
    surface: WebView,
    placement: NativePagePlacement,
    page: &'static NativePage,
    window: Entity,
}

struct NativePageMetadata {
    webview: Entity,
    page_url: String,
    title: Option<String>,
    favicon: Option<String>,
}

#[derive(Resource, Clone)]
struct NativePageMetadataSender(async_channel::Sender<NativePageMetadata>);

#[derive(Resource)]
struct NativePageMetadataReceiver(async_channel::Receiver<NativePageMetadata>);

fn apply_native_page_metadata(
    receiver: Res<NativePageMetadataReceiver>,
    mut pages: Query<&mut PageMetadata>,
) {
    while let Ok(update) = receiver.0.try_recv() {
        let Ok(mut metadata) = pages.get_mut(update.webview) else {
            continue;
        };
        if !metadata.url.starts_with(&update.page_url) {
            continue;
        }
        if let Some(title) = update.title
            && !title.is_empty()
        {
            metadata.title = title;
        }
        if let Some(favicon) = update.favicon
            && !favicon.is_empty()
        {
            metadata.icon = vmux_core::PageIcon::favicon(favicon);
        }
    }
}

impl HostedPages {
    fn get(&self, page: Entity) -> Option<&HostedPage> {
        self.0.get(&page)
    }

    fn layout(&self, window: Option<Entity>) -> Option<Entity> {
        for (entity, hosted) in self.0.iter() {
            if hosted.placement == NativePagePlacement::Layout
                && window.is_none_or(|window| hosted.window == window)
            {
                return Some(*entity);
            }
        }

        None
    }
}

fn open_native_pages(world: &mut World) {
    let registered = world
        .query::<&NativePageRegistration>()
        .iter(world)
        .copied()
        .collect::<Vec<_>>();
    let mut wanted = Vec::new();
    for registration in registered {
        for entity in registration.placement().claim(world, registration) {
            wanted.push((entity, registration));
        }
    }
    if wanted.is_empty() {
        return;
    }

    let primary_window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .ok();
    let embedder = match PageEmbedder::load(world) {
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

    for (entity, registration) in wanted {
        let page = registration.page();
        let placement = registration.placement();
        let Some(window_entity) = host_window_for(world, entity).or(primary_window) else {
            report_waiting("page has no host window entity");
            continue;
        };
        let current = world
            .get_non_send::<HostedPages>()
            .and_then(|hosted| hosted.0.get(&entity))
            .map(|hosted| (hosted.page, hosted.window));
        if current
            .is_some_and(|(current, window)| std::ptr::eq(current, page) && window == window_entity)
        {
            continue;
        }
        let instance = registration.instance(world, entity);
        if current.is_some_and(|(current, window)| {
            current.transparent == page.transparent && window == window_entity
        }) {
            let remounted = {
                let mut hosted = world.non_send_mut::<HostedPages>();
                let hosted = hosted.0.get_mut(&entity).expect("the page was just found");
                let remounted = hosted.surface.navigate(page, instance);
                hosted.page = page;
                hosted.placement = placement;
                remounted
            };
            if remounted {
                world
                    .entity_mut(entity)
                    .remove::<vmux_core::page::PageReady>();
            }
            info!("native_page: navigated {entity:?} to {}", page.url);
            continue;
        }
        if current.is_some() {
            world.non_send_mut::<HostedPages>().0.remove(&entity);
        }
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(1.0, 1.0).into(),
        };
        let built = WINIT_WINDOWS.with(|winit_windows| {
            let winit_windows = winit_windows.borrow();
            let window = winit_windows.get_window(window_entity)?;
            Some(WebView::build(
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
                if placement.paints_in_front() {
                    surface.raise_above_layers();
                }
                world
                    .non_send_mut::<Browsers>()
                    .set_externally_hosted(entity);
                info!(
                    "native_page: hosting {} for {entity:?} as {placement:?}, {appearance:?}",
                    page.url
                );
                world.non_send_mut::<HostedPages>().0.insert(
                    entity,
                    HostedPage {
                        surface,
                        placement,
                        page,
                        window: window_entity,
                    },
                );
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

fn place_native_pages(
    hosted: Option<NonSendMut<HostedPages>>,
    frames: Res<PaneFrames>,
    windows: Query<&Window>,
    pages: Query<(), With<HostsPage>>,
    capturing: Query<&HostWindow, (With<LayoutCef>, LayoutPointerCapture)>,
    settings: Res<AppSettings>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Some(mut hosted) = hosted else {
        return;
    };
    let held = hosted.0.len();
    hosted.0.retain(|entity, _| pages.contains(*entity));
    if hosted.0.len() != held
        && let Some(proxy) = proxy
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
    for (entity, page) in hosted.0.iter() {
        let window = windows.get(page.window).ok();
        let Some(bounds) = page.placement.bounds(*entity, window, &frames) else {
            page.surface.set_visible(false);
            continue;
        };
        page.surface.set_bounds(bounds);
        page.surface
            .set_corner_radius(settings.layout.radius as f64, frames.all_corners(*entity));
        let ring = frames.ring_of(*entity);
        page.surface.set_focus_ring(ring.width as f64, ring.rgb);
        page.surface.set_visible(true);
        let window_is_capturing = capturing.iter().any(|host| host.0 == page.window);
        if let Some(order) = page.placement.pointer_order(window_is_capturing) {
            page.surface.order_among_siblings(order);
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

fn focus_native_page(
    hosted: Option<NonSend<HostedPages>>,
    intent: Res<crate::host_focus::HostFocusIntent>,
    focused_window: Res<vmux_layout::window::FocusedWindow>,
) {
    let Some(hosted) = hosted else {
        return;
    };
    let wanted = match *intent {
        crate::host_focus::HostFocusIntent::LayoutView => hosted.layout(focused_window.0),
        crate::host_focus::HostFocusIntent::NativePane(page) => Some(page),
        _ => return,
    };
    let Some(page) = wanted.and_then(|entity| hosted.get(entity)) else {
        return;
    };
    page.surface.take_first_responder();
}

fn host_window_for(world: &World, entity: Entity) -> Option<Entity> {
    let mut current = entity;
    loop {
        if let Some(host) = world.get::<bevy_cef::prelude::HostWindow>(current) {
            return Some(host.0);
        }
        current = world.get::<ChildOf>(current).map(Relationship::get)?;
    }
}

fn forward_host_emit(host_emit: On<BinHostEmitEvent>, hosted: Option<NonSend<HostedPages>>) {
    let Some(hosted) = hosted else {
        return;
    };
    let Some(page) = hosted.get(host_emit.webview()) else {
        return;
    };
    let host = embedded_page_host_of(page.page.url).unwrap_or_default();
    if !host_emit.target().accepts(&host) {
        warn!(
            "blocked binary host event {} for unexpected native page host {host}",
            host_emit.id()
        );
        return;
    }
    page.surface.deliver(host_emit.id(), host_emit.payload());
}

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

fn sync_native_page_scale(
    hosted: Option<NonSend<HostedPages>>,
    zoom: Query<(Entity, &ZoomLevel), Changed<ZoomLevel>>,
) {
    let Some(hosted) = hosted else {
        return;
    };
    for (entity, level) in zoom.iter() {
        let Some(page) = hosted.0.get(&entity) else {
            continue;
        };
        page.surface.set_page_scale(page_scale_of(level.0));
    }
}

fn page_scale_of(level: f64) -> f64 {
    1.2f64.powf(level)
}

fn appearance_of(mode: ColorScheme) -> Appearance {
    match mode {
        ColorScheme::Light => Appearance::Light,
        ColorScheme::Dark => Appearance::Dark,
        ColorScheme::Device => Appearance::System,
    }
}

trait NativePagePlacementExt {
    fn paints_in_front(self) -> bool;
    fn pointer_order(self, capturing: bool) -> Option<SiblingOrder>;
    fn claim(self, world: &mut World, registration: NativePageRegistration) -> Vec<Entity>;
    fn bounds(
        self,
        entity: Entity,
        window: Option<&Window>,
        frames: &PaneFrames,
    ) -> Option<wry::Rect>;
}

impl NativePagePlacementExt for NativePagePlacement {
    fn paints_in_front(self) -> bool {
        matches!(
            self,
            NativePagePlacement::Layout | NativePagePlacement::Modal
        )
    }

    fn pointer_order(self, capturing: bool) -> Option<SiblingOrder> {
        match self {
            NativePagePlacement::Layout if !capturing => Some(SiblingOrder::Back),
            NativePagePlacement::Layout | NativePagePlacement::Modal => Some(SiblingOrder::Front),
            NativePagePlacement::Pane => None,
        }
    }

    fn claim(self, world: &mut World, registration: NativePageRegistration) -> Vec<Entity> {
        match self {
            NativePagePlacement::Layout => world
                .query_filtered::<Entity, With<LayoutCef>>()
                .iter(world)
                .collect(),
            NativePagePlacement::Pane | NativePagePlacement::Modal => {
                let candidates = world
                    .query_filtered::<(Entity, &PageMetadata), (With<HostsPage>, Without<LayoutCef>)>()
                    .iter(world)
                    .map(|(entity, metadata)| (entity, metadata.url.clone()))
                    .collect::<Vec<_>>();
                let mut claimed = Vec::new();
                for (entity, url) in candidates {
                    if registration.answers_for(world, entity, &url) {
                        claimed.push(entity);
                    }
                }

                claimed
            }
        }
    }

    fn bounds(
        self,
        entity: Entity,
        window: Option<&Window>,
        frames: &PaneFrames,
    ) -> Option<wry::Rect> {
        match self {
            NativePagePlacement::Layout => {
                let window = window?;
                Some(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
                    size: wry::dpi::LogicalSize::new(window.width(), window.height()).into(),
                })
            }
            NativePagePlacement::Pane | NativePagePlacement::Modal => {
                let frame = frames.frame(entity)?;
                Some(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(frame.left, frame.top).into(),
                    size: wry::dpi::LogicalSize::new(frame.width, frame.height).into(),
                })
            }
        }
    }
}

#[derive(Clone)]
struct PageEmbedder {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    metadata: NativePageMetadataSender,
    requester: Requester,
    waker: PageWaker,
}

impl PageEmbedder {
    fn load(world: &mut World) -> Result<Self, &'static str> {
        let Some(requester) = world.get_resource::<Requester>().cloned() else {
            return Err("no Requester resource, the CEF custom scheme plugin has not built yet");
        };
        let Some(bin_ipc) = world.get_resource::<BinIpcEventRawSender>() else {
            return Err("no BinIpcEventRawSender resource, the cef ipc plugin has not built yet");
        };
        let Some(metadata) = world.get_resource::<NativePageMetadataSender>() else {
            return Err("no native page metadata sender");
        };

        Ok(Self {
            bin_ipc: bin_ipc.0.clone(),
            metadata: metadata.clone(),
            requester,
            waker: PageWaker::from_proxy(world.get_resource::<EventLoopProxyWrapper>()),
        })
    }

    fn embed(&self, entity: Entity, url: &str) -> Embedding {
        let page_url = Rc::new(RefCell::new(url.to_string()));
        Embedding {
            outbox: Rc::new(PageOutbox {
                bin_ipc: self.bin_ipc.clone(),
                webview: entity,
                host: RefCell::new(embedded_page_host_of(url).unwrap_or_default()),
                page_url: page_url.clone(),
                metadata: self.metadata.clone(),
                waker: self.waker.clone(),
            }),
            assets: Rc::new(PageAssets {
                requester: self.requester.clone(),
                waker: self.waker.clone(),
                page_url,
                simulator_frames: SimulatorFrameProxy::default(),
            }),
            waker: Rc::new(self.waker.clone()),
        }
    }
}

#[derive(Clone)]
struct PageWaker(Option<EventLoopProxy<WinitUserEvent>>);

impl PageWaker {
    fn from_proxy(proxy: Option<&EventLoopProxyWrapper>) -> Self {
        Self(proxy.map(|proxy| (*proxy).clone()))
    }
}

static PAGE_WAKE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl vmux_native::Wake for PageWaker {
    fn wake(&self) {
        let Some(proxy) = self.0.as_ref() else {
            return;
        };
        if PAGE_WAKE_PENDING.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

pub(crate) fn accept_page_wakes(_: bevy::ecs::system::NonSendMarker) {
    PAGE_WAKE_PENDING.store(false, std::sync::atomic::Ordering::Release);
}

struct PageOutbox {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: RefCell<String>,
    page_url: Rc<RefCell<String>>,
    metadata: NativePageMetadataSender,
    waker: PageWaker,
}

impl vmux_native::Outbox for PageOutbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        self.bin_ipc
            .send_blocking(BinIpcEventRaw {
                webview: self.webview,
                host: self.host.borrow().clone(),
                id: id.to_string(),
                payload: bytes.to_vec(),
            })
            .map_err(|_| EventListenerError::Unsupported)
    }

    fn set_page(&self, url: &str) {
        *self.host.borrow_mut() = embedded_page_host_of(url).unwrap_or_default();
        *self.page_url.borrow_mut() = url.to_string();
    }

    fn set_title(&self, title: &str) {
        if self
            .metadata
            .0
            .send_blocking(NativePageMetadata {
                webview: self.webview,
                page_url: self.page_url.borrow().clone(),
                title: Some(title.to_string()),
                favicon: None,
            })
            .is_ok()
        {
            vmux_native::Wake::wake(&self.waker);
        }
    }

    fn set_favicon(&self, url: &str) {
        if self
            .metadata
            .0
            .send_blocking(NativePageMetadata {
                webview: self.webview,
                page_url: self.page_url.borrow().clone(),
                title: None,
                favicon: Some(url.to_string()),
            })
            .is_ok()
        {
            vmux_native::Wake::wake(&self.waker);
        }
    }
}

struct PageAssets {
    requester: Requester,
    waker: PageWaker,
    page_url: Rc<RefCell<String>>,
    simulator_frames: SimulatorFrameProxy,
}

impl vmux_native::Assets for PageAssets {
    fn fetch(&self, url: &str, reply: AssetReply) {
        match SimulatorFrameRequest::parse(url, &self.page_url.borrow()) {
            Ok(Some(request)) => {
                self.simulator_frames.fetch(request, reply);
                return;
            }
            Ok(None) => {}
            Err(error) => {
                error!("simulator frame asset rejected url={url}: {error}");
                reply.fail(error);
                return;
            }
        }
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

struct SimulatorFrameRequest {
    port: u16,
    capability: String,
    after: u64,
}

#[derive(Default)]
struct SimulatorFrameProxy {
    sender: Mutex<Option<mpsc::Sender<SimulatorFrameJob>>>,
}

struct SimulatorFrameJob {
    request: SimulatorFrameRequest,
    reply: AssetReply,
}

struct SimulatorFrame {
    generation: u64,
    bytes: Vec<u8>,
}

impl SimulatorFrameProxy {
    fn fetch(&self, request: SimulatorFrameRequest, reply: AssetReply) {
        let sender = match self.sender() {
            Ok(sender) => sender,
            Err(error) => {
                reply.fail(&error);
                return;
            }
        };
        if let Err(error) = sender.send(SimulatorFrameJob { request, reply }) {
            error.0.reply.fail("simulator frame proxy stopped");
        }
    }

    fn sender(&self) -> Result<mpsc::Sender<SimulatorFrameJob>, String> {
        let mut sender = self
            .sender
            .lock()
            .map_err(|_| "simulator frame proxy lock failed".to_string())?;
        if let Some(sender) = sender.as_ref() {
            return Ok(sender.clone());
        }
        let (next, receiver) = mpsc::channel::<SimulatorFrameJob>();
        std::thread::Builder::new()
            .name("vmux-simulator-frame-proxy".into())
            .spawn(move || {
                for job in receiver {
                    job.run();
                }
            })
            .map_err(|error| format!("could not start simulator frame proxy: {error}"))?;
        *sender = Some(next.clone());
        Ok(next)
    }
}

impl SimulatorFrameJob {
    fn run(self) {
        match self.request.fetch() {
            Ok(frame) => {
                let mut body = Vec::with_capacity(8 + frame.bytes.len());
                body.extend_from_slice(&frame.generation.to_le_bytes());
                body.extend_from_slice(&frame.bytes);
                self.reply.respond(200, "application/octet-stream", body);
            }
            Err(error) => {
                error!("simulator frame proxy request failed: {error}");
                self.reply.fail(&error);
            }
        }
    }
}

impl SimulatorFrameRequest {
    const PATH: &'static str = "/__simulator-frame";

    fn parse(request_url: &str, page_url: &str) -> Result<Option<Self>, &'static str> {
        let parsed = url::Url::parse(request_url).map_err(|_| "simulator frame URL is invalid")?;
        if parsed.path() != Self::PATH {
            return Ok(None);
        }
        if parsed.scheme() != "vmux" {
            return Err("simulator frame URL is unavailable");
        }
        let page = url::Url::parse(page_url).map_err(|_| "simulator page URL is invalid")?;
        if page.scheme() != "vmux" || page.host_str() != Some("simulator") {
            return Err("simulator frames are unavailable to this page");
        }
        let mut port = None;
        let mut capability = None;
        let mut after = None;
        for (name, value) in parsed.query_pairs() {
            match name.as_ref() {
                "port" => port = value.parse().ok(),
                "capability" => capability = Some(value.into_owned()),
                "after" => after = value.parse().ok(),
                _ => {}
            }
        }
        let Some(port) = port else {
            return Err("simulator frame port is invalid");
        };
        if port == 0 {
            return Err("simulator frame port is invalid");
        }
        let Some(capability) = capability else {
            return Err("simulator frame capability is missing");
        };
        if capability.len() != 32 || !capability.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("simulator frame capability is invalid");
        }
        Ok(Some(Self {
            port,
            capability,
            after: after.unwrap_or(0),
        }))
    }

    fn fetch(self) -> Result<SimulatorFrame, String> {
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut socket = TcpStream::connect(address)
            .map_err(|error| format!("simulator frame connection failed: {error}"))?;
        socket
            .set_nodelay(true)
            .map_err(|error| format!("simulator frame socket setup failed: {error}"))?;
        let request = format!(
            "GET /{capability}?after={after} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
            capability = self.capability,
            after = self.after,
            port = self.port,
        );
        socket
            .write_all(request.as_bytes())
            .map_err(|error| format!("simulator frame request failed: {error}"))?;
        let mut response = Vec::new();
        socket
            .read_to_end(&mut response)
            .map_err(|error| format!("simulator frame response failed: {error}"))?;
        SimulatorFrame::decode(response, self.after)
    }
}

impl SimulatorFrame {
    fn decode(response: Vec<u8>, after: u64) -> Result<Self, String> {
        let header_end = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .map(|index| index + 4)
            .ok_or_else(|| "simulator frame response has no headers".to_string())?;
        let headers = std::str::from_utf8(&response[..header_end])
            .map_err(|_| "simulator frame response headers are invalid".to_string())?;
        let mut lines = headers.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| "simulator frame response status is invalid".to_string())?;
        if status != 200 {
            return Err(format!("simulator frame returned {status}"));
        }
        let mut generation = None;
        let mut content_length = None;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if name.eq_ignore_ascii_case("x-vmux-generation") {
                generation = value.trim().parse::<u64>().ok();
            }
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
        let bytes = response[header_end..].to_vec();
        if content_length != Some(bytes.len()) {
            return Err("simulator frame response length is invalid".to_string());
        }
        Ok(Self {
            generation: generation.unwrap_or(after.wrapping_add(1)),
            bytes,
        })
    }
}

fn report_waiting(reason: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        info!("native_page: waiting, {reason}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativePageMetadataReceiver, NativePageMetadataSender, NativePagePlacementExt, PageOutbox,
        SimulatorFrame, SimulatorFrameRequest,
    };
    use bevy::prelude::{App, MinimalPlugins, Update};
    use vmux_native::{NativePagePlacement, SiblingOrder};

    #[test]
    fn the_layout_is_asked_for_the_pointer_only_while_a_surface_of_its_own_is_up() {
        assert_eq!(
            NativePagePlacement::Layout.pointer_order(false),
            Some(SiblingOrder::Back)
        );
        assert_eq!(
            NativePagePlacement::Layout.pointer_order(true),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            NativePagePlacement::Modal.pointer_order(false),
            Some(SiblingOrder::Front)
        );
        assert_eq!(NativePagePlacement::Pane.pointer_order(false), None);
        assert!(NativePagePlacement::Layout.paints_in_front());
    }

    #[test]
    fn a_navigated_page_emits_as_its_new_host() {
        let (tx, rx) = async_channel::bounded(1);
        let (metadata_tx, _metadata_rx) = async_channel::bounded(1);
        let outbox = PageOutbox {
            bin_ipc: tx,
            webview: bevy::prelude::Entity::PLACEHOLDER,
            host: std::cell::RefCell::new("start".to_string()),
            page_url: std::rc::Rc::new(std::cell::RefCell::new("vmux://start/".to_string())),
            metadata: NativePageMetadataSender(metadata_tx),
            waker: super::PageWaker(None),
        };

        vmux_native::Outbox::set_page(&outbox, "vmux://sessions/claude");
        vmux_native::Outbox::send(&outbox, "event", &[1, 2, 3]).unwrap();

        let emitted = rx.recv_blocking().unwrap();
        assert_eq!(emitted.host, "sessions");
    }

    #[test]
    fn document_metadata_updates_the_hosted_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, super::apply_native_page_metadata);
        let (metadata_tx, metadata_rx) = async_channel::unbounded();
        app.insert_resource(NativePageMetadataReceiver(metadata_rx));
        let page = app
            .world_mut()
            .spawn(vmux_core::PageMetadata {
                title: "vmux://history/".to_string(),
                url: "vmux://history/".to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            })
            .id();
        let outbox = PageOutbox {
            bin_ipc: async_channel::unbounded().0,
            webview: page,
            host: std::cell::RefCell::new("history".to_string()),
            page_url: std::rc::Rc::new(std::cell::RefCell::new("vmux://history/".to_string())),
            metadata: NativePageMetadataSender(metadata_tx),
            waker: super::PageWaker(None),
        };

        vmux_native::Outbox::set_title(&outbox, "History");
        vmux_native::Outbox::set_favicon(&outbox, "vmux://history/assets/favicons/history.svg");
        app.update();

        let metadata = app.world().get::<vmux_core::PageMetadata>(page).unwrap();
        assert_eq!(metadata.title, "History");
        assert_eq!(
            metadata.icon,
            vmux_core::PageIcon::favicon("vmux://history/assets/favicons/history.svg")
        );
    }

    #[test]
    fn simulator_page_accepts_its_frame_request() {
        let request = SimulatorFrameRequest::parse(
            "vmux://simulator/__simulator-frame?port=58352&capability=0123456789abcdef0123456789abcdef&after=42",
            "vmux://simulator/",
        )
        .unwrap()
        .unwrap();

        assert_eq!(request.port, 58352);
        assert_eq!(request.capability, "0123456789abcdef0123456789abcdef");
        assert_eq!(request.after, 42);
    }

    #[test]
    fn simulator_frame_response_preserves_generation_and_body() {
        let response =
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nX-Vmux-Generation: 42\r\n\r\nabc".to_vec();

        let frame = SimulatorFrame::decode(response, 41).unwrap();

        assert_eq!(frame.generation, 42);
        assert_eq!(frame.bytes, b"abc");
    }

    #[test]
    fn other_pages_cannot_request_simulator_frames() {
        let request = SimulatorFrameRequest::parse(
            "vmux://simulator/__simulator-frame?port=58352&capability=0123456789abcdef0123456789abcdef&after=42",
            "vmux://start/",
        );

        assert_eq!(
            request.err(),
            Some("simulator frames are unavailable to this page")
        );
    }

    #[test]
    fn malformed_simulator_frame_requests_are_rejected() {
        for request_url in [
            "vmux://simulator/__simulator-frame?port=0&capability=0123456789abcdef0123456789abcdef",
            "vmux://simulator/__simulator-frame?port=not-a-port&capability=0123456789abcdef0123456789abcdef",
            "vmux://simulator/__simulator-frame?port=58352&capability=too-short",
            "https://simulator/__simulator-frame?port=58352&capability=0123456789abcdef0123456789abcdef",
        ] {
            assert!(
                SimulatorFrameRequest::parse(request_url, "vmux://simulator/").is_err(),
                "{request_url}"
            );
        }
    }

    #[test]
    fn normal_assets_are_not_simulator_frame_requests() {
        assert!(matches!(
            SimulatorFrameRequest::parse("vmux://simulator/assets/index.css", "vmux://simulator/"),
            Ok(None)
        ));
    }

    #[test]
    fn simulator_frame_requests_use_the_documents_same_origin_host() {
        assert!(
            SimulatorFrameRequest::parse(
                "vmux://start/__simulator-frame?port=58352&capability=0123456789abcdef0123456789abcdef&after=42",
                "vmux://simulator/",
            )
            .unwrap()
            .is_some()
        );
    }
}
