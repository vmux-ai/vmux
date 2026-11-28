use std::collections::HashSet;

use bevy_app::{App, Plugin, PostUpdate, PreUpdate, Update};
#[cfg(all(feature = "native", target_os = "macos"))]
use bevy_ecs::prelude::NonSendMut;
use bevy_ecs::prelude::{
    Add, Changed, ChildOf, Commands, Component, Entity, EntityEvent, Has, IntoScheduleConfigs,
    MessageWriter, On, Query, Remove, Res, ResMut, Resource, SystemSet, With, Without,
};
use bevy_ecs::system::SystemParam;
use bevy_window::{PrimaryWindow, Window};
use vmux_api::UiEventPermissions;
#[cfg(all(feature = "native", target_os = "macos"))]
use vmux_ecs::UiPageReady;
use vmux_ecs::{
    FilePath, PageMetadata, UiEmitEvent, UiIpcBlocked, UiIpcEpoch, UiIpcGeneration,
    UiIpcNavigationRevision, UiIpcPlugin, UiIpcReady, UiReceiveRaw, UiStateFlushSet,
};
use vmux_flex::{ComputedNode, FlexPlugin, LayoutSystems, WindowViewportPlugin};

use crate::Page;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageHostSet {
    Drain,
    Manage,
    Present,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageHostMode {
    #[default]
    Auto,
    Headless,
}

#[derive(Default)]
pub struct PageHostPlugin {
    mode: PageHostMode,
}

impl PageHostPlugin {
    pub fn headless() -> Self {
        Self {
            mode: PageHostMode::Headless,
        }
    }
}

impl Plugin for PageHostPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<UiIpcPlugin>() {
            app.add_plugins(UiIpcPlugin);
        }
        if !app.is_plugin_added::<FlexPlugin>() {
            app.add_plugins(FlexPlugin);
        }
        if !app.is_plugin_added::<WindowViewportPlugin>() {
            app.add_plugins(WindowViewportPlugin);
        }
        let native_enabled =
            self.mode == PageHostMode::Auto && cfg!(all(feature = "native", target_os = "macos"));
        app.insert_resource(BackendMode(native_enabled))
            .init_resource::<RegisteredPermissions>()
            .configure_sets(PreUpdate, PageHostSet::Drain)
            .configure_sets(Update, PageHostSet::Manage)
            .configure_sets(
                PostUpdate,
                PageHostSet::Present
                    .after(LayoutSystems::Layout)
                    .after(UiStateFlushSet),
            )
            .add_systems(PreUpdate, drain_events.in_set(PageHostSet::Drain))
            .add_systems(
                Update,
                (ensure_metadata, sync_permissions, mount, unmount_retired)
                    .chain()
                    .in_set(PageHostSet::Manage),
            )
            .add_systems(
                PostUpdate,
                (sync_bounds, sync_appearance, render)
                    .chain()
                    .in_set(PageHostSet::Present),
            )
            .add_observer(deliver)
            .add_observer(evaluate_script)
            .add_observer(focus)
            .add_observer(block_interaction)
            .add_observer(restore_interaction);
        #[cfg(all(feature = "native", target_os = "macos"))]
        app.init_non_send::<crate::host_native::NativeRenderer>();
    }
}

#[derive(Resource, Clone, Copy)]
struct BackendMode(bool);

#[derive(Resource, Default)]
struct RegisteredPermissions(HashSet<usize>);

#[derive(Component, Clone, Copy)]
#[require(
    UiIpcGeneration,
    UiIpcNavigationRevision,
    UiIpcEpoch,
    PageLifecycle,
    PageAppearance,
    PageBounds
)]
pub struct HostedPage {
    page: &'static Page,
}

impl HostedPage {
    pub const fn new(page: &'static Page) -> Self {
        Self { page }
    }

