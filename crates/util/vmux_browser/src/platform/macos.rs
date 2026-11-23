use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::rc::Rc;
use std::sync::{Arc, Condvar, LazyLock, Mutex};

use bevy::ecs::archetype::ArchetypeId;
use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::EntityRef;
use bevy::prelude::*;
use bevy::prelude::{
    App, Changed, ChildOf, Commands, Component, Entity, EntityEvent, Has, MessageReader,
    MessageWriter, NonSend, NonSendMut, On, Plugin, Query, Ref, Remove, Res, ResMut, Resource,
    Single, Window, With, Without,
};
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WINIT_WINDOWS, WinitUserEvent};
use bevy_cef::prelude::{
    BinHostEmitEvent, BinIpcEventRawSender, HostWindow, RequestReload, RequestReloadIgnoreCache,
    UiInput, ZoomLevel,
};
use bevy_cef_core::prelude::{
    BinIpcEpoch, BinIpcEventRaw, BinIpcNavigationRevision, BinIpcSurfaceGeneration, Browsers,
    CefRequest, CefResponse, Requester, Responser, asset_load_path_from_request_url,
};
use vmux_api::PageIcon;
use vmux_api::UiEventPermissions;
use vmux_ecs::page::HostsPage;
use vmux_ecs::page::PageReady;
use vmux_ecs::{PageMetadata, PageOpenSet};
use vmux_layout::LayoutCef;
use vmux_layout::overlay::{LayoutOverlayDismissRevision, LayoutOverlayOpened};
use vmux_page::{
    Appearance, AssetReply, Assets as RendererAssets, Embedding, LifecycleSink, LifecycleUpdate,
    Outbox as RendererOutbox, PageClaimDirty, PagePlacement, PageRegistration, SiblingOrder,
    Wake as RendererWake, WebView,
};
use vmux_setting::{AppSettings, ResolvedColorScheme, ResolvedScheme};
use vmux_ui::hooks::EventListenerError;

use super::native_focus::{
    FocusAttempt, FocusSet, HostedSurface, HostedSurfaceGeneration, NativeFocusPlugin,
    ResponderCompleted, ResponderRequested,
};

use crate::host::LayoutPointerCapture;
use crate::present::{AllCorners, FocusRing, PaneFrame, WindowFrameQueries};
use vmux_layout::presentation::Layer as PresentationLayer;

use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct MacosBrowserPlugin;

