use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::rc::Rc;
use std::sync::{Arc, Condvar, LazyLock, Mutex};

use bevy::ecs::archetype::ArchetypeId;
use bevy::ecs::relationship::Relationship;
use bevy::ecs::world::EntityRef;
use bevy::prelude::*;
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
use vmux_layout::window::FocusedWindow;
use vmux_page::{
    Appearance, AssetReply, Assets as RendererAssets, Embedding, LifecycleSink, LifecycleUpdate,
    Outbox as RendererOutbox, Page, PageClaimDirty, PagePlacement, PageRegistration, SiblingOrder,
    Wake as RendererWake, WebView,
};
use vmux_setting::{AppSettings, ResolvedColorScheme, ResolvedScheme};
use vmux_ui::hooks::EventListenerError;

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
        .add_message::<vmux_layout::presentation::NativeSelectionCommitted>()
        .init_non_send::<PageRenderer>()
        .init_resource::<SurfacePresentations>()
        .init_resource::<SurfaceSettleRetries>()
        .init_resource::<OpenPageDropdown>()
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
                open.after(PageOpenSet::HandleKnownPages),
                bevy::ecs::schedule::ApplyDeferred,
                sync_surface_interaction,
                apply_lifecycle,
                apply_metadata,
                dismiss_page_dropdowns,
                mark_handoff_painted,
                sync_appearance,
                sync_scale,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (place, bevy::ecs::schedule::ApplyDeferred)
                .chain()
                .in_set(vmux_layout::presentation::NativePresentationSet::Place)
                .after(vmux_ecs::UiStateFlushSet)
                .after(crate::BrowserSystemSet::SyncWindowedFrames),
        )
        .add_systems(
            PostUpdate,
            (place, bevy::ecs::schedule::ApplyDeferred)
                .chain()
                .in_set(vmux_layout::presentation::NativePresentationSet::Finalize)
                .run_if(on_message::<vmux_layout::presentation::NativeSelectionCommitted>),
        )
        .add_systems(
            PostUpdate,
            render.after(vmux_layout::presentation::NativePresentationSet::Finalize),
        )
        .add_systems(
            PostUpdate,
            focus.after(crate::BrowserSystemSet::HostFocusApplied),
        )
        .add_observer(forward_host_emit)
        .add_observer(reload)
        .add_observer(hard_reload)
        .add_observer(mark_candidate_added::<HostsPage>)
        .add_observer(mark_candidate_removed::<HostsPage>)
        .add_observer(mark_candidate_added::<LayoutCef>)
        .add_observer(mark_candidate_removed::<LayoutCef>);
    }
}

#[derive(Component, Clone)]
struct HostedSurface {
    placement: PagePlacement,
    page: &'static Page,
    window: Entity,
    url: String,
}