    pub const fn page(&self) -> &'static Page {
        self.page
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PageStylesheet(pub String);

#[derive(SystemParam)]
struct TargetWindows<'w, 's> {
    windows: Query<'w, 's, &'static Window>,
    parents: Query<'w, 's, &'static ChildOf>,
    primary: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

impl TargetWindows<'_, '_> {
    fn target(&self, child_of: Option<&ChildOf>, is_root: bool) -> Option<Entity> {
        let mut current = child_of.map(ChildOf::parent);
        while let Some(candidate) = current {
            if self.windows.contains(candidate) {
                return Some(candidate);
            }
            current = self.parents.get(candidate).ok().map(ChildOf::parent);
        }
        if is_root {
            return self.primary.iter().next();
        }
        None
    }

    fn get(&self, entity: Entity) -> Option<&Window> {
        self.windows.get(entity).ok()
    }

    #[cfg(all(feature = "native", target_os = "macos"))]
    fn contains(&self, entity: Entity) -> bool {
        self.windows.contains(entity)
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRoot;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PageBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl PageBounds {
    fn from_window(window: &Window) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: window.resolution.width().max(1.0),
            height: window.resolution.height().max(1.0),
        }
    }

    fn from_computed(node: &ComputedNode) -> Self {
        let logical = node.to_logical();
        Self {
            left: logical.min().x,
            top: logical.min().y,
            width: logical.size.x.max(1.0),
            height: logical.size.y.max(1.0),
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageAppearance {
    Light,
    #[default]
    Dark,
    System,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PageLifecycle {
    pub mounted: bool,
    pub mount_generation: u64,
    pub navigation_revision: u64,
    pub frames_applied: u64,
    pub ui_events: u64,
    pub host_events: u64,
    pub renders: u64,
    pub scripts_evaluated: u64,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct PageScriptRequest {
    #[event_target]
    pub page: Entity,
    pub script: String,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct PageFocusRequest {
    #[event_target]
    pub page: Entity,
}

fn ensure_metadata(
    pages: Query<(Entity, &HostedPage), Without<PageMetadata>>,
    mut commands: Commands,
) {
    for (entity, hosted) in &pages {
        let page = hosted.page();
        commands.entity(entity).try_insert(PageMetadata {
            url: page.url.to_string(),
            title: page.title.to_string(),
            ..Default::default()
        });
    }
}

fn sync_permissions(
    pages: Query<&HostedPage>,
    mut registered: ResMut<RegisteredPermissions>,
    mut commands: Commands,
) {
    for hosted in &pages {
        let page = hosted.page();
        let key = std::ptr::from_ref(page) as usize;
        if !registered.0.insert(key) {
            continue;
        }
        commands.spawn(page.page_permissions());
        if page.document_url() != page.url {
            commands.spawn(UiEventPermissions {
                url: page.document_url(),
                owns_subtree: page.owns_subtree,
                permissions: page.permissions,
            });
        }
    }
}

type UnmountedPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static HostedPage,
        Option<&'static ChildOf>,
        Option<&'static ComputedNode>,
        Option<&'static PageStylesheet>,
        Option<&'static FilePath>,
        &'static PageAppearance,
        &'static mut UiIpcGeneration,
        &'static mut UiIpcNavigationRevision,
        &'static mut UiIpcEpoch,
        &'static mut PageBounds,
        &'static mut PageLifecycle,
        Has<PageRoot>,
        Has<UiIpcBlocked>,
    ),
    Without<UiIpcReady>,
>;

#[allow(clippy::too_many_arguments)]
fn mount(
    mut pages: UnmountedPages,
    windows: TargetWindows,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] proxy: Option<
        Res<bevy_winit::EventLoopProxyWrapper>,
    >,
    #[cfg(all(feature = "native", target_os = "macos"))] mut renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
    mut commands: Commands,
) {
    let native = mode.0;
    for (
        entity,
        hosted,
        child_of,
        computed,
        stylesheet,
        asset_dir,
        appearance,
        mut generation,
        mut revision,
        mut epoch,
        mut bounds,
        mut lifecycle,
        is_root,
        blocked,
    ) in &mut pages
    {
        let target_window = windows.target(child_of, is_root);
        let resolved_bounds = computed
            .filter(|node| !node.is_empty())
            .map(PageBounds::from_computed)
            .or_else(|| {
                target_window
                    .and_then(|window| windows.get(window))
                    .map(PageBounds::from_window)
            })
            .unwrap_or(PageBounds {
                left: 0.0,
                top: 0.0,
                width: 800.0,
                height: 600.0,
            });
        let next_generation = generation.0.wrapping_add(1).max(1);
        #[cfg(all(feature = "native", target_os = "macos"))]
        if native {
            if renderer.contains(entity) {
                continue;
            }
            let Some(window_entity) = target_window else {
                continue;
            };
            let event_proxy = proxy.as_ref().map(|wrapper| (***wrapper).clone());
            match renderer.mount(
                entity,
                window_entity,
                hosted.page(),
                resolved_bounds,
                *appearance,
                !blocked,
                next_generation,
                stylesheet.map(|sheet| sheet.0.clone()),
                asset_dir.map(|path| path.as_path().to_path_buf()),
                event_proxy,
            ) {
                Ok((nav_rev, ipc_epoch)) => {
                    generation.0 = next_generation;
                    revision.0 = nav_rev;
                    epoch.0 = ipc_epoch;
                    *bounds = resolved_bounds;
                    lifecycle.mounted = true;
                    lifecycle.mount_generation = next_generation;
                    lifecycle.navigation_revision = nav_rev;
                    commands.entity(entity).try_insert(UiIpcReady);
                }
                Err(error) => {
                    tracing::debug!("vmux_page: deferring mount for {entity:?}: {error}");
                }
            }
            continue;
        }
        let _ = (hosted, stylesheet, asset_dir, appearance, blocked, native);
        generation.0 = next_generation;
        revision.0 = revision.0.max(1);
        epoch.0 = epoch.0.max(1);
        *bounds = resolved_bounds;
        lifecycle.mounted = true;
        lifecycle.mount_generation = next_generation;
        lifecycle.navigation_revision = revision.0;
        commands.entity(entity).try_insert(UiIpcReady);
    }
}

#[cfg(all(feature = "native", target_os = "macos"))]
fn unmount_retired(
    live: Query<(Option<&ChildOf>, Has<PageRoot>), With<HostedPage>>,
    windows: TargetWindows,
    mode: Res<BackendMode>,
    mut renderer: NonSendMut<crate::host_native::NativeRenderer>,
    mut commands: Commands,
) {
    if !mode.0 {
        return;
    }
    for entity in renderer.mounted_entities() {
        let should_unmount = match live.get(entity) {
            Err(_) => true,
            Ok((child_of, is_root)) => {
                let target = windows.target(child_of, is_root);
                let mounted_window = renderer.window_of(entity);
                let window_missing = mounted_window.is_some_and(|window| !windows.contains(window));
                let target_changed = target.is_none() || mounted_window != target;
                window_missing || target_changed
            }
        };
        if should_unmount
            && renderer.unmount(entity)
            && let Ok(mut entity_commands) = commands.get_entity(entity)
        {
            entity_commands.try_remove::<UiIpcReady>();
        }
    }
}

#[cfg(not(all(feature = "native", target_os = "macos")))]
fn unmount_retired(_mode: Res<BackendMode>) {}

type BoundsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static ChildOf>,
        Option<&'static ComputedNode>,
        Has<PageRoot>,
        &'static mut PageBounds,
    ),
    (With<HostedPage>, With<UiIpcReady>),
>;

fn sync_bounds(
    mut pages: BoundsQuery,
    windows: TargetWindows,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] mut renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    for (entity, child_of, computed, is_root, mut bounds) in &mut pages {
        let next = if let Some(node) = computed.filter(|node| !node.is_empty()) {
            PageBounds::from_computed(node)
        } else {
            let Some(window) = windows
                .target(child_of, is_root)
                .and_then(|id| windows.get(id))
            else {
                continue;
            };
            PageBounds::from_window(window)
        };
        if *bounds != next {
            *bounds = next;
            #[cfg(all(feature = "native", target_os = "macos"))]
            if mode.0 {
                renderer.set_bounds(entity, next);
            }
            let _ = (entity, &mode);
        }
    }
}

type ChangedAppearanceQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static PageAppearance),
    (With<HostedPage>, With<UiIpcReady>, Changed<PageAppearance>),
>;

fn sync_appearance(
    pages: ChangedAppearanceQuery,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] mut renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    for (entity, appearance) in &pages {
        #[cfg(all(feature = "native", target_os = "macos"))]
        if mode.0 {
            renderer.set_appearance(entity, *appearance);
        }
        let _ = (entity, appearance, &mode);
    }
}

fn render(
    mut lifecycles: Query<&mut PageLifecycle, (With<HostedPage>, With<UiIpcReady>)>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    #[cfg(all(feature = "native", target_os = "macos"))]
    if mode.0 {
        renderer.render_all();
    }
    let _ = &mode;
    for mut lifecycle in &mut lifecycles {
        lifecycle.renders = lifecycle.renders.saturating_add(1);
    }
}

#[cfg(all(feature = "native", target_os = "macos"))]
fn drain_events(
    mode: Res<BackendMode>,
    renderer: NonSendMut<crate::host_native::NativeRenderer>,
    mut lifecycles: Query<(&mut PageLifecycle, &mut UiIpcNavigationRevision)>,
    mut raw_events: MessageWriter<UiReceiveRaw>,
    mut commands: Commands,
) {
    if !mode.0 {
        return;
    }
    for event in renderer.drain() {
        match event.update {
            crate::host_native::NativeUpdate::UiEvent(raw) => {
                if renderer.generation_of(event.page) != Some(raw.generation) {
                    continue;
                }
                if let Ok((mut lifecycle, _)) = lifecycles.get_mut(event.page) {
                    lifecycle.ui_events = lifecycle.ui_events.saturating_add(1);
                }
                raw_events.write(raw);
            }
            crate::host_native::NativeUpdate::Lifecycle(update) => {
                let Ok((mut lifecycle, mut revision)) = lifecycles.get_mut(event.page) else {
                    continue;
                };
                match update {
                    crate::LifecycleUpdate::Ready(rev) => {
                        revision.0 = rev.max(1);
                        lifecycle.navigation_revision = revision.0;
                        commands.trigger(UiPageReady { page: event.page });
                    }
                    crate::LifecycleUpdate::Navigated(rev) => {
                        revision.0 = rev.max(1);
                        lifecycle.navigation_revision = revision.0;
                    }
                    crate::LifecycleUpdate::FrameApplied(_) => {
                        lifecycle.frames_applied = lifecycle.frames_applied.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(not(all(feature = "native", target_os = "macos")))]
fn drain_events(
    _mode: Res<BackendMode>,
    _raw_events: MessageWriter<UiReceiveRaw>,
    _commands: Commands,
) {
}

type DeliverablePagesQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static UiIpcGeneration,
        &'static UiIpcNavigationRevision,
        &'static UiIpcEpoch,
        &'static PageMetadata,
        &'static mut PageLifecycle,
    ),
    (With<HostedPage>, With<UiIpcReady>),
>;

fn deliver(
    trigger: On<UiEmitEvent>,
    mut pages: DeliverablePagesQuery,
    blocked: Query<(), With<UiIpcBlocked>>,
    permissions: Query<&UiEventPermissions>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    let event = trigger.event();
    let entity = event.target;
    if blocked.contains(entity) {
        return;
    }
    let Ok((generation, revision, epoch, metadata, mut lifecycle)) = pages.get_mut(entity) else {
        return;
    };
    if !event.matches_authority(generation.0, revision.0, epoch.0) {
        return;
    }
    if !UiEventPermissions::allows_page(permissions.iter(), &metadata.url, event.permission) {
        return;
    }
    let delivered = {
        #[cfg(all(feature = "native", target_os = "macos"))]
        if mode.0 {
            renderer.deliver(entity, event.id, &event.payload)
        } else {
            true
        }
        #[cfg(not(all(feature = "native", target_os = "macos")))]
        {
            let _ = &mode;
            true
        }
    };
    if delivered {
        lifecycle.host_events = lifecycle.host_events.saturating_add(1);
        event.receipt.mark_delivered();
    }
}

fn evaluate_script(
    trigger: On<PageScriptRequest>,
    mut lifecycles: Query<&mut PageLifecycle, (With<HostedPage>, With<UiIpcReady>)>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    let request = trigger.event();
    let Ok(mut lifecycle) = lifecycles.get_mut(request.page) else {
        return;
    };
    let evaluated = {
        #[cfg(all(feature = "native", target_os = "macos"))]
        if mode.0 {
            renderer.evaluate_script(request.page, &request.script)
        } else {
            true
        }
        #[cfg(not(all(feature = "native", target_os = "macos")))]
        {
            let _ = &mode;
            true
        }
    };
    if evaluated {
        lifecycle.scripts_evaluated = lifecycle.scripts_evaluated.saturating_add(1);
    }
}

fn focus(
    trigger: On<PageFocusRequest>,
    pages: Query<(), (With<HostedPage>, With<UiIpcReady>)>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    let entity = trigger.event().page;
    if !pages.contains(entity) {
        return;
    }
    #[cfg(all(feature = "native", target_os = "macos"))]
    if mode.0 {
        renderer.focus(entity);
    }
    let _ = (entity, &mode);
}

fn block_interaction(
    trigger: On<Add, UiIpcBlocked>,
    mut epochs: Query<&mut UiIpcEpoch, With<HostedPage>>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] mut renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    let entity = trigger.event_target();
    let Ok(mut epoch) = epochs.get_mut(entity) else {
        return;
    };
    #[cfg(all(feature = "native", target_os = "macos"))]
    if mode.0 {
        if let Some(next) = renderer.set_interactive(entity, false) {
            epoch.0 = next;
        }
        return;
    }
    let _ = &mode;
    epoch.0 = epoch.0.wrapping_add(1).max(1);
}

fn restore_interaction(
    trigger: On<Remove, UiIpcBlocked>,
    mut epochs: Query<&mut UiIpcEpoch, With<HostedPage>>,
    mode: Res<BackendMode>,
    #[cfg(all(feature = "native", target_os = "macos"))] mut renderer: NonSendMut<
        crate::host_native::NativeRenderer,
    >,
) {
    let entity = trigger.event_target();
    let Ok(mut epoch) = epochs.get_mut(entity) else {
        return;
    };
    #[cfg(all(feature = "native", target_os = "macos"))]
    if mode.0 {
        if let Some(next) = renderer.set_interactive(entity, true) {
            epoch.0 = next;
        }
        return;
    }
    let _ = &mode;
    epoch.0 = epoch.0.wrapping_add(1).max(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::*;
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ecs::{UiStatePlugin, UiStateWrite};

    #[vmux_api::ui_state(Default, Eq)]
    struct CounterUiState {
        count: i32,
    }

    #[derive(Resource, Default)]
    struct CapturedEmits(Vec<(&'static str, Vec<u8>)>);

    fn capture_emit(trigger: On<UiEmitEvent>, mut captured: ResMut<CapturedEmits>) {
        captured
            .0
            .push((trigger.event().id, trigger.event().payload.to_vec()));
    }

    #[component]
    fn CounterPageView() -> Element {
        rsx! { div { "counter" } }
    }

    static COUNTER_PAGE: Page = Page {
        permissions: &[CounterUiState::PERMISSION],
        ..Page::pane("app://counter/", CounterPageView).titled("Counter")
    };

    #[test]
    fn headless_page_mounts_syncs_window_bounds_delivers_ui_state_and_tears_down() {
        let mut app = App::new();
        app.add_plugins((
            PageHostPlugin::headless(),
            UiStatePlugin::<CounterUiState>::default(),
        ))
        .init_resource::<CapturedEmits>()
        .add_observer(capture_emit);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (900, 640).into(),
                    ..Default::default()
                },
                PrimaryWindow,
            ))
            .id();
        let page = app
            .world_mut()
            .spawn((
                HostedPage::new(&COUNTER_PAGE),
                PageRoot,
                PageStylesheet("body{margin:0}".into()),
                ChildOf(window),
            ))
            .id();
        app.update();

        assert!(app.world().get::<UiIpcReady>(page).is_some());
        let bounds = *app.world().get::<PageBounds>(page).unwrap();
        assert_eq!(bounds.width, 900.0);
        assert_eq!(bounds.height, 640.0);

        app.world_mut()
            .trigger(UiStateWrite::<CounterUiState>::from_event(
                page,
                &CounterUiState { count: 12 },
            ));
        app.update();

        let captured = app.world().resource::<CapturedEmits>();
        assert_eq!(captured.0.len(), 1);
        assert_eq!(captured.0[0].0, CounterUiState::id());
        let frame = rkyv::from_bytes::<
            UiStateFrame<CounterUiState, CounterUiState>,
            rkyv::rancor::Error,
        >(&captured.0[0].1)
        .unwrap();
        assert_eq!(frame, UiStateFrame::Snapshot(CounterUiState { count: 12 }));
        assert_eq!(
            app.world().get::<PageLifecycle>(page).unwrap().host_events,
            1
        );

        app.world_mut().trigger(PageScriptRequest {
            page,
            script: "document.body.dataset.ok = '1'".into(),
        });
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .get::<PageLifecycle>(page)
                .unwrap()
                .scripts_evaluated,
            1
        );

        app.world_mut().entity_mut(page).despawn();
        app.update();
        assert!(app.world().get_entity(page).is_err());
    }

    #[test]
    fn blocking_page_advances_epoch_and_defers_ui_state_until_unblocked() {
        let mut app = App::new();
        app.add_plugins((
            PageHostPlugin::headless(),
            UiStatePlugin::<CounterUiState>::default(),
        ));
        let page = app
            .world_mut()
            .spawn((HostedPage::new(&COUNTER_PAGE), UiIpcBlocked))
            .id();
        app.update();
        let epoch_before = app.world().get::<UiIpcEpoch>(page).unwrap().0;

        app.world_mut()
            .trigger(UiStateWrite::<CounterUiState>::from_event(
                page,
                &CounterUiState { count: 3 },
            ));
        app.update();
        assert_eq!(
            app.world().get::<PageLifecycle>(page).unwrap().host_events,
            0
        );

        app.world_mut().entity_mut(page).remove::<UiIpcBlocked>();
        app.update();
        let epoch_after = app.world().get::<UiIpcEpoch>(page).unwrap().0;
        assert!(epoch_after > epoch_before);
        assert_eq!(
            app.world().get::<PageLifecycle>(page).unwrap().host_events,
            1
        );
    }

    #[test]
    fn non_root_child_page_without_computed_node_tracks_window_resize() {
        let mut app = App::new();
        app.add_plugins(PageHostPlugin::headless());
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: bevy_window::WindowResolution::new(800, 600),
                ..Default::default()
            })
            .id();
        let page = app
            .world_mut()
            .spawn((HostedPage::new(&COUNTER_PAGE), ChildOf(window)))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<PageBounds>(page).unwrap(),
            PageBounds {
                left: 0.0,
                top: 0.0,
                width: 800.0,
                height: 600.0,
            }
        );

        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution = bevy_window::WindowResolution::new(1024, 768);
        app.update();
        assert_eq!(
            *app.world().get::<PageBounds>(page).unwrap(),
            PageBounds {
                left: 0.0,
                top: 0.0,
                width: 1024.0,
                height: 768.0,
            }
        );
    }
}