impl Plugin for MacosBrowserPlugin {
    fn build(&self, app: &mut App) {
        let metadata = PageMetadataMailbox::default();
        let lifecycle = PageLifecycleMailbox::default();
        let startup = Mutex::new(Some((metadata, lifecycle)));
        app.configure_sets(
            PostUpdate,
            vmux_ecs::UiStateFlushSet
                .after(crate::BrowserSystemSet::SyncWindowedFrames)
                .after(vmux_layout::projection::LayoutUiProjectionSet::Publish),
        )
        .configure_sets(
            PostUpdate,
            crate::BrowserSystemSet::HostFocusApplied
                .after(vmux_layout::presentation::NativePresentationSet::Finalize),
        )
        .add_message::<vmux_layout::presentation::NativeSurfaceReused>()
        .add_message::<vmux_ecs::overlay::PopupDismissRequest>()
        .add_systems(Update, dismiss_popups)
        .add_observer(stop_loading)
        .add_message::<vmux_layout::presentation::NativeSelectionCommitted>()
        .add_plugins((
            NativeFocusPlugin,
            SurfacePresentationPlugin,
            PageLifecyclePlugin,
        ))
        .init_non_send::<PageRenderer>()
        .init_resource::<OpenPageDropdown>()
        .init_resource::<OpenScratch>()
        .add_systems(Startup, move |mut commands: Commands| {
            let (metadata, lifecycle) = startup
                .lock()
                .unwrap()
                .take()
                .expect("page metadata runtime can only start once");
            commands.spawn((
                Name::new("Page metadata"),
                PageMetadataSender(metadata.clone()),
                PageMetadataReceiver(metadata),
                PageLifecycleSender(lifecycle.clone()),
                PageLifecycleReceiver(lifecycle),
            ));
        })
        .add_systems(First, accept_wakes)
        .add_systems(
            Update,
            (
                discover_page_claims.after(PageOpenSet::HandleKnownPages),
                retire_released_surfaces,
                open_wanted_surfaces,
                bevy::ecs::schedule::ApplyDeferred,
                sync_surface_interaction,
                apply_metadata.after(PageLifecycleSet),
                dismiss_page_dropdowns,
                mark_handoff_painted,
                sync_appearance,
                sync_scale,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            render.after(vmux_layout::presentation::NativePresentationSet::Finalize),
        )
        .add_systems(PostUpdate, focus.in_set(FocusSet::Apply))
        .add_observer(forward_host_emit)
        .add_observer(reload)
        .add_observer(hard_reload)
        .add_observer(mark_candidate_added::<HostsPage>)
        .add_observer(mark_candidate_removed::<HostsPage>)
        .add_observer(mark_candidate_added::<LayoutCef>)
        .add_observer(mark_candidate_removed::<LayoutCef>);
    }
}

struct SurfacePresentationPlugin;

impl Plugin for SurfacePresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_layout::presentation::NativeSurfaceReused>()
            .add_message::<vmux_layout::presentation::NativeSelectionCommitted>()
            .init_resource::<SurfaceSettleWake>()
            .add_message::<SurfaceWakeRequested>()
            .add_systems(
                PostUpdate,
                wake.after(vmux_layout::presentation::NativePresentationSet::Place)
                    .after(vmux_layout::presentation::NativePresentationSet::Finalize),
            )
            .add_observer(confirm)
            .add_observer(retire_history::<HostedSurface>)
            .add_observer(retire_history::<HostedSurfaceGeneration>)
            .add_systems(
                PostUpdate,
                (
                    initialize,
                    collect,
                    place,
                    bevy::ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(vmux_layout::presentation::NativePresentationSet::Place)
                    .after(vmux_ecs::UiStateFlushSet)
                    .after(crate::BrowserSystemSet::SyncWindowedFrames),
            )
            .add_systems(
                PostUpdate,
                (initialize, place, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(vmux_layout::presentation::NativePresentationSet::Finalize)
                    .run_if(on_message::<vmux_layout::presentation::NativeSelectionCommitted>),
            );
    }
}

fn reload(event: On<RequestReload>, renderer: NonSend<PageRenderer>, mut commands: Commands) {
    if let Some(surface) = renderer.active.get(&event.webview)
        && surface.reload()
    {
        commands
            .entity(event.webview)
            .insert(vmux_layout::Loading)
            .remove::<PageReady>();
    }
}

fn hard_reload(
    event: On<RequestReloadIgnoreCache>,
    renderer: NonSend<PageRenderer>,
    mut commands: Commands,
) {
    if let Some(surface) = renderer.active.get(&event.webview)
        && surface.reload_ignore_cache()
    {
        commands
            .entity(event.webview)
            .insert(vmux_layout::Loading)
            .remove::<PageReady>();
    }
}

#[derive(Clone, Copy)]
struct PageClaim {
    registration: PageRegistration,
    archetype: ArchetypeId,
    instance_present: bool,
    host_window: Option<Entity>,
}

#[derive(Default)]
struct PageDiscovery {
    registrations: Vec<PageRegistration>,
    claims: bevy::ecs::entity::EntityHashMap<PageClaim>,
    candidates: bevy::ecs::entity::EntityHashSet,
    wanted: Vec<(Entity, PageRegistration, (u8, usize), bool)>,
    released: Vec<Entity>,
    retry: bevy::ecs::entity::EntityHashSet,
}

#[derive(Resource, Default)]
struct OpenScratch {
    initialized: bool,
    reported_waiting: bool,
    generations: u64,
    discovery: PageDiscovery,
    page_candidates: Vec<Entity>,
    tree: Vec<Entity>,
    seen: bevy::ecs::entity::EntityHashSet,
    forced: bevy::ecs::entity::EntityHashSet,
    current_active: bevy::ecs::entity::EntityHashSet,
    active_waiting: bevy::ecs::entity::EntityHashSet,
}

impl OpenScratch {
    fn report_waiting(&mut self, reason: &str) {
        if !self.reported_waiting {
            self.reported_waiting = true;
            info!("browser_platform: waiting, {reason}");
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PageClaims<'w, 's> {
    entities: Query<'w, 's, EntityRef<'static>, Without<bevy::ecs::resource::IsResource>>,
}

impl<'w, 's> std::ops::Deref for PageClaims<'w, 's> {
    type Target = Query<'w, 's, EntityRef<'static>, Without<bevy::ecs::resource::IsResource>>;
    fn deref(&self) -> &Self::Target {
        &self.entities
    }
}

impl PageClaims<'_, '_> {
    fn claim(
        &self,
        discovery: &mut PageDiscovery,
        candidate: EntityRef<'_>,
        force: bool,
    ) -> Option<(PageRegistration, (u8, usize), bool)> {
        let entity = candidate.id();
        let archetype = candidate.archetype().id();
        let metadata_changed = candidate
            .get_ref::<PageMetadata>()
            .is_some_and(|metadata| metadata.is_changed());
        let handoff_changed = candidate
            .get_ref::<vmux_layout::hosted_page::SurfaceHandoff>()
            .is_some_and(|handoff| handoff.is_changed());
        if !force
            && !metadata_changed
            && !handoff_changed
            && let Some(claim) = discovery.claims.get(&entity)
            && claim.archetype == archetype
            && !claim.registration.instance_changed(candidate)
        {
            return None;
        }
        let host_window = self.host_window(entity);
        let previous = discovery.claims.get(&entity).copied();
        let mut selected = None;
        for registration in &discovery.registrations {
            let Some(priority) = Self::registration_claim_priority(candidate, *registration) else {
                continue;
            };
            if selected
                .as_ref()
                .is_none_or(|(_, current)| *current < priority)
            {
                selected = Some((*registration, priority));
            }
        }
        match selected {
            Some((registration, priority)) => {
                let instance_present = registration.instance_present(candidate);
                let changed = previous.is_none_or(|claim| {
                    !claim.registration.same_as(registration)
                        || claim.instance_present != instance_present
                        || claim.host_window != host_window
                });
                discovery.claims.insert(
                    entity,
                    PageClaim {
                        registration,
                        archetype,
                        instance_present,
                        host_window,
                    },
                );
                return Some((registration, priority, changed));
            }
            None => {
                discovery.claims.remove(&entity);
            }
        }
        None
    }

    fn host_window(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            let entity = self.entities.get(current).ok()?;
            if let Some(host) = entity.get::<HostWindow>() {
                return Some(host.0);
            }
            current = entity.get::<ChildOf>().map(Relationship::get)?;
        }
    }

    fn registration_claim_priority(
        entity: EntityRef<'_>,
        registration: PageRegistration,
    ) -> Option<(u8, usize)> {
        let page = registration.page();
        match registration.placement() {
            PagePlacement::Layout => {
                if !entity.contains::<LayoutCef>() {
                    return None;
                }
            }
            PagePlacement::Pane | PagePlacement::Modal => {
                if !entity.contains::<HostsPage>() || entity.contains::<LayoutCef>() {
                    return None;
                }
                let metadata = entity.get::<PageMetadata>()?;
                if !registration.answers_for(entity, &metadata.url) {
                    return None;
                }
            }
        }
        let Some(url) = entity
            .get::<PageMetadata>()
            .map(|metadata| metadata.url.as_str())
        else {
            return Some((3, page.url.len()));
        };
        if url == page.url {
            return Some((2, page.url.len()));
        }
        if page.answers_for(url) {
            return Some((1, page.url.len()));
        }
        Some((3, page.url.len()))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct OpenChanges<'w, 's> {
    registrations: Query<'w, 's, Ref<'static, PageRegistration>>,
    host_tree: Query<'w, 's, Entity, Or<(Changed<HostWindow>, Changed<ChildOf>)>>,
    presentations: Query<'w, 's, (), Changed<PresentationLayer>>,
    dirty: Query<'w, 's, Entity, With<PageClaimDirty>>,
    removed_registrations: RemovedComponents<'w, 's, PageRegistration>,
    removed_host_windows: RemovedComponents<'w, 's, HostWindow>,
    removed_parents: RemovedComponents<'w, 's, ChildOf>,
    removed_hosted: RemovedComponents<'w, 's, HostsPage>,
    removed_layout: RemovedComponents<'w, 's, LayoutCef>,
}

impl PageDiscovery {
    fn refresh_registrations(&mut self, registrations: impl Iterator<Item = PageRegistration>) {
        self.registrations.clear();
        self.registrations.extend(registrations);
        self.claims.clear();
    }

    fn retry(&mut self, entity: Entity) {
        self.claims.remove(&entity);
        self.retry.insert(entity);
    }

    fn retry_wanted(&mut self) {
        for (entity, _, _, _) in &self.wanted {
            self.claims.remove(entity);
            self.retry.insert(*entity);
        }
    }
}

fn mark_candidate_added<C: Component>(event: On<Add, C>, mut commands: Commands) {
    commands.entity(event.entity).try_insert(PageClaimDirty);
}

fn mark_candidate_removed<C: Component>(event: On<Remove, C>, mut commands: Commands) {
    commands.entity(event.entity).try_insert(PageClaimDirty);
}

struct PageRenderer {
    active: HashMap<Entity, WebView>,
    replacements: HashMap<Entity, WebView>,
    retired: Vec<RetiredPage>,
    retirement_wake_pending: Arc<AtomicBool>,
}

fn dismiss_popups(
    mut requests: MessageReader<vmux_ecs::overlay::PopupDismissRequest>,
    renderer: NonSend<PageRenderer>,
) {
    if requests.read().count() == 0 {
        return;
    }
    for surface in renderer.active.values() {
        surface.evaluate_script("window.dispatchEvent(new Event('vmux-deactivated'));");
    }
}

fn stop_loading(
    request: On<crate::command::StopLoadingRequested>,
    renderer: NonSend<PageRenderer>,
    mut commands: Commands,
) {
    if let Some(surface) = renderer.active.get(&request.event().webview) {
        surface.evaluate_script("window.stop();");
        commands
            .entity(request.event().webview)
            .remove::<vmux_layout::Loading>();
    }
}

impl Default for PageRenderer {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            replacements: HashMap::new(),
            retired: Vec::new(),
            retirement_wake_pending: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct RetiredPage {
    _surface: WebView,
    expires_at: std::time::Instant,
}

impl RetiredPage {
    const LIFETIME: std::time::Duration = std::time::Duration::from_millis(500);

    fn new(surface: WebView, now: std::time::Instant) -> Self {
        surface.set_visible(false);
        Self {
            _surface: surface,
            expires_at: now + Self::LIFETIME,
        }
    }
}

impl PageRenderer {
    fn appearance(scheme: ResolvedScheme) -> Appearance {
        match scheme {
            ResolvedScheme::Light => Appearance::Light,
            ResolvedScheme::Dark => Appearance::Dark,
        }
    }

    fn retire(
        &mut self,
        entity: Entity,
        now: std::time::Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) -> bool {
        let mut retired = false;
        if let Some(surface) = self.replacements.remove(&entity) {
            self.hold_retired(entity, surface, now);
            retired = true;
        }
        if let Some(surface) = self.active.remove(&entity) {
            self.hold_retired(entity, surface, now);
            retired = true;
        }
        if retired {
            self.schedule_retirement_wake(now, proxy);
        }
        retired
    }

    fn install(
        &mut self,
        entity: Entity,
        surface: WebView,
        now: std::time::Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let Some(current) = self.active.insert(entity, surface) else {
            return;
        };
        current.set_interactive(false);
        if let std::collections::hash_map::Entry::Vacant(e) = self.replacements.entry(entity) {
            e.insert(current);
        } else {
            self.hold_retired(entity, current, now);
            self.schedule_retirement_wake(now, proxy);
        }
    }

    fn commit_replacement(
        &mut self,
        entity: Entity,
        now: std::time::Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let Some(surface) = self.replacements.remove(&entity) else {
            return;
        };
        self.hold_retired(entity, surface, now);
        self.schedule_retirement_wake(now, proxy);
    }

    fn hold_retired(&mut self, _entity: Entity, surface: WebView, now: std::time::Instant) {
        self.retired.push(RetiredPage::new(surface, now));
    }

    fn collect_retired(&mut self, now: std::time::Instant, proxy: Option<&EventLoopProxyWrapper>) {
        self.retired.retain(|surface| surface.expires_at > now);
        self.schedule_retirement_wake(now, proxy);
    }

    fn schedule_retirement_wake(
        &self,
        now: std::time::Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let Some(proxy) = proxy else {
            return;
        };
        let Some(deadline) = self.retired.iter().map(|surface| surface.expires_at).min() else {
            return;
        };
        if self
            .retirement_wake_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let delay = deadline.saturating_duration_since(now);
        let pending = self.retirement_wake_pending.clone();
        let proxy: EventLoopProxy<WinitUserEvent> = (**proxy).clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            pending.store(false, Ordering::Release);
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SurfacePresentation {
    Hidden,
    Visible {
        frame: PaneFrame,
        radius: f64,
        opacity: f32,
        all_corners: bool,
        ring: FocusRing,
        order: Option<SiblingOrder>,
    },
}

impl SurfacePresentation {
    fn settle(self, surface: &WebView, retain_visible: bool, pending: bool) -> bool {
        match self {
            Self::Hidden => {
                surface.set_visible(false);
                true
            }
            Self::Visible { frame, opacity, .. } => {
                let bounds = wry::Rect {
                    position: wry::dpi::LogicalPosition::new(frame.left, frame.top).into(),
                    size: wry::dpi::LogicalSize::new(frame.width, frame.height).into(),
                };
                if !frame.matches(surface.bounds()) {
                    surface.set_bounds(bounds);
                }
                let settled = frame.matches(surface.bounds());
                surface.set_visible(settled || retain_visible);
                settled && (!pending || surface.presentation_settled(opacity))
            }
        }
    }

    fn apply(self, surface: &WebView, retain_visible: bool, pending: bool) -> bool {
        match self {
            Self::Hidden => {
                surface.set_visible(false);
                true
            }
            Self::Visible {
                frame,
                radius,
                opacity,
                all_corners,
                ring,
                order,
            } => {
                surface.set_corner_radius(radius, all_corners);
                surface.set_focus_ring(ring.width as f64, ring.rgb);
                surface.set_opacity(opacity);
                if let Some(order) = order {
                    surface.order_among_siblings(order);
                }
                Self::Visible {
                    frame,
                    radius,
                    opacity,
                    all_corners,
                    ring,
                    order,
                }
                .settle(surface, retain_visible, pending)
            }
        }
    }
}

impl PaneFrame {
    fn matches(self, bounds: Option<wry::Rect>) -> bool {
        let Some(bounds) = bounds else {
            return false;
        };
        let wry::dpi::Position::Logical(position) = bounds.position else {
            return false;
        };
        let wry::dpi::Size::Logical(size) = bounds.size else {
            return false;
        };
        Self::near(position.x, self.left)
            && Self::near(position.y, self.top)
            && Self::near(size.width, self.width)
            && Self::near(size.height, self.height)
    }

    fn near(actual: f64, expected: f32) -> bool {
        (actual - expected as f64).abs() <= 1.0
    }
}

#[derive(Clone, Copy)]
struct AppliedSurfacePresentation {
    generation: u64,
    presentation: SurfacePresentation,
    settled: bool,
}

#[derive(Clone, Copy)]
struct CachedNativeFrame {
    generation: u64,
    navigation_revision: u64,
    frame: PaneFrame,
}

impl CachedNativeFrame {
    fn matches_frame(self, generation: u64, navigation_revision: u64, frame: PaneFrame) -> bool {
        self.generation == generation
            && self.navigation_revision == navigation_revision
            && self.frame == frame
    }

    fn matches(
        self,
        generation: u64,
        navigation_revision: u64,
        presentation: SurfacePresentation,
        bounds: Option<wry::Rect>,
    ) -> bool {
        let SurfacePresentation::Visible { frame, opacity, .. } = presentation else {
            return false;
        };
        self.matches_frame(generation, navigation_revision, frame)
            && opacity == 1.0
            && frame.matches(bounds)
    }
}

#[derive(Component, Default)]
struct SurfaceHistory {
    applied: Option<AppliedSurfacePresentation>,
    cached: Option<CachedNativeFrame>,
    attempt: Option<SurfaceSettleAttempt>,
}

#[derive(Clone, Copy)]
struct SurfaceSettleAttempt {
    generation: u64,
    presentation: SurfacePresentation,
    started: std::time::Instant,
}

impl SurfaceHistory {
    fn remembered(
        &self,
        generation: u64,
        navigation_revision: u64,
        presentation: SurfacePresentation,
    ) -> bool {
        match presentation {
            SurfacePresentation::Visible {
                frame,
                opacity: 1.0,
                ..
            } => self
                .cached
                .is_some_and(|cached| cached.matches_frame(generation, navigation_revision, frame)),
            _ => true,
        }
    }

    fn record(&mut self, generation: u64, presentation: SurfacePresentation, settled: bool) {
        self.applied = Some(AppliedSurfacePresentation {
            generation,
            presentation,
            settled,
        });
    }

    fn matches(&self, generation: u64, presentation: SurfacePresentation) -> bool {
        self.applied.is_some_and(|current| {
            current.generation == generation && current.presentation == presentation
        })
    }

    fn remember(
        &mut self,
        generation: u64,
        navigation_revision: u64,
        presentation: SurfacePresentation,
        bounds: Option<wry::Rect>,
        native_settled: bool,
    ) {
        let SurfacePresentation::Visible { frame, opacity, .. } = presentation else {
            return;
        };
        if opacity == 1.0
            && self
                .cached
                .is_some_and(|cached| cached.matches_frame(generation, navigation_revision, frame))
        {
            return;
        }
        if opacity != 1.0 || !frame.matches(bounds) || !native_settled {
            return;
        }
        self.cached = Some(CachedNativeFrame {
            generation,
            navigation_revision,
            frame,
        });
    }

    fn retry(
        &mut self,
        generation: u64,
        presentation: SurfacePresentation,
        now: std::time::Instant,
    ) -> bool {
        match self.attempt.as_mut() {
            Some(attempt)
                if attempt.generation == generation && attempt.presentation == presentation =>
            {
                now.saturating_duration_since(attempt.started) < SURFACE_SETTLE_TIMEOUT
            }
            _ => {
                self.attempt = Some(SurfaceSettleAttempt {
                    generation,
                    presentation,
                    started: now,
                });
                true
            }
        }
    }
}

const SURFACE_SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
const SURFACE_SETTLE_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

#[derive(Message)]
struct SurfaceWakeRequested {
    deadline: std::time::Instant,
}

fn wake(
    mut requests: MessageReader<SurfaceWakeRequested>,
    service: Res<SurfaceSettleWake>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    for request in requests.read() {
        service.schedule(request.deadline, proxy.as_deref());
    }
}

#[derive(Resource, Default)]
struct SurfaceSettleWake {
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    shared: Arc<(Mutex<SurfaceSettleWakeState>, Condvar)>,
}

#[derive(Default)]
struct SurfaceSettleWakeState {
    deadline: Option<std::time::Instant>,
    proxy: Option<EventLoopProxy<WinitUserEvent>>,
    stopping: bool,
}

impl SurfaceSettleWake {
    fn schedule(&self, deadline: std::time::Instant, proxy: Option<&EventLoopProxyWrapper>) {
        let Some(proxy) = proxy else {
            return;
        };
        {
            let (state, wake) = &*self.shared;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.stopping || state.deadline.is_some_and(|current| current <= deadline) {
                return;
            }
            state.deadline = Some(deadline);
            state.proxy = Some((**proxy).clone());
            wake.notify_one();
        }
        self.start();
    }

    fn start(&self) {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if worker.is_none() {
            let shared = self.shared.clone();
            *worker = Some(std::thread::spawn(move || Self::run(shared)));
        }
    }

    fn run(shared: Arc<(Mutex<SurfaceSettleWakeState>, Condvar)>) {
        let (state, wake) = &*shared;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if state.stopping {
                return;
            }
            let Some(deadline) = state.deadline else {
                state = wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                continue;
            };
            let delay = deadline.saturating_duration_since(std::time::Instant::now());
            if !delay.is_zero() {
                let (next, timeout) = wake
                    .wait_timeout(state, delay)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
                if !timeout.timed_out() || state.deadline != Some(deadline) {
                    continue;
                }
            }
            state.deadline = None;
            let proxy = state.proxy.clone();
            drop(state);
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
            state = shared
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

impl Drop for SurfaceSettleWake {
    fn drop(&mut self) {
        {
            let (state, wake) = &*self.shared;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.stopping = true;
            state.deadline = None;
            state.proxy = None;
            wake.notify_one();
        }
        let worker = self
            .worker
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}

struct PageMetadataUpdate {
    webview: Entity,
    generation: u64,
    navigation_revision: u64,
    ipc_epoch: u64,
    page_url: String,
    title: Option<String>,
    favicon: Option<String>,
}

#[derive(Clone, Default)]
struct PageMetadataMailbox {
    pending: Arc<Mutex<HashMap<(Entity, u64, u64, u64), PageMetadataUpdate>>>,
}

impl PageMetadataMailbox {
    fn send(&self, update: PageMetadataUpdate) {
        let key = (
            update.webview,
            update.generation,
            update.navigation_revision,
            update.ipc_epoch,
        );
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(current) = pending.get_mut(&key) {
            current.page_url = update.page_url;
            if update.title.is_some() {
                current.title = update.title;
            }
            if update.favicon.is_some() {
                current.favicon = update.favicon;
            }
            return;
        }
        pending.insert(key, update);
    }

    fn drain_into(&self, updates: &mut Vec<PageMetadataUpdate>) {
        updates.clear();
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        updates.extend(pending.drain().map(|(_, update)| update));
    }
}

#[derive(Component, Clone)]
struct PageMetadataSender(PageMetadataMailbox);

#[derive(Component)]
struct PageMetadataReceiver(PageMetadataMailbox);

#[derive(Message)]
struct PageLifecycleEnvelope {
    at: std::time::Instant,
    webview: Entity,
    generation: u64,
    navigation_revision: u64,
    ipc_epoch: u64,
    update: LifecycleUpdate,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PageLifecycleKey {
    webview: Entity,
    generation: u64,
    navigation_revision: u64,
    ipc_epoch: u64,
}

impl From<&PageLifecycleEnvelope> for PageLifecycleKey {
    fn from(envelope: &PageLifecycleEnvelope) -> Self {
        Self {
            webview: envelope.webview,
            generation: envelope.generation,
            navigation_revision: envelope.navigation_revision,
            ipc_epoch: envelope.ipc_epoch,
        }
    }
}

#[derive(Default)]
struct PageLifecycleBatch {
    document_observed_at: Option<std::time::Instant>,
    mounts: u64,
    ready: Option<u64>,
    remounts: u64,
    frame_pending: Option<bool>,
    frame_delivered: Option<u64>,
    frames_applied: u64,
    frame_applied: Option<u64>,
    frame_painted: Option<u64>,
    frame_failed: Option<u64>,
    ui_events: u64,
    host_events: u64,
    listeners_cleared: bool,
    listeners_added: usize,
    measurements_cleared: bool,
    measurement_delta: i64,
}

#[derive(Message)]
struct PageLifecycleBatchEnvelope {
    key: PageLifecycleKey,
    batch: PageLifecycleBatch,
}

impl PageLifecycleBatch {
    fn record(&mut self, update: LifecycleUpdate, at: std::time::Instant) {
        if matches!(
            update,
            LifecycleUpdate::Mounted | LifecycleUpdate::Remounted | LifecycleUpdate::Ready(_)
        ) {
            self.document_observed_at = Some(
                self.document_observed_at
                    .map_or(at, |previous| previous.max(at)),
            );
        }
        match update {
            LifecycleUpdate::Mounted => self.mounts = self.mounts.saturating_add(1),
            LifecycleUpdate::Ready(revision) => self.ready = Some(revision),
            LifecycleUpdate::Remounted => self.remounts = self.remounts.saturating_add(1),
            LifecycleUpdate::Navigated(_) => {}
            LifecycleUpdate::FramePending => self.frame_pending = Some(true),
            LifecycleUpdate::FrameDelivered(revision) => {
                self.frame_pending = Some(false);
                self.frame_delivered = Some(self.frame_delivered.unwrap_or_default().max(revision));
            }
            LifecycleUpdate::FrameApplied(revision) => {
                self.frames_applied = self.frames_applied.saturating_add(1);
                self.frame_applied = Some(self.frame_applied.unwrap_or_default().max(revision));
            }
            LifecycleUpdate::FramePainted(revision) => {
                self.frame_painted = Some(self.frame_painted.unwrap_or_default().max(revision));
            }
            LifecycleUpdate::FrameFailed(revision) => {
                self.frame_pending = Some(false);
                self.frame_failed = Some(self.frame_failed.unwrap_or_default().max(revision));
            }
            LifecycleUpdate::UiEventHandled => {
                self.ui_events = self.ui_events.saturating_add(1);
            }
            LifecycleUpdate::HostEventDelivered => {
                self.host_events = self.host_events.saturating_add(1);
            }
            LifecycleUpdate::ListenerRegistered => {
                self.listeners_added = self.listeners_added.saturating_add(1);
            }
            LifecycleUpdate::ListenersCleared => {
                self.listeners_cleared = true;
                self.listeners_added = 0;
            }
            LifecycleUpdate::MeasurementStarted => {
                self.measurement_delta = self.measurement_delta.saturating_add(1);
            }
            LifecycleUpdate::MeasurementFinished => {
                self.measurement_delta = self.measurement_delta.saturating_sub(1);
            }
            LifecycleUpdate::MeasurementsCleared => {
                self.measurements_cleared = true;
                self.measurement_delta = 0;
            }
            LifecycleUpdate::DropdownOpened
            | LifecycleUpdate::PageInteracted
            | LifecycleUpdate::HistoryRequested(_) => {}
        }
    }
}

#[derive(Default)]
struct PageLifecyclePending {
    batches: HashMap<PageLifecycleKey, PageLifecycleBatch>,
    semantic: VecDeque<PageLifecycleEnvelope>,
}

#[derive(Clone, Default)]
struct PageLifecycleMailbox {
    pending: Arc<Mutex<PageLifecyclePending>>,
}

impl PageLifecycleMailbox {
    fn send(&self, envelope: PageLifecycleEnvelope) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if matches!(
            envelope.update,
            LifecycleUpdate::Navigated(_)
                | LifecycleUpdate::DropdownOpened
                | LifecycleUpdate::PageInteracted
                | LifecycleUpdate::HistoryRequested(_)
        ) {
            pending.semantic.push_back(envelope);
            return;
        }
        pending
            .batches
            .entry(PageLifecycleKey::from(&envelope))
            .or_default()
            .record(envelope.update, envelope.at);
    }

    fn drain_into(
        &self,
        batches: &mut Vec<(PageLifecycleKey, PageLifecycleBatch)>,
        semantic: &mut Vec<PageLifecycleEnvelope>,
    ) {
        batches.clear();
        semantic.clear();
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        batches.extend(pending.batches.drain());
        semantic.extend(pending.semantic.drain(..));
    }
}

#[derive(Component, Clone)]
struct PageLifecycleSender(PageLifecycleMailbox);

#[derive(Component)]
struct PageLifecycleReceiver(PageLifecycleMailbox);

#[derive(Resource, Default)]
struct OpenPageDropdown(Option<Entity>);

impl OpenPageDropdown {
    fn opened(&mut self, surface: Entity) -> Option<Entity> {
        self.0
            .replace(surface)
            .filter(|previous| *previous != surface)
    }

    fn interacted(&mut self, surface: Entity) -> Option<Entity> {
        let previous = self.0?;
        if previous == surface {
            return None;
        }
        self.0 = None;
        Some(previous)
    }
}

const CLOSE_PAGE_DROPDOWNS: &str = "for (const popup of document.querySelectorAll('[data-vmux-dropdown-popup]:popover-open')) popup.hidePopover();";

fn apply_metadata(
    receiver: Single<&PageMetadataReceiver>,
    mut pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcNavigationRevision,
        &BinIpcEpoch,
        &mut PageMetadata,
    )>,
) {
    let mut updates = Vec::new();
    receiver.0.drain_into(&mut updates);
    for update in updates.drain(..) {
        let Ok((generation, navigation, epoch, mut metadata)) = pages.get_mut(update.webview)
        else {
            continue;
        };
        if generation.0 != update.generation
            || navigation.0 != update.navigation_revision
            || epoch.0 != update.ipc_epoch
        {
            continue;
        }
        if metadata.url != update.page_url {
            continue;
        }
        if let Some(title) = update.title
            && !title.is_empty()
        {
            metadata.title = title;
        }
        if let Some(favicon) = update.favicon
            && !favicon.is_empty()
            && !metadata.url.starts_with("vmux://")
        {
            metadata.icon = PageIcon::favicon(favicon);
        }
    }
}

struct PageLifecyclePlugin;

impl Plugin for PageLifecyclePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::native_bridge::NativeHistoryPlugin>() {
            app.add_plugins(crate::native_bridge::NativeHistoryPlugin);
        }
        app.init_resource::<OpenPageDropdown>()
            .add_message::<PageLifecycleEnvelope>()
            .add_message::<PageLifecycleBatchEnvelope>()
            .add_systems(
                Update,
                (
                    receive_lifecycle,
                    invalidate_documents,
                    apply_semantic,
                    apply_batches,
                )
                    .chain()
                    .in_set(PageLifecycleSet)
                    .after(sync_surface_interaction),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PageLifecycleSet;

fn receive_lifecycle(
    receiver: Single<&PageLifecycleReceiver>,
    mut batch_events: MessageWriter<PageLifecycleBatchEnvelope>,
    mut semantic_events: MessageWriter<PageLifecycleEnvelope>,
) {
    let mut batches = Vec::new();
    let mut semantic = Vec::new();
    receiver.0.drain_into(&mut batches, &mut semantic);
    for (key, batch) in batches {
        batch_events.write(PageLifecycleBatchEnvelope { key, batch });
    }
    for envelope in semantic {
        semantic_events.write(envelope);
    }
}

fn invalidate_documents(
    mut batches: MessageReader<PageLifecycleBatchEnvelope>,
    mut semantic: MessageReader<PageLifecycleEnvelope>,
    pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcEpoch,
        &BinIpcNavigationRevision,
    )>,
    mut history: crate::native_bridge::HistoryClaims,
) {
    let semantic: Vec<_> = semantic.read().collect();
    for envelope in batches.read() {
        let key = &envelope.key;
        let batch = &envelope.batch;
        let Some(at) = batch.document_observed_at else {
            continue;
        };
        let Ok((generation, epoch, navigation)) = pages.get(key.webview) else {
            continue;
        };
        if generation.0 != key.generation || epoch.0 != key.ipc_epoch {
            continue;
        }
        let mut latest_navigation = navigation.0;
        for envelope in &semantic {
            if envelope.webview != key.webview
                || envelope.generation != key.generation
                || envelope.ipc_epoch != key.ipc_epoch
            {
                continue;
            }
            if let LifecycleUpdate::Navigated(revision) = envelope.update
                && revision == envelope.navigation_revision
                && revision >= latest_navigation
            {
                latest_navigation = revision;
            }
        }
        if latest_navigation == key.navigation_revision {
            history.invalidate(key.webview, at);
        }
    }
}

fn apply_semantic(
    mut semantic: MessageReader<PageLifecycleEnvelope>,
    renderer: Option<NonSend<PageRenderer>>,
    mut pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcEpoch,
        &mut BinIpcNavigationRevision,
        &mut crate::HostedPageLifecycle,
    )>,
    blocked: Query<(), With<vmux_layout::hosted_page::SurfaceBlocked>>,
    source_hosts: Query<&HostWindow, Without<LayoutCef>>,
    mut layouts: Query<(Entity, &HostWindow, &mut LayoutOverlayDismissRevision), With<LayoutCef>>,
    mut open_dropdown: ResMut<OpenPageDropdown>,
    mut history: crate::native_bridge::HistoryClaims,
    mut commands: Commands,
) {
    for envelope in semantic.read() {
        let Ok((generation, epoch, mut navigation, mut page)) = pages.get_mut(envelope.webview)
        else {
            continue;
        };
        if generation.0 != envelope.generation || epoch.0 != envelope.ipc_epoch {
            continue;
        }
        match envelope.update {
            LifecycleUpdate::Navigated(revision) => {
                if revision != envelope.navigation_revision || revision < navigation.0 {
                    continue;
                }
                navigation.0 = revision;
                history.invalidate(envelope.webview, envelope.at);
                page.navigation_revision = page.navigation_revision.max(revision);
                page.hydration_frame_count = None;
                continue;
            }
            LifecycleUpdate::DropdownOpened
            | LifecycleUpdate::PageInteracted
            | LifecycleUpdate::HistoryRequested(_) => {}
            _ => continue,
        }
        if navigation.0 != envelope.navigation_revision || blocked.contains(envelope.webview) {
            continue;
        }
        let previous = match envelope.update {
            LifecycleUpdate::HistoryRequested(direction) => {
                let operation = match direction {
                    vmux_page::HistoryDirection::Back => crate::NavigationOperation::Back,
                    vmux_page::HistoryDirection::Forward => crate::NavigationOperation::Forward,
                };
                if !history.claim(envelope.webview, operation, envelope.at) {
                    continue;
                }
                commands.write_message(crate::NavigationRequest::surface(
                    envelope.webview,
                    operation,
                ));
                continue;
            }
            LifecycleUpdate::DropdownOpened => open_dropdown.opened(envelope.webview),
            LifecycleUpdate::PageInteracted => open_dropdown.interacted(envelope.webview),
            _ => continue,
        };
        if let Some(previous) = previous
            && let Some(surface) = renderer
                .as_ref()
                .and_then(|renderer| renderer.active.get(&previous))
        {
            surface.evaluate_script(CLOSE_PAGE_DROPDOWNS);
        }
        let Ok(source_host) = source_hosts.get(envelope.webview) else {
            continue;
        };
        let Some((layout, _, mut revision)) = layouts
            .iter_mut()
            .find(|(_, host, _)| host.0 == source_host.0)
        else {
            continue;
        };
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                layout,
                &revision.next_effect(),
            ),
        );
    }
}

fn apply_batches(
    mut batches: MessageReader<PageLifecycleBatchEnvelope>,
    mut pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcEpoch,
        &BinIpcNavigationRevision,
        &mut crate::HostedPageLifecycle,
    )>,
    handoffs: Query<(), With<vmux_layout::hosted_page::SurfaceHandoff>>,
    mut commands: Commands,
) {
    for envelope in batches.read() {
        let key = &envelope.key;
        let batch = &envelope.batch;
        let Ok((generation, epoch, navigation, mut page)) = pages.get_mut(key.webview) else {
            continue;
        };
        if generation.0 != key.generation || navigation.0 != key.navigation_revision {
            continue;
        }
        let current_epoch = epoch.0 == key.ipc_epoch;
        if !current_epoch {
            continue;
        }
        page.navigation_revision = page.navigation_revision.max(navigation.0);
        if batch.mounts > 0 {
            page.mount_revision = page.mount_revision.wrapping_add(batch.mounts);
            page.navigation_revision = page.navigation_revision.max(1);
        }
        if batch.remounts > 0 {
            page.remount_revision = page.remount_revision.wrapping_add(batch.remounts);
            page.hydration_frame_count = None;
        }
        if batch.ready == Some(navigation.0) {
            commands
                .entity(key.webview)
                .remove::<vmux_layout::Loading>();
            commands.trigger(UiInput {
                webview: key.webview,
                payload: PageReady {},
            });
        }
        if let Some(pending) = batch.frame_pending {
            page.frame_pending = pending;
        }
        if let Some(revision) = batch.frame_delivered {
            page.frame_revision_delivered = page.frame_revision_delivered.max(revision);
        }
        if batch.frames_applied > 0 {
            page.frames_applied = page
                .frames_applied
                .wrapping_add(batch.frames_applied)
                .max(1);
        }
        if let Some(revision) = batch.frame_applied {
            page.frame_revision_applied = page.frame_revision_applied.max(revision);
        }
        if batch.remounts > 0 {
            page.frame_revision_painted = 0;
        }
        if let Some(revision) = batch.frame_painted {
            page.frame_revision_painted = page.frame_revision_painted.max(revision);
        }
        if batch.frame_failed == Some(navigation.0) && handoffs.contains(key.webview) {
            commands
                .entity(key.webview)
                .try_insert(vmux_layout::hosted_page::SurfaceHandoffFailed);
        }
        page.ui_events = page.ui_events.wrapping_add(batch.ui_events);
        page.host_events = page.host_events.wrapping_add(batch.host_events);
        if batch.host_events > 0 && page.hydration_frame_count.is_none() {
            page.hydration_frame_count = Some(page.frames_applied);
        }
        if batch.listeners_cleared {
            page.listeners = batch.listeners_added;
        } else {
            page.listeners = page.listeners.saturating_add(batch.listeners_added);
        }
        let measurements = if batch.measurements_cleared {
            0
        } else {
            page.measurements
        };
        page.measurements = if batch.measurement_delta >= 0 {
            measurements.saturating_add(batch.measurement_delta as usize)
        } else {
            measurements.saturating_sub(batch.measurement_delta.unsigned_abs() as usize)
        };
    }
}

fn dismiss_page_dropdowns(
    mut opened: MessageReader<LayoutOverlayOpened>,
    layouts: Query<&HostWindow, With<LayoutCef>>,
    surfaces: Query<&HostedSurface>,
    renderer: Option<NonSend<PageRenderer>>,
    mut open_dropdown: ResMut<OpenPageDropdown>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    for event in opened.read() {
        let Ok(host) = layouts.get(event.layout) else {
            continue;
        };
        let Some(owner) = open_dropdown.0 else {
            continue;
        };
        let Ok(surface_state) = surfaces.get(owner) else {
            open_dropdown.0 = None;
            continue;
        };
        if surface_state.window != host.0 {
            continue;
        }
        if let Some(surface) = renderer.active.get(&owner) {
            surface.evaluate_script(CLOSE_PAGE_DROPDOWNS);
        }
        open_dropdown.0 = None;
    }
}

fn mark_handoff_painted(
    pending: Query<
        (
            Entity,
            &crate::HostedPageLifecycle,
            &vmux_layout::hosted_page::SurfaceHandoff,
            &ChildOf,
        ),
        (
            With<vmux_layout::hosted_page::SurfaceHandoff>,
            Without<vmux_layout::hosted_page::SurfacePaintReady>,
        ),
    >,
    navigations: Query<&vmux_ecs::PageNavigation>,
    mut commands: Commands,
) {
    for (entity, lifecycle, handoff, parent) in &pending {
        let Ok(navigation) = navigations.get(parent.parent()) else {
            continue;
        };
        if !handoff.matches(navigation) {
            continue;
        }
        if !handoff.can_finish(
            navigation,
            lifecycle.painted_after(handoff.baseline_navigation_revision()),
        ) {
            continue;
        }
        commands
            .entity(entity)
            .insert(vmux_layout::hosted_page::SurfacePaintReady);
    }
}

fn sync_surface_interaction(
    blocked: Query<
        Entity,
        Or<(
            Added<vmux_layout::hosted_page::SurfaceBlocked>,
            Added<vmux_ecs::PageTransition>,
        )>,
    >,
    restore: Query<
        Entity,
        (
            With<vmux_layout::hosted_page::SurfaceInteractionRestore>,
            Without<vmux_layout::hosted_page::SurfaceBlocked>,
            Without<vmux_ecs::PageTransition>,
        ),
    >,
    renderer: Option<NonSend<PageRenderer>>,
    mut commands: Commands,
) {
    let Some(renderer) = renderer else {
        return;
    };
    for entity in &blocked {
        if let Some(surface) = renderer.active.get(&entity) {
            let epoch = surface.set_interactive(false);
            commands.entity(entity).try_insert(BinIpcEpoch(epoch));
        }
    }
    for entity in &restore {
        let mut entity_commands = commands.entity(entity);
        if let Some(surface) = renderer.active.get(&entity) {
            let epoch = surface.set_interactive(true);
            entity_commands.try_insert(BinIpcEpoch(epoch));
        }
        entity_commands.try_remove::<vmux_layout::hosted_page::SurfaceInteractionRestore>();
        commands.queue_silenced(bevy::ecs::system::command::trigger(
            vmux_ecs::page::PageActivated::new(entity),
        ));
    }
}

fn discover_page_claims(
    mut state: ParamSet<(
        PageClaims,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
        Query<
            EntityRef,
            (
                Or<(With<HostsPage>, With<LayoutCef>)>,
                Without<bevy::ecs::resource::IsResource>,
            ),
        >,
        Query<
            Entity,
            Or<(
                Changed<PageMetadata>,
                Changed<vmux_layout::hosted_page::SurfaceHandoff>,
            )>,
        >,
    )>,
    mut changes: OpenChanges,
    frame_queries: WindowFrameQueries,
    time: Res<Time>,
    mut scratch: ResMut<OpenScratch>,
    mut commands: Commands,
) {
    let full_scan = collect_page_candidates(&mut state, &mut changes, &mut scratch, &mut commands);
    if scratch.page_candidates.is_empty() {
        return;
    }
    let OpenScratch {
        discovery,
        page_candidates,
        seen,
        forced,
        current_active,
        active_waiting,
        ..
    } = &mut *scratch;
    {
        let entities = state.p0();
        for &entity in page_candidates.iter() {
            if seen.contains(&entity) {
                continue;
            }
            let Ok(candidate) = entities.get(entity) else {
                discovery.claims.remove(&entity);
                discovery.candidates.remove(&entity);
                active_waiting.remove(&entity);
                continue;
            };
            if !candidate.contains::<HostsPage>() && !candidate.contains::<LayoutCef>() {
                discovery.claims.remove(&entity);
                discovery.candidates.remove(&entity);
                active_waiting.remove(&entity);
                continue;
            }
            seen.insert(entity);
            discovery.candidates.insert(entity);
            if frame_queries.page_is_active(entity) {
                current_active.insert(entity);
            }
            if let Some((registration, priority, changed)) =
                entities.claim(discovery, candidate, forced.contains(&entity))
            {
                discovery
                    .wanted
                    .push((entity, registration, priority, changed));
            } else if !discovery.claims.contains_key(&entity)
                && candidate.contains::<HostedSurface>()
            {
                discovery.released.push(entity);
            }
        }
    }
    if full_scan {
        discovery.claims.retain(|entity, _| seen.contains(entity));
        discovery.candidates.clone_from(seen);
    }
    let now = time.elapsed();
    {
        let mut handoffs = state.p1();
        for (entity, _, _, _) in &discovery.wanted {
            let active = current_active.contains(entity);
            let newly_active = active && !active_waiting.contains(entity);
            let Ok(mut handoff) = handoffs.get_mut(*entity) else {
                continue;
            };
            if !handoff.navigation_started() && (newly_active || !active && handoff.timed_out(now))
            {
                handoff.defer(now);
            }
        }
    }
    if full_scan {
        std::mem::swap(active_waiting, current_active);
    } else {
        for entity in seen.iter().copied() {
            if current_active.contains(&entity) {
                active_waiting.insert(entity);
            } else {
                active_waiting.remove(&entity);
            }
        }
    }
}

fn collect_page_candidates(
    state: &mut ParamSet<(
        PageClaims,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
        Query<
            EntityRef,
            (
                Or<(With<HostsPage>, With<LayoutCef>)>,
                Without<bevy::ecs::resource::IsResource>,
            ),
        >,
        Query<
            Entity,
            Or<(
                Changed<PageMetadata>,
                Changed<vmux_layout::hosted_page::SurfaceHandoff>,
            )>,
        >,
    )>,
    changes: &mut OpenChanges,
    scratch: &mut OpenScratch,
    commands: &mut Commands,
) -> bool {
    let OpenScratch {
        initialized,
        discovery,
        page_candidates,
        tree,
        seen,
        forced,
        current_active,
        ..
    } = scratch;
    let registration_removed = changes.removed_registrations.read().count() > 0;
    let registrations_changed = !*initialized
        || changes
            .registrations
            .iter()
            .any(|registration| registration.is_changed())
        || registration_removed;
    if registrations_changed {
        discovery.refresh_registrations(changes.registrations.iter().map(|value| *value));
    }
    let presentation_changed = changes.presentations.iter().next().is_some();
    let host_tree_removed = changes.removed_host_windows.read().count() > 0
        || changes.removed_parents.read().count() > 0;
    let full_scan =
        !*initialized || registrations_changed || presentation_changed || host_tree_removed;
    *initialized = true;
    discovery.wanted.clear();
    discovery.released.clear();
    page_candidates.clear();
    tree.clear();
    seen.clear();
    forced.clear();
    current_active.clear();
    tree.extend(changes.host_tree.iter());
    if !tree.is_empty() {
        let entities = state.p0();
        while let Some(entity) = tree.pop() {
            if discovery.candidates.contains(&entity) {
                page_candidates.push(entity);
                forced.insert(entity);
            }
            let Ok(entity) = entities.get(entity) else {
                continue;
            };
            if let Some(children) = entity.get::<Children>() {
                tree.extend(children.iter());
            }
        }
    }
    if full_scan {
        let candidates = state.p2();
        page_candidates.extend(candidates.iter().map(|candidate| candidate.id()));
        if presentation_changed || host_tree_removed {
            forced.extend(page_candidates.iter().copied());
        }
    } else {
        page_candidates.extend(discovery.retry.drain());
        page_candidates.extend(state.p3().iter());
        page_candidates.extend(changes.dirty.iter());
    }
    page_candidates.extend(changes.removed_hosted.read());
    page_candidates.extend(changes.removed_layout.read());
    for entity in changes.dirty.iter() {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.try_remove::<PageClaimDirty>();
        }
    }
    full_scan
}

fn retire_released_surfaces(
    mut renderer: NonSendMut<PageRenderer>,
    mut browsers: NonSendMut<Browsers>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut scratch: ResMut<OpenScratch>,
    mut commands: Commands,
) {
    let released = std::mem::take(&mut scratch.discovery.released);
    if released.is_empty() {
        return;
    }
    let now = std::time::Instant::now();
    for entity in &released {
        renderer.retire(*entity, now, proxy.as_deref());
        browsers.unset_externally_hosted(*entity);
    }
    for entity in released {
        scratch.active_waiting.remove(&entity);
        commands.entity(entity).try_remove::<(
            HostedSurface,
            HostedSurfaceGeneration,
            crate::HostedPageLifecycle,
            BinIpcEpoch,
            BinIpcSurfaceGeneration,
            BinIpcNavigationRevision,
            PageReady,
            vmux_layout::hosted_page::SurfacePaintReady,
            vmux_layout::hosted_page::SurfacePresentationReady,
            vmux_layout::hosted_page::SurfaceTransitionReady,
        )>();
    }
}

fn open_wanted_surfaces(
    mut state: ParamSet<(
        PageClaims,
        NonSendMut<PageRenderer>,
        NonSendMut<Browsers>,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
    )>,
    requester: Option<Res<Requester>>,
    bin_ipc: Option<Res<BinIpcEventRawSender>>,
    frame_queries: WindowFrameQueries,
    primary_windows: Query<Entity, With<PrimaryWindow>>,
    metadata_sender: Query<&PageMetadataSender>,
    lifecycle_sender: Query<&PageLifecycleSender>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    appearance: Single<&ResolvedColorScheme>,
    time: Res<Time>,
    mut scratch: ResMut<OpenScratch>,
    mut commands: Commands,
) {
    if scratch.discovery.wanted.is_empty() {
        return;
    }
    let primary_window = primary_windows.iter().next();
    let embedder = match load_page_embedder(
        requester.as_deref(),
        bin_ipc.as_deref(),
        &metadata_sender,
        &lifecycle_sender,
        proxy.as_deref(),
    ) {
        Ok(embedder) => embedder,
        Err(reason) => {
            scratch.discovery.retry_wanted();
            scratch.report_waiting(reason);
            return;
        }
    };
    let appearance = PageRenderer::appearance(appearance.0);
    let mut wanted = std::mem::take(&mut scratch.discovery.wanted);
    for (entity, registration, _, claim_changed) in wanted.drain(..) {
        let placement = registration.placement();
        let Some((
            window_entity,
            current,
            reactivating,
            baseline_navigation_revision,
            instance_changed,
            url,
            blocked,
            is_warm_spare,
        )) = inspect_wanted_surface(
            &state.p0(),
            entity,
            registration,
            primary_window,
            claim_changed,
            &mut scratch,
        )
        else {
            continue;
        };
        let has_surface = state.p1().active.contains_key(&entity);
        if !has_surface
            && placement == PagePlacement::Pane
            && !frame_queries.page_is_active(entity)
            && !is_warm_spare
        {
            continue;
        }
        if reuse_hosted_surface(
            &mut state,
            entity,
            registration,
            window_entity,
            current.as_ref(),
            has_surface,
            reactivating,
            instance_changed,
            baseline_navigation_revision,
            &url,
            blocked,
            time.elapsed(),
            &mut commands,
        ) {
            continue;
        }
        build_hosted_surface(
            &mut state,
            &mut scratch,
            &embedder,
            entity,
            registration,
            window_entity,
            current.is_some(),
            url,
            blocked,
            appearance,
            proxy.as_deref(),
            time.elapsed(),
            &mut commands,
        );
    }
    scratch.discovery.wanted = wanted;
}

fn inspect_wanted_surface(
    entities: &PageClaims,
    entity: Entity,
    registration: PageRegistration,
    primary_window: Option<Entity>,
    claim_changed: bool,
    scratch: &mut OpenScratch,
) -> Option<(
    Entity,
    Option<HostedSurface>,
    bool,
    u64,
    bool,
    String,
    bool,
    bool,
)> {
    let page = registration.page();
    let Some(window_entity) = entities.host_window(entity).or(primary_window) else {
        scratch.discovery.retry(entity);
        scratch.report_waiting("page has no host window entity");
        return None;
    };
    let candidate = entities.get(entity).ok()?;
    let reactivating = candidate
        .get::<vmux_layout::hosted_page::SurfaceHandoff>()
        .is_some_and(|handoff| !handoff.navigation_started());
    let baseline_navigation_revision = candidate
        .get::<crate::HostedPageLifecycle>()
        .map(|lifecycle| lifecycle.navigation_revision)
        .unwrap_or_default();
    let instance_changed = claim_changed || registration.instance_changed(candidate);
    let url = candidate
        .get::<PageMetadata>()
        .map(|metadata| metadata.url.clone())
        .unwrap_or_else(|| page.url.to_string());
    let blocked = candidate.contains::<vmux_layout::hosted_page::SurfaceBlocked>()
        || candidate.contains::<vmux_ecs::PageTransition>();
    let is_warm_spare = candidate.contains::<vmux_layout::warm_page::WarmPageSpare>();
    Some((
        window_entity,
        candidate.get::<HostedSurface>().cloned(),
        reactivating,
        baseline_navigation_revision,
        instance_changed,
        url,
        blocked,
        is_warm_spare,
    ))
}

#[allow(clippy::too_many_arguments)]
fn reuse_hosted_surface(
    state: &mut ParamSet<(
        PageClaims,
        NonSendMut<PageRenderer>,
        NonSendMut<Browsers>,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
    )>,
    entity: Entity,
    registration: PageRegistration,
    window_entity: Entity,
    current: Option<&HostedSurface>,
    has_surface: bool,
    reactivating: bool,
    instance_changed: bool,
    baseline_navigation_revision: u64,
    url: &str,
    blocked: bool,
    now: std::time::Duration,
    commands: &mut Commands,
) -> bool {
    let page = registration.page();
    let placement = registration.placement();
    let Some(current) = current.filter(|current| current.window == window_entity && has_surface)
    else {
        return false;
    };
    let same_page = std::ptr::eq(current.page, page);
    let same_document = !same_page
        && current.page.transparent == page.transparent
        && current.page.document_url() == page.document_url();
    if !same_page && !same_document {
        return false;
    }
    let route_changed = current.url != url;
    let placement_changed = current.placement != placement;
    let fragment_only =
        same_page && route_changed && current.url.split('#').next() == url.split('#').next();
    if fragment_only && !reactivating && !instance_changed {
        state
            .p1()
            .active
            .get(&entity)
            .expect("the page was just found")
            .set_url(url);
    } else if same_document || route_changed || reactivating || instance_changed {
        let instance = {
            let entities = state.p0();
            let Ok(candidate) = entities.get(entity) else {
                return true;
            };
            registration.instance(candidate)
        };
        let remounted = {
            let renderer = state.p1();
            let surface = renderer
                .active
                .get(&entity)
                .expect("the page was just found");
            let ipc_epoch = if blocked {
                surface.set_interactive(false)
            } else {
                surface.ipc_epoch()
            };
            let remounted = surface.navigate(page, url, instance);
            commands.entity(entity).insert((
                BinIpcNavigationRevision(surface.navigation_revision()),
                BinIpcEpoch(ipc_epoch),
            ));
            remounted
        };
        if remounted {
            commands.entity(entity).remove::<PageReady>();
        }
        if same_page {
            info!("browser_platform: updated {entity:?} within {}", page.url);
        } else {
            info!("browser_platform: navigated {entity:?} to {}", page.url);
        }
        if reactivating && let Ok(mut handoff) = state.p3().get_mut(entity) {
            handoff.begin_navigation(baseline_navigation_revision, now);
        }
    }
    if same_document || route_changed || placement_changed || instance_changed {
        commands.entity(entity).insert(HostedSurface {
            placement,
            page,
            window: window_entity,
            url: url.to_string(),
        });
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn build_hosted_surface(
    state: &mut ParamSet<(
        PageClaims,
        NonSendMut<PageRenderer>,
        NonSendMut<Browsers>,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
    )>,
    scratch: &mut OpenScratch,
    embedder: &PageEmbedder,
    entity: Entity,
    registration: PageRegistration,
    window_entity: Entity,
    replacing: bool,
    url: String,
    blocked: bool,
    appearance: Appearance,
    proxy: Option<&EventLoopProxyWrapper>,
    now: std::time::Duration,
    commands: &mut Commands,
) {
    let page = registration.page();
    let placement = registration.placement();
    let bounds = wry::Rect {
        position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
        size: wry::dpi::LogicalSize::new(1.0, 1.0).into(),
    };
    let instance = {
        let entities = state.p0();
        let Ok(candidate) = entities.get(entity) else {
            return;
        };
        registration.instance(candidate)
    };
    scratch.generations = scratch.generations.wrapping_add(1).max(1);
    let generation = scratch.generations;
    let built = WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let window = winit_windows.get_window(window_entity)?;
        Some(WebView::build(
            page,
            &**window,
            bounds,
            embedder.embed(entity, &url, generation),
            instance,
            appearance,
            !blocked,
        ))
    });
    match built {
        None => {
            scratch.discovery.retry(entity);
            scratch.report_waiting("primary window has no winit window yet");
        }
        Some(Ok(surface)) => {
            if replacing {
                commands.entity(entity).remove::<(
                    HostedSurface,
                    HostedSurfaceGeneration,
                    crate::HostedPageLifecycle,
                    BinIpcEpoch,
                    BinIpcSurfaceGeneration,
                    BinIpcNavigationRevision,
                    PageReady,
                    vmux_layout::hosted_page::SurfacePaintReady,
                    vmux_layout::hosted_page::SurfacePresentationReady,
                    vmux_layout::hosted_page::SurfaceTransitionReady,
                )>();
            }
            let ipc_epoch = if blocked {
                surface.set_interactive(false)
            } else {
                surface.ipc_epoch()
            };
            let navigation_revision = surface.navigation_revision();
            if Placement::paints_in_front(placement) {
                surface.raise_above_layers();
            }
            state.p2().set_externally_hosted(entity);
            info!(
                "browser_platform: hosting {} for {entity:?} as {placement:?}, {appearance:?}",
                page.url
            );
            state
                .p1()
                .install(entity, surface, std::time::Instant::now(), proxy);
            commands.entity(entity).insert((
                HostedSurface {
                    placement,
                    page,
                    window: window_entity,
                    url,
                },
                HostedSurfaceGeneration(generation),
                crate::HostedPageLifecycle::default(),
                BinIpcEpoch(ipc_epoch),
                BinIpcSurfaceGeneration(generation),
                BinIpcNavigationRevision(navigation_revision),
            ));
            if let Ok(mut handoff) = state.p3().get_mut(entity) {
                handoff.begin_navigation(0, now);
            }
        }
        Some(Err(error)) => {
            error!(
                "browser_platform: build_as_child failed for {}: {error}",
                page.url
            );
            commands
                .entity(entity)
                .insert(vmux_layout::hosted_page::SurfaceHandoffFailed);
        }
    }
}

#[derive(EntityEvent)]
struct PresentationCompleted {
    #[event_target]
    surface: Entity,
    generation: u64,
    presentation: SurfacePresentation,
    pending: bool,
    reusable: bool,
    cached: bool,
}

type HostedPresentations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut SurfaceHistory,
        Ref<'static, HostedSurface>,
        &'static HostedSurfaceGeneration,
        Option<&'static vmux_layout::hosted_page::SurfaceHandoff>,
        Option<&'static vmux_layout::page_slot::SurfaceOpacity>,
        Has<vmux_layout::hosted_page::SurfacePaintReady>,
        Has<PageReady>,
        Has<vmux_ecs::PagePresented>,
        Has<vmux_layout::hosted_page::SurfaceBlocked>,
        Has<vmux_ecs::PageTransition>,
        Has<vmux_layout::PendingWebviewReveal>,
    ),
    With<HostsPage>,
>;

#[derive(SystemParam)]
struct Placement<'w, 's> {
    ready_spares: Query<'w, 's, (), (With<vmux_layout::warm_page::WarmPageSpare>, With<PageReady>)>,
    frames: Query<'w, 's, &'static PaneFrame>,
    rings: Query<'w, 's, &'static FocusRing>,
    corners: Query<'w, 's, &'static AllCorners>,
    windows: Query<'w, 's, &'static Window>,
    capturing: Query<'w, 's, &'static HostWindow, (With<LayoutCef>, LayoutPointerCapture)>,
    settings: Res<'w, AppSettings>,
    frame_queries: WindowFrameQueries<'w, 's>,
}

impl Placement<'_, '_> {
    fn presentation(
        &self,
        entity: Entity,
        page: &HostedSurface,
        handoff: Option<&vmux_layout::hosted_page::SurfaceHandoff>,
        opacity: Option<&vmux_layout::page_slot::SurfaceOpacity>,
        paint_ready: bool,
    ) -> SurfacePresentation {
        if self.ready_spares.contains(entity) {
            return SurfacePresentation::Hidden;
        }
        let window = self.windows.get(page.window).ok();
        let Some(frame) = self.frame(page.placement, entity, window) else {
            return SurfacePresentation::Hidden;
        };
        let all_corners = self.corners.get(entity).is_ok_and(|corners| corners.0);
        let ring = self.rings.get(entity).copied().unwrap_or_default();
        let capturing = self.capturing.iter().any(|host| host.0 == page.window);
        let pending = handoff.is_some_and(|handoff| !handoff.retain_visible());
        let opacity = if handoff.is_some_and(|handoff| handoff.requires_paint(paint_ready)) {
            0.0
        } else {
            match opacity {
                Some(opacity) => opacity.value(),
                None if pending => 0.0,
                None => 1.0,
            }
        };
        SurfacePresentation::Visible {
            frame,
            radius: self.settings.layout.radius as f64,
            opacity,
            all_corners,
            ring,
            order: Self::pointer_order(
                page.placement,
                capturing,
                pending,
                self.frame_queries.presentation(entity),
            ),
        }
    }

    fn paints_in_front(placement: PagePlacement) -> bool {
        matches!(placement, PagePlacement::Layout | PagePlacement::Modal)
    }

    fn pointer_order(
        placement: PagePlacement,
        capturing: bool,
        pending: bool,
        presentation: PresentationLayer,
    ) -> Option<SiblingOrder> {
        if capturing && placement == PagePlacement::Pane {
            return Some(SiblingOrder::Back);
        }
        match presentation {
            PresentationLayer::Retained => return Some(SiblingOrder::Front),
            PresentationLayer::Incoming => return Some(SiblingOrder::Back),
            PresentationLayer::Current | PresentationLayer::Hidden => {}
        }
        if pending {
            return Some(SiblingOrder::Front);
        }
        match placement {
            PagePlacement::Layout if !capturing => Some(SiblingOrder::Back),
            PagePlacement::Layout | PagePlacement::Modal => Some(SiblingOrder::Front),
            PagePlacement::Pane => Some(SiblingOrder::Front),
        }
    }

    fn frame(
        &self,
        placement: PagePlacement,
        entity: Entity,
        window: Option<&Window>,
    ) -> Option<PaneFrame> {
        match placement {
            PagePlacement::Layout => {
                let window = window?;
                Some(PaneFrame {
                    left: 0.0,
                    top: 0.0,
                    width: window.width(),
                    height: window.height(),
                })
            }
            PagePlacement::Pane | PagePlacement::Modal => self.frames.get(entity).ok().copied(),
        }
    }
}

fn initialize(
    surfaces: Query<
        Entity,
        (
            With<HostedSurface>,
            With<HostedSurfaceGeneration>,
            Without<SurfaceHistory>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &surfaces {
        commands.entity(entity).insert(SurfaceHistory::default());
    }
}

fn retire_history<T: Component>(removed: On<Remove, T>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<SurfaceHistory>();
}

fn collect(
    renderer: Option<NonSendMut<PageRenderer>>,
    all_hosted: Query<(), With<HostsPage>>,
    histories: Query<Entity, With<SurfaceHistory>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(mut renderer) = renderer else {
        return;
    };
    let now = std::time::Instant::now();
    let stale = renderer
        .active
        .keys()
        .filter(|entity| !all_hosted.contains(**entity))
        .copied()
        .collect::<Vec<_>>();
    let held = renderer.active.len();
    for entity in stale {
        renderer.retire(entity, now, proxy.as_deref());
    }
    renderer.collect_retired(now, proxy.as_deref());
    for entity in &histories {
        if !renderer.active.contains_key(&entity) {
            commands.entity(entity).try_remove::<SurfaceHistory>();
        }
    }
    if renderer.active.len() != held
        && let Some(proxy) = proxy.as_ref()
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn place(
    renderer: Option<NonSend<PageRenderer>>,
    mut hosted: HostedPresentations,
    lookup: Placement,
    mut wakes: MessageWriter<SurfaceWakeRequested>,
    mut commands: Commands,
) {
    let Some(renderer) = renderer else {
        return;
    };
    let now = std::time::Instant::now();
    for (
        entity,
        mut history,
        page,
        generation,
        handoff,
        opacity,
        paint_ready,
        ready,
        presented,
        blocked,
        transitioning,
        revealing,
    ) in &mut hosted
    {
        let Some(surface) = renderer.active.get(&entity) else {
            continue;
        };
        let pending = handoff.is_some_and(|handoff| !handoff.retain_visible());
        let next = lookup.presentation(entity, &page, handoff, opacity, paint_ready);
        let reusable = ready
            && presented
            && handoff.is_none()
            && !blocked
            && !transitioning
            && !revealing
            && !renderer.replacements.contains_key(&entity);
        let cached = reusable
            && lookup.frame_queries.presentation(entity) == PresentationLayer::Incoming
            && history.cached.is_some_and(|cached| {
                cached.matches(
                    generation.0,
                    surface.navigation_revision(),
                    next,
                    surface.bounds(),
                )
            });
        let previous = history.applied;
        let same = !page.is_changed() && history.matches(generation.0, next);
        let native_settled = match next {
            SurfacePresentation::Hidden => true,
            SurfacePresentation::Visible { opacity, .. } => {
                !pending || surface.presentation_settled(opacity)
            }
        };
        let retain_visible = matches!(previous, Some(AppliedSurfacePresentation { generation: current, presentation: SurfacePresentation::Visible { .. }, .. }) if current == generation.0);
        let settled = if same && previous.is_some_and(|current| current.settled) && native_settled {
            true
        } else if same {
            next.settle(surface, retain_visible, pending)
        } else {
            next.apply(surface, retain_visible, pending)
        };
        history.record(generation.0, next, settled);
        if settled {
            history.attempt = None;
            commands.trigger(PresentationCompleted {
                surface: entity,
                generation: generation.0,
                presentation: next,
                pending,
                reusable,
                cached,
            });
            continue;
        }
        commands.entity(entity).try_remove::<(
            vmux_layout::hosted_page::SurfacePresentationReady,
            vmux_layout::hosted_page::SurfaceTransitionReady,
        )>();
        if history.retry(generation.0, next, now) {
            wakes.write(SurfaceWakeRequested {
                deadline: std::time::Instant::now() + SURFACE_SETTLE_RETRY_INTERVAL,
            });
            continue;
        }
        surface.set_visible(false);
        commands
            .entity(entity)
            .try_insert(vmux_layout::hosted_page::SurfaceHandoffFailed);
        history.record(generation.0, SurfacePresentation::Hidden, true);
        history.attempt = None;
    }
}

fn confirm(
    completed: On<PresentationCompleted>,
    mut surfaces: Query<
        (
            &HostedSurfaceGeneration,
            &mut SurfaceHistory,
            Option<&vmux_layout::hosted_page::SurfaceHandoff>,
            Has<vmux_layout::hosted_page::SurfacePaintReady>,
            Has<vmux_layout::hosted_page::SurfacePresentationReady>,
            Has<vmux_layout::hosted_page::SurfaceTransitionReady>,
        ),
        With<HostedSurface>,
    >,
    renderer: Option<NonSendMut<PageRenderer>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let entity = completed.surface;
    let Ok((generation, mut history, handoff, paint_ready, presentation_ready, transition_ready)) =
        surfaces.get_mut(entity)
    else {
        return;
    };
    let Some(applied) = history.applied else {
        return;
    };
    if generation.0 != completed.generation
        || applied.generation != generation.0
        || applied.presentation != completed.presentation
        || !applied.settled
    {
        return;
    }
    sync_presentation_readiness(
        applied.presentation,
        entity,
        completed.pending,
        presentation_ready,
        transition_ready,
        &mut commands,
    );
    let Some(mut renderer) = renderer else {
        return;
    };
    let Some(surface) = renderer.active.get(&entity) else {
        return;
    };
    if completed.cached && surface.presentation_settled(1.0) {
        commands.write_message(vmux_layout::presentation::NativeSurfaceReused { surface: entity });
    }
    if completed.reusable
        && !history.remembered(
            generation.0,
            surface.navigation_revision(),
            applied.presentation,
        )
    {
        history.remember(
            generation.0,
            surface.navigation_revision(),
            applied.presentation,
            surface.bounds(),
            surface.presentation_settled(1.0),
        );
    }
    if renderer.replacements.contains_key(&entity)
        && (handoff.is_none() || paint_ready)
        && matches!(applied.presentation, SurfacePresentation::Visible { .. })
    {
        renderer.commit_replacement(entity, std::time::Instant::now(), proxy.as_deref());
    }
}

fn sync_presentation_readiness(
    presentation: SurfacePresentation,
    entity: Entity,
    pending: bool,
    presentation_ready: bool,
    transition_ready: bool,
    commands: &mut Commands,
) {
    let presented = matches!(presentation, SurfacePresentation::Visible { .. });
    if presented && !presentation_ready {
        commands
            .entity(entity)
            .try_insert(vmux_layout::hosted_page::SurfacePresentationReady);
    } else if !presented && presentation_ready {
        commands
            .entity(entity)
            .try_remove::<vmux_layout::hosted_page::SurfacePresentationReady>();
    }
    let transitioned = pending
        && matches!(
            presentation,
            SurfacePresentation::Visible { opacity, .. } if opacity >= 1.0
        );
    if transitioned && !transition_ready {
        commands
            .entity(entity)
            .try_insert(vmux_layout::hosted_page::SurfaceTransitionReady);
    } else if !transitioned && transition_ready {
        commands
            .entity(entity)
            .try_remove::<vmux_layout::hosted_page::SurfaceTransitionReady>();
    }
}

fn render(
    renderer: Option<NonSend<PageRenderer>>,
    ready_spares: Query<(), (With<vmux_layout::warm_page::WarmPageSpare>, With<PageReady>)>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    let mut dirty = Vec::new();
    {
        let mut pending = PAGE_RENDER_DIRTY
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        dirty.extend(pending.drain());
    }
    for entity in dirty.drain(..) {
        let Some(surface) = renderer.active.get(&entity) else {
            continue;
        };
        surface.sync_interactive_state();
        if ready_spares.contains(entity) {
            continue;
        }
        surface.render();
    }
}

fn focus(
    mut requested: MessageReader<ResponderRequested>,
    attempts: Query<&FocusAttempt>,
    renderer: Option<NonSend<PageRenderer>>,
    mut completed: MessageWriter<ResponderCompleted>,
) {
    for request in requested.read() {
        let accepted = match (attempts.get(request.attempt), renderer.as_deref()) {
            (Ok(attempt), Some(renderer)) => match renderer.active.get(&attempt.surface) {
                Some(surface) => surface.take_first_responder(),
                None => false,
            },
            _ => false,
        };
        completed.write(ResponderCompleted {
            attempt: request.attempt,
            accepted,
        });
    }
}

fn forward_host_emit(
    host_emit: On<BinHostEmitEvent>,
    renderer: Option<NonSend<PageRenderer>>,
    hosted: Query<&HostedSurface>,
    authority: Query<(
        &BinIpcSurfaceGeneration,
        &BinIpcNavigationRevision,
        &BinIpcEpoch,
    )>,
    permissions: Query<&UiEventPermissions>,
    blocked: Query<(), With<vmux_layout::hosted_page::SurfaceBlocked>>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    let Ok(page) = hosted.get(host_emit.webview()) else {
        return;
    };
    let Some(surface) = renderer.active.get(&host_emit.webview()) else {
        return;
    };
    let Ok((surface_generation, navigation_revision, ipc_epoch)) =
        authority.get(host_emit.webview())
    else {
        return;
    };
    if !host_emit.matches_authority(surface_generation.0, navigation_revision.0, ipc_epoch.0) {
        return;
    }
    if blocked.contains(host_emit.webview()) {
        return;
    }
    if !UiEventPermissions::allows_page(permissions.iter(), page.page.url, host_emit.permission()) {
        warn!(
            "blocked binary host event {} for unexpected page URL {}",
            host_emit.id(),
            page.page.url
        );
        return;
    }
    if surface.deliver(host_emit.id(), host_emit.payload()) {
        host_emit.mark_delivered();
    }
}

fn sync_appearance(
    renderer: Option<NonSend<PageRenderer>>,
    appearance: Single<Ref<ResolvedColorScheme>>,
) {
    if !appearance.is_changed() {
        return;
    }
    let Some(renderer) = renderer else {
        return;
    };
    let appearance = PageRenderer::appearance(appearance.0);
    info!("browser_platform: colour scheme set to {appearance:?}");
    for surface in renderer.active.values() {
        surface.set_appearance(appearance);
    }
    for surface in renderer.replacements.values() {
        surface.set_appearance(appearance);
    }
}

fn sync_scale(
    renderer: Option<NonSend<PageRenderer>>,
    zoom: Query<(Entity, &ZoomLevel), Changed<ZoomLevel>>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    for (entity, level) in zoom.iter() {
        let scale = page_scale_of(level.0);
        if let Some(surface) = renderer.active.get(&entity) {
            surface.set_page_scale(scale);
        }
        if let Some(surface) = renderer.replacements.get(&entity) {
            surface.set_page_scale(scale);
        }
    }
}

fn page_scale_of(level: f64) -> f64 {
    1.2f64.powf(level)
}

#[derive(Clone)]
struct PageEmbedder {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    metadata: PageMetadataSender,
    lifecycle: PageLifecycleSender,
    requester: Requester,
    waker: PageWaker,
}

impl PageEmbedder {
    fn embed(&self, entity: Entity, url: &str, generation: u64) -> Embedding {
        let page_url = Rc::new(RefCell::new(url.to_string()));
        let navigation_revision = Rc::new(Cell::new(1));
        let ipc_epoch = Rc::new(Cell::new(1));
        Embedding {
            outbox: Rc::new(PageOutbox {
                bin_ipc: self.bin_ipc.clone(),
                webview: entity,
                generation,
                page_url: page_url.clone(),
                navigation_revision: navigation_revision.clone(),
                ipc_epoch: ipc_epoch.clone(),
                metadata: self.metadata.clone(),
                waker: self.waker.clone(),
            }),
            assets: Rc::new(PageAssets {
                requester: self.requester.clone(),
                waker: self.waker.clone(),
                page_url,
                simulator_frames: SimulatorFrameProxy::default(),
            }),
            waker: Rc::new(self.waker.for_entity(entity)),
            lifecycle: Rc::new(PageLifecycleOutbox {
                webview: entity,
                generation,
                navigation_revision: navigation_revision.clone(),
                ipc_epoch: ipc_epoch.clone(),
                sender: self.lifecycle.clone(),
                waker: self.waker.clone(),
            }),
            navigation_revision,
            ipc_epoch,
        }
    }
}

fn load_page_embedder(
    requester: Option<&Requester>,
    bin_ipc: Option<&BinIpcEventRawSender>,
    metadata: &Query<&PageMetadataSender>,
    lifecycle: &Query<&PageLifecycleSender>,
    proxy: Option<&EventLoopProxyWrapper>,
) -> Result<PageEmbedder, &'static str> {
    let Some(requester) = requester.cloned() else {
        return Err("no Requester resource, the CEF custom scheme plugin has not built yet");
    };
    let Some(bin_ipc) = bin_ipc.map(|sender| sender.0.clone()) else {
        return Err("no BinIpcEventRawSender resource, the cef ipc plugin has not built yet");
    };
    let metadata = metadata
        .single()
        .map_err(|_| "no page metadata sender")?
        .clone();
    let lifecycle = lifecycle
        .single()
        .map_err(|_| "no page lifecycle sender")?
        .clone();

    Ok(PageEmbedder {
        bin_ipc,
        metadata,
        lifecycle,
        requester,
        waker: PageWaker::from_proxy(proxy),
    })
}

struct PageLifecycleOutbox {
    webview: Entity,
    generation: u64,
    navigation_revision: Rc<Cell<u64>>,
    ipc_epoch: Rc<Cell<u64>>,
    sender: PageLifecycleSender,
    waker: PageWaker,
}

impl LifecycleSink for PageLifecycleOutbox {
    fn update(&self, update: LifecycleUpdate) {
        self.sender.0.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: self.webview,
            generation: self.generation,
            navigation_revision: self.navigation_revision.get(),
            ipc_epoch: self.ipc_epoch.get(),
            update,
        });
        RendererWake::wake(&self.waker);
    }
}

#[derive(Clone)]
struct PageWaker {
    state: Arc<PageWake>,
    task: std::task::Waker,
}

struct PageWake {
    proxy: Option<EventLoopProxy<WinitUserEvent>>,
    dirty: Option<Entity>,
}

impl PageWaker {
    fn from_proxy(proxy: Option<&EventLoopProxyWrapper>) -> Self {
        Self::new(proxy.map(|proxy| (*proxy).clone()), None)
    }

    fn for_entity(&self, entity: Entity) -> Self {
        Self::new(self.state.proxy.clone(), Some(entity))
    }

    fn new(proxy: Option<EventLoopProxy<WinitUserEvent>>, dirty: Option<Entity>) -> Self {
        let state = Arc::new(PageWake { proxy, dirty });
        Self {
            task: std::task::Waker::from(state.clone()),
            state,
        }
    }
}

static PAGE_WAKE_PENDING: AtomicBool = AtomicBool::new(false);
static PAGE_RENDER_DIRTY: LazyLock<Mutex<HashSet<Entity>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

impl RendererWake for PageWaker {
    fn wake(&self) {
        self.state.wake();
    }

    fn task_waker(&self) -> std::task::Waker {
        self.task.clone()
    }
}

impl PageWake {
    fn wake(&self) {
        if let Some(entity) = self.dirty {
            PAGE_RENDER_DIRTY
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(entity);
        }
        let Some(proxy) = self.proxy.as_ref() else {
            return;
        };
        if PAGE_WAKE_PENDING.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

impl std::task::Wake for PageWake {
    fn wake(self: Arc<Self>) {
        PageWake::wake(self.as_ref());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        PageWake::wake(self.as_ref());
    }
}

fn accept_wakes(_: bevy::ecs::system::NonSendMarker) {
    PAGE_WAKE_PENDING.store(false, Ordering::Release);
}

struct PageOutbox {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    generation: u64,
    page_url: Rc<RefCell<String>>,
    navigation_revision: Rc<Cell<u64>>,
    ipc_epoch: Rc<Cell<u64>>,
    metadata: PageMetadataSender,
    waker: PageWaker,
}

impl RendererOutbox for PageOutbox {
    fn send(
        &self,
        id: &str,
        bytes: &[u8],
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> Result<(), EventListenerError> {
        self.bin_ipc
            .send_blocking(BinIpcEventRaw {
                webview: self.webview,
                page_url: self.page_url.borrow().clone(),
                surface_generation: Some(self.generation),
                navigation_revision: Some(navigation_revision),
                ipc_epoch: Some(ipc_epoch),
                id: id.to_string(),
                payload: bytes.to_vec(),
            })
            .map_err(|_| EventListenerError::Unsupported)
    }

    fn set_page(&self, url: &str) {
        *self.page_url.borrow_mut() = url.to_string();
    }

    fn set_title(&self, title: &str) {
        self.metadata.0.send(PageMetadataUpdate {
            webview: self.webview,
            generation: self.generation,
            navigation_revision: self.navigation_revision.get(),
            ipc_epoch: self.ipc_epoch.get(),
            page_url: self.page_url.borrow().clone(),
            title: Some(title.to_string()),
            favicon: None,
        });
        RendererWake::wake(&self.waker);
    }

    fn set_favicon(&self, url: &str) {
        self.metadata.0.send(PageMetadataUpdate {
            webview: self.webview,
            generation: self.generation,
            navigation_revision: self.navigation_revision.get(),
            ipc_epoch: self.ipc_epoch.get(),
            page_url: self.page_url.borrow().clone(),
            title: None,
            favicon: Some(url.to_string()),
        });
        RendererWake::wake(&self.waker);
    }
}

struct PageAssets {
    requester: Requester,
    waker: PageWaker,
    page_url: Rc<RefCell<String>>,
    simulator_frames: SimulatorFrameProxy,
}

impl RendererAssets for PageAssets {
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
            error!("browser_platform: vmux:// url maps to no asset path, url={url}");
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
            error!("browser_platform: vmux:// request channel closed, uri={uri}");
            reply.fail("request channel closed");
            return;
        }
        RendererWake::wake(&self.waker);
        std::thread::spawn(move || match rx.recv_blocking() {
            Ok(response) => reply.respond(
                response.status_code as u16,
                &response.mime_type,
                response.data,
            ),
            Err(_) => {
                error!("browser_platform: vmux:// responder dropped, uri={uri}");
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

struct SimulatorFrameProxy {
    current_job: Arc<std::sync::atomic::AtomicU64>,
}

struct SimulatorFrameJob {
    id: u64,
    current: Arc<std::sync::atomic::AtomicU64>,
    request: SimulatorFrameRequest,
    reply: AssetReply,
}

struct SimulatorFrame {
    generation: u64,
    bytes: Vec<u8>,
}

impl SimulatorFrameProxy {
    fn fetch(&self, request: SimulatorFrameRequest, reply: AssetReply) {
        let previous = self
            .current_job
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.wrapping_add(1).max(1))
            })
            .unwrap_or_default();
        let id = previous.wrapping_add(1).max(1);
        let pending = Arc::new(Mutex::new(Some(SimulatorFrameJob {
            id,
            current: self.current_job.clone(),
            request,
            reply,
        })));
        let running = pending.clone();
        let spawned = std::thread::Builder::new()
            .name("vmux-simulator-frame-proxy".into())
            .spawn(move || {
                let Ok(mut pending) = running.lock() else {
                    return;
                };
                let Some(job) = pending.take() else {
                    return;
                };
                drop(pending);
                job.run();
            });
        if let Err(error) = spawned
            && let Ok(mut pending) = pending.lock()
            && let Some(job) = pending.take()
        {
            job.reply
                .fail(&format!("could not start simulator frame proxy: {error}"));
        }
    }
}

impl Default for SimulatorFrameProxy {
    fn default() -> Self {
        Self {
            current_job: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

impl SimulatorFrameJob {
    fn run(self) {
        if !self.is_current() {
            self.reply.fail("simulator frame request was superseded");
            return;
        }
        match self.request.fetch() {
            Ok(frame) => {
                if !self.is_current() {
                    self.reply.fail("simulator frame request was superseded");
                    return;
                }
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

    fn is_current(&self) -> bool {
        self.current.load(Ordering::Acquire) == self.id
    }
}

impl SimulatorFrameRequest {
    const PATH: &'static str = "/__simulator-frame";
    const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
    const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
    const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

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

    fn fetch(&self) -> Result<SimulatorFrame, String> {
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut socket = TcpStream::connect_timeout(&address.into(), Self::CONNECT_TIMEOUT)
            .map_err(|error| format!("simulator frame connection failed: {error}"))?;
        socket
            .set_nodelay(true)
            .map_err(|error| format!("simulator frame socket setup failed: {error}"))?;
        socket
            .set_read_timeout(Some(Self::READ_TIMEOUT))
            .map_err(|error| format!("simulator frame read timeout setup failed: {error}"))?;
        socket
            .set_write_timeout(Some(Self::WRITE_TIMEOUT))
            .map_err(|error| format!("simulator frame write timeout setup failed: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::{
        AppliedSurfacePresentation, CachedNativeFrame, HostedSurfaceGeneration, MacosBrowserPlugin,
        OpenPageDropdown, PageLifecycleEnvelope, PageLifecycleMailbox, PageLifecycleReceiver,
        PageMetadataMailbox, PageMetadataReceiver, PageMetadataSender, PageOutbox, Placement,
        SURFACE_SETTLE_TIMEOUT, SimulatorFrame, SimulatorFrameRequest, SurfaceHistory,
        SurfacePresentation, SurfaceSettleWake,
    };
    use bevy::ecs::schedule::Schedules;
    use bevy::prelude::{
        App, Commands, IntoScheduleConfigs, Local, MinimalPlugins, On, Res, ResMut, Resource,
        Update,
    };
    use bevy_cef::prelude::UiInput;
    use bevy_cef_core::prelude::{BinIpcEpoch, BinIpcNavigationRevision};
    use vmux_ecs::page::PageReady;
    use vmux_page::{LifecycleUpdate, PagePlacement, SiblingOrder};

    use crate::present::PaneFrame;
    use vmux_layout::presentation::Layer as PresentationLayer;

    #[test]
    fn cached_native_frame_rejects_a_different_surface_navigation_opacity_or_bounds() {
        let frame = PaneFrame {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let cached = CachedNativeFrame {
            generation: 4,
            navigation_revision: 7,
            frame,
        };
        let visible = SurfacePresentation::Visible {
            frame,
            radius: 0.0,
            opacity: 1.0,
            all_corners: false,
            ring: crate::present::FocusRing::default(),
            order: Some(SiblingOrder::Front),
        };
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(10.0, 20.0).into(),
            size: wry::dpi::LogicalSize::new(200.0, 100.0).into(),
        };

        assert!(cached.matches(4, 7, visible, Some(bounds)));
        assert!(!cached.matches(5, 7, visible, Some(bounds)));
        assert!(!cached.matches(4, 8, visible, Some(bounds)));
        assert!(!cached.matches(4, 7, SurfacePresentation::Hidden, Some(bounds)));
        assert!(!cached.matches(4, 7, visible, None));
        let transparent = SurfacePresentation::Visible {
            frame,
            radius: 0.0,
            opacity: 0.0,
            all_corners: false,
            ring: crate::present::FocusRing::default(),
            order: Some(SiblingOrder::Front),
        };
        assert!(!cached.matches(4, 7, transparent, Some(bounds)));
        let stale_bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(10.0, 20.0).into(),
            size: wry::dpi::LogicalSize::new(160.0, 100.0).into(),
        };
        assert!(!cached.matches(4, 7, visible, Some(stale_bounds)));
        let resized = SurfacePresentation::Visible {
            frame: PaneFrame {
                width: 160.0,
                ..frame
            },
            radius: 0.0,
            opacity: 1.0,
            all_corners: false,
            ring: crate::present::FocusRing::default(),
            order: Some(SiblingOrder::Front),
        };
        assert!(!cached.matches(4, 7, resized, Some(stale_bounds)));
    }

    #[derive(Default, Resource)]
    struct ReadyEvents(usize);

    #[derive(Resource)]
    struct LifecycleSender(PageLifecycleMailbox);

    fn record_ready(_: On<UiInput<PageReady>>, mut events: ResMut<ReadyEvents>) {
        events.0 += 1;
    }

    fn queue_ready_surface(
        mut commands: Commands,
        sender: Res<LifecycleSender>,
        mut queued: Local<bool>,
    ) {
        if *queued {
            return;
        }
        *queued = true;
        let page = commands
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(1),
                BinIpcNavigationRevision(1),
                crate::HostedPageLifecycle::default(),
            ))
            .id();
        sender.0.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
    }

    fn despawn_restoring_surface(
        surfaces: bevy::prelude::Query<
            bevy::prelude::Entity,
            bevy::prelude::With<vmux_layout::hosted_page::SurfaceInteractionRestore>,
        >,
        mut commands: Commands,
    ) {
        for surface in &surfaces {
            commands.entity(surface).despawn();
        }
    }

    #[test]
    fn interaction_restore_ignores_an_already_despawned_surface() {
        let mut app = App::new();
        app.init_non_send::<super::PageRenderer>().add_systems(
            Update,
            (despawn_restoring_surface, super::sync_surface_interaction).chain(),
        );
        let surface = app
            .world_mut()
            .spawn(vmux_layout::hosted_page::SurfaceInteractionRestore)
            .id();

        app.update();

        assert!(app.world().get_entity(surface).is_err());
    }

    #[test]
    fn update_schedule_accepts_page_renderer_accesses() {
        let mut app = App::new();
        app.add_plugins(MacosBrowserPlugin);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
    }

    #[test]
    fn pane_frame_rejects_a_surface_positioned_from_a_stale_parent_size() {
        let frame = PaneFrame {
            left: 0.0,
            top: 0.0,
            width: 1728.0,
            height: 1083.0,
        };
        let stale = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, -282.0).into(),
            size: wry::dpi::LogicalSize::new(1728.0, 1083.0).into(),
        };
        let settled = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(1728.0, 1083.0).into(),
        };

        assert!(!frame.matches(Some(stale)));
        assert!(frame.matches(Some(settled)));
    }

    #[test]
    fn surface_settle_deadlines_are_independent_and_restart_for_new_targets() {
        let mut first = SurfaceHistory::default();
        let mut second = SurfaceHistory::default();
        let started = std::time::Instant::now();
        let later = started + SURFACE_SETTLE_TIMEOUT;
        assert!(first.retry(1, SurfacePresentation::Hidden, started));
        assert!(second.retry(1, SurfacePresentation::Hidden, later));
        assert!(!first.retry(1, SurfacePresentation::Hidden, later));
        assert!(second.retry(1, SurfacePresentation::Hidden, later));
        assert!(first.retry(2, SurfacePresentation::Hidden, later));
    }

    #[test]
    fn surface_settle_worker_releases_ownership_on_teardown() {
        let wake = SurfaceSettleWake::default();
        wake.shared.0.lock().unwrap().deadline =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
        let lifetime = std::sync::Arc::downgrade(&wake.shared);
        wake.start();
        drop(wake);
        assert!(lifetime.upgrade().is_none());
    }

    struct HistoryFixture {
        app: App,
        surface: bevy::prelude::Entity,
    }

    impl HistoryFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, super::SurfacePresentationPlugin))
                .init_resource::<vmux_setting::AppSettings>();
            let surface = app
                .world_mut()
                .spawn((
                    super::HostedSurface {
                        placement: PagePlacement::Pane,
                        page: &const {
                            vmux_page::Page::pane("vmux://presentation-test", vmux_layout::ui::Page)
                        },
                        window: bevy::prelude::Entity::PLACEHOLDER,
                        url: "vmux://presentation-test".to_string(),
                    },
                    HostedSurfaceGeneration(1),
                ))
                .id();
            app.update();
            Self { app, surface }
        }

        fn retry(&mut self, now: std::time::Instant) -> bool {
            self.app
                .world_mut()
                .get_mut::<SurfaceHistory>(self.surface)
                .unwrap()
                .retry(1, SurfacePresentation::Hidden, now)
        }

        fn visible(opacity: f32) -> SurfacePresentation {
            SurfacePresentation::Visible {
                frame: PaneFrame {
                    left: 0.0,
                    top: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                radius: 0.0,
                opacity,
                all_corners: false,
                ring: crate::present::FocusRing::default(),
                order: None,
            }
        }

        fn applied(&mut self, generation: u64, presentation: SurfacePresentation) {
            self.app
                .world_mut()
                .get_mut::<SurfaceHistory>(self.surface)
                .unwrap()
                .applied = Some(AppliedSurfacePresentation {
                generation,
                presentation,
                settled: true,
            });
        }

        fn complete(&mut self, generation: u64, presentation: SurfacePresentation) {
            self.app.world_mut().trigger(super::PresentationCompleted {
                surface: self.surface,
                generation,
                presentation,
                pending: true,
                reusable: false,
                cached: false,
            });
            self.app.world_mut().flush();
        }

        fn ready(&self) -> bool {
            self.app
                .world()
                .get::<vmux_layout::hosted_page::SurfacePresentationReady>(self.surface)
                .is_some()
        }

        fn transitioned(&self) -> bool {
            self.app
                .world()
                .get::<vmux_layout::hosted_page::SurfaceTransitionReady>(self.surface)
                .is_some()
        }
    }

    #[test]
    fn native_surface_reattachment_retires_its_settle_deadline() {
        let mut fixture = HistoryFixture::new();
        let started = std::time::Instant::now();
        assert!(fixture.retry(started));
        let generation = fixture
            .app
            .world_mut()
            .entity_mut(fixture.surface)
            .take::<HostedSurfaceGeneration>()
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.surface)
            .insert(generation);
        fixture.app.update();
        assert!(fixture.retry(started + SURFACE_SETTLE_TIMEOUT));
    }

    #[test]
    fn settled_presentation_publishes_readiness_and_completed_transition() {
        let mut fixture = HistoryFixture::new();
        let presentation = HistoryFixture::visible(1.0);
        fixture.applied(1, presentation);
        fixture.complete(1, presentation);
        assert!(fixture.ready());
        assert!(fixture.transitioned());
    }

    #[test]
    fn a_retired_generation_cannot_publish_readiness() {
        let mut fixture = HistoryFixture::new();
        let presentation = HistoryFixture::visible(1.0);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.surface)
            .insert(HostedSurfaceGeneration(2));
        fixture.applied(2, presentation);
        fixture.complete(1, presentation);
        assert!(!fixture.ready());
        assert!(!fixture.transitioned());
    }

    #[test]
    fn an_earlier_target_cannot_complete_a_later_presentation() {
        let mut fixture = HistoryFixture::new();
        fixture.applied(1, HistoryFixture::visible(0.0));
        fixture.complete(1, HistoryFixture::visible(1.0));
        assert!(!fixture.ready());
        assert!(!fixture.transitioned());
    }

    #[test]
    fn first_paint_survives_an_interaction_epoch_change_before_mount_delivery() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_plugins(super::PageLifecyclePlugin)
            .add_observer(record_ready);
        let lifecycle = PageLifecycleMailbox::default();
        app.world_mut()
            .spawn(PageLifecycleReceiver(lifecycle.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(2),
                BinIpcNavigationRevision(1),
                crate::HostedPageLifecycle::default(),
            ))
            .id();
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Mounted,
        });
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 2,
            update: LifecycleUpdate::Ready(1),
        });

        app.update();

        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
        assert!(
            !app.world()
                .get::<crate::HostedPageLifecycle>(page)
                .unwrap()
                .painted_after(0)
        );
        for update in [
            LifecycleUpdate::FrameDelivered(1),
            LifecycleUpdate::FrameApplied(1),
            LifecycleUpdate::FramePainted(1),
        ] {
            lifecycle.send(PageLifecycleEnvelope {
                at: std::time::Instant::now(),
                webview: page,
                generation: 4,
                navigation_revision: 1,
                ipc_epoch: 2,
                update,
            });
        }
        app.update();

        assert!(
            app.world()
                .get::<crate::HostedPageLifecycle>(page)
                .unwrap()
                .painted_after(0)
        );
    }

    #[test]
    fn stale_ready_does_not_activate_a_new_navigation() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_plugins(super::PageLifecyclePlugin)
            .add_observer(record_ready);
        let lifecycle = PageLifecycleMailbox::default();
        app.world_mut()
            .spawn(PageLifecycleReceiver(lifecycle.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(1),
                BinIpcNavigationRevision(2),
                crate::HostedPageLifecycle::default(),
            ))
            .id();

        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
        app.update();
        assert_eq!(app.world().resource::<ReadyEvents>().0, 0);

        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(2),
        });
        app.update();
        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
    }

    #[test]
    fn document_reload_advances_the_authoritative_navigation_revision() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_plugins(super::PageLifecyclePlugin)
            .add_observer(record_ready);
        let lifecycle = PageLifecycleMailbox::default();
        app.world_mut()
            .spawn(PageLifecycleReceiver(lifecycle.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(1),
                BinIpcNavigationRevision(1),
                crate::HostedPageLifecycle::default(),
                vmux_layout::Loading,
            ))
            .id();
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Navigated(2),
        });
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
        app.update();
        assert!(app.world().get::<vmux_layout::Loading>(page).is_some());
        assert_eq!(app.world().resource::<ReadyEvents>().0, 0);

        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(2),
        });

        app.update();

        assert_eq!(
            app.world().get::<BinIpcNavigationRevision>(page).unwrap().0,
            2
        );
        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
        assert!(app.world().get::<vmux_layout::Loading>(page).is_none());
    }

    #[test]
    fn newer_navigation_rejects_stale_lifecycle_in_the_same_batch() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_plugins(super::PageLifecyclePlugin)
            .add_observer(record_ready);
        let lifecycle = PageLifecycleMailbox::default();
        app.world_mut()
            .spawn(PageLifecycleReceiver(lifecycle.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(1),
                BinIpcNavigationRevision(1),
                crate::HostedPageLifecycle::default(),
            ))
            .id();
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Navigated(2),
        });
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(2),
        });

        app.update();

        assert_eq!(
            app.world().get::<BinIpcNavigationRevision>(page).unwrap().0,
            2
        );
        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
    }

    #[test]
    fn exhausted_frame_recovery_fails_the_surface_handoff() {
        let mut app = App::new();
        app.init_resource::<OpenPageDropdown>()
            .add_plugins(super::PageLifecyclePlugin);
        let lifecycle = PageLifecycleMailbox::default();
        app.world_mut()
            .spawn(PageLifecycleReceiver(lifecycle.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcEpoch(1),
                BinIpcNavigationRevision(1),
                crate::HostedPageLifecycle::default(),
                vmux_layout::hosted_page::SurfaceHandoff::new(
                    Vec::new(),
                    vmux_ecs::PageOpenId::default(),
                    0,
                    std::time::Duration::ZERO,
                ),
            ))
            .id();
        lifecycle.send(PageLifecycleEnvelope {
            at: std::time::Instant::now(),
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::FrameFailed(1),
        });

        app.update();

        assert!(
            app.world()
                .get::<vmux_layout::hosted_page::SurfaceHandoffFailed>(page)
                .is_some()
        );
    }

    #[test]
    fn lifecycle_mailbox_preserves_all_interactions_in_order() {
        let lifecycle = PageLifecycleMailbox::default();
        let first = bevy::prelude::Entity::from_bits(1);
        let second = bevy::prelude::Entity::from_bits(2);
        for webview in [first, second] {
            lifecycle.send(PageLifecycleEnvelope {
                at: std::time::Instant::now(),
                webview,
                generation: 1,
                navigation_revision: 1,
                ipc_epoch: 1,
                update: LifecycleUpdate::DropdownOpened,
            });
        }
        let mut batches = Vec::new();
        let mut interactions = Vec::new();

        lifecycle.drain_into(&mut batches, &mut interactions);

        assert_eq!(
            interactions
                .iter()
                .map(|interaction| interaction.webview)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
    }

    #[test]
    fn ready_waits_for_the_surface_components_queued_in_the_same_frame() {
        let mut app = App::new();
        let lifecycle = PageLifecycleMailbox::default();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .insert_resource(LifecycleSender(lifecycle.clone()))
            .add_observer(record_ready)
            .add_plugins(super::PageLifecyclePlugin)
            .add_systems(
                Update,
                (queue_ready_surface, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .before(super::PageLifecycleSet),
            );
        app.world_mut().spawn(PageLifecycleReceiver(lifecycle));

        app.update();

        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
    }

    #[test]
    fn native_document_timestamps_preserve_only_a_later_history_gesture() {
        for begin_after_document in [false, true] {
            let mut fixture = HistoryLifecycleFixture::new();
            let at = std::time::Instant::now();
            let begin = if begin_after_document {
                at + std::time::Duration::from_millis(32)
            } else {
                at
            };
            fixture.wheel(begin);
            fixture.send(
                4,
                1,
                7,
                at + std::time::Duration::from_millis(16),
                LifecycleUpdate::Mounted,
            );
            fixture.send(
                4,
                1,
                7,
                at + std::time::Duration::from_millis(33),
                LifecycleUpdate::HistoryRequested(vmux_page::HistoryDirection::Back),
            );
            let events = fixture.events();
            let expected = if begin_after_document {
                vec![crate::NavigationRequest::surface(
                    fixture.surface,
                    crate::NavigationOperation::Back,
                )]
            } else {
                Vec::new()
            };
            assert_eq!(events, expected);
        }
    }

    #[test]
    fn old_native_history_requests_and_stale_document_generations_cannot_consume_new_input() {
        for stale in 0..4 {
            let mut fixture = HistoryLifecycleFixture::new();
            let at = std::time::Instant::now();
            fixture.wheel(at + std::time::Duration::from_millis(32));
            match stale {
                0 => fixture.send(
                    5,
                    1,
                    7,
                    at + std::time::Duration::from_millis(40),
                    LifecycleUpdate::Mounted,
                ),
                1 => fixture.send(
                    4,
                    1,
                    8,
                    at + std::time::Duration::from_millis(40),
                    LifecycleUpdate::Mounted,
                ),
                2 => fixture.send(
                    4,
                    0,
                    7,
                    at + std::time::Duration::from_millis(40),
                    LifecycleUpdate::Mounted,
                ),
                3 => fixture.send(
                    4,
                    1,
                    7,
                    at + std::time::Duration::from_millis(16),
                    LifecycleUpdate::HistoryRequested(vmux_page::HistoryDirection::Back),
                ),
                _ => unreachable!(),
            }
            assert!(fixture.events().is_empty());
            fixture.send(
                4,
                1,
                7,
                at + std::time::Duration::from_millis(41),
                LifecycleUpdate::HistoryRequested(vmux_page::HistoryDirection::Back),
            );
            assert_eq!(
                fixture.events(),
                vec![crate::NavigationRequest::surface(
                    fixture.surface,
                    crate::NavigationOperation::Back
                )]
            );
        }
    }

    #[test]
    fn native_navigation_and_document_batches_keep_history_correlated_to_the_new_revision() {
        let mut fixture = HistoryLifecycleFixture::new();
        let at = std::time::Instant::now();
        fixture.send(
            4,
            2,
            7,
            at + std::time::Duration::from_millis(16),
            LifecycleUpdate::Navigated(2),
        );
        fixture.send(
            4,
            2,
            7,
            at + std::time::Duration::from_millis(24),
            LifecycleUpdate::Mounted,
        );
        fixture.send(
            4,
            1,
            7,
            at + std::time::Duration::from_millis(48),
            LifecycleUpdate::Mounted,
        );
        fixture.wheel(at + std::time::Duration::from_millis(32));
        fixture.send(
            4,
            2,
            7,
            at + std::time::Duration::from_millis(33),
            LifecycleUpdate::HistoryRequested(vmux_page::HistoryDirection::Back),
        );
        assert_eq!(
            fixture.events(),
            vec![crate::NavigationRequest::surface(
                fixture.surface,
                crate::NavigationOperation::Back
            )]
        );
    }

    struct HistoryLifecycleFixture {
        app: App,
        mailbox: PageLifecycleMailbox,
        window: bevy::prelude::Entity,
        surface: bevy::prelude::Entity,
        requests: bevy::ecs::message::MessageCursor<crate::NavigationRequest>,
    }

    impl HistoryLifecycleFixture {
        fn new() -> Self {
            let mailbox = PageLifecycleMailbox::default();
            let mut app = App::new();
            app.add_plugins(super::PageLifecyclePlugin)
                .add_message::<crate::NavigationRequest>();
            app.world_mut()
                .spawn(PageLifecycleReceiver(mailbox.clone()));
            let window = app.world_mut().spawn(bevy::window::Window::default()).id();
            let stack = app
                .world_mut()
                .spawn(bevy_cef::prelude::HostWindow(window))
                .id();
            let surface = app
                .world_mut()
                .spawn((
                    vmux_layout::Browser,
                    PaneFrame {
                        left: 10.0,
                        top: 20.0,
                        width: 200.0,
                        height: 100.0,
                    },
                    bevy::prelude::ChildOf(stack),
                    HostedSurfaceGeneration(4),
                    BinIpcNavigationRevision(1),
                    BinIpcEpoch(7),
                    crate::HostedPageLifecycle::default(),
                ))
                .id();
            app.world_mut()
                .entity_mut(stack)
                .insert(vmux_layout::page_slot::Current::new(
                    Some(surface),
                    vmux_ecs::PageMetadata::default(),
                ));
            app.update();
            Self {
                app,
                mailbox,
                window,
                surface,
                requests: Default::default(),
            }
        }

        fn wheel(&mut self, at: std::time::Instant) {
            self.app
                .world_mut()
                .write_message(vmux_input::NativeWheelInput {
                    window: Some(self.window),
                    position: bevy::prelude::Vec2::new(50.0, 50.0),
                    delta: bevy::prelude::Vec2::new(-240.0, 0.0),
                    phase: vmux_input::WheelPhase::Begin,
                    at,
                    has_native_event: true,
                    precise: true,
                    modifiers: Default::default(),
                });
        }

        fn send(
            &self,
            generation: u64,
            navigation_revision: u64,
            ipc_epoch: u64,
            at: std::time::Instant,
            update: LifecycleUpdate,
        ) {
            self.mailbox.send(PageLifecycleEnvelope {
                webview: self.surface,
                generation,
                navigation_revision,
                ipc_epoch,
                at,
                update,
            });
        }

        fn events(&mut self) -> Vec<crate::NavigationRequest> {
            self.app.update();
            self.requests
                .read(
                    self.app
                        .world()
                        .resource::<bevy::prelude::Messages<crate::NavigationRequest>>(),
                )
                .copied()
                .collect()
        }
    }

    #[test]
    fn the_layout_is_asked_for_the_pointer_only_while_a_surface_of_its_own_is_up() {
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Layout,
                false,
                false,
                PresentationLayer::Current
            ),
            Some(SiblingOrder::Back)
        );
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Layout,
                true,
                false,
                PresentationLayer::Current
            ),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Modal,
                false,
                false,
                PresentationLayer::Current
            ),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Pane,
                false,
                false,
                PresentationLayer::Current
            ),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::pointer_order(PagePlacement::Pane, false, true, PresentationLayer::Current),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::pointer_order(PagePlacement::Pane, true, false, PresentationLayer::Current),
            Some(SiblingOrder::Back)
        );
        assert!(Placement::paints_in_front(PagePlacement::Layout));
    }

    #[test]
    fn tab_transition_keeps_the_retained_surface_in_front() {
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Pane,
                false,
                true,
                PresentationLayer::Retained
            ),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            Placement::pointer_order(
                PagePlacement::Pane,
                false,
                true,
                PresentationLayer::Incoming
            ),
            Some(SiblingOrder::Back)
        );
    }

    #[test]
    fn a_navigated_page_emits_from_its_new_url() {
        let (tx, rx) = async_channel::bounded(1);
        let metadata = PageMetadataMailbox::default();
        let outbox = PageOutbox {
            bin_ipc: tx,
            webview: bevy::prelude::Entity::PLACEHOLDER,
            generation: 3,
            page_url: std::rc::Rc::new(std::cell::RefCell::new("vmux://start/".to_string())),
            navigation_revision: std::rc::Rc::new(std::cell::Cell::new(1)),
            ipc_epoch: std::rc::Rc::new(std::cell::Cell::new(1)),
            metadata: PageMetadataSender(metadata),
            waker: super::PageWaker::from_proxy(None),
        };

        vmux_page::Outbox::set_page(&outbox, "vmux://sessions/claude");
        vmux_page::Outbox::send(&outbox, "event", &[1, 2, 3], 7, 9).unwrap();

        let emitted = rx.recv_blocking().unwrap();
        assert_eq!(emitted.page_url, "vmux://sessions/claude");
        assert_eq!(emitted.surface_generation, Some(3));
        assert_eq!(emitted.navigation_revision, Some(7));
        assert_eq!(emitted.ipc_epoch, Some(9));
    }

    #[test]
    fn document_title_updates_an_internal_page_without_replacing_its_icon() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, super::apply_metadata);
        let metadata = PageMetadataMailbox::default();
        app.world_mut()
            .spawn(PageMetadataReceiver(metadata.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcNavigationRevision(1),
                BinIpcEpoch(1),
                vmux_ecs::PageMetadata {
                    title: "vmux://history/".to_string(),
                    url: "vmux://history/".to_string(),
                    icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Clock),
                    bg_color: None,
                },
            ))
            .id();
        let outbox = PageOutbox {
            bin_ipc: async_channel::unbounded().0,
            webview: page,
            generation: 4,
            page_url: std::rc::Rc::new(std::cell::RefCell::new("vmux://history/".to_string())),
            navigation_revision: std::rc::Rc::new(std::cell::Cell::new(1)),
            ipc_epoch: std::rc::Rc::new(std::cell::Cell::new(1)),
            metadata: PageMetadataSender(metadata),
            waker: super::PageWaker::from_proxy(None),
        };

        vmux_page::Outbox::set_title(&outbox, "History");
        vmux_page::Outbox::set_favicon(&outbox, "vmux://history/assets/favicons/history.svg");
        app.update();

        let metadata = app.world().get::<vmux_ecs::PageMetadata>(page).unwrap();
        assert_eq!(metadata.title, "History");
        assert_eq!(
            metadata.icon,
            vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Clock)
        );
    }

    #[test]
    fn metadata_from_a_previous_subtree_route_is_rejected() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, super::apply_metadata);
        let mailbox = PageMetadataMailbox::default();
        app.world_mut().spawn(PageMetadataReceiver(mailbox.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcNavigationRevision(2),
                BinIpcEpoch(1),
                vmux_ecs::PageMetadata {
                    title: "Current".to_string(),
                    url: "vmux://sessions/current".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        mailbox.send(super::PageMetadataUpdate {
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            page_url: "vmux://sessions/previous".to_string(),
            title: Some("Previous".to_string()),
            favicon: None,
        });

        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_ecs::PageMetadata>(page)
                .unwrap()
                .title,
            "Current"
        );
    }

    #[test]
    fn metadata_from_a_previous_interaction_epoch_is_rejected() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, super::apply_metadata);
        let mailbox = PageMetadataMailbox::default();
        app.world_mut().spawn(PageMetadataReceiver(mailbox.clone()));
        let page = app
            .world_mut()
            .spawn((
                HostedSurfaceGeneration(4),
                BinIpcNavigationRevision(2),
                BinIpcEpoch(2),
                vmux_ecs::PageMetadata {
                    title: "Current".to_string(),
                    url: "vmux://sessions/current".to_string(),
                    ..Default::default()
                },
            ))
            .id();
        mailbox.send(super::PageMetadataUpdate {
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            page_url: "vmux://sessions/current".to_string(),
            title: Some("Previous".to_string()),
            favicon: None,
        });

        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_ecs::PageMetadata>(page)
                .unwrap()
                .title,
            "Current"
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
