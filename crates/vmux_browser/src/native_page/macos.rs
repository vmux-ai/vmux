use std::collections::HashMap;
use std::rc::Rc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::{BinHostEmitEvent, BinIpcEventRawSender, ZoomLevel};
use bevy_cef_core::prelude::{
    BinIpcEventRaw, Browsers, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};
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
        app.add_systems(
            Update,
            (
                open_native_pages,
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
}

impl HostedPages {
    fn get(&self, page: Entity) -> Option<&HostedPage> {
        self.0.get(&page)
    }

    fn layout(&self) -> Option<Entity> {
        for (entity, hosted) in self.0.iter() {
            if hosted.placement == Placement::Layout {
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

fn place_native_pages(
    hosted: Option<NonSendMut<HostedPages>>,
    frames: Res<PaneFrames>,
    window: Query<&Window, With<PrimaryWindow>>,
    pages: Query<(), With<HostsPage>>,
    capturing: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    settings: Res<AppSettings>,
) {
    let Some(mut hosted) = hosted else {
        return;
    };
    hosted.0.retain(|entity, _| pages.contains(*entity));
    let window = window.single().ok();
    let capturing = !capturing.is_empty();
    let all_corners = frames.all_corners();
    for (entity, page) in hosted.0.iter() {
        let Some(bounds) = page.placement.bounds(*entity, window, &frames) else {
            page.surface.set_visible(false);
            continue;
        };
        page.surface.set_bounds(bounds);
        page.surface
            .set_corner_radius(settings.layout.radius as f64, all_corners);
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

        Ok(Self {
            bin_ipc: bin_ipc.0.clone(),
            requester,
            waker: PageWaker::of(world.get_resource::<EventLoopProxyWrapper>()),
        })
    }

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

struct PageOutbox {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: String,
}

impl vmux_native::Outbox for PageOutbox {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
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
    use super::{Placement, SiblingOrder};

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
}