fn reload(event: On<RequestReload>, renderer: NonSend<PageRenderer>, mut commands: Commands) {
    if let Some(surface) = renderer.active.get(&event.webview)
        && surface.reload()
    {
        commands.entity(event.webview).remove::<PageReady>();
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
        commands.entity(event.webview).remove::<PageReady>();
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct HostedSurfaceGeneration(u64);

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

#[derive(Default)]
struct OpenScratch {
    initialized: bool,
    generations: u64,
    discovery: PageDiscovery,
    page_candidates: Vec<Entity>,
    tree: Vec<Entity>,
    seen: bevy::ecs::entity::EntityHashSet,
    forced: bevy::ecs::entity::EntityHashSet,
    current_active: bevy::ecs::entity::EntityHashSet,
    active_waiting: bevy::ecs::entity::EntityHashSet,
}

#[derive(bevy::ecs::system::SystemParam)]
struct PageClaims<'w, 's> {
    entities: Query<'w, 's, EntityRef<'static>>,
}

impl<'w, 's> std::ops::Deref for PageClaims<'w, 's> {
    type Target = Query<'w, 's, EntityRef<'static>>;
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
            let Some(priority) = registration_claim_priority(candidate, *registration) else {
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

#[derive(Resource, Default)]
struct SurfacePresentations {
    applied: HashMap<Entity, AppliedSurfacePresentation>,
    cached: HashMap<Entity, CachedNativeFrame>,
}

impl SurfacePresentations {
    fn remember(
        &mut self,
        entity: Entity,
        generation: u64,
        surface: &WebView,
        presentation: SurfacePresentation,
    ) {
        let SurfacePresentation::Visible { frame, opacity, .. } = presentation else {
            return;
        };
        if opacity == 1.0
            && self.cached.get(&entity).is_some_and(|cached| {
                cached.matches_frame(generation, surface.navigation_revision(), frame)
            })
        {
            return;
        }
        if opacity != 1.0
            || !frame.matches(surface.bounds())
            || !surface.presentation_settled(opacity)
        {
            return;
        }
        self.cached.insert(
            entity,
            CachedNativeFrame {
                generation,
                navigation_revision: surface.navigation_revision(),
                frame,
            },
        );
    }
}

const SURFACE_SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
const SURFACE_SETTLE_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

#[derive(Resource, Default)]
struct SurfaceSettleRetries {
    started: HashMap<(Entity, u64), (SurfacePresentation, std::time::Instant)>,
    wake: SurfaceSettleWake,
}

impl SurfaceSettleRetries {
    fn clear(&mut self, entity: Entity) {
        self.started
            .retain(|(candidate, _), _| *candidate != entity);
    }

    fn retry(
        &mut self,
        entity: Entity,
        generation: u64,
        presentation: SurfacePresentation,
        now: std::time::Instant,
    ) -> bool {
        self.started.retain(|(candidate, current_generation), _| {
            *candidate != entity || *current_generation == generation
        });
        let (_, started) = self
            .started
            .entry((entity, generation))
            .and_modify(|(target, started)| {
                if *target != presentation {
                    *target = presentation;
                    *started = now;
                }
            })
            .or_insert((presentation, now));
        now.saturating_duration_since(*started) < SURFACE_SETTLE_TIMEOUT
    }

    fn schedule(&self, proxy: Option<&EventLoopProxyWrapper>) {
        self.wake.schedule(proxy);
    }
}

#[derive(Default)]
struct SurfaceSettleWake {
    shared: Arc<(Mutex<SurfaceSettleWakeState>, Condvar)>,
}

#[derive(Default)]
struct SurfaceSettleWakeState {
    deadline: Option<std::time::Instant>,
    proxy: Option<EventLoopProxy<WinitUserEvent>>,
    worker_started: bool,
}

impl SurfaceSettleWake {
    fn schedule(&self, proxy: Option<&EventLoopProxyWrapper>) {
        let Some(proxy) = proxy else {
            return;
        };
        let deadline = std::time::Instant::now() + SURFACE_SETTLE_RETRY_INTERVAL;
        let start_worker = {
            let (state, wake) = &*self.shared;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.deadline.is_some_and(|current| current <= deadline) {
                return;
            }
            state.deadline = Some(deadline);
            state.proxy = Some((**proxy).clone());
            let start_worker = !state.worker_started;
            state.worker_started = true;
            wake.notify_one();
            start_worker
        };
        if start_worker {
            let shared = self.shared.clone();
            std::thread::spawn(move || Self::run(shared));
        }
    }

    fn run(shared: Arc<(Mutex<SurfaceSettleWakeState>, Condvar)>) {
        let (state, wake) = &*shared;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
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

struct PageLifecycleEnvelope {
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
    mounts: u64,
    ready: Option<u64>,
    remounts: u64,
    frame_pending: Option<bool>,
    frame_delivered: Option<u64>,
    frames_applied: u64,
    frame_applied: Option<u64>,
    frame_failed: Option<u64>,
    ui_events: u64,
    host_events: u64,
    listeners_cleared: bool,
    listeners_added: usize,
    measurements_cleared: bool,
    measurement_delta: i64,
}

impl PageLifecycleBatch {
    fn record(&mut self, update: LifecycleUpdate) {
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
            .record(envelope.update);
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

const CLOSE_PAGE_DROPDOWNS: &str = "for (const popup of document.querySelectorAll('[data-vmux-dropdown-popup]:popover-open')) popup.hidePopover();";

fn apply_metadata(
    receiver: Single<&PageMetadataReceiver>,
    mut updates: Local<Vec<PageMetadataUpdate>>,
    mut pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcNavigationRevision,
        &BinIpcEpoch,
        &mut PageMetadata,
    )>,
) {
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

fn apply_lifecycle(
    receiver: Single<&PageLifecycleReceiver>,
    renderer: Option<NonSend<PageRenderer>>,
    mut batches: Local<Vec<(PageLifecycleKey, PageLifecycleBatch)>>,
    mut semantic: Local<Vec<PageLifecycleEnvelope>>,
    mut pages: Query<(
        &HostedSurfaceGeneration,
        &BinIpcEpoch,
        &mut BinIpcNavigationRevision,
        &mut crate::HostedPageLifecycle,
    )>,
    blocked: Query<(), With<vmux_layout::hosted_page::SurfaceBlocked>>,
    handoffs: Query<(), With<vmux_layout::hosted_page::SurfaceHandoff>>,
    source_hosts: Query<&HostWindow, Without<LayoutCef>>,
    mut layouts: Query<(Entity, &HostWindow, &mut LayoutOverlayDismissRevision), With<LayoutCef>>,
    mut open_dropdown: ResMut<OpenPageDropdown>,
    mut commands: Commands,
) {
    receiver.0.drain_into(&mut batches, &mut semantic);
    for envelope in semantic.drain(..) {
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
        match envelope.update {
            LifecycleUpdate::HistoryRequested(direction) => {
                let operation = match direction {
                    vmux_page::HistoryDirection::Back => crate::NavigationOperation::Back,
                    vmux_page::HistoryDirection::Forward => crate::NavigationOperation::Forward,
                };
                if !crate::NativeBridge::claim_history_input(envelope.webview, operation) {
                    continue;
                }
                commands.write_message(crate::NavigationRequest::surface(
                    envelope.webview,
                    operation,
                ));
                continue;
            }
            LifecycleUpdate::DropdownOpened => {
                if let Some(previous) = open_dropdown.0.replace(envelope.webview)
                    && previous != envelope.webview
                    && let Some(surface) = renderer
                        .as_ref()
                        .and_then(|renderer| renderer.active.get(&previous))
                {
                    surface.evaluate_script(CLOSE_PAGE_DROPDOWNS);
                }
            }
            LifecycleUpdate::PageInteracted => {
                if let Some(previous) = open_dropdown.0
                    && previous != envelope.webview
                {
                    if let Some(surface) = renderer
                        .as_ref()
                        .and_then(|renderer| renderer.active.get(&previous))
                    {
                        surface.evaluate_script(CLOSE_PAGE_DROPDOWNS);
                    }
                    open_dropdown.0 = None;
                }
            }
            _ => continue,
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
    for (key, batch) in batches.drain(..) {
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

fn open(
    mut state: ParamSet<(
        PageClaims,
        NonSendMut<PageRenderer>,
        NonSendMut<Browsers>,
        Query<&mut vmux_layout::hosted_page::SurfaceHandoff>,
        Query<EntityRef, Or<(With<HostsPage>, With<LayoutCef>)>>,
        Query<
            Entity,
            Or<(
                Changed<PageMetadata>,
                Changed<vmux_layout::hosted_page::SurfaceHandoff>,
            )>,
        >,
    )>,
    requester: Option<Res<Requester>>,
    bin_ipc: Option<Res<BinIpcEventRawSender>>,
    mut changes: OpenChanges,
    frame_queries: WindowFrameQueries,
    primary_windows: Query<Entity, With<PrimaryWindow>>,
    metadata_sender: Query<&PageMetadataSender>,
    lifecycle_sender: Query<&PageLifecycleSender>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    appearance: Single<&ResolvedColorScheme>,
    time: Res<Time>,
    mut scratch: Local<OpenScratch>,
    mut commands: Commands,
) {
    let OpenScratch {
        initialized,
        generations,
        discovery,
        page_candidates,
        tree,
        seen,
        forced,
        current_active,
        active_waiting,
    } = &mut *scratch;
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
        let candidates = state.p4();
        page_candidates.extend(candidates.iter().map(|candidate| candidate.id()));
        if presentation_changed || host_tree_removed {
            forced.extend(page_candidates.iter().copied());
        }
    } else {
        page_candidates.extend(discovery.retry.drain());
        let changed = state.p5();
        page_candidates.extend(changed.iter());
        page_candidates.extend(changes.dirty.iter());
    }
    page_candidates.extend(changes.removed_hosted.read());
    page_candidates.extend(changes.removed_layout.read());
    for entity in changes.dirty.iter() {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.try_remove::<PageClaimDirty>();
        }
    }
    if page_candidates.is_empty() {
        return;
    }
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
        let mut handoffs = state.p3();
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
        std::mem::swap(&mut *active_waiting, &mut *current_active);
    } else {
        for entity in seen.iter().copied() {
            if current_active.contains(&entity) {
                active_waiting.insert(entity);
            } else {
                active_waiting.remove(&entity);
            }
        }
    }
    let released = std::mem::take(&mut discovery.released);
    if !released.is_empty() {
        let now = std::time::Instant::now();
        {
            let mut renderer = state.p1();
            for entity in &released {
                renderer.retire(*entity, now, proxy.as_deref());
            }
        }
        {
            let mut browsers = state.p2();
            for entity in &released {
                browsers.unset_externally_hosted(*entity);
            }
        }
        for entity in released {
            active_waiting.remove(&entity);
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
    if discovery.wanted.is_empty() {
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
            discovery.retry_wanted();
            report_waiting(reason);
            return;
        }
    };
    let appearance = appearance_of(appearance.0);

    let mut wanted = std::mem::take(&mut discovery.wanted);
    for (entity, registration, _, claim_changed) in wanted.drain(..) {
        let page = registration.page();
        let placement = registration.placement();
        let candidate = {
            let entities = state.p0();
            let Some(window_entity) = entities.host_window(entity).or(primary_window) else {
                discovery.retry(entity);
                report_waiting("page has no host window entity");
                continue;
            };
            let Ok(candidate) = entities.get(entity) else {
                continue;
            };
            (
                window_entity,
                candidate.get::<HostedSurface>().cloned(),
                candidate
                    .get::<vmux_layout::hosted_page::SurfaceHandoff>()
                    .map(|handoff| (handoff.navigation_started(), handoff.retain_visible())),
                candidate
                    .get::<crate::HostedPageLifecycle>()
                    .map(|lifecycle| lifecycle.navigation_revision)
                    .unwrap_or_default(),
                claim_changed || registration.instance_changed(candidate),
                candidate
                    .get::<PageMetadata>()
                    .map(|metadata| metadata.url.clone())
                    .unwrap_or_else(|| page.url.to_string()),
                candidate.contains::<vmux_layout::hosted_page::SurfaceBlocked>()
                    || candidate.contains::<vmux_ecs::PageTransition>(),
            )
        };
        let (
            window_entity,
            current,
            handoff,
            baseline_navigation_revision,
            instance_changed,
            url,
            blocked,
        ) = candidate;
        let reactivating = handoff.is_some_and(|(started, _)| !started);
        let has_surface = state.p1().active.contains_key(&entity);
        if !has_surface
            && placement == PagePlacement::Pane
            && !frame_queries.page_is_active(entity)
            && !state.p0().get(entity).is_ok_and(|candidate| {
                candidate.contains::<vmux_layout::warm_page::WarmPageSpare>()
            })
        {
            continue;
        }
        if let Some(current) = current.as_ref().filter(|current| {
            std::ptr::eq(current.page, page) && current.window == window_entity && has_surface
        }) {
            let route_changed = current.url != url;
            let placement_changed = current.placement != placement;
            let fragment_only =
                route_changed && current.url.split('#').next() == url.split('#').next();
            if fragment_only && !reactivating && !instance_changed {
                let renderer = state.p1();
                renderer
                    .active
                    .get(&entity)
                    .expect("the page was just found")
                    .set_url(&url);
            } else if route_changed || reactivating || instance_changed {
                let instance = {
                    let entities = state.p0();
                    let Ok(candidate) = entities.get(entity) else {
                        continue;
                    };
                    registration.instance(candidate)
                };
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
                let remounted = surface.navigate(page, &url, instance);
                let navigation_revision = surface.navigation_revision();
                if remounted {
                    commands.entity(entity).remove::<PageReady>();
                }
                commands.entity(entity).insert((
                    BinIpcNavigationRevision(navigation_revision),
                    BinIpcEpoch(ipc_epoch),
                ));
                info!("browser_platform: updated {entity:?} within {}", page.url);
                if reactivating && let Ok(mut handoff) = state.p3().get_mut(entity) {
                    handoff.begin_navigation(baseline_navigation_revision, time.elapsed());
                }
            }
            if route_changed || placement_changed || instance_changed {
                commands.entity(entity).insert(HostedSurface {
                    placement,
                    page,
                    window: window_entity,
                    url,
                });
            }
            continue;
        }
        if current.as_ref().is_some_and(|current| {
            current.page.transparent == page.transparent
                && current.page.document_url() == page.document_url()
                && current.window == window_entity
                && has_surface
        }) {
            let instance = {
                let entities = state.p0();
                let Ok(candidate) = entities.get(entity) else {
                    continue;
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
                let remounted = surface.navigate(page, &url, instance);
                commands.entity(entity).insert((
                    BinIpcNavigationRevision(surface.navigation_revision()),
                    BinIpcEpoch(ipc_epoch),
                ));
                remounted
            };
            if reactivating && let Ok(mut handoff) = state.p3().get_mut(entity) {
                handoff.begin_navigation(baseline_navigation_revision, time.elapsed());
            }
            commands.entity(entity).insert(HostedSurface {
                placement,
                page,
                window: window_entity,
                url,
            });
            if remounted {
                commands.entity(entity).remove::<PageReady>();
            }
            info!("browser_platform: navigated {entity:?} to {}", page.url);
            continue;
        }
        let replacing = current.is_some();
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(1.0, 1.0).into(),
        };
        let instance = {
            let entities = state.p0();
            let Ok(candidate) = entities.get(entity) else {
                continue;
            };
            registration.instance(candidate)
        };
        *generations = generations.wrapping_add(1).max(1);
        let generation = *generations;
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
            ))
        });
        match built {
            None => {
                discovery.retry(entity);
                report_waiting("primary window has no winit window yet");
                continue;
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
                if placement.paints_in_front() {
                    surface.raise_above_layers();
                }
                state.p2().set_externally_hosted(entity);
                info!(
                    "browser_platform: hosting {} for {entity:?} as {placement:?}, {appearance:?}",
                    page.url
                );
                state
                    .p1()
                    .install(entity, surface, std::time::Instant::now(), proxy.as_deref());
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
                    handoff.begin_navigation(0, time.elapsed());
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
    discovery.wanted = wanted;
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

fn place(
    renderer: Option<NonSendMut<PageRenderer>>,
    all_hosted: Query<(), With<HostsPage>>,
    hosted: Query<
        (
            Entity,
            Ref<HostedSurface>,
            &HostedSurfaceGeneration,
            Option<&vmux_layout::hosted_page::SurfaceHandoff>,
            Option<&vmux_layout::page_slot::SurfaceOpacity>,
            Has<vmux_layout::hosted_page::SurfacePresentationReady>,
            Has<vmux_layout::hosted_page::SurfaceTransitionReady>,
            Has<vmux_layout::hosted_page::SurfacePaintReady>,
            Has<PageReady>,
            Has<vmux_ecs::PagePresented>,
            Has<vmux_layout::hosted_page::SurfaceBlocked>,
            Has<vmux_ecs::PageTransition>,
            Has<vmux_layout::PendingWebviewReveal>,
        ),
        With<HostsPage>,
    >,
    ready_spares: Query<(), (With<vmux_layout::warm_page::WarmPageSpare>, With<PageReady>)>,
    frames: Query<&PaneFrame>,
    rings: Query<&FocusRing>,
    corners: Query<&AllCorners>,
    windows: Query<&Window>,
    capturing: Query<&HostWindow, (With<LayoutCef>, LayoutPointerCapture)>,
    settings: Res<AppSettings>,
    frame_queries: WindowFrameQueries,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut presentations: ResMut<SurfacePresentations>,
    mut retries: ResMut<SurfaceSettleRetries>,
    mut reused: MessageWriter<vmux_layout::presentation::NativeSurfaceReused>,
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
    presentations
        .applied
        .retain(|entity, _| renderer.active.contains_key(entity));
    presentations
        .cached
        .retain(|entity, _| renderer.active.contains_key(entity));
    retries
        .started
        .retain(|(entity, _), _| renderer.active.contains_key(entity));
    if renderer.active.len() != held
        && let Some(proxy) = proxy.as_ref()
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
    let mut committed_replacements = Vec::new();
    for (
        entity,
        page,
        generation,
        handoff,
        opacity,
        presentation_ready,
        transition_ready,
        paint_ready,
        ready,
        presented,
        blocked,
        transitioning,
        revealing,
    ) in &hosted
    {
        let Some(surface) = renderer.active.get(&entity) else {
            continue;
        };
        let pending = handoff.is_some_and(|handoff| !handoff.retain_visible());
        let presentation = frame_queries.presentation(entity);
        let hidden = ready_spares.contains(entity);
        let next = if hidden {
            SurfacePresentation::Hidden
        } else {
            let window = windows.get(page.window).ok();
            let Some(frame) = page.placement.frame(entity, window, &frames) else {
                if presentation_ready || transition_ready {
                    commands.entity(entity).try_remove::<(
                        vmux_layout::hosted_page::SurfacePresentationReady,
                        vmux_layout::hosted_page::SurfaceTransitionReady,
                    )>();
                }
                let hidden_applied = presentations.applied.get(&entity).is_some_and(|current| {
                    current.generation == generation.0
                        && current.presentation == SurfacePresentation::Hidden
                        && current.settled
                });
                if page.is_changed() || !hidden_applied {
                    SurfacePresentation::Hidden.apply(surface, false, false);
                    presentations.applied.insert(
                        entity,
                        AppliedSurfacePresentation {
                            generation: generation.0,
                            presentation: SurfacePresentation::Hidden,
                            settled: true,
                        },
                    );
                    retries.clear(entity);
                }
                continue;
            };
            let all_corners = corners.get(entity).is_ok_and(|corners| corners.0);
            let ring = rings.get(entity).copied().unwrap_or_default();
            let window_is_capturing = capturing.iter().any(|host| host.0 == page.window);
            let awaiting_first_paint =
                handoff.is_some_and(|handoff| handoff.requires_paint(paint_ready));
            let opacity = if awaiting_first_paint {
                0.0
            } else {
                opacity.map_or_else(
                    || {
                        if pending { 0.0 } else { 1.0 }
                    },
                    |opacity| opacity.value(),
                )
            };
            SurfacePresentation::Visible {
                frame,
                radius: settings.layout.radius as f64,
                opacity,
                all_corners,
                ring,
                order: page
                    .placement
                    .pointer_order(window_is_capturing, pending, presentation),
            }
        };
        let reusable = ready
            && presented
            && handoff.is_none()
            && !blocked
            && !transitioning
            && !revealing
            && !renderer.replacements.contains_key(&entity);
        let cached = reusable
            && presentation == PresentationLayer::Incoming
            && presentations.cached.get(&entity).is_some_and(|cached| {
                cached.matches(
                    generation.0,
                    surface.navigation_revision(),
                    next,
                    surface.bounds(),
                )
            });
        let previous = presentations.applied.get(&entity).copied();
        let same = !page.is_changed()
            && previous.is_some_and(|current| {
                current.generation == generation.0 && current.presentation == next
            });

        let native_settled = match next {
            SurfacePresentation::Hidden => true,
            SurfacePresentation::Visible { opacity, .. } => {
                !pending || surface.presentation_settled(opacity)
            }
        };
        if same && previous.is_some_and(|current| current.settled) && native_settled {
            sync_presentation_readiness(
                next,
                entity,
                pending,
                presentation_ready,
                transition_ready,
                &mut commands,
            );
            if cached && surface.presentation_settled(1.0) {
                reused.write(vmux_layout::presentation::NativeSurfaceReused { surface: entity });
            }
            if reusable {
                presentations.remember(entity, generation.0, surface, next);
            }
            if renderer.replacements.contains_key(&entity)
                && (handoff.is_none() || paint_ready)
                && matches!(next, SurfacePresentation::Visible { .. })
            {
                committed_replacements.push(entity);
            }
            continue;
        }
        let retain_visible = matches!(
            previous,
            Some(AppliedSurfacePresentation {
                generation: current_generation,
                presentation: SurfacePresentation::Visible { .. },
                ..
            }) if current_generation == generation.0
        );
        let settled = if same {
            next.settle(surface, retain_visible, pending)
        } else {
            next.apply(surface, retain_visible, pending)
        };
        presentations.applied.insert(
            entity,
            AppliedSurfacePresentation {
                generation: generation.0,
                presentation: next,
                settled,
            },
        );
        if settled {
            sync_presentation_readiness(
                next,
                entity,
                pending,
                presentation_ready,
                transition_ready,
                &mut commands,
            );
            retries.clear(entity);
            if cached && surface.presentation_settled(1.0) {
                reused.write(vmux_layout::presentation::NativeSurfaceReused { surface: entity });
            }
            if reusable {
                presentations.remember(entity, generation.0, surface, next);
            }
            if renderer.replacements.contains_key(&entity)
                && (handoff.is_none() || paint_ready)
                && matches!(next, SurfacePresentation::Visible { .. })
            {
                committed_replacements.push(entity);
            }
        } else if retries.retry(entity, generation.0, next, now) {
            commands.entity(entity).try_remove::<(
                vmux_layout::hosted_page::SurfacePresentationReady,
                vmux_layout::hosted_page::SurfaceTransitionReady,
            )>();
            retries.schedule(proxy.as_deref());
        } else {
            surface.set_visible(false);
            commands
                .entity(entity)
                .try_remove::<(
                    vmux_layout::hosted_page::SurfacePresentationReady,
                    vmux_layout::hosted_page::SurfaceTransitionReady,
                )>()
                .try_insert(vmux_layout::hosted_page::SurfaceHandoffFailed);
            presentations.applied.insert(
                entity,
                AppliedSurfacePresentation {
                    generation: generation.0,
                    presentation: SurfacePresentation::Hidden,
                    settled: true,
                },
            );
            retries.clear(entity);
        }
    }
    for entity in committed_replacements {
        renderer.commit_replacement(entity, now, proxy.as_deref());
    }
}

fn render(
    renderer: Option<NonSend<PageRenderer>>,
    ready_spares: Query<(), (With<vmux_layout::warm_page::WarmPageSpare>, With<PageReady>)>,
    mut dirty: Local<Vec<Entity>>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    dirty.clear();
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
    renderer: Option<NonSend<PageRenderer>>,
    hosted: Query<(Entity, &HostedSurface, &HostedSurfaceGeneration)>,
    intent: Single<&crate::host_focus::HostFocusIntent>,
    focused_window: FocusedWindow,
    mut focused: Local<Option<(Entity, u64)>>,
) {
    let Some(renderer) = renderer else {
        return;
    };
    let wanted = match **intent {
        crate::host_focus::HostFocusIntent::LayoutView => hosted
            .iter()
            .find(|(_, page, _)| {
                page.placement == PagePlacement::Layout
                    && focused_window
                        .entity()
                        .is_none_or(|window| page.window == window)
            })
            .map(|(entity, _, generation)| (entity, generation.0)),
        crate::host_focus::HostFocusIntent::HostedPane(page) => hosted
            .get(page)
            .ok()
            .map(|(entity, _, generation)| (entity, generation.0)),
        _ => {
            *focused = None;
            return;
        }
    };
    let Some((entity, generation)) = wanted else {
        *focused = None;
        return;
    };
    if *focused == Some((entity, generation)) && !focused_window.is_changed() {
        return;
    }
    let Some(surface) = renderer.active.get(&entity) else {
        *focused = None;
        return;
    };
    if surface.take_first_responder() {
        *focused = Some((entity, generation));
    } else {
        *focused = None;
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
    let appearance = appearance_of(appearance.0);
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

fn appearance_of(scheme: ResolvedScheme) -> Appearance {
    match scheme {
        ResolvedScheme::Light => Appearance::Light,
        ResolvedScheme::Dark => Appearance::Dark,
    }
}

trait PagePlacementExt {
    fn paints_in_front(self) -> bool;
    fn pointer_order(
        self,
        capturing: bool,
        pending: bool,
        presentation: PresentationLayer,
    ) -> Option<SiblingOrder>;
    fn frame(
        self,
        entity: Entity,
        window: Option<&Window>,
        frames: &Query<&PaneFrame>,
    ) -> Option<PaneFrame>;
}

impl PagePlacementExt for PagePlacement {
    fn paints_in_front(self) -> bool {
        matches!(self, PagePlacement::Layout | PagePlacement::Modal)
    }

    fn pointer_order(
        self,
        capturing: bool,
        pending: bool,
        presentation: PresentationLayer,
    ) -> Option<SiblingOrder> {
        if capturing && self == PagePlacement::Pane {
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
        match self {
            PagePlacement::Layout if !capturing => Some(SiblingOrder::Back),
            PagePlacement::Layout | PagePlacement::Modal => Some(SiblingOrder::Front),
            PagePlacement::Pane => Some(SiblingOrder::Front),
        }
    }

    fn frame(
        self,
        entity: Entity,
        window: Option<&Window>,
        frames: &Query<&PaneFrame>,
    ) -> Option<PaneFrame> {
        match self {
            PagePlacement::Layout => {
                let window = window?;
                Some(PaneFrame {
                    left: 0.0,
                    top: 0.0,
                    width: window.width(),
                    height: window.height(),
                })
            }
            PagePlacement::Pane | PagePlacement::Modal => frames.get(entity).ok().copied(),
        }
    }
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
        if matches!(
            update,
            LifecycleUpdate::Mounted
                | LifecycleUpdate::Remounted
                | LifecycleUpdate::Navigated(_)
                | LifecycleUpdate::Ready(_)
        ) {
            crate::NativeBridge::invalidate_history_input(self.webview);
        }
        self.sender.0.send(PageLifecycleEnvelope {
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

fn report_waiting(reason: &str) {
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        info!("browser_platform: waiting, {reason}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CachedNativeFrame, HostedSurfaceGeneration, MacosBrowserPlugin, OpenPageDropdown,
        PageLifecycleEnvelope, PageLifecycleMailbox, PageLifecycleReceiver, PageMetadataMailbox,
        PageMetadataReceiver, PageMetadataSender, PageOutbox, PagePlacementExt,
        SURFACE_SETTLE_TIMEOUT, SimulatorFrame, SimulatorFrameRequest, SurfacePresentation,
        SurfaceSettleRetries,
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
    fn surface_settle_retries_are_time_bounded() {
        let entity = bevy::prelude::Entity::PLACEHOLDER;
        let generation = 1;
        let mut retries = SurfaceSettleRetries::default();
        let started = std::time::Instant::now();
        let presentation = SurfacePresentation::Hidden;

        assert!(retries.retry(entity, generation, presentation, started));
        assert!(retries.retry(
            entity,
            generation,
            presentation,
            started + SURFACE_SETTLE_TIMEOUT - std::time::Duration::from_millis(1)
        ));
        assert!(!retries.retry(
            entity,
            generation,
            presentation,
            started + SURFACE_SETTLE_TIMEOUT
        ));
        retries.clear(entity);
        assert!(retries.retry(
            entity,
            generation,
            presentation,
            started + SURFACE_SETTLE_TIMEOUT
        ));
        assert!(retries.retry(
            entity,
            generation + 1,
            presentation,
            started + SURFACE_SETTLE_TIMEOUT
        ));
        assert_eq!(retries.started.len(), 1);
    }

    #[test]
    fn first_paint_survives_an_interaction_epoch_change_before_mount_delivery() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_systems(Update, super::apply_lifecycle)
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
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Mounted,
        });
        lifecycle.send(PageLifecycleEnvelope {
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
        ] {
            lifecycle.send(PageLifecycleEnvelope {
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
            .add_systems(Update, super::apply_lifecycle)
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
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
        app.update();
        assert_eq!(app.world().resource::<ReadyEvents>().0, 0);

        lifecycle.send(PageLifecycleEnvelope {
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
            .add_systems(Update, super::apply_lifecycle)
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
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Navigated(2),
        });
        lifecycle.send(PageLifecycleEnvelope {
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
    fn newer_navigation_rejects_stale_lifecycle_in_the_same_batch() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .init_resource::<OpenPageDropdown>()
            .add_systems(Update, super::apply_lifecycle)
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
            webview: page,
            generation: 4,
            navigation_revision: 1,
            ipc_epoch: 1,
            update: LifecycleUpdate::Ready(1),
        });
        lifecycle.send(PageLifecycleEnvelope {
            webview: page,
            generation: 4,
            navigation_revision: 2,
            ipc_epoch: 1,
            update: LifecycleUpdate::Navigated(2),
        });
        lifecycle.send(PageLifecycleEnvelope {
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
            .add_systems(Update, super::apply_lifecycle);
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
            .add_systems(
                Update,
                (
                    queue_ready_surface,
                    bevy::ecs::schedule::ApplyDeferred,
                    super::apply_lifecycle,
                )
                    .chain(),
            );
        app.world_mut().spawn(PageLifecycleReceiver(lifecycle));

        app.update();

        assert_eq!(app.world().resource::<ReadyEvents>().0, 1);
    }

    #[test]
    fn the_layout_is_asked_for_the_pointer_only_while_a_surface_of_its_own_is_up() {
        assert_eq!(
            PagePlacement::Layout.pointer_order(false, false, PresentationLayer::Current),
            Some(SiblingOrder::Back)
        );
        assert_eq!(
            PagePlacement::Layout.pointer_order(true, false, PresentationLayer::Current),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            PagePlacement::Modal.pointer_order(false, false, PresentationLayer::Current),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            PagePlacement::Pane.pointer_order(false, false, PresentationLayer::Current),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            PagePlacement::Pane.pointer_order(false, true, PresentationLayer::Current),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            PagePlacement::Pane.pointer_order(true, false, PresentationLayer::Current),
            Some(SiblingOrder::Back)
        );
        assert!(PagePlacement::Layout.paints_in_front());
    }

    #[test]
    fn tab_transition_keeps_the_retained_surface_in_front() {
        assert_eq!(
            PagePlacement::Pane.pointer_order(false, true, PresentationLayer::Retained),
            Some(SiblingOrder::Front)
        );
        assert_eq!(
            PagePlacement::Pane.pointer_order(false, true, PresentationLayer::Incoming),
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
