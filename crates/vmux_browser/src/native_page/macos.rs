use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawSender, ZoomLevel};
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};
use vmux_core::PageOpenSet;
use vmux_core::host::page::HostsPage;
use vmux_core::page_metadata::PageMetadata;
use vmux_layout::LayoutCef;
use vmux_native::{Appearance, AssetReply, Embedding, NativePage, SiblingOrder, WebView};
use vmux_setting::{AppSettings, ColorScheme};
use vmux_ui::hooks::EventListenerError;

use super::{NativePages, Placement};
use crate::LayoutPointerCapture;
use crate::present::PaneFrames;

pub(super) struct NativePagesMacosPlugin;

impl Plugin for NativePagesMacosPlugin {
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
    placement: Placement,
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
            if hosted.placement == Placement::Layout
                && window.is_none_or(|window| hosted.window == window)
            {
                return Some(*entity);
            }
        }

        None
    }
}

fn open_native_pages(world: &mut World) {
    let registered = world.resource::<NativePages>().0.clone();
    let mut wanted = Vec::new();
    for (page, placement, instance) in registered {
        for entity in placement.claim(world, page) {
            wanted.push((entity, page, placement, instance));
        }
    }
    if wanted.is_empty() {
        return;
    }

    let primary_window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .ok();
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
        let instance = match read_instance {
            Some(read) => read(world, entity),
            None => vmux_native::Instance::default(),
        };
        if current.is_some_and(|(current, window)| {
            current.transparent == page.transparent && window == window_entity
        }) {
            world
                .entity_mut(entity)
                .remove::<vmux_core::page::PageReady>();
            {
                let mut hosted = world.non_send_mut::<HostedPages>();
                let hosted = hosted.0.get_mut(&entity).expect("the page was just found");
                hosted.surface.navigate(page, instance);
                hosted.page = page;
                hosted.placement = placement;
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
    capturing: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
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
    let capturing = !capturing.is_empty();
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
        if let Some(order) = page.placement.pointer_order(capturing) {
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
    let Some(page) = hosted.get(host_emit.webview) else {
        return;
    };
    page.surface.deliver(&host_emit.id, &host_emit.payload);
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

impl Placement {
    fn paints_in_front(self) -> bool {
        matches!(self, Self::Layout | Self::Modal)
    }

    fn pointer_order(self, capturing: bool) -> Option<SiblingOrder> {
        match self {
            Self::Layout if !capturing => Some(SiblingOrder::Back),
            Self::Layout | Self::Modal => Some(SiblingOrder::Front),
            Self::Pane => None,
        }
    }

    fn claim(self, world: &mut World, page: &NativePage) -> Vec<Entity> {
        match self {
            Self::Layout => world
                .query_filtered::<Entity, With<LayoutCef>>()
                .iter(world)
                .collect(),
            Self::Pane | Self::Modal => {
                let mut claimed = Vec::new();
                let mut pages = world.query_filtered::<
                    (Entity, &PageMetadata, Has<vmux_terminal::Terminal>),
                    (With<HostsPage>, Without<LayoutCef>),
                >();
                for (entity, meta, terminal) in pages.iter(world) {
                    let url = if terminal {
                        super::TERMINAL_PAGE.url
                    } else {
                        &meta.url
                    };
                    if page.answers_for(url) {
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

#[derive(Clone)]
struct PageEmbedder {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    metadata: NativePageMetadataSender,
    requester: Requester,
    waker: PageWaker,
}

impl PageEmbedder {
    fn of(world: &mut World) -> Result<Self, &'static str> {
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
            waker: PageWaker::of(world.get_resource::<EventLoopProxyWrapper>()),
        })
    }

    fn embed(&self, entity: Entity, url: &str) -> Embedding {
        Embedding {
            outbox: Rc::new(PageOutbox {
                bin_ipc: self.bin_ipc.clone(),
                webview: entity,
                host: RefCell::new(embedded_page_host_of(url).unwrap_or_default()),
                page_url: RefCell::new(url.to_string()),
                metadata: self.metadata.clone(),
                waker: self.waker.clone(),
            }),
            assets: Rc::new(PageAssets {
                requester: self.requester.clone(),
                waker: self.waker.clone(),
            }),
            waker: Rc::new(self.waker.clone()),
        }
    }
}

#[derive(Clone)]
struct PageWaker(Option<EventLoopProxy<WinitUserEvent>>);

impl PageWaker {
    fn of(proxy: Option<&EventLoopProxyWrapper>) -> Self {
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
    page_url: RefCell<String>,
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
}

impl vmux_native::Assets for PageAssets {
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
        NativePageMetadataReceiver, NativePageMetadataSender, PageOutbox, Placement, SiblingOrder,
    };
    use bevy::prelude::{App, MinimalPlugins, Update};

    #[test]
    fn the_layout_is_asked_for_the_pointer_only_while_a_surface_of_its_own_is_up() {
        assert_eq!(
            Placement::Layout.pointer_order(false),
            Some(SiblingOrder::Back)
        );
        assert_eq!(
            Placement::Layout.pointer_order(true),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::Modal.pointer_order(false),
            Some(SiblingOrder::Front)
        );
        assert_eq!(Placement::Pane.pointer_order(false), None);
        assert!(Placement::Layout.paints_in_front());
    }

    #[test]
    fn a_navigated_page_emits_as_its_new_host() {
        let (tx, rx) = async_channel::bounded(1);
        let (metadata_tx, _metadata_rx) = async_channel::bounded(1);
        let outbox = PageOutbox {
            bin_ipc: tx,
            webview: bevy::prelude::Entity::PLACEHOLDER,
            host: std::cell::RefCell::new("start".to_string()),
            page_url: std::cell::RefCell::new("vmux://start/".to_string()),
            metadata: NativePageMetadataSender(metadata_tx),
            waker: super::PageWaker(None),
        };

        vmux_native::Outbox::set_page(&outbox, "vmux://sessions/claude");
        vmux_native::Outbox::send(&outbox, "event", &[1, 2, 3]).unwrap();

        let emitted = rx.recv_blocking().unwrap();
        assert_eq!(emitted.host, "sessions");
    }

    #[test]
    fn a_cli_terminal_keeps_the_terminal_renderer() {
        let mut world = bevy::prelude::World::new();
        let terminal = world
            .spawn((
                vmux_core::host::page::HostsPage,
                vmux_terminal::Terminal,
                vmux_core::PageMetadata {
                    url: "vmux://sessions/claude/cli".to_string(),
                    ..Default::default()
                },
            ))
            .id();

        assert_eq!(
            Placement::Pane.claim(&mut world, &super::super::TERMINAL_PAGE),
            vec![terminal]
        );
        assert!(
            Placement::Pane
                .claim(&mut world, &super::super::CHAT_PAGE)
                .is_empty()
        );
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
            page_url: std::cell::RefCell::new("vmux://history/".to_string()),
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
}
