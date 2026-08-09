//! Browser and page-open orchestration on top of `bevy_cef`: page resolution, CEF
//! backend management, and input forwarding between the native layout and embedded pages.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod extensions;
mod host_focus;
#[cfg(target_os = "macos")]
mod host_focus_native;
mod scroll;
mod snapshot;
pub use host_focus::HostFocusIntent;
mod remote;

use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    input::{
        ButtonState, InputSystems,
        keyboard::KeyboardInput,
        mouse::{MouseButton, MouseButtonInput, MouseWheel},
    },
    material::AlphaMode,
    picking::pointer::PointerButton,
    prelude::*,
    ui::{UiGlobalTransform, UiSystems},
    window::{CursorMoved, PrimaryWindow, WindowResized},
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{
    CefEmbeddedHosts, CommandLineConfig, RenderTextureMessage, webview_debug_log,
};
#[cfg(target_os = "macos")]
use bevy_cef_core::prelude::{NativeMouseButtons, NativeMouseMovePresenter};
#[cfg(target_os = "macos")]
use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, BrowserNavigationCommand, BrowserViewCommand,
    LayoutCommand, ReadAppCommands, StackCommand, WriteAppCommands, event::CommandBarActionEvent,
    open::OpenCommand,
};
use vmux_core::{
    CefPageAttachRequest, HostSpawnRegistry, OscTitle, PageMetadata, PageOpenError,
    PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
    page::{PageManifest, PageReady},
};
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
use vmux_layout::command_bar::handler::{CommandBarNativeSize, PendingCommandBarReveal};
use vmux_layout::command_bar::state::{CommandBarState, CommandBarStateQuery};
use vmux_layout::event::{RemoteCommandEvent, RemoteCopyEvent, SideSheetCommandEvent};
pub use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, PendingWebviewReveal, UpdateState,
    bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive},
    command_bar::panel::CommandBarPanelActive,
    event::{
        DebugSimulateDownload, DebugUpdateClear, DebugUpdateReady, HEADER_HEIGHT_PX,
        HeaderCommandEvent, LAYOUT_COMMAND_BAR_OPEN_EVENT, LAYOUT_STATE_EVENT, LayoutStateEvent,
        PANE_TREE_EVENT, PaneNode, PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT,
        StackNode, StackRow, StacksHostEvent, TAB_BOUNDARY_EVENT, TABS_EVENT, TabBoundary,
        TabBoundaryEvent, TabRow, TabsHostEvent, UPDATE_CLEARED_EVENT, UPDATE_PROGRESS_EVENT,
        UPDATE_READY_EVENT, UpdateClearedEvent, UpdateProgressEvent, UpdateReadyEvent,
    },
    pane::{Pane, PaneHoverIntent, PaneSplit, SideSheetCardCollapsed, first_stack_in_pane},
    side_sheet::{
        SideSheet, SideSheetPaneExpanded, SideSheetPosition, SideSheetSectionsExpanded,
        SideSheetWidth,
    },
    stack::{
        ActiveTabParam, Stack, active_stack_in_pane, collect_leaf_panes, focused_stack,
        stack_bundle,
    },
    tab::{Tab, TabWorktree},
    window::{
        Modal, VmuxWindow, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN, WEBVIEW_Z_MODAL, WEBVIEW_Z_SIDE_SHEET,
    },
};

use vmux_setting::AppSettings;
use vmux_terminal::{self as terminal, RestartPty, Terminal};
use vmux_ui::theme::{THEME_EVENT, ThemeEvent};

/// Wires browser orchestration: resolves CEF embedded hosts from page manifests, manages
/// the CEF backend, and forwards pointer and cursor input between the layout and pages.
pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let profile = vmux_core::profile::active_profile_name();
        let startup_settings = vmux_setting::read_settings_from_disk();
        let startup_locale =
            vmux_ui::i18n::requested_locale(Some(&startup_settings.appearance.locale));
        let startup_accept_language_list = browser_accept_language_list(&startup_locale);
        let prepared_extensions = crate::extensions::load::apply_env().unwrap_or_else(|error| {
            bevy::log::error!(%error, "failed to prepare extensions; starting without them");
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
            Vec::new()
        });
        let conformance_extension = std::env::var("VMUX_EXTENSION_CONFORMANCE_ID").ok();
        let extension_registrations = prepared_extensions
            .iter()
            .map(|runtime| crate::extensions::bridge::BridgeRegistration {
                extension_id: runtime.extension_id.clone(),
                authorization: crate::extensions::bridge::BridgeAuthorization {
                    permissions: runtime.granted_permissions.iter().cloned().collect(),
                    host_permissions: runtime
                        .granted_host_permissions
                        .iter()
                        .map(|pattern| {
                            vmux_core::extension::match_pattern::ChromeMatchPattern::parse(pattern)
                                .unwrap_or_else(|error| {
                                    panic!("invalid stored host permission: {error}")
                                })
                        })
                        .collect(),
                    conformance: conformance_extension.as_deref()
                        == Some(runtime.extension_id.as_str()),
                },
            })
            .collect::<Vec<_>>();
        let extension_bridge = crate::extensions::bridge::ExtensionBridgeServer::start_registered(
            &profile,
            extension_registrations,
        )
        .unwrap_or_else(|error| panic!("failed to start extension bridge: {error}"));
        if crate::extensions::broker::extension_conformance_enabled() {
            app.init_resource::<crate::extensions::broker::ConformanceWakeTimer>();
        }
        app.add_plugins(extensions::ExtensionsPlugin);
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        webview_debug_log(format!("BrowserPlugin embedded_hosts={embedded_hosts:?}"));
        let cef_command_line = cef_command_line_config();
        configure_cef_backend_sync(app)
            .insert_resource(crate::extensions::load::PreparedExtensions(
                prepared_extensions,
            ))
            .insert_resource(extension_bridge)
            .init_resource::<crate::extensions::bridge_page::ExtensionBridgeLifecycle>()
            .init_resource::<crate::extensions::bridge_page::ExtensionInfrastructureEntities>()
            .init_resource::<crate::extensions::broker::BridgeSubscriptions>()
            .init_resource::<crate::extensions::broker::BridgeResponseCache>()
            .init_resource::<crate::extensions::broker::PendingBridgeEvents>()
            .init_resource::<crate::extensions::model::ChromeModel>()
            .init_resource::<crate::extensions::model::ChromeStableIds>()
            .init_resource::<crate::extensions::windows::ExtensionWindows>()
            .add_message::<crate::extensions::model::ChromeModelEvent>()
            .add_message::<crate::extensions::windows::CloseExtensionWindowRequest>()
            .add_message::<crate::extensions::windows::UpdateHostWindowRequest>()
            .add_systems(
                Update,
                (
                    crate::extensions::bridge_page::stop_extension_bridge_pages,
                    crate::extensions::bridge_page::spawn_extension_bridge_pages,
                )
                    .chain()
                    .before(CefSystems::CreateAndResize),
            )
            .add_systems(
                Update,
                crate::extensions::broker::drain_bridge_requests
                    .after(crate::extensions::windows::sync_extension_windows),
            )
            .add_systems(
                Update,
                crate::extensions::windows::sync_extension_windows
                    .after(crate::extensions::project::rebuild_chrome_model),
            )
            .add_systems(
                Update,
                (
                    crate::extensions::windows::route_close_extension_windows,
                    crate::extensions::windows::apply_host_window_updates,
                )
                    .after(crate::extensions::broker::drain_bridge_requests),
            )
            .add_systems(
                Update,
                crate::extensions::project::rebuild_chrome_model
                    .after(vmux_layout::apply_cef_state_from_webview)
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                crate::extensions::broker::forward_chrome_model_events
                    .after(crate::extensions::project::rebuild_chrome_model),
            )
            .add_systems(
                Update,
                crate::extensions::broker::fire_conformance_wake_timer,
            )
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_message::<PageOpenRequest>()
            .add_message::<CefPageAttachRequest>()
            .add_message::<vmux_layout::OpenBesideRequest>()
            .configure_sets(Update, CefSystems::CreateAndResize.after(ReadAppCommands))
            .configure_sets(
                Update,
                (
                    PageOpenSet::ResolveTarget,
                    PageOpenSet::HandleKnownPages,
                    PageOpenSet::Fallback,
                    PageOpenSet::Respond,
                )
                    .chain()
                    .after(ReadAppCommands),
            )
            .add_plugins(
                (
                    CefPlugin {
                        command_line_config: cef_command_line,
                        root_cache_path: cef_root_cache_path(),
                        locale: startup_locale,
                        accept_language_list: startup_accept_language_list,
                        embedded_hosts,
                        ..default()
                    },
                    remote::RemoteDesktopPlugin,
                    BinEventEmitterPlugin::<(
                        HeaderCommandEvent,
                        SideSheetCommandEvent,
                        RemoteCommandEvent,
                        RemoteCopyEvent,
                    )>::for_hosts(&["layout"]),
                    BinEventEmitterPlugin::<(
                        DebugUpdateReady,
                        DebugUpdateClear,
                        DebugSimulateDownload,
                    )>::for_hosts(&["debug"]),
                ),
            )
            .add_observer(on_webview_ready_send_theme)
            .add_observer(on_header_command_emit)
            .add_observer(on_side_sheet_command_emit)
            .add_observer(request_layout_frame_burst)
            .add_observer(on_reload_notify_header)
            .add_observer(on_hard_reload_notify_header)
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear)
            .add_systems(
                Update,
                sync_appearance_to_cef
                    .before(CefSystems::CreateAndResize)
                    .run_if(resource_changed::<AppSettings>),
            )
            .add_systems(Update, sync_layout_mesh_visibility)
            .add_systems(
                PreUpdate,
                (
                    sync_layout_cef_pointer_target,
                    dismiss_command_bar_from_native_monitor,
                    dismiss_windowed_command_bar_on_outside_click
                        .run_if(on_message::<MouseButtonInput>),
                    forward_layout_cef_cursor_move.run_if(on_message::<CursorMoved>),
                    forward_layout_cef_mouse_button.run_if(on_message::<MouseButtonInput>),
                )
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(
                Update,
                (
                    handle_browser_commands.in_set(ReadAppCommands),
                    vmux_layout::apply_cef_state_from_webview,
                    apply_page_icons.after(vmux_layout::apply_cef_state_from_webview),
                    drain_loading_state,
                    drain_committed_navigation,
                    spawn_popup_stacks,
                    handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
                    attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                    handle_unclaimed_page_open_tasks.in_set(PageOpenSet::Fallback),
                    respond_page_open_tasks.in_set(PageOpenSet::Respond),
                    handle_browser_navigate_requests.after(vmux_terminal::ServiceMessageSet),
                    handle_browser_go_back_requests,
                    handle_browser_go_forward_requests,
                    handle_open_in_new_stack_requests,
                    handle_browser_open_history.in_set(ReadAppCommands),
                ),
            )
            .add_systems(
                Update,
                (sync_page_metadata_to_tab, spawn_visit_on_navigation)
                    .chain()
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .add_systems(
                Update,
                vmux_layout::mirror_metadata_to_url
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .add_systems(
                Update,
                (
                    push_layout_state_emit,
                    push_stacks_host_emit,
                    push_pane_tree_emit,
                    push_tabs_host_emit,
                    push_bookmarks_host_emit,
                    push_update_notice_emit,
                    push_tab_boundary_emit,
                )
                    .after(vmux_layout::apply_cef_state_from_webview)
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_keyboard_target,
                    sync_windowed_content_mesh_materials,
                    sync_modal_mesh_visibility,
                    sync_children_to_ui,
                    sync_windowed_layout,
                    sync_windowed_frames,
                    sync_windowed_command_bar,
                    flush_native_command_bar_pointer_events,
                    apply_repaint_nudge,
                    sync_cef_webview_resize_after_ui,
                    sync_webview_pane_corner_clip,
                    sync_osr_webview_focus,
                    flush_pending_osr_textures,
                )
                    .chain()
                    .after(UiSystems::Layout)
                    .before(render_standard_materials),
            )
            .add_systems(
                Last,
                (
                    refresh_layout_cef_hover,
                    refresh_active_windowed_hover,
                    sync_layout_cef_frame_rate,
                )
                    .chain(),
            )
            .init_resource::<HostFocusIntent>()
            .init_resource::<LayoutFrameRateBurst>()
            .init_resource::<PendingNavSnapshots>()
            .init_resource::<RecentBrowserInteraction>()
            .init_resource::<HostSpawnRegistry>()
            .add_systems(
                PreUpdate,
                log_command_bar_keyboard_input.after(bevy_cef::prelude::CefKeyboardInputSet),
            )
            .add_systems(Update, track_browser_interaction)
            .add_systems(
                PostUpdate,
                (
                    host_focus::compute_host_focus_intent,
                    host_focus::apply_windowed_host_focus,
                )
                    .chain()
                    // Must run after the active windowed page is shown + raised, otherwise
                    // set_focus lands on a hidden/back view and never sticks.
                    .after(sync_windowed_frames)
                    .after(sync_windowed_command_bar),
            );

        app.add_systems(
            Update,
            (
                snapshot::drive_pending_nav_snapshots,
                scroll::run_scrolls,
                snapshot::start_snapshots,
                snapshot::shape_snapshot_results,
            )
                .chain()
                .after(WriteAppCommands),
        );

        #[cfg(target_os = "macos")]
        app.add_systems(Last, host_focus_native::apply_winit_host_focus);
    }
}

#[derive(Clone, Copy, Debug, Message)]
pub(crate) struct WebviewLoadCompleted {
    webview: Entity,
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
enum BrowserSystems {
    SyncCefBackend,
}

fn configure_cef_backend_sync(app: &mut App) -> &mut App {
    app.configure_sets(
        Update,
        BrowserSystems::SyncCefBackend
            .after(vmux_layout::scene::SceneSystems::CompleteModeTransition)
            .before(CefSystems::CreateAndResize),
    )
    .add_systems(
        Update,
        sync_cef_backend_for_interaction_mode
            .in_set(BrowserSystems::SyncCefBackend)
            .after(PageOpenSet::Fallback)
            .after(spawn_popup_stacks),
    )
}

fn log_command_bar_keyboard_input(
    mut events: MessageReader<KeyboardInput>,
    modal_q: CommandBarStateQuery,
) {
    if !vmux_layout::command_bar::handler::is_command_bar_open(&modal_q) {
        return;
    }
    for event in events.read() {
        if event.state == ButtonState::Pressed {
            bevy::log::info!(key = ?event.key_code, repeat = event.repeat, "command bar keyboard received");
        }
    }
}

fn cef_command_line_config() -> CommandLineConfig {
    CommandLineConfig {
        switches: vmux_core::profile::cef_keychain_switches().to_vec(),
        switch_values: vec![("disable-features", "BackForwardCache")],
    }
}

fn on_webview_ready_send_theme(
    trigger: On<BinReceive<PageReady>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    cef_q: Query<(), With<LayoutCef>>,
    modal_q: Query<(), With<Modal>>,
    mut zoom_q: Query<&mut bevy_cef::prelude::ZoomLevel>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    webview_debug_log(format!("on_webview_ready_send_theme entity={entity:?}"));
    if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
        let payload = theme_event(&settings);
        commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
    }
    // CEF / modal must never carry a stale zoom (e.g. from a previous
    // session where pinch-zoom was allowed); force them to 0 once the
    // webview is ready, both on the component and on the CEF host.
    if cef_q.get(entity).is_ok() || modal_q.get(entity).is_ok() {
        if let Ok(mut zoom) = zoom_q.get_mut(entity) {
            zoom.0 = 0.0;
        }
        browsers.set_zoom_level(&entity, 0.0);
    }
}

fn theme_event(settings: &AppSettings) -> ThemeEvent {
    let locale = vmux_ui::i18n::requested_locale(Some(&settings.appearance.locale));
    ThemeEvent {
        radius: settings.layout.radius,
        catalog: external_locale_catalog(&locale),
        locale,
    }
}

fn browser_accept_language_list(locale: &str) -> String {
    let locale = locale.trim();
    let language = locale.split('-').next().unwrap_or(locale);
    if language.eq_ignore_ascii_case("en") {
        if locale.eq_ignore_ascii_case(language) {
            "en,en-US;q=0.9".to_string()
        } else {
            format!("{locale},en;q=0.9")
        }
    } else if locale.eq_ignore_ascii_case(language) {
        format!("{locale},en-US;q=0.9,en;q=0.8")
    } else {
        format!("{locale},{language};q=0.9,en-US;q=0.8,en;q=0.7")
    }
}

fn external_locale_catalog(locale: &str) -> Option<String> {
    let directory = vmux_core::profile::config_dir().join("locales");
    [locale, locale.split('-').next().unwrap_or(locale)]
        .into_iter()
        .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
}

fn map_color_scheme(mode: vmux_setting::ColorScheme) -> bevy_cef::prelude::CefColorMode {
    match mode {
        vmux_setting::ColorScheme::Light => bevy_cef::prelude::CefColorMode::Light,
        vmux_setting::ColorScheme::Dark => bevy_cef::prelude::CefColorMode::Dark,
        vmux_setting::ColorScheme::Device => bevy_cef::prelude::CefColorMode::System,
    }
}

fn sync_appearance_to_cef(
    settings: Res<AppSettings>,
    mut scheme: ResMut<bevy_cef::prelude::CefColorScheme>,
    mut accept_language_list: Option<ResMut<bevy_cef::prelude::CefAcceptLanguageList>>,
    mut browsers: Option<NonSendMut<Browsers>>,
    ready: Query<Entity, With<PageReady>>,
    mut commands: Commands,
) {
    let mode = map_color_scheme(settings.appearance.mode);
    if scheme.0 != mode {
        scheme.0 = mode;
    }
    let locale = vmux_ui::i18n::requested_locale(Some(&settings.appearance.locale));
    let next_accept_language_list = browser_accept_language_list(&locale);
    if accept_language_list
        .as_deref()
        .is_none_or(|current| current.0 != next_accept_language_list)
    {
        if let Some(current) = accept_language_list.as_deref_mut() {
            current.0 = next_accept_language_list.clone();
        }
        if let Some(browsers) = browsers.as_deref_mut() {
            browsers.set_accept_language_list(&next_accept_language_list);
        }
    }
    let browsers = browsers.as_deref();
    let Some(browsers) = browsers else { return };
    let payload = theme_event(&settings);
    for entity in &ready {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
        }
    }
}

#[cfg(test)]
#[path = "lib.accept_language.test.rs"]
mod accept_language_tests;
type CefPointerRegionQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Header>,
        Option<&'static SideSheet>,
        &'static Node,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Option<&'static Visibility>,
        Has<Open>,
    ),
    Or<(With<Header>, With<SideSheet>)>,
>;

#[derive(Clone, Copy)]
struct CefPointerHitRect {
    center: Vec2,
    size: Vec2,
    interactive: bool,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct NativeLayoutPointerState {
    regions: Vec<CefPointerHitRect>,
    pointer_inside: bool,
    position_px: Option<Vec2>,
    buttons: NativeMouseButtons,
    pending: bool,
}

#[cfg(target_os = "macos")]
static NATIVE_LAYOUT_POINTER_STATE: LazyLock<Mutex<NativeLayoutPointerState>> =
    LazyLock::new(|| Mutex::new(NativeLayoutPointerState::default()));
static NATIVE_LAYOUT_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
static NATIVE_LAYOUT_ACTIVITY: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[derive(Default)]
struct NativeLayoutMousePresenterState {
    scale: f32,
    presenter: Option<NativeMouseMovePresenter>,
}

#[cfg(target_os = "macos")]
thread_local! {
    static NATIVE_LAYOUT_MOUSE_PRESENTER: RefCell<NativeLayoutMousePresenterState> =
        RefCell::new(NativeLayoutMousePresenterState::default());
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeLayoutPointerMoveResult {
    pub owns_pointer: bool,
    pub presenter_active: bool,
    pub region_changed: bool,
    pub pending: bool,
}

fn cef_pointer_hit_rect_contains(rect: CefPointerHitRect, point: Vec2) -> bool {
    if !rect.interactive {
        return false;
    }
    let half = rect.size * 0.5;
    let min = rect.center - half;
    let max = rect.center + half;
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

#[cfg(target_os = "macos")]
fn physical_cef_pointer_hit_rect(mut rect: CefPointerHitRect, scale: f32) -> CefPointerHitRect {
    rect.center *= scale;
    rect.size *= scale;
    rect
}

#[cfg(target_os = "macos")]
fn set_native_layout_pointer_regions(regions: impl IntoIterator<Item = CefPointerHitRect>) {
    let mut state = NATIVE_LAYOUT_POINTER_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.regions.clear();
    state.regions.extend(regions);
}

#[cfg(target_os = "macos")]
fn set_native_layout_mouse_presenter(scale: f32, presenter: Option<NativeMouseMovePresenter>) {
    NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow_mut(|state| {
        state.scale = scale;
        let same_browser = state
            .presenter
            .as_ref()
            .map(NativeMouseMovePresenter::browser_id)
            == presenter.as_ref().map(NativeMouseMovePresenter::browser_id);
        if !same_browser {
            state.presenter = presenter;
        }
    });
}

#[cfg(target_os = "macos")]
fn native_layout_mouse_presenter_active() -> bool {
    NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| state.presenter.is_some())
}

#[cfg(target_os = "macos")]
fn clear_native_layout_pointer_state() {
    let should_flush = {
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.regions.clear();
        let should_flush = state.pointer_inside && state.position_px.is_some();
        state.pointer_inside = false;
        state.pending |= should_flush;
        should_flush
    };
    NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
    NATIVE_LAYOUT_ACTIVITY.store(false, Ordering::Relaxed);
    if should_flush {
        flush_native_layout_pointer_move();
    }
    set_native_layout_mouse_presenter(1.0, None);
}

#[cfg(target_os = "macos")]
fn queue_native_layout_pointer_sample(
    state: &mut NativeLayoutPointerState,
    position: Vec2,
    buttons: NativeMouseButtons,
) -> NativeLayoutPointerMoveResult {
    let inside = state
        .regions
        .iter()
        .copied()
        .any(|rect| cef_pointer_hit_rect_contains(rect, position));
    let was_inside = state.pointer_inside;
    let sample_changed =
        state.position_px != Some(position) || state.buttons != buttons || was_inside != inside;
    state.pointer_inside = inside;
    state.position_px = Some(position);
    state.buttons = buttons;
    if (was_inside || inside) && sample_changed {
        state.pending = true;
    }
    NativeLayoutPointerMoveResult {
        owns_pointer: was_inside || inside,
        presenter_active: false,
        region_changed: was_inside != inside,
        pending: state.pending,
    }
}

#[cfg(target_os = "macos")]
pub fn queue_native_layout_pointer_move(
    x_px: f32,
    y_px: f32,
    buttons: NativeMouseButtons,
) -> NativeLayoutPointerMoveResult {
    if !x_px.is_finite() || !y_px.is_finite() {
        return NativeLayoutPointerMoveResult::default();
    }
    let (mut result, inside) = {
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let position = Vec2::new(x_px, y_px);
        let result = queue_native_layout_pointer_sample(&mut state, position, buttons);
        (result, state.pointer_inside)
    };
    NATIVE_LAYOUT_POINTER_INSIDE.store(inside, Ordering::Relaxed);
    result.presenter_active = native_layout_mouse_presenter_active();
    result
}

#[cfg(target_os = "macos")]
pub fn flush_native_layout_pointer_move() -> bool {
    let Some((position_px, buttons, inside)) = ({
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.pending {
            None
        } else {
            state.pending = false;
            state
                .position_px
                .map(|position| (position, state.buttons, state.pointer_inside))
        }
    }) else {
        return false;
    };
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send(position_px / state.scale, buttons, !inside);
        true
    });
    if !forwarded {
        NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending = true;
    }
    forwarded
}

pub fn native_layout_pointer_is_inside() -> bool {
    NATIVE_LAYOUT_POINTER_INSIDE.load(Ordering::Relaxed)
}

/// Hand a wheel event to the layout webview, reporting whether it took it.
///
/// The layout shell is offscreen-rendered into a `CALayer` above the panes, but a `CALayer` is not
/// in AppKit's hit-test chain: a native windowed page underneath wins every `scrollWheel` aimed at
/// whatever the layout is drawing on top. The monitor swallows the event when this returns true.
#[cfg(target_os = "macos")]
pub fn forward_native_layout_scroll(position_px: Vec2, delta: Vec2) -> bool {
    if !native_layout_pointer_is_inside() {
        return false;
    }
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send_wheel(position_px / state.scale, delta);
        true
    });
    if forwarded {
        NATIVE_LAYOUT_SCROLL_AT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .replace(std::time::Instant::now());
    }
    forwarded
}

/// Hand a click to the layout webview, reporting whether it took it.
///
/// Only for clicks AppKit would otherwise deliver to a native windowed pane. Everywhere else the
/// winit content view is the hit-test target, winit raises `MouseButtonInput`, and the ordinary
/// Bevy pointer path already works — swallowing those would trade a working route for this one.
#[cfg(target_os = "macos")]
pub fn forward_native_layout_click(
    position_px: Vec2,
    button: bevy::picking::pointer::PointerButton,
    mouse_up: bool,
) -> bool {
    // The release has to follow the press that was forwarded, even if the pointer has since left
    // the hit region — the regions track the cursor, and a press delivered without its release
    // leaves the DOM latched in a pressed state.
    let latched = NATIVE_LAYOUT_CLICK_LATCH.load(Ordering::Relaxed);
    let owed_release = mouse_up && latched;
    if !native_layout_pointer_is_inside() && !owed_release {
        return false;
    }
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send_click(position_px / state.scale, button, mouse_up);
        true
    });
    if forwarded {
        NATIVE_LAYOUT_CLICK_LATCH.store(!mouse_up, Ordering::Relaxed);
    }
    forwarded
}

/// Whether a press was handed to the layout webview and still owes it a release.
#[cfg(target_os = "macos")]
static NATIVE_LAYOUT_CLICK_LATCH: AtomicBool = AtomicBool::new(false);

/// When the AppKit monitor last handed the layout webview a wheel event.
///
/// Swallowing the `NSEvent` keeps it out of winit, so `MouseWheel` never reaches Bevy and the
/// frame-rate governor would leave the layout at its idle rate for the whole scroll.
#[cfg(target_os = "macos")]
static NATIVE_LAYOUT_SCROLL_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn native_layout_scroll_at() -> Option<std::time::Instant> {
    *NATIVE_LAYOUT_SCROLL_AT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(not(target_os = "macos"))]
fn native_layout_scroll_at() -> Option<std::time::Instant> {
    None
}

pub fn set_native_layout_activity(active: bool) -> bool {
    NATIVE_LAYOUT_ACTIVITY.swap(active, Ordering::Relaxed) != active
}

fn native_layout_activity_active() -> bool {
    NATIVE_LAYOUT_ACTIVITY.load(Ordering::Relaxed)
}

fn cef_pointer_hit_rect(
    header: Option<&Header>,
    side_sheet: Option<&SideSheet>,
    node: &Node,
    computed: &ComputedNode,
    transform: &UiGlobalTransform,
    visibility: Option<&Visibility>,
    open: bool,
) -> CefPointerHitRect {
    let interactive = (header.is_some() || side_sheet.is_some())
        && open
        && node.display != Display::None
        && !matches!(visibility, Some(Visibility::Hidden))
        && computed.size.x > 0.0
        && computed.size.y > 0.0;
    CefPointerHitRect {
        center: transform.transform_point2(Vec2::ZERO),
        size: computed.size,
        interactive,
    }
}

fn cef_pointer_regions_contains(
    cursor_pos: Vec2,
    cef_regions: &CefPointerRegionQuery<'_, '_>,
) -> bool {
    cef_regions
        .iter()
        .map(
            |(header, side_sheet, node, computed, transform, visibility, open)| {
                cef_pointer_hit_rect(
                    header, side_sheet, node, computed, transform, visibility, open,
                )
            },
        )
        .any(|rect| cef_pointer_hit_rect_contains(rect, cursor_pos))
}

fn sync_layout_cef_pointer_target(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout_q: Query<(Entity, Has<CefPointerTarget>), With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut commands: Commands,
) {
    let Ok((layout, has_target)) = layout_q.single() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        return;
    };
    #[cfg(target_os = "macos")]
    let should_target = {
        let inside = !pointer_capture_q.is_empty()
            || windows
                .single()
                .ok()
                .and_then(|window| {
                    let scale = window.resolution.scale_factor();
                    (scale.is_finite() && scale > 0.0).then_some(scale)
                })
                .and_then(|scale| {
                    vmux_layout::native_pointer::snapshot()
                        .map(|pointer| pointer.position_px / scale)
                })
                .is_some_and(|position| cef_pointer_regions_contains(position, &cef_regions));
        modal_pointer_targets.is_empty() && inside
    };
    #[cfg(not(target_os = "macos"))]
    let should_target = modal_pointer_targets.is_empty()
        && (!pointer_capture_q.is_empty()
            || windows
                .single()
                .ok()
                .and_then(Window::cursor_position)
                .is_some_and(|pos| cef_pointer_regions_contains(pos, &cef_regions)));
    NATIVE_LAYOUT_POINTER_INSIDE.store(should_target, Ordering::Relaxed);
    if should_target && !has_target {
        commands.entity(layout).insert(CefPointerTarget);
    } else if !should_target && has_target {
        commands.entity(layout).remove::<CefPointerTarget>();
    }
}

#[cfg(target_os = "macos")]
fn forward_layout_cef_cursor_move(mut events: MessageReader<CursorMoved>) {
    for _ in events.read() {}
}

#[cfg(not(target_os = "macos"))]
fn forward_layout_cef_cursor_move(
    mut events: MessageReader<CursorMoved>,
    buttons: Res<ButtonInput<MouseButton>>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut was_in_region: Local<bool>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
        for _ in events.read() {}
        *was_in_region = false;
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        *was_in_region = false;
        return;
    };
    for event in events.read() {
        let in_region = !pointer_capture_q.is_empty()
            || cef_pointer_regions_contains(event.position, &cef_regions);
        if in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), event.position, false);
        } else if *was_in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), event.position, true);
        }
        *was_in_region = in_region;
    }
}

fn forward_layout_cef_mouse_button(
    mut events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
    suppress: Res<CefSuppressPointerInput>,
    browsers: NonSend<Browsers>,
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut captured: Local<bool>,
) {
    if suppress.0 || !modal_pointer_targets.is_empty() {
        for _ in events.read() {}
        *captured = false;
        return;
    }
    let Ok(layout) = layout_q.single() else {
        for _ in events.read() {}
        *captured = false;
        return;
    };
    for event in events.read() {
        let Some(button) = pointer_button_from_mouse_button(event.button) else {
            continue;
        };
        let Ok(window) = windows.get(event.window) else {
            continue;
        };
        #[cfg(target_os = "macos")]
        let native_pointer = vmux_layout::native_pointer::snapshot();
        #[cfg(target_os = "macos")]
        let position = native_pointer
            .map(|pointer| pointer.position_px / window.resolution.scale_factor())
            .or_else(|| window.cursor_position());
        #[cfg(not(target_os = "macos"))]
        let position = window.cursor_position();
        let Some(position) = position else {
            continue;
        };
        let inside =
            !pointer_capture_q.is_empty() || cef_pointer_regions_contains(position, &cef_regions);
        if event.state == ButtonState::Pressed && inside {
            *captured = true;
        }
        if inside || *captured {
            #[cfg(target_os = "macos")]
            if let Some(pointer) = native_pointer {
                browsers.send_native_mouse_move(&layout, pointer.buttons, position, !inside);
            }
            browsers.send_mouse_click(
                &layout,
                position,
                button,
                event.state == ButtonState::Released,
            );
        }
        if event.state == ButtonState::Released {
            *captured = false;
        }
    }
}

fn dismiss_windowed_command_bar_on_outside_click(
    mut events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    modal_q: Query<(Entity, Option<&HostWindow>), (With<Modal>, With<WebviewWindowed>)>,
    mut commands: Commands,
) {
    let Ok((modal_e, host_window)) = modal_q.single() else {
        for _ in events.read() {}
        return;
    };
    // Read the same published rectangle the AppKit monitor tests against. Recomputing it here
    // gave one click two different answers depending on which path saw it first.
    let route = native_command_bar_route();
    let window_entity = host_window
        .map(|h| h.0)
        .or_else(|| primary_window.single().ok());
    let Some(window_entity) = window_entity else {
        for _ in events.read() {}
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        if event.window != window_entity {
            continue;
        }
        let cursor = window
            .physical_cursor_position()
            .map(|pos| Vec2::new(pos.x, pos.y));
        if command_bar_windowed_click_should_dismiss(
            route.owns_input,
            event.button,
            event.state,
            cursor,
            route.frame,
        ) {
            commands.trigger(BinReceive::<CommandBarActionEvent> {
                webview: modal_e,
                payload: CommandBarActionEvent {
                    action: "dismiss".to_string(),
                    value: String::new(),
                    target: None,
                    target_url: None,
                    attachments: Vec::new(),
                },
            });
            break;
        }
    }
}

fn dismiss_command_bar_from_native_monitor(
    modal_q: Query<Entity, With<Modal>>,
    mut commands: Commands,
) {
    if !take_native_command_bar_dismiss_requested() {
        return;
    }
    let Ok(modal_e) = modal_q.single() else {
        return;
    };
    commands.trigger(BinReceive::<CommandBarActionEvent> {
        webview: modal_e,
        payload: CommandBarActionEvent {
            action: "dismiss".to_string(),
            value: String::new(),
            target: None,
            target_url: None,
            attachments: Vec::new(),
        },
    });
}

fn pointer_button_from_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// The layout page owns the keyboard whenever one of its own DOM surfaces has focus — the bookmark
/// field, a context menu, or the command bar panel. One rule rather than a special case per
/// surface, since every consumer has to agree or the keyboard lands somewhere else.
pub(crate) type LayoutKeyboardCapture = Or<(
    With<BookmarkTextInputActive>,
    With<BookmarkContextMenuActive>,
    With<CommandBarPanelActive>,
)>;

/// Layout-page surfaces that float free of the laid-out regions and must own the whole window's
/// pointer input while they are up.
///
/// A context menu or the command bar panel can extend past any published hit rect, and both
/// dismiss on an outside click, so the layout webview has to see every move and click. A focused
/// bookmark field does not qualify — it stays inside the side sheet's own region.
pub(crate) type LayoutPointerCapture =
    Or<(With<BookmarkContextMenuActive>, With<CommandBarPanelActive>)>;

fn sync_keyboard_target(
    mode: Res<vmux_layout::scene::InteractionMode>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    modal_q: Query<(Entity, &Node, Has<CefKeyboardTarget>), With<Modal>>,
    layout_keyboard_q: Query<Entity, (With<LayoutCef>, LayoutKeyboardCapture)>,
    content_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    terminal_q: Query<(), With<vmux_terminal::Terminal>>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
    mut commands: Commands,
) {
    if let Some(modal) = modal_q.iter().find_map(|(entity, node, keyboard_target)| {
        (node.display != Display::None && keyboard_target).then_some(entity)
    }) {
        for (browser_e, has_kb) in &content_q {
            if browser_e != modal && has_kb {
                commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
            }
        }
        suppress.0 = false;
        return;
    }

    if let Ok(layout) = layout_keyboard_q.single() {
        for (browser_e, has_kb) in &content_q {
            if browser_e == layout {
                if !has_kb {
                    commands.entity(browser_e).try_insert(CefKeyboardTarget);
                }
            } else if has_kb {
                commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
            }
        }
        suppress.0 = false;
        return;
    }

    // In Player mode, only sync when a pane has been clicked (Focused sub-state).
    // In Roaming (no CefKeyboardTarget on any pane browser), skip sync to prevent
    // re-assigning the target to the previously active pane.
    if *mode == vmux_layout::scene::InteractionMode::Player {
        let has_pane_target = content_q
            .iter()
            .any(|(e, has_kb)| has_kb && !status_q.contains(e) && !side_sheet_q.contains(e));
        if !has_pane_target {
            return;
        }
    }
    let active_stack_opt = focus.stack;
    let Some(active_stack_entity) = active_stack_opt else {
        return;
    };
    for (browser_e, has_kb) in &content_q {
        if status_q.contains(browser_e) || side_sheet_q.contains(browser_e) {
            continue;
        }

        let in_active = child_of_q
            .get(browser_e)
            .ok()
            .map(|co| co.get() == active_stack_entity)
            .unwrap_or(false);

        if in_active {
            if !has_kb {
                commands.entity(browser_e).try_insert(CefKeyboardTarget);
            }
            // Suppress CEF keyboard forwarding when a terminal is focused —
            // terminals receive input via the service, not CEF key events.
            suppress.0 = terminal_q.contains(browser_e);
        } else if has_kb {
            commands.entity(browser_e).try_remove::<CefKeyboardTarget>();
        }
    }
}

fn tab_of(
    start: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let mut e = start;
    loop {
        if tab_q.contains(e) {
            return Some(e);
        }
        match child_of_q.get(e) {
            Ok(co) => e = co.get(),
            Err(_) => return None,
        }
    }
}

fn tab_ancestor(
    start: Entity,
    child_of_q: &Query<&ChildOf>,
    tabs_q: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    let mut e = start;
    loop {
        if tabs_q.contains(e) {
            return Some(e);
        }
        match child_of_q.get(e) {
            Ok(co) => e = co.get(),
            Err(_) => return None,
        }
    }
}

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            &mut Transform,
            &ComputedNode,
            &bevy::ui::ComputedStackIndex,
            &UiGlobalTransform,
            &ChildOf,
            &mut WebviewSize,
            Option<&Header>,
            Option<&SideSheet>,
            Option<&Modal>,
            Option<&Visibility>,
            Option<&HistorySwipeVisualOffset>,
            Has<PendingWebviewReveal>,
            Has<PendingCommandBarReveal>,
            Has<LayoutCef>,
            Has<WebviewWindowed>,
        ),
        With<Browser>,
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    tabs_q: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_q: Query<(), (With<Tab>, With<vmux_core::Active>)>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

    for (
        mut tf,
        self_computed,
        self_stack_index,
        self_ui_gt,
        child_of,
        mut webview_size,
        status,
        side_sheet,
        modal,
        visibility,
        history_swipe_visual,
        pending_webview_reveal,
        pending_command_bar_reveal,
        is_layout_cef,
        is_windowed,
    ) in browser_q.iter_mut()
    {
        let parent = child_of.get();
        let pane_entity = child_of_q.get(parent).map(|co| co.get()).unwrap_or(parent);
        let (computed, ui_gt) = match pane_rect.get(pane_entity) {
            Ok((cn, gt)) => (cn, gt),
            Err(_) => (self_computed, self_ui_gt),
        };

        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            continue;
        }

        let is_cef_ui = status.is_some() || side_sheet.is_some() || modal.is_some();

        let under_inactive_tab = parent != glass_entity
            && !is_cef_ui
            && match tab_ancestor(parent, &child_of_q, &tabs_q) {
                Some(tab) => !active_tab_q.contains(tab),
                None => false,
            };

        let size_px = computed.size;
        let renderable = webview_layout_is_renderable(
            size_px,
            visibility,
            pending_webview_reveal || pending_command_bar_reveal,
        );
        match hidden_webview_sizing(renderable, under_inactive_tab) {
            HiddenWebviewSizing::Render => {}
            HiddenWebviewSizing::HideKeepSize => {
                tf.scale = Vec3::splat(1.0e-6);
                continue;
            }
            HiddenWebviewSizing::Collapse => {
                tf.scale = Vec3::splat(1.0e-6);
                if webview_size.0 != Vec2::ONE {
                    webview_size.0 = Vec2::ONE;
                }
                continue;
            }
        }

        // Check if this browser's parent tab is the active tab in its pane
        let is_active_stack = if parent != glass_entity && !is_cef_ui {
            active_stack_in_pane(pane_entity, &pane_children, &tab_ts) == Some(parent)
        } else {
            true
        };

        // Keep rendering the previous tab behind while a new empty tab
        // (without CEF content) is pending in the command bar flow.
        let is_previous_stack =
            new_stack_ctx.stack.is_some() && new_stack_ctx.previous_stack == Some(parent);

        let is_inactive_stack =
            parent != glass_entity && !is_cef_ui && !is_active_stack && !is_previous_stack;

        let is_inactive_tab = under_inactive_tab;

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        let new_scale = if is_inactive_stack || is_inactive_tab {
            Vec3::splat(1e-6)
        } else {
            Vec3::new(sx, sy, 1.0)
        };
        tf.scale = new_scale;

        let center_ui = ui_gt.transform_point2(Vec2::ZERO);
        let glass_center_ui = glass_ui_gt.transform_point2(Vec2::ZERO);
        let delta_px = center_ui - glass_center_ui;

        let tx = delta_px.x / glass_size_px.x;
        let ty = -delta_px.y / glass_size_px.y;
        let z = if modal.is_some() {
            WEBVIEW_Z_MODAL
        } else if is_layout_cef || status.is_some() {
            WEBVIEW_Z_HEADER
        } else if side_sheet.is_some() {
            WEBVIEW_Z_SIDE_SHEET
        } else if parent != glass_entity {
            if is_active_stack {
                WEBVIEW_Z_MAIN
            } else {
                WEBVIEW_Z_MAIN - 0.01
            }
        } else {
            0.01 + self_stack_index.0 as f32 * 0.001
        };
        let history_swipe_tx = if parent != glass_entity && !is_cef_ui {
            history_swipe_visual
                .map(|visual| visual.offset_px / glass_size_px.x)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        tf.translation = Vec3::new(tx + history_swipe_tx, ty, z);

        // A windowed modal's node fills the whole layout area, but its native view is a small
        // centred box that `sync_windowed_command_bar` sizes. Writing the node size here makes the
        // page lay out at the full width inside that box, so the shell renders far too wide.
        if modal.is_some() && is_windowed {
            continue;
        }

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}

fn webview_should_use_windowed(mode: vmux_layout::scene::InteractionMode) -> bool {
    cfg!(target_os = "macos") && mode == vmux_layout::scene::InteractionMode::User
}

fn transform_near(a: &Transform, b: &Transform) -> bool {
    a.translation.distance(b.translation) < 0.001
        && a.scale.distance(b.scale) < 0.001
        && a.rotation.dot(b.rotation).abs() > 0.9999
}

#[derive(Clone, Copy, PartialEq)]
struct WindowedBackendSignature {
    width: f32,
    height: f32,
    scale: f32,
}

#[derive(Resource, Default)]
struct WindowedBackendCameraState {
    mismatch: Option<WindowedBackendSignature>,
}

fn windowed_backend_signature(world: &mut World) -> Option<WindowedBackendSignature> {
    let mut window_q = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let Ok(window) = window_q.single(world) else {
        return None;
    };
    Some(WindowedBackendSignature {
        width: window.resolution.width(),
        height: window.resolution.height(),
        scale: window.resolution.scale_factor(),
    })
}

fn clear_windowed_backend_camera_state(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<WindowedBackendCameraState>() {
        state.mismatch = None;
    }
}

fn camera_supports_windowed_webviews(world: &mut World) -> bool {
    let expected = {
        let mut window_q = world.query_filtered::<&Window, With<PrimaryWindow>>();
        let Ok(window) = window_q.single(world) else {
            return true;
        };
        let height = window.resolution.height().max(1.0);
        let aspect = window.resolution.width() / height;
        vmux_layout::scene::frame_main_camera_transform(window, aspect, 0.0)
    };
    let camera = {
        let mut camera_q =
            world.query_filtered::<&Transform, With<vmux_layout::scene::MainCamera>>();
        let Ok(camera) = camera_q.single(world) else {
            return true;
        };
        *camera
    };
    transform_near(&camera, &expected)
}

fn windowed_backend_should_use_windowed(
    world: &mut World,
    mode: vmux_layout::scene::InteractionMode,
) -> bool {
    if !webview_should_use_windowed(mode) {
        clear_windowed_backend_camera_state(world);
        return false;
    }
    if camera_supports_windowed_webviews(world) {
        clear_windowed_backend_camera_state(world);
        return true;
    }
    let Some(signature) = windowed_backend_signature(world) else {
        clear_windowed_backend_camera_state(world);
        return true;
    };
    if !world.contains_resource::<WindowedBackendCameraState>() {
        world.insert_resource(WindowedBackendCameraState::default());
    }
    let mut state = world.resource_mut::<WindowedBackendCameraState>();
    let should_keep_windowed = state.mismatch != Some(signature);
    state.mismatch = Some(signature);
    should_keep_windowed
}

fn set_windowed_content_mesh_material(
    material: &mut WebviewExtendStandardMaterial,
    windowed: bool,
) {
    let alpha = if windowed { 0.0 } else { 1.0 };
    material.base.base_color = material.base.base_color.with_alpha(alpha);
    material.base.alpha_mode =
        webview_content_alpha_mode(alpha, material.extension.pane_corner_clip.x);
}

fn webview_content_alpha_mode(alpha: f32, radius: f32) -> AlphaMode {
    if alpha < 1.0 {
        AlphaMode::Blend
    } else if radius > 0.0 {
        AlphaMode::AlphaToCoverage
    } else {
        AlphaMode::Opaque
    }
}

fn sync_windowed_content_mesh_materials(
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    browsers: Query<
        (
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
            Has<WebviewWindowed>,
        ),
        (
            With<Browser>,
            Without<LayoutCef>,
            Without<Modal>,
            Without<Header>,
            Without<SideSheet>,
        ),
    >,
) {
    for (handle, windowed) in &browsers {
        if let Some(mut material) = materials.get_mut(handle.id()) {
            set_windowed_content_mesh_material(&mut material, windowed);
        }
    }
}

fn sync_modal_mesh_visibility(
    modal_q: Query<
        (
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
            Has<WebviewWindowed>,
        ),
        With<Modal>,
    >,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (handle, windowed) in &modal_q {
        if let Some(mut material) = materials.get_mut(handle.id()) {
            set_windowed_content_mesh_material(&mut material, windowed);
        }
    }
}

/// The layout renders on the OSR mesh in both modes: a wgpu quad that resizes with the Bevy
/// frame, so it tracks a live window resize (a native overlay cannot — its frame only updates from a
/// Bevy schedule the macOS resize loop starves). Keep the material visible.
///
/// This drives the material's alpha rather than `Visibility`: the OSR focus pipeline treats a
/// `Visibility::Hidden` webview as hidden and tells CEF to stop rendering it. Keeping the entity
/// visible leaves OSR running. Alpha mode stays `Blend` so pages show through the layout's
/// transparent areas.
fn sync_layout_mesh_visibility(
    mode: Res<vmux_layout::scene::InteractionMode>,
    layout_q: Query<&WebviewMaterialHandle<WebviewExtendStandardMaterial>, With<LayoutCef>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let want_alpha = if *mode == vmux_layout::scene::InteractionMode::User {
        0.0
    } else {
        1.0
    };
    for mat_handle in &layout_q {
        let Some(mut material) = materials.get_mut(mat_handle.id()) else {
            continue;
        };
        if material.base.alpha_mode != AlphaMode::Blend {
            material.base.alpha_mode = AlphaMode::Blend;
        }
        if material.base.base_color.alpha() != want_alpha {
            material.base.base_color.set_alpha(want_alpha);
        }
    }
}

fn sync_cef_backend_for_interaction_mode(world: &mut World) {
    let mode = world
        .get_resource::<vmux_layout::scene::InteractionMode>()
        .copied()
        .unwrap_or_default();
    let base_windowed = windowed_backend_should_use_windowed(world, mode);
    let mut query = world.query_filtered::<(
        Entity,
        Has<LayoutCef>,
        Has<Modal>,
        Has<WebviewNativeOverlay>,
        Has<WebviewNativeDirectOverlay>,
    ), (With<Browser>, With<WebviewSource>)>();
    let entities: Vec<(Entity, bool, bool, bool, bool)> = query.iter(world).collect();
    // The command bar stays OSR. A windowed CEF child view never receives DOM input here: real
    // `NSEvent`s are dropped even when it holds first responder, and `send_key_event` forwarding is
    // a windowless API that produces no DOM key events. The bar is the one surface hosting a real
    // text field, so it needs the OSR path that input injection actually reaches.
    let target_windowed =
        |is_layout: bool, is_modal: bool| base_windowed && !is_layout && !is_modal;
    let target_native_overlay = |is_layout: bool, is_modal: bool| {
        cfg!(target_os = "macos")
            && mode == vmux_layout::scene::InteractionMode::User
            && (is_layout || is_modal)
    };
    let target_native_direct_overlay = |is_layout: bool| {
        cfg!(target_os = "macos") && mode == vmux_layout::scene::InteractionMode::User && is_layout
    };
    let mut recreate = Vec::new();
    {
        let browsers = world.non_send::<Browsers>();
        for &(entity, is_layout, is_modal, actual_native_overlay, actual_direct_overlay) in
            &entities
        {
            let has_browser = browsers.has_browser(entity);
            let actual_windowed = browsers.is_windowed(&entity);
            let want_windowed = target_windowed(is_layout, is_modal);
            let want_native_overlay = target_native_overlay(is_layout, is_modal);
            let want_native_direct_overlay = target_native_direct_overlay(is_layout);
            let needs_recreate = actual_windowed.is_some_and(|actual| actual != want_windowed)
                || has_browser
                    && (actual_native_overlay != want_native_overlay
                        || actual_direct_overlay != want_native_direct_overlay);
            if needs_recreate {
                recreate.push(entity);
            }
        }
    }
    if !recreate.is_empty() {
        let mut browsers = world.non_send_mut::<Browsers>();
        for entity in &recreate {
            browsers.close(entity);
        }
    }
    for (entity, is_layout, is_modal, _, _) in entities {
        let want_windowed = target_windowed(is_layout, is_modal);
        let want_native_overlay = target_native_overlay(is_layout, is_modal);
        let want_native_direct_overlay = target_native_direct_overlay(is_layout);
        let marker_matches = world.get::<WebviewWindowed>(entity).is_some() == want_windowed;
        let overlay_matches =
            world.get::<WebviewNativeOverlay>(entity).is_some() == want_native_overlay;
        let direct_overlay_matches =
            world.get::<WebviewNativeDirectOverlay>(entity).is_some() == want_native_direct_overlay;
        let needs_recreate = recreate.contains(&entity);
        if marker_matches && overlay_matches && direct_overlay_matches && !needs_recreate {
            continue;
        }
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        if want_windowed {
            entity_mut.insert(WebviewWindowed);
        } else {
            entity_mut.remove::<WebviewWindowed>();
        }
        if want_native_overlay {
            entity_mut.insert(WebviewNativeOverlay);
        } else {
            entity_mut.remove::<WebviewNativeOverlay>();
        }
        if want_native_direct_overlay {
            entity_mut.insert(WebviewNativeDirectOverlay);
        } else {
            entity_mut.remove::<WebviewNativeDirectOverlay>();
        }
        if needs_recreate {
            entity_mut
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
}

/// Pick the focus-ring width + color for a windowed browser pane. The local
/// user's ring (their accent) draws on their focused stack; each agent's ring
/// (a distinct per-agent hue) draws on the agent's own active pane. User takes
/// precedence when a pane is active for both.
fn windowed_ring_for(
    stack: Entity,
    pane: Entity,
    focus: &vmux_layout::stack::FocusedStack,
    visible_pane_count: usize,
    active_panes: &vmux_layout::active_panes::ActivePanes,
    settings: &AppSettings,
    scale: f32,
) -> (f32, [f32; 3], Option<vmux_core::agent::AgentKind>) {
    use vmux_layout::active_panes::ProfileId;
    let width = settings.layout.focus_ring.width * scale;
    let user = &settings.layout.focus_ring.color;
    if focus.stack == Some(stack) && visible_pane_count > 1 {
        return (width, [user.r, user.g, user.b], None);
    }
    for (profile, active) in active_panes.0.iter() {
        if let ProfileId::Agent(key) = profile
            && active.pane == Some(pane)
        {
            return (width, agent_ring_rgb(key), active.kind);
        }
    }
    (0.0, [user.r, user.g, user.b], None)
}

/// Deterministic, distinct ring color per agent (so multiple agents read apart).
fn agent_ring_rgb(key: &str) -> [f32; 3] {
    let mut h: u64 = 1469598103934665603;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    hsl_to_rgb((h % 360) as f32, 0.85, 0.62)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

const CLAUDE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/claude.png");
const CODEX_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/codex.png");
const VIBE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/vibe.png");

/// A decoded, premultiplied-RGBA agent logo, ready to hand to the native badge.
struct LogoBitmap {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_premultiplied(png: &[u8]) -> Option<LogoBitmap> {
    let img = image::load_from_memory(png).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    let mut rgba = img.into_raw();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u16;
        px[0] = (px[0] as u16 * a / 255) as u8;
        px[1] = (px[1] as u16 * a / 255) as u8;
        px[2] = (px[2] as u16 * a / 255) as u8;
    }
    Some(LogoBitmap {
        rgba,
        width,
        height,
    })
}

/// The agent's logo bitmap, decoded once and cached for the process lifetime.
fn agent_logo(kind: vmux_core::agent::AgentKind) -> Option<&'static LogoBitmap> {
    use std::sync::OnceLock;
    use vmux_core::agent::AgentKind;
    static CLAUDE: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    static CODEX: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    static VIBE: OnceLock<Option<LogoBitmap>> = OnceLock::new();
    let (cell, png) = match kind {
        AgentKind::Claude => (&CLAUDE, CLAUDE_LOGO_PNG),
        AgentKind::Codex => (&CODEX, CODEX_LOGO_PNG),
        AgentKind::Vibe => (&VIBE, VIBE_LOGO_PNG),
    };
    cell.get_or_init(|| decode_premultiplied(png)).as_ref()
}

/// Stable per-kind tag the native layer caches on, so the badge image is only
/// rebuilt when the owning agent's kind changes.
fn agent_kind_tag(kind: vmux_core::agent::AgentKind) -> u8 {
    use vmux_core::agent::AgentKind;
    match kind {
        AgentKind::Claude => 1,
        AgentKind::Codex => 2,
        AgentKind::Vibe => 3,
    }
}

fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

/// The agent's brand color (Claude clay / Codex green / Mistral purple), used as
/// the badge circle fill behind its logo.
fn agent_brand_rgb(kind: vmux_core::agent::AgentKind) -> [f32; 3] {
    hex_to_rgb(&vmux_core::team::AvatarSpec::for_agent(kind).color).unwrap_or([0.5, 0.5, 0.5])
}

/// Position windowed (native) content webviews to match their pane rect. Reads the mesh scale set
/// by `sync_children_to_ui` (visible active pane has a real scale; inactive panes ~1e-6) to pick
/// which native view to show. No-op for OSR webviews / non-macOS (`set_windowed_*` are no-ops).
fn sync_windowed_frames(
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    layout_hidden: Res<vmux_layout::toggle::LayoutHidden>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    active_panes: Res<vmux_layout::active_panes::ActivePanes>,
    clear_color: Res<ClearColor>,
    browser_q: Query<
        (
            Entity,
            &Transform,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
        ),
        (
            With<Browser>,
            With<WebviewWindowed>,
            Without<LayoutCef>,
            Without<Modal>,
        ),
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    header_rect: Query<(&ComputedNode, &UiGlobalTransform), (With<Header>, With<Open>)>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut last_raised_frame: Local<std::collections::HashMap<Entity, (i32, i32, i32, i32)>>,
    mut last_visible_pages: Local<Vec<Entity>>,
    mut last_windowed_pages: Local<Vec<Entity>>,
    mut visible_frames: Local<Vec<WindowedFrameRect>>,
) {
    let visible_pane_count =
        visible_pane_count_for_windowed_sync(focus.tab, &all_children, &leaf_panes);
    let header_frame = header_rect
        .iter()
        .find_map(|(computed, ui_gt)| windowed_frame_rect_from_computed(computed, ui_gt));
    let force_raise = layout_hidden.is_changed();
    let mut hidden = Vec::new();
    let mut visible = Vec::new();
    visible_frames.clear();
    for (entity, tf, self_computed, self_ui_gt, child_of) in &browser_q {
        if tf.scale.x <= 1.0e-3 {
            hidden.push(entity);
            continue;
        }
        visible.push(entity);
        let parent = child_of.get();
        let pane_entity = child_of_q.get(parent).map(|co| co.get()).unwrap_or(parent);
        let (computed, ui_gt) = pane_rect
            .get(pane_entity)
            .unwrap_or((self_computed, self_ui_gt));
        let Some(pane_frame) = windowed_frame_rect_from_computed(computed, ui_gt) else {
            continue;
        };
        let scale = 1.0 / computed.inverse_scale_factor.max(1.0e-6);
        let frame = windowed_page_frame_rect(
            pane_frame,
            header_frame,
            layout_hidden.0,
            visible_pane_count,
        );
        let became_visible = !last_visible_pages.contains(&entity);
        if became_visible {
            browsers.set_windowed_hidden(&entity, false);
        }
        browsers.set_windowed_frame(
            &entity,
            frame.left,
            frame.top,
            frame.width,
            frame.height,
            scale,
        );
        let all_corners = windowed_page_all_corners(layout_hidden.0, visible_pane_count);
        browsers.set_windowed_corner_radius(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
        );
        let (focus_ring_width, focus_ring_rgb, focus_ring_kind) = windowed_ring_for(
            parent,
            pane_entity,
            &focus,
            visible_pane_count,
            &active_panes,
            &settings,
            scale,
        );
        browsers.set_windowed_focus_ring(&entity, focus_ring_width, scale, focus_ring_rgb);
        let badge = focus_ring_kind.and_then(|kind| {
            agent_logo(kind).map(|logo| {
                (
                    logo.rgba.as_slice(),
                    logo.width,
                    logo.height,
                    agent_brand_rgb(kind),
                    agent_kind_tag(kind),
                )
            })
        });
        browsers.set_agent_badge(&entity, scale, badge);
        let cover_rgb = clear_color.0.to_srgba();
        browsers.set_windowed_corner_cover(
            &entity,
            settings.layout.radius * scale,
            scale,
            all_corners,
            [cover_rgb.red, cover_rgb.green, cover_rgb.blue],
        );
        if browsers.has_browser(entity) {
            visible_frames.push(frame);
            let key = (
                frame.left.round() as i32,
                frame.top.round() as i32,
                frame.width.round() as i32,
                frame.height.round() as i32,
            );
            let changed = last_raised_frame.insert(entity, key) != Some(key);
            if force_raise || changed || became_visible {
                browsers.raise_windowed_to_front(&entity);
            }
        }
    }
    let current_windowed: Vec<Entity> = visible.iter().chain(&hidden).copied().collect();
    let newly_windowed: Vec<Entity> = current_windowed
        .iter()
        .copied()
        .filter(|entity| !last_windowed_pages.contains(entity))
        .collect();
    let ever_shown: Vec<Entity> = last_raised_frame.keys().copied().collect();
    for entity in windowed_pages_to_hide(&hidden, &last_visible_pages, &ever_shown, &newly_windowed)
    {
        browsers.set_windowed_hidden(&entity, true);
    }
    *last_visible_pages = visible;
    *last_windowed_pages = current_windowed;
    *visible_frames = set_native_windowed_page_frames(std::mem::take(&mut *visible_frames));
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowedFrameRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl WindowedFrameRect {
    fn right(self) -> f32 {
        self.left + self.width
    }

    fn bottom(self) -> f32 {
        self.top + self.height
    }
}

#[cfg(target_os = "macos")]
static NATIVE_WINDOWED_PAGE_FRAMES: LazyLock<Mutex<Vec<WindowedFrameRect>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[cfg(any(target_os = "macos", test))]
fn windowed_frame_contains(frame: WindowedFrameRect, point: Vec2) -> bool {
    point.x >= frame.left
        && point.x <= frame.right()
        && point.y >= frame.top
        && point.y <= frame.bottom()
}

#[cfg(target_os = "macos")]
fn set_native_windowed_page_frames(mut frames: Vec<WindowedFrameRect>) -> Vec<WindowedFrameRect> {
    let mut published = NATIVE_WINDOWED_PAGE_FRAMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::swap(&mut *published, &mut frames);
    frames.clear();
    frames
}

#[cfg(not(target_os = "macos"))]
fn set_native_windowed_page_frames(mut frames: Vec<WindowedFrameRect>) -> Vec<WindowedFrameRect> {
    frames.clear();
    frames
}

#[cfg(target_os = "macos")]
pub fn native_windowed_page_contains_point(x_px: f32, y_px: f32) -> bool {
    let point = Vec2::new(x_px, y_px);
    NATIVE_WINDOWED_PAGE_FRAMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .copied()
        .any(|frame| windowed_frame_contains(frame, point))
}

#[cfg(not(target_os = "macos"))]
pub fn native_windowed_page_contains_point(_: f32, _: f32) -> bool {
    false
}

fn windowed_frame_rect_from_computed(
    computed: &ComputedNode,
    ui_gt: &UiGlobalTransform,
) -> Option<WindowedFrameRect> {
    let size = computed.size;
    if size.x <= 0.0 || size.y <= 0.0 || !size.x.is_finite() || !size.y.is_finite() {
        return None;
    }
    let center = ui_gt.transform_point2(Vec2::ZERO);
    Some(WindowedFrameRect {
        left: center.x - size.x * 0.5,
        top: center.y - size.y * 0.5,
        width: size.x,
        height: size.y,
    })
}

fn windowed_page_frame_rect(
    pane: WindowedFrameRect,
    header: Option<WindowedFrameRect>,
    layout_hidden: bool,
    visible_pane_count: usize,
) -> WindowedFrameRect {
    let Some(header) = header else {
        return pane;
    };
    if layout_hidden {
        return pane;
    }
    let (left, right) = if visible_pane_count == 1 {
        (header.left.ceil(), header.right().floor())
    } else {
        (pane.left.ceil(), pane.right().floor())
    };
    let top = header.bottom().ceil().max(pane.top.ceil());
    let bottom = pane.bottom().floor();
    if right <= left || bottom <= top {
        return pane;
    }
    WindowedFrameRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    }
}

fn visible_pane_count_for_windowed_sync(
    focused_tab: Option<Entity>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> usize {
    if let Some(tab) = focused_tab {
        let mut leaves = Vec::new();
        collect_leaf_panes(tab, all_children, leaf_panes, &mut leaves);
        if !leaves.is_empty() {
            return leaves.len();
        }
    }
    leaf_panes.iter().count().max(1)
}

fn windowed_pages_to_hide(
    hidden: &[Entity],
    prev_visible: &[Entity],
    ever_shown: &[Entity],
    newly_windowed: &[Entity],
) -> Vec<Entity> {
    hidden
        .iter()
        .copied()
        .filter(|entity| {
            prev_visible.contains(entity)
                || !ever_shown.contains(entity)
                || newly_windowed.contains(entity)
        })
        .collect()
}

fn windowed_page_all_corners(layout_hidden: bool, visible_pane_count: usize) -> bool {
    layout_hidden || visible_pane_count > 1
}

fn sync_windowed_layout(
    browsers: NonSend<Browsers>,
    layout_q: Query<(Entity, Option<&HostWindow>), (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut last_raised_frame: Local<std::collections::HashMap<Entity, (i32, i32, i32, i32)>>,
) {
    for (entity, host_window) in &layout_q {
        let window_entity = host_window
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let Some(window_entity) = window_entity else {
            continue;
        };
        let Ok(window) = windows.get(window_entity) else {
            continue;
        };
        let scale = window.resolution.scale_factor();
        let w = window.resolution.physical_width() as f32;
        let h = window.resolution.physical_height() as f32;
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        browsers.set_windowed_hidden(&entity, false);
        browsers.set_windowed_frame(&entity, 0.0, 0.0, w, h, scale);
        if browsers.has_browser(entity) {
            let key = (0, 0, w.round() as i32, h.round() as i32);
            let changed = last_raised_frame.insert(entity, key) != Some(key);
            if changed {
                browsers.raise_windowed_to_front(&entity);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowedHoverRefreshFrame {
    left_px: f32,
    top_px: f32,
    width_px: f32,
    height_px: f32,
    scale: f32,
}

fn windowed_hover_refresh_frame(
    computed: &ComputedNode,
    ui_gt: &UiGlobalTransform,
) -> Option<WindowedHoverRefreshFrame> {
    let size_px = computed.size;
    let scale = 1.0 / computed.inverse_scale_factor.max(1.0e-6);
    if size_px.x <= 0.0
        || size_px.y <= 0.0
        || !size_px.x.is_finite()
        || !size_px.y.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
    {
        return None;
    }
    let center = ui_gt.transform_point2(Vec2::ZERO);
    Some(WindowedHoverRefreshFrame {
        left_px: center.x - size_px.x * 0.5,
        top_px: center.y - size_px.y * 0.5,
        width_px: size_px.x,
        height_px: size_px.y,
        scale,
    })
}

fn windowed_hover_refresh_position(
    cursor_px: Vec2,
    frame: WindowedHoverRefreshFrame,
) -> Option<Vec2> {
    if cursor_px.x < frame.left_px
        || cursor_px.x > frame.left_px + frame.width_px
        || cursor_px.y < frame.top_px
        || cursor_px.y > frame.top_px + frame.height_px
    {
        return None;
    }
    Some(Vec2::new(
        (cursor_px.x - frame.left_px) / frame.scale,
        (cursor_px.y - frame.top_px) / frame.scale,
    ))
}

#[derive(Default)]
struct LayoutHoverRefreshState {
    #[cfg(not(target_os = "macos"))]
    sequence: u64,
    #[cfg(not(target_os = "macos"))]
    position: Option<Vec2>,
    #[cfg(not(target_os = "macos"))]
    in_region: bool,
}

fn reset_layout_cef_hover(
    browsers: &Browsers,
    buttons: &ButtonInput<MouseButton>,
    layout: Entity,
    state: &mut LayoutHoverRefreshState,
) {
    #[cfg(target_os = "macos")]
    {
        let _ = (browsers, buttons, layout);
        clear_native_layout_pointer_state();
        *state = LayoutHoverRefreshState::default();
    }
    #[cfg(not(target_os = "macos"))]
    {
        if state.in_region {
            browsers.send_mouse_move(
                &layout,
                buttons.get_pressed(),
                state.position.unwrap_or_default(),
                true,
            );
        }
        *state = LayoutHoverRefreshState::default();
    }
}

fn refresh_layout_cef_hover(
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    suppress: Res<CefSuppressPointerInput>,
    layout_q: Query<Entity, With<LayoutCef>>,
    pointer_capture_q: Query<(), (With<LayoutCef>, LayoutPointerCapture)>,
    cef_regions: CefPointerRegionQuery<'_, '_>,
    modal_pointer_targets: Query<(), (With<Modal>, With<CefPointerTarget>)>,
    mut state: Local<LayoutHoverRefreshState>,
) {
    let Ok(layout) = layout_q.single() else {
        #[cfg(target_os = "macos")]
        clear_native_layout_pointer_state();
        #[cfg(not(target_os = "macos"))]
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        *state = LayoutHoverRefreshState::default();
        return;
    };
    if suppress.0 || !modal_pointer_targets.is_empty() {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    }
    let Some(window_entity) = primary_window.single().ok() else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    };
    let scale = window.resolution.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
        reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
        return;
    }
    let pointer_capture = !pointer_capture_q.is_empty();
    #[cfg(target_os = "macos")]
    {
        if pointer_capture {
            set_native_layout_pointer_regions([physical_cef_pointer_hit_rect(
                CefPointerHitRect {
                    center: Vec2::new(window.width() * 0.5, window.height() * 0.5),
                    size: Vec2::new(window.width(), window.height()),
                    interactive: true,
                },
                scale,
            )]);
        } else {
            set_native_layout_pointer_regions(
                cef_regions
                    .iter()
                    .map(
                        |(header, side_sheet, node, computed, transform, visibility, open)| {
                            cef_pointer_hit_rect(
                                header, side_sheet, node, computed, transform, visibility, open,
                            )
                        },
                    )
                    .filter(|rect| rect.interactive)
                    .map(|rect| physical_cef_pointer_hit_rect(rect, scale)),
            );
        }
        set_native_layout_mouse_presenter(scale, browsers.native_mouse_move_presenter(&layout));
        if let Some(pointer) = vmux_layout::native_pointer::snapshot() {
            let result = queue_native_layout_pointer_move(
                pointer.position_px.x,
                pointer.position_px.y,
                pointer.buttons,
            );
            if result.owns_pointer {
                flush_native_layout_pointer_move();
            }
        }
        *state = LayoutHoverRefreshState::default();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(cursor_px) = vmux_layout::pane::pane_hover_cursor_position(window_entity, window)
        else {
            reset_layout_cef_hover(&browsers, &buttons, layout, &mut state);
            return;
        };
        let sequence = 0;
        let position = cursor_px / scale;
        let in_region = pointer_capture || cef_pointer_regions_contains(position, &cef_regions);
        NATIVE_LAYOUT_POINTER_INSIDE.store(in_region, Ordering::Relaxed);
        let unchanged = state.sequence == sequence
            && state.position == Some(position)
            && state.in_region == in_region;
        if unchanged {
            return;
        }
        if in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), position, false);
        } else if state.in_region {
            browsers.send_mouse_move(&layout, buttons.get_pressed(), position, true);
        }
        state.sequence = sequence;
        state.position = Some(position);
        state.in_region = in_region;
    }
}

#[derive(Default)]
struct WindowedHoverRefreshState {
    entity: Option<Entity>,
    position: Option<Vec2>,
}

fn refresh_active_windowed_hover(
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    modal_q: CommandBarStateQuery,
    active_q: Query<
        (
            Entity,
            &Transform,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&HostWindow>,
        ),
        (
            With<Browser>,
            With<WebviewWindowed>,
            With<CefKeyboardTarget>,
            Without<LayoutCef>,
            Without<Modal>,
            Without<Header>,
            Without<SideSheet>,
        ),
    >,
    mut state: Local<WindowedHoverRefreshState>,
) {
    if vmux_layout::command_bar::handler::is_command_bar_open(&modal_q) {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    if native_left_mouse_down() {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    let Some((entity, transform, computed, ui_gt, host_window)) = active_q.iter().next() else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    if transform.scale.x <= 1.0e-3 {
        *state = WindowedHoverRefreshState::default();
        return;
    }
    let Some(window_entity) = host_window
        .map(|host| host.0)
        .or_else(|| primary_window.single().ok())
    else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Some(cursor_px) = vmux_layout::pane::pane_hover_cursor_position(window_entity, window)
    else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Some(frame) = windowed_hover_refresh_frame(computed, ui_gt) else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    let Some(position) = windowed_hover_refresh_position(cursor_px, frame) else {
        *state = WindowedHoverRefreshState::default();
        return;
    };
    if state.entity == Some(entity) && state.position == Some(position) {
        return;
    }
    browsers.send_mouse_move(&entity, buttons.get_pressed(), position, false);
    state.entity = Some(entity);
    state.position = Some(position);
}

const LAYOUT_IDLE_FRAME_RATE: i32 = 10;
const LAYOUT_ACTIVE_FRAME_RATE: i32 = 60;
const LAYOUT_INPUT_BURST: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
struct LayoutFrameRateState {
    native_sequence: u64,
    last_input: Option<std::time::Instant>,
    dragging_layout: bool,
}

#[derive(Resource, Default)]
struct LayoutFrameRateBurst {
    last_emit: Option<std::time::Instant>,
}

fn request_layout_frame_burst(
    trigger: On<BinHostEmitEvent>,
    mut layouts: Query<&mut WebviewMaxFrameRate, With<LayoutCef>>,
    browsers: NonSend<Browsers>,
    mut burst: ResMut<LayoutFrameRateBurst>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    if !matches!(
        trigger.id.as_str(),
        TABS_EVENT | STACKS_EVENT | LAYOUT_COMMAND_BAR_OPEN_EVENT
    ) {
        return;
    }
    let Ok(mut cap) = layouts.get_mut(trigger.webview) else {
        return;
    };
    cap.0 = LAYOUT_ACTIVE_FRAME_RATE;
    browsers.set_windowless_frame_rate(&trigger.webview, LAYOUT_ACTIVE_FRAME_RATE);
    burst.last_emit = Some(std::time::Instant::now());
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn layout_frame_rate(
    now: std::time::Instant,
    last_input: Option<std::time::Instant>,
    native_activity: bool,
    dragging: bool,
) -> i32 {
    if native_activity
        || dragging
        || last_input.is_some_and(|last| now.saturating_duration_since(last) < LAYOUT_INPUT_BURST)
    {
        LAYOUT_ACTIVE_FRAME_RATE
    } else {
        LAYOUT_IDLE_FRAME_RATE
    }
}

fn sync_layout_cef_frame_rate(
    mut cursor_events: MessageReader<CursorMoved>,
    mut button_events: MessageReader<MouseButtonInput>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut key_events: MessageReader<KeyboardInput>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut layout_q: Query<(&mut WebviewMaxFrameRate, Has<CefKeyboardTarget>), With<LayoutCef>>,
    burst: Res<LayoutFrameRateBurst>,
    mut state: Local<LayoutFrameRateState>,
) {
    let owns_keyboard = layout_q.iter().any(|(_, keyboard_target)| keyboard_target);
    let inside = native_layout_pointer_is_inside();
    let pointer = vmux_layout::native_pointer::snapshot();
    let native_changed = pointer.is_some_and(|pointer| {
        if pointer.sequence == state.native_sequence {
            return false;
        }
        state.native_sequence = pointer.sequence;
        true
    });
    let pointer_moved = native_changed || cursor_events.read().count() > 0;
    let button_changed = button_events.read().count() > 0;
    let wheel_changed = wheel_events.read().count() > 0;
    // Typing into the command bar panel or the bookmark field moves no pointer, so without this
    // the layout webview paints those keystrokes at the idle rate and they look dropped.
    let key_changed = key_events.read().count() > 0;
    let input_changed = pointer_moved || button_changed || wheel_changed;
    let now = std::time::Instant::now();
    if (inside || state.dragging_layout) && (button_changed || wheel_changed)
        || (owns_keyboard && key_changed)
    {
        state.last_input = Some(now);
    }
    let native_dragging = pointer.is_some_and(|pointer| {
        pointer.buttons.left || pointer.buttons.right || pointer.buttons.middle
    });
    let any_pressed = native_dragging || buttons.get_pressed().next().is_some();
    if !any_pressed {
        state.dragging_layout = false;
    } else if inside && input_changed {
        state.dragging_layout = true;
    }
    // The AppKit monitor raises the activity flag on every layout-owned pointer move but only
    // lowers it on a move the layout does not own. While the command bar panel captures the whole
    // window there is no such move, so a flag left standing would pin the webview at the active
    // frame rate for as long as the panel is open. A frame with no new pointer sample means the
    // pointer stopped.
    if !native_changed && native_layout_activity_active() {
        set_native_layout_activity(false);
    }
    let desired = layout_frame_rate(
        now,
        state
            .last_input
            .max(burst.last_emit)
            .max(native_layout_scroll_at()),
        native_layout_activity_active(),
        state.dragging_layout,
    );
    let Ok((mut cap, _)) = layout_q.single_mut() else {
        return;
    };
    if cap.0 != desired {
        cap.0 = desired;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CommandBarWindowedFrame {
    left_px: f32,
    top_px: f32,
    width_px: f32,
    height_px: f32,
}

const COMMAND_BAR_NATIVE_RADIUS_PX: f32 = 16.0;

/// One consistent view of the command bar for the AppKit event thread.
///
/// The `NSEvent` monitor samples this on every key and mouse event, at arbitrary points relative to
/// the Bevy frame that wrote it. Publishing openness, hit frame, and scale as a single value stops
/// it observing a combination no frame ever produced — a stored frame left behind by a closed bar
/// used to turn clicks inside that rectangle into dismiss requests.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CommandBarRoute {
    generation: u64,
    owns_input: bool,
    /// Present only while the surface is on screen; a revealing bar has no clickable rectangle.
    frame: Option<CommandBarWindowedFrame>,
    scale: f32,
}

static NATIVE_COMMAND_BAR_ROUTE: LazyLock<Mutex<CommandBarRoute>> =
    LazyLock::new(|| Mutex::new(CommandBarRoute::default()));
#[cfg(target_os = "macos")]
static NATIVE_COMMAND_BAR_POINTER_EVENTS: LazyLock<Mutex<Vec<NativeCommandBarPointerEvent>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static NATIVE_COMMAND_BAR_DISMISS_REQUESTED: AtomicBool = AtomicBool::new(false);
static NATIVE_LEFT_MOUSE_DOWN: AtomicBool = AtomicBool::new(false);

fn native_command_bar_route() -> CommandBarRoute {
    *NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn publish_native_command_bar_route(
    owns_input: bool,
    frame: Option<CommandBarWindowedFrame>,
    scale: f32,
) {
    let mut stored = NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let generation = stored.generation.wrapping_add(1);
    *stored = CommandBarRoute {
        generation,
        owns_input,
        frame,
        scale,
    };
    if !owns_input {
        NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(false, Ordering::Relaxed);
    }
}

pub fn native_command_bar_is_open() -> bool {
    native_command_bar_route().owns_input
}

#[cfg(target_os = "macos")]
pub fn native_command_bar_contains_point(x_px: f32, y_px: f32) -> bool {
    native_command_bar_local_position(x_px, y_px).is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn native_command_bar_contains_point(_: f32, _: f32) -> bool {
    false
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum NativeCommandBarPointerEvent {
    Move {
        position: Vec2,
        buttons: NativeMouseButtons,
    },
    Button {
        position: Vec2,
        button: PointerButton,
        released: bool,
    },
}

pub fn set_native_left_mouse_down(down: bool) {
    NATIVE_LEFT_MOUSE_DOWN.store(down, Ordering::Relaxed);
}

pub fn native_left_mouse_down() -> bool {
    NATIVE_LEFT_MOUSE_DOWN.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
fn native_command_bar_local_position(x_px: f32, y_px: f32) -> Option<Vec2> {
    let route = native_command_bar_route();
    if !route.owns_input {
        return None;
    }
    let frame = route
        .frame
        .filter(|frame| command_bar_windowed_frame_contains(*frame, Vec2::new(x_px, y_px)))?;
    let scale = route.scale.max(1.0e-6);
    Some(Vec2::new(
        (x_px - frame.left_px) / scale,
        (y_px - frame.top_px) / scale,
    ))
}

#[cfg(target_os = "macos")]
pub fn queue_native_command_bar_pointer_move(
    x_px: f32,
    y_px: f32,
    buttons: NativeMouseButtons,
) -> bool {
    let Some(position) = native_command_bar_local_position(x_px, y_px) else {
        return false;
    };
    NATIVE_COMMAND_BAR_POINTER_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(NativeCommandBarPointerEvent::Move { position, buttons });
    true
}

#[cfg(target_os = "macos")]
pub fn queue_native_command_bar_pointer_button(
    x_px: f32,
    y_px: f32,
    button: u8,
    released: bool,
) -> bool {
    let Some(position) = native_command_bar_local_position(x_px, y_px) else {
        return false;
    };
    let button = match button {
        1 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        _ => PointerButton::Primary,
    };
    NATIVE_COMMAND_BAR_POINTER_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(NativeCommandBarPointerEvent::Button {
            position,
            button,
            released,
        });
    true
}

fn command_bar_windowed_frame(
    window_width_px: f32,
    window_height_px: f32,
    scale: f32,
    measured_size: Option<Vec2>,
    bounds: Option<WindowedFrameRect>,
) -> Option<CommandBarWindowedFrame> {
    if !window_width_px.is_finite()
        || !window_height_px.is_finite()
        || !scale.is_finite()
        || window_width_px <= 0.0
        || window_height_px <= 0.0
        || scale <= 0.0
    {
        return None;
    }

    const MARGIN: f32 = 16.0;
    const MAX_W: f32 = 576.0;
    const MIN_W: f32 = 240.0;
    const MIN_H: f32 = 56.0;
    const FALLBACK_H: f32 = 360.0;

    let window_bounds = WindowedFrameRect {
        left: 0.0,
        top: 0.0,
        width: window_width_px,
        height: window_height_px,
    };
    let bounds = bounds.unwrap_or(window_bounds);
    let area_left = bounds.left / scale;
    let area_top = bounds.top / scale;
    let area_w = bounds.width / scale;
    let area_h = bounds.height / scale;
    let top = area_top + area_h * 0.15;
    let available_w = (area_w - MARGIN * 2.0).max(1.0);
    let min_w = MIN_W.min(available_w);
    let box_w = available_w.min(MAX_W).max(min_w);
    let available_h = (area_top + area_h - top - MARGIN).max(1.0);
    let min_h = MIN_H.min(available_h);
    let measured_h = measured_size
        .map(|size| size.y)
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(FALLBACK_H);
    let box_h = measured_h.min(available_h).max(min_h);
    let box_x = area_left + ((area_w - box_w) * 0.5).max(0.0);

    Some(CommandBarWindowedFrame {
        left_px: box_x * scale,
        top_px: top * scale,
        width_px: box_w * scale,
        height_px: box_h * scale,
    })
}

#[cfg(target_os = "macos")]
fn windowed_frames_union(frames: &[WindowedFrameRect]) -> Option<WindowedFrameRect> {
    let first = *frames.first()?;
    let mut left = first.left;
    let mut top = first.top;
    let mut right = first.right();
    let mut bottom = first.bottom();
    for frame in &frames[1..] {
        left = left.min(frame.left);
        top = top.min(frame.top);
        right = right.max(frame.right());
        bottom = bottom.max(frame.bottom());
    }
    Some(WindowedFrameRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(target_os = "macos")]
fn native_windowed_page_bounds() -> Option<WindowedFrameRect> {
    let frames = NATIVE_WINDOWED_PAGE_FRAMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    windowed_frames_union(&frames)
}

#[cfg(not(target_os = "macos"))]
fn native_windowed_page_bounds() -> Option<WindowedFrameRect> {
    None
}

fn hide_windowed_command_bar(browsers: &Browsers, entity: Entity) {
    browsers.set_windowed_hidden(&entity, true);
}

fn command_bar_windowed_click_should_dismiss(
    open: bool,
    button: MouseButton,
    state: ButtonState,
    cursor: Option<Vec2>,
    frame: Option<CommandBarWindowedFrame>,
) -> bool {
    if !open || button != MouseButton::Left || state != ButtonState::Pressed {
        return false;
    }
    let (Some(cursor), Some(frame)) = (cursor, frame) else {
        return false;
    };
    !command_bar_windowed_frame_contains(frame, cursor)
}

fn command_bar_windowed_frame_contains(frame: CommandBarWindowedFrame, cursor: Vec2) -> bool {
    cursor.x >= frame.left_px
        && cursor.x <= frame.left_px + frame.width_px
        && cursor.y >= frame.top_px
        && cursor.y <= frame.top_px + frame.height_px
}

pub fn request_native_command_bar_dismiss() -> bool {
    if !native_command_bar_route().owns_input {
        return false;
    }
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(true, Ordering::Relaxed);
    true
}

pub fn request_native_command_bar_dismiss_for_mouse_down(x_px: f32, y_px: f32) -> bool {
    if !x_px.is_finite() || !y_px.is_finite() {
        return false;
    }
    let route = native_command_bar_route();
    if !route.owns_input {
        return false;
    }
    let Some(frame) = route.frame else {
        return false;
    };
    if command_bar_windowed_frame_contains(frame, Vec2::new(x_px, y_px)) {
        return false;
    }
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.store(true, Ordering::Relaxed);
    true
}

pub fn take_native_command_bar_dismiss_requested() -> bool {
    NATIVE_COMMAND_BAR_DISMISS_REQUESTED.swap(false, Ordering::Relaxed)
}

/// The surface exists but must stay off screen — either prewarmed before any open, or revealing
/// while the page paints. Both keep the native view alive and parked outside the window so it can
/// still hold first responder.
fn command_bar_windowed_view_should_render_hidden(
    display: Display,
    visibility: Visibility,
) -> bool {
    display != Display::None && visibility == Visibility::Hidden
}

fn sync_windowed_command_bar(
    browsers: NonSend<Browsers>,
    modal_q: Query<
        (
            Entity,
            &Node,
            &Visibility,
            Has<CefKeyboardTarget>,
            Has<WebviewWindowed>,
            Option<&HostWindow>,
            Option<&CommandBarNativeSize>,
        ),
        With<Modal>,
    >,
    native_size_changed: Query<(), Changed<CommandBarNativeSize>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut was_open: Local<bool>,
) {
    let matched = modal_q.single();
    let Ok((entity, node, visibility, has_keyboard_target, is_windowed, host_window, native_size)) =
        matched
    else {
        publish_native_command_bar_route(false, None, 1.0);
        *was_open = false;
        return;
    };
    let state = CommandBarState::from_modal(node.display, *visibility, has_keyboard_target);
    let open = state.is_shown();
    let owns_input = state.owns_input();
    let render_hidden = command_bar_windowed_view_should_render_hidden(node.display, *visibility);
    if !open && !render_hidden {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            browsers.set_windowed_focus(&entity, false);
            hide_windowed_command_bar(&browsers, entity);
        }
        *was_open = false;
        return;
    }
    if !browsers.has_browser(entity) {
        publish_native_command_bar_route(owns_input, None, 1.0);
        return;
    }
    let window_entity = host_window
        .map(|h| h.0)
        .or_else(|| primary_window.single().ok());
    let Some(window_entity) = window_entity else {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            hide_windowed_command_bar(&browsers, entity);
        }
        return;
    };
    let Ok(window) = windows.get(window_entity) else {
        publish_native_command_bar_route(owns_input, None, 1.0);
        if is_windowed {
            hide_windowed_command_bar(&browsers, entity);
        }
        return;
    };
    let scale = window.resolution.scale_factor();
    if !is_windowed {
        let frame = if open {
            native_size.map(|size| CommandBarWindowedFrame {
                left_px: size.shell_left * scale,
                top_px: size.shell_top * scale,
                width_px: size.shell_width * scale,
                height_px: size.shell_height * scale,
            })
        } else {
            None
        };
        publish_native_command_bar_route(owns_input, frame, scale);
        *was_open = open;
        return;
    }
    if render_hidden {
        publish_native_command_bar_route(owns_input, None, scale);
        let Some(frame) = command_bar_windowed_frame(
            window.resolution.physical_width() as f32,
            window.resolution.physical_height() as f32,
            scale,
            native_size.map(|size| Vec2::new(size.shell_width, size.shell_height)),
            native_windowed_page_bounds(),
        ) else {
            hide_windowed_command_bar(&browsers, entity);
            return;
        };
        // A revealing bar already owns input, so keep the renderer focused; only a prewarmed,
        // never-opened surface gets unfocused here.
        if !owns_input {
            browsers.set_windowed_focus(&entity, false);
        }
        browsers.resize(
            &entity,
            Vec2::new(frame.width_px / scale, frame.height_px / scale),
            scale,
        );
        browsers.set_windowed_frame(
            &entity,
            -frame.width_px - 16.0 * scale,
            -frame.height_px - 16.0 * scale,
            frame.width_px,
            frame.height_px,
            scale,
        );
        browsers.set_windowed_hidden(&entity, false);
        *was_open = false;
        return;
    }
    let measured = native_size.map(|size| Vec2::new(size.shell_width, size.shell_height));
    let Some(frame) = command_bar_windowed_frame(
        window.resolution.physical_width() as f32,
        window.resolution.physical_height() as f32,
        scale,
        measured,
        native_windowed_page_bounds(),
    ) else {
        publish_native_command_bar_route(owns_input, None, scale);
        hide_windowed_command_bar(&browsers, entity);
        return;
    };
    publish_native_command_bar_route(owns_input, Some(frame), scale);

    browsers.set_windowed_frame(
        &entity,
        frame.left_px,
        frame.top_px,
        frame.width_px,
        frame.height_px,
        scale,
    );
    browsers.resize(
        &entity,
        Vec2::new(frame.width_px / scale, frame.height_px / scale),
        scale,
    );
    // A windowed CEF view cannot be transparent, so the page's own `rounded-2xl` leaves opaque
    // square corners behind it. Clip the outer view instead. This only touches the outermost
    // `CefBrowserHostView` layer, unlike `set_windowed_z_position`, which reorders that layer among
    // its siblings and leaves the view painting nothing but its background.
    browsers.set_windowed_corner_radius(&entity, COMMAND_BAR_NATIVE_RADIUS_PX * scale, scale, true);
    browsers.set_windowed_hidden(&entity, false);
    // Frontmost sibling wins AppKit hit-testing; the raise is a no-op once it already is.
    browsers.raise_windowed_to_front(&entity);
    browsers.set_windowed_focus(&entity, true);
    if !*was_open || native_size_changed.contains(entity) {
        browsers.nudge_windowed_repaint(&entity);
        *was_open = true;
    }
}

#[cfg(target_os = "macos")]
fn flush_native_command_bar_pointer_events(
    browsers: NonSend<Browsers>,
    modal_q: Query<Entity, (With<Modal>, With<WebviewWindowed>)>,
) {
    let Ok(entity) = modal_q.single() else {
        return;
    };
    let events = {
        let mut pending = NATIVE_COMMAND_BAR_POINTER_EVENTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *pending)
    };
    for event in events {
        match event {
            NativeCommandBarPointerEvent::Move { position, buttons } => {
                browsers.send_native_mouse_move(&entity, buttons, position, false);
            }
            NativeCommandBarPointerEvent::Button {
                position,
                button,
                released,
            } => {
                bevy::log::info!(
                    ?entity,
                    ?position,
                    ?button,
                    released,
                    "command bar native pointer forwarded"
                );
                browsers.send_mouse_click(&entity, position, button, released);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn flush_native_command_bar_pointer_events() {}

fn apply_repaint_nudge(browsers: NonSend<Browsers>, ready: Query<Entity, Changed<PageReady>>) {
    for entity in &ready {
        browsers.nudge_windowed_repaint(&entity);
    }
}

fn sync_cef_webview_resize_after_ui(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), (With<Browser>, Without<Modal>)>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut last_entries: Local<Vec<(u64, Vec2, f32)>>,
    mut window_resized: MessageReader<WindowResized>,
    mut first_run: Local<Option<std::time::Instant>>,
) {
    // Force-resize all CEF browsers (tabs, terminals, side sheets, modals) on
    // window resize so backgrounded surfaces also repaint at the new size
    // instead of showing a stale frame until they become active.
    let force = window_resized.read().count() > 0;
    if force {
        last_entries.clear();
    }
    let mut pushed_any = false;
    let mut awaiting_create = false;
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            awaiting_create = true;
            continue;
        }
        let key = entity.to_bits();
        let window_entity = host_window
            .get(entity)
            .ok()
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let device_scale_factor = window_entity
            .and_then(|e| windows.get(e).ok())
            .map(|w| w.resolution.scale_factor())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        if last_entries
            .iter()
            .any(|(k, s, sf)| *k == key && *s == size.0 && (*sf - device_scale_factor).abs() < 0.01)
        {
            continue;
        }
        browsers.resize(&entity, size.0, device_scale_factor);
        webview_debug_log(format!(
            "resize entity={entity:?} size={:?} scale={device_scale_factor} force={force}",
            size.0
        ));
        pushed_any = true;
        if let Some(entry) = last_entries.iter_mut().find(|(k, _, _)| *k == key) {
            entry.1 = size.0;
            entry.2 = device_scale_factor;
        } else {
            last_entries.push((key, size.0, device_scale_factor));
        }
    }
    let within_startup_grace = first_run
        .get_or_insert_with(std::time::Instant::now)
        .elapsed()
        < std::time::Duration::from_secs(10);
    if windowed_reconcile_should_wake(pushed_any, awaiting_create, within_startup_grace)
        && let Some(proxy) = proxy.as_ref()
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn windowed_reconcile_should_wake(
    pushed_any: bool,
    awaiting_create: bool,
    within_startup_grace: bool,
) -> bool {
    pushed_any || (awaiting_create && within_startup_grace)
}

/// Walks up from a browser entity to find its enclosing Tab, then counts
/// leaf panes under that tab. Returns None if the parent chain doesn't
/// reach a Tab.
fn pane_count_for_browser(
    browser_e: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<(), With<Tab>>,
    _pane_q: &Query<(), With<Pane>>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<usize> {
    let mut cur = browser_e;
    let tab = loop {
        let parent = child_of_q.get(cur).ok()?.get();
        if tab_q.get(parent).is_ok() {
            break parent;
        }
        cur = parent;
    };
    let mut leaves = Vec::new();
    collect_leaf_panes(tab, all_children, leaf_panes, &mut leaves);
    Some(leaves.len())
}

fn sync_webview_pane_corner_clip(
    settings: Res<AppSettings>,
    layout_hidden: Res<vmux_layout::toggle::LayoutHidden>,
    mode: Res<vmux_layout::scene::InteractionMode>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    tabs: Query<
        (
            Entity,
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        (With<Browser>, Without<LayoutCef>, Without<Modal>),
    >,
    status: Query<
        (
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        With<Header>,
    >,
    side_sheet: Query<
        (
            &WebviewSize,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        With<SideSheet>,
    >,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<(), With<Tab>>,
    pane_q: Query<(), With<Pane>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) {
    let r = settings.layout.radius;
    for (browser_e, size, mat_h) in &tabs {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        let pane_count = pane_count_for_browser(
            browser_e,
            &child_of_q,
            &tab_q,
            &pane_q,
            &all_children,
            &leaf_panes,
        )
        .unwrap_or(1);
        let corner_mode = if *mode == vmux_layout::scene::InteractionMode::Player
            || layout_hidden.0
            || pane_count > 1
        {
            0.0
        } else {
            1.0
        };
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, corner_mode);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
    for (size, mat_h) in &side_sheet {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mut mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
            mat.base.alpha_mode = webview_content_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
}

fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<
        (
            Entity,
            Option<&Visibility>,
            Option<&ComputedNode>,
            Has<PendingWebviewReveal>,
            Has<PendingCommandBarReveal>,
            Has<Modal>,
            Has<CefKeyboardTarget>,
            Has<WebviewWindowed>,
            Has<LayoutCef>,
            Has<BookmarkTextInputActive>,
            Has<BookmarkContextMenuActive>,
            Has<CommandBarPanelActive>,
        ),
        With<WebviewSource>,
    >,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    child_of_q: Query<&ChildOf>,

    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
    mut last_active: Local<Option<Entity>>,
    mut last_ready_set: Local<Vec<Entity>>,
) {
    ready.clear();
    let mut layout_shells = Vec::new();
    let mut modal_keyboard_target = None;
    let mut layout_keyboard_target = None;
    let window_visible = primary_window.visible;
    let window_focused = primary_window.focused;
    for (
        entity,
        visibility,
        computed,
        pending_reveal,
        pending_command_bar_reveal,
        is_modal,
        has_keyboard_target,
        is_windowed,
        is_layout,
        bookmark_text_input_active,
        bookmark_context_menu_active,
        command_bar_panel_active,
    ) in webviews.iter()
    {
        if !browsers.has_browser(entity) {
            continue;
        }
        let size = computed.map(|node| node.size).unwrap_or(Vec2::ONE);
        if webview_osr_should_run(
            size,
            visibility,
            pending_reveal || pending_command_bar_reveal,
        ) {
            ready.push(entity);
            if is_layout {
                layout_shells.push(entity);
                if bookmark_text_input_active
                    || bookmark_context_menu_active
                    || command_bar_panel_active
                {
                    layout_keyboard_target = Some(entity);
                }
            }
            if is_modal && has_keyboard_target {
                modal_keyboard_target = Some((entity, is_windowed));
            }
        } else if keep_hidden_osr_webview_warm(is_modal, is_windowed, window_visible) {
            browsers.set_osr_not_hidden(&entity);
        } else {
            browsers.set_osr_hidden(&entity);
        }
    }
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());
    let active_stack_opt = focus.stack;
    let active_stack = active_stack_opt.and_then(|tab| {
        ready
            .iter()
            .copied()
            .find(|&b| child_of_q.get(b).ok().map(|co| co.get()) == Some(tab))
    });
    let active = layout_keyboard_target
        .or_else(|| choose_osr_active_webview(modal_keyboard_target, active_stack, ready[0]));

    if !window_visible {
        if last_active.is_some() || *last_ready_set != *ready {
            webview_debug_log(format!("osr focus window_hidden ready={ready:?}"));
            browsers.sync_osr_focus_to_active_pane(None, &[]);
            *last_active = None;
            last_ready_set.clone_from(&ready);
        }
    } else if !window_focused {
        if last_active.is_some() || *last_ready_set != *ready {
            webview_debug_log(format!("osr focus window_unfocused ready={ready:?}"));
            browsers.sync_osr_focus_to_active_pane(None, &[]);
            *last_active = None;
            last_ready_set.clone_from(&ready);
        }
    } else if *last_active == active && *last_ready_set == *ready {
    } else {
        auxiliary.clear();
        let (active, next_auxiliary) = osr_focus_targets(
            ready.as_slice(),
            active,
            layout_keyboard_target.is_some(),
            |e| layout_shells.contains(&e),
        );
        auxiliary.extend(next_auxiliary);
        webview_debug_log(format!(
            "osr focus active={active:?} auxiliary={:?} ready={ready:?}",
            auxiliary.as_slice()
        ));
        browsers.sync_osr_focus_to_active_pane(active, auxiliary.as_slice());
        *last_active = active;
        last_ready_set.clone_from(&ready);
    }
    for &e in ready.iter() {
        let mut parent_is_stack = false;
        let mut pane_is_leaf = false;
        let mut is_active = false;
        let mut is_prev = false;

        if let Ok(parent) = child_of_q.get(e).map(|co| co.get()) {
            parent_is_stack = tab_ts.get(parent).is_ok();
            if parent_is_stack && let Ok(pane) = child_of_q.get(parent).map(|co| co.get()) {
                pane_is_leaf = leaf_panes.contains(pane);
                if pane_is_leaf {
                    is_active =
                        active_stack_in_pane(pane, &pane_children_q, &tab_ts) == Some(parent);
                    // Keep previous tab's webview visible while an empty new tab is
                    // pending (user is picking content in the command bar).
                    is_prev = new_stack_ctx.stack.is_some()
                        && new_stack_ctx.previous_stack == Some(parent);
                }
            }
        }

        if should_show_osr_webview(
            window_visible,
            parent_is_stack,
            pane_is_leaf,
            is_active,
            is_prev,
        ) {
            browsers.set_osr_not_hidden(&e);
        } else {
            browsers.set_osr_hidden(&e);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiddenWebviewSizing {
    Render,
    HideKeepSize,
    Collapse,
}

fn hidden_webview_sizing(renderable: bool, under_inactive_tab: bool) -> HiddenWebviewSizing {
    if renderable {
        HiddenWebviewSizing::Render
    } else if under_inactive_tab {
        HiddenWebviewSizing::HideKeepSize
    } else {
        HiddenWebviewSizing::Collapse
    }
}

fn webview_layout_is_renderable(
    size_px: Vec2,
    visibility: Option<&Visibility>,
    pending_reveal: bool,
) -> bool {
    (pending_reveal || !matches!(visibility, Some(Visibility::Hidden)))
        && size_px.x > 0.0
        && size_px.y > 0.0
}

fn webview_osr_should_run(
    size_px: Vec2,
    visibility: Option<&Visibility>,
    pending_reveal: bool,
) -> bool {
    pending_reveal || webview_layout_is_renderable(size_px, visibility, false)
}

fn keep_hidden_osr_webview_warm(is_modal: bool, is_windowed: bool, window_visible: bool) -> bool {
    is_modal && !is_windowed && window_visible
}

fn choose_osr_active_webview(
    modal_keyboard_target: Option<(Entity, bool)>,
    active_stack: Option<Entity>,
    fallback: Entity,
) -> Option<Entity> {
    if modal_keyboard_target.is_some_and(|(_, is_windowed)| is_windowed) {
        None
    } else {
        modal_keyboard_target
            .map(|(entity, _)| entity)
            .or(active_stack)
            .or(Some(fallback))
    }
}

fn osr_focus_targets(
    ready: &[Entity],
    active: Option<Entity>,
    allow_layout_active: bool,
    mut is_layout: impl FnMut(Entity) -> bool,
) -> (Option<Entity>, Vec<Entity>) {
    let active = active.filter(|&e| allow_layout_active || !is_layout(e));
    let auxiliary = ready
        .iter()
        .copied()
        .filter(|&e| Some(e) != active)
        .collect();
    (active, auxiliary)
}

fn should_show_osr_webview(
    window_visible: bool,
    parent_is_stack: bool,
    pane_is_leaf: bool,
    stack_is_active: bool,
    stack_is_previous_new_stack: bool,
) -> bool {
    if !window_visible {
        return false;
    }
    if !parent_is_stack || !pane_is_leaf {
        return true;
    }
    stack_is_active || stack_is_previous_new_stack
}

fn drain_loading_state(
    receiver: Res<WebviewLoadingStateReceiver>,
    mut commands: Commands,
    mut completed: MessageWriter<WebviewLoadCompleted>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        let Ok(mut ecmds) = commands.get_entity(ev.webview) else {
            continue;
        };
        if ev.is_loading {
            ecmds.insert(Loading);
        } else {
            ecmds.remove::<Loading>();
            completed.write(WebviewLoadCompleted {
                webview: ev.webview,
            });
        }
        ecmds.insert(NavigationState {
            can_go_back: ev.can_go_back,
            can_go_forward: ev.can_go_forward,
        });
    }
}

fn drain_committed_navigation(
    receiver: Res<WebviewCommittedNavigationReceiver>,
    infrastructure: Res<crate::extensions::bridge_page::ExtensionInfrastructureEntities>,
    mut writer: MessageWriter<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if infrastructure.contains(ev.webview) {
            continue;
        }
        writer.write(ev);
    }
}

#[cfg(test)]
#[path = "lib.committed_navigation.test.rs"]
mod committed_navigation_tests;
fn spawn_popup_stacks(
    popup_rx: Res<WebviewPopupReceiver>,
    child_of_q: Query<&ChildOf>,
    stack_q: Query<(), With<Stack>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    while let Ok(ev) = popup_rx.0.try_recv() {
        if ev.target_url.is_empty() {
            continue;
        }
        let Ok(stack_co) = child_of_q.get(ev.webview) else {
            continue;
        };
        let stack = stack_co.get();
        if !stack_q.contains(stack) {
            continue;
        }
        let Ok(pane_co) = child_of_q.get(stack) else {
            continue;
        };
        let pane = pane_co.get();
        if !leaf_panes.contains(pane) {
            continue;
        }
        let new_stack = commands
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        commands.spawn((
            Browser::new(&mut meshes, &mut webview_mt, &ev.target_url),
            ChildOf(new_stack),
        ));
    }
}

fn flush_pending_osr_textures(
    mut ew: MessageWriter<RenderTextureMessage>,
    browsers: NonSend<Browsers>,
) {
    for texture in browsers.drain_render_textures() {
        ew.write(texture);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutWindowPadding {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

fn val_px(value: Val) -> f32 {
    match value {
        Val::Px(px) => px,
        _ => 0.0,
    }
}

fn layout_window_padding_from_node(node: &Node) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: val_px(node.padding.top),
        right: val_px(node.padding.right),
        bottom: val_px(node.padding.bottom),
        left: val_px(node.padding.left),
    }
}

fn layout_window_padding_from_settings(settings: &AppSettings) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: settings.layout.window.pad_top(),
        right: settings.layout.window.pad_right(),
        bottom: settings.layout.window.pad_bottom(),
        left: settings.layout.window.pad_left(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutFixedOffsets {
    left: f32,
    top: f32,
    right: f32,
    height: f32,
}

fn layout_fixed_offsets_from_computed(
    computed: &ComputedNode,
    transform: &UiGlobalTransform,
    window_width_px: f32,
) -> Option<LayoutFixedOffsets> {
    if computed.size.x <= 0.0 || computed.size.y <= 0.0 || window_width_px <= 0.0 {
        return None;
    }

    let inverse_scale = computed.inverse_scale_factor.max(1.0e-6);
    let size = computed.size * inverse_scale;
    let center = transform.transform_point2(Vec2::ZERO) * inverse_scale;
    let window_width = window_width_px * inverse_scale;
    let left = center.x - size.x * 0.5;
    let top = center.y - size.y * 0.5;
    let right = window_width - (center.x + size.x * 0.5);

    Some(LayoutFixedOffsets {
        left,
        top,
        right,
        height: size.y,
    })
}

fn push_layout_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    header_q: Query<(Has<Open>, Option<&ComputedNode>, Option<&UiGlobalTransform>), With<Header>>,
    side_sheet_q: Query<(&SideSheetPosition, Has<Open>), With<SideSheet>>,
    window_q: Query<&Node, With<VmuxWindow>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    side_sheet_width: Res<SideSheetWidth>,
    settings: Res<AppSettings>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    let window_padding = window_q
        .single()
        .ok()
        .map(layout_window_padding_from_node)
        .unwrap_or_else(|| layout_window_padding_from_settings(&settings));
    let header_open = header_q.iter().any(|(is_open, _, _)| is_open);
    let window_width_px = windows
        .single()
        .ok()
        .map(|window| window.resolution.physical_width() as f32)
        .unwrap_or(0.0);
    let header_offsets = header_q.iter().find_map(|(_, computed, transform)| {
        let computed = computed?;
        let transform = transform?;
        layout_fixed_offsets_from_computed(computed, transform, window_width_px)
    });

    let payload = LayoutStateEvent {
        header_open,
        side_sheet_open: side_sheet_q
            .iter()
            .any(|(pos, is_open)| *pos == SideSheetPosition::Left && is_open),
        header_height: header_offsets
            .map(|offsets| offsets.height)
            .unwrap_or(HEADER_HEIGHT_PX),
        side_sheet_width: side_sheet_width.0,
        pane_gap: vmux_layout::event::PANE_GAP_PX,
        radius: settings.layout.radius,
        header_left: header_offsets.map(|offsets| offsets.left),
        header_top: header_offsets.map(|offsets| offsets.top),
        header_right: header_offsets.map(|offsets| offsets.right),
        window_pad_top: window_padding.top,
        window_pad_right: window_padding.right,
        window_pad_bottom: window_padding.bottom,
        window_pad_left: window_padding.left,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        LAYOUT_STATE_EVENT,
        &payload,
    ));
    *last = body;
}

fn should_emit_new_stack_placeholder(
    pending_stack: Option<Entity>,
    active_stack: Option<Entity>,
    rows: &[StackRow],
) -> bool {
    let Some(pending_stack) = pending_stack else {
        return false;
    };
    if active_stack != Some(pending_stack) {
        return false;
    }
    !rows
        .iter()
        .any(|row| row.is_active && !row.url.is_empty() && row.url != "about:blank")
}

fn should_emit_cached_payload(body: &str, last: &str, page_ready_changed: bool) -> bool {
    page_ready_changed || body != last
}

fn push_stacks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    browser_q: Query<
        (
            &PageMetadata,
            &ChildOf,
            Option<&NavigationState>,
            Option<&OscTitle>,
        ),
        With<Browser>,
    >,
    stack_q: Query<(), With<Stack>>,
    zoomed_q: Query<(), With<vmux_layout::pane::Zoomed>>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    let active_pane = focus.pane;
    let active_stack_opt = focus.stack;
    if let Some(active_stack_entity) = active_stack_opt
        && !stack_q.contains(active_stack_entity)
    {
        return;
    }
    let mut rows: Vec<StackRow> = Vec::new();
    let mut can_go_back = false;
    let mut can_go_forward = false;
    let _ = active_stack_opt.is_none();
    if let Some(active_stack_entity) = active_stack_opt {
        for (meta, child_of, nav_state, osc) in &browser_q {
            let stack_entity = child_of.get();
            let stack_pane = child_of_q.get(stack_entity).ok().map(|co| co.get());
            if stack_pane != active_pane {
                continue;
            }
            let is_active = stack_entity == active_stack_entity;
            if is_active && let Some(ns) = nav_state {
                can_go_back = ns.can_go_back;
                can_go_forward = ns.can_go_forward;
            }
            rows.push(StackRow {
                title: effective_title(osc, &meta.title).to_string(),
                url: meta.url.clone(),
                icon: meta.icon.clone(),
                is_active,
                bg_color: meta.bg_color.clone(),
            });
        }
    }
    if should_emit_new_stack_placeholder(new_stack_ctx.stack, active_stack_opt, &rows) {
        rows.retain(|r| !r.is_active);
        rows.push(StackRow {
            title: "New Stack".to_string(),
            url: String::new(),
            icon: vmux_core::PageIcon::None,
            is_active: true,
            bg_color: None,
        });
    }
    if active_stack_opt.is_some() && rows.is_empty() {
        return;
    }
    let is_zoomed = focus.tab.map(|t| zoomed_q.get(t).is_ok()).unwrap_or(false);
    let payload = StacksHostEvent {
        stacks: rows,
        can_go_back,
        can_go_forward,
        is_zoomed,
    };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, STACKS_EVENT, &payload));
    *last = ron_body;
}

fn apply_page_icons(
    manifests: Query<&vmux_core::page::PageManifest>,
    mut metas: Query<&mut PageMetadata, Changed<PageMetadata>>,
) {
    for mut meta in &mut metas {
        if meta.icon.is_none() {
            if meta.url.starts_with("file:") {
                meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Files);
                continue;
            }
            if meta.url.starts_with("chrome-extension://") {
                meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Puzzle);
                continue;
            }
        }
        let Some(host) = meta
            .url
            .strip_prefix("vmux://")
            .and_then(|rest| rest.split('/').next())
            .filter(|host| !host.is_empty() && *host != "agent")
        else {
            continue;
        };
        let Some(manifest) = manifests.iter().find(|manifest| manifest.host == host) else {
            continue;
        };
        if meta.icon.is_none()
            && let Some(builtin) = manifest.icon
        {
            meta.icon = vmux_core::PageIcon::Builtin(builtin);
        }
        if !manifest.title.is_empty() && meta.title == meta.url {
            meta.title = manifest.title.to_string();
        }
    }
}

#[cfg(test)]
#[path = "lib.apply_page_icons.test.rs"]
mod apply_page_icons_tests;
fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    new_stack_ctx: Res<vmux_layout::NewStackContext>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    tab_q: Query<(), With<Tab>>,
    tab_sections: Query<&SideSheetSectionsExpanded, With<Tab>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    collapsed_panes: Query<(), With<SideSheetCardCollapsed>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<(&PageMetadata, Has<Loading>, Option<&OscTitle>), With<Browser>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let active_pane = focus.pane;

    let Some(tab_e) = focus.tab else {
        return;
    };
    if !tab_q.contains(tab_e) {
        return;
    }
    let sections = tab_sections.get(tab_e).copied().unwrap_or_default();
    let mut tab_leaf_panes = Vec::new();
    collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut tab_leaf_panes);

    let mut panes: Vec<PaneNode> = Vec::new();
    for &pane_entity in &tab_leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let active_stack = active_stack_in_pane(pane_entity, &pane_children, &stack_ts);
        let mut stacks: Vec<StackNode> = Vec::new();
        let mut stack_index: usize = 0;
        if let Ok(children) = pane_children.get(pane_entity) {
            for child in children.iter() {
                if !stack_q.contains(child) {
                    continue;
                }
                let stack_is_active = active_stack == Some(child);
                let mut found_browser = false;
                if let Ok(stack_kids) = stack_children.get(child) {
                    for browser_e in stack_kids.iter() {
                        if let Ok((meta, loading, osc)) = browser_meta.get(browser_e) {
                            let is_new_stack = new_stack_ctx.stack == Some(child)
                                && (meta.url.is_empty() || meta.url == "about:blank");
                            stacks.push(StackNode {
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    effective_title(osc, &meta.title).to_string()
                                },
                                url: if is_new_stack {
                                    String::new()
                                } else {
                                    meta.url.clone()
                                },
                                icon: if is_new_stack {
                                    vmux_core::PageIcon::None
                                } else {
                                    meta.icon.clone()
                                },
                                is_active: stack_is_active,
                                stack_index: stack_index as u32,
                                is_loading: loading,
                                bg_color: meta.bg_color.clone(),
                            });
                            found_browser = true;
                        }
                    }
                }
                if !found_browser {
                    stacks.push(StackNode {
                        title: "New Stack".to_string(),
                        url: String::new(),
                        icon: vmux_core::PageIcon::None,
                        is_active: stack_is_active,
                        stack_index: stack_index as u32,
                        is_loading: false,
                        bg_color: None,
                    });
                }
                stack_index += 1;
            }
        }
        panes.push(PaneNode {
            id: pane_entity.to_bits(),
            is_active,
            collapsed: collapsed_panes.contains(pane_entity),
            projects_expanded: sections.projects,
            bookmarks_expanded: sections.bookmarks,
            knowledge_expanded: sections.knowledge,
            tools_expanded: sections.tools,
            stacks,
        });
    }
    let payload = PaneTreeEvent { panes };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        PANE_TREE_EVENT,
        &payload,
    ));
    *last = ron_body;
}

fn tab_boundary_dir(
    tab: &Tab,
    settings: &AppSettings,
    active_space: Option<&vmux_space::spaces::ActiveSpace>,
) -> Option<(std::path::PathBuf, vmux_setting::DirSource)> {
    match tab.startup_dir.as_deref() {
        Some(path) => Some((
            vmux_setting::validate_tab_workspace_dir(path)
                .unwrap_or_else(|_| std::path::PathBuf::from(path)),
            vmux_setting::DirSource::Tab,
        )),
        None => {
            let active_space = active_space?;
            vmux_setting::resolve_startup_dir_for_tab_with_source(
                settings,
                &active_space.record.id,
                None,
            )
        }
    }
}

fn abbreviate_home(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        if !home.is_empty()
            && let Some(rest) = s.strip_prefix(home.as_ref())
        {
            return format!("~{rest}");
        }
    }
    s.into_owned()
}

/// Emit the active tab's working-directory boundary from the shared event-driven Git cache.
#[allow(clippy::too_many_arguments)]
fn push_tab_boundary_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    tabs: Query<&Tab>,
    worktrees: Query<&TabWorktree>,
    settings: Res<AppSettings>,
    active_space: Option<Res<vmux_space::spaces::ActiveSpace>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut last: Local<String>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    let boundary = focus.tab.and_then(|tab_e| {
        let tab = tabs.get(tab_e).ok()?;
        let (path, source) = tab_boundary_dir(tab, &settings, active_space.as_deref())?;
        let info = repo_info
            .as_mut()
            .and_then(|cache| cache.bypass_change_detection().get(&path));
        let wt = worktrees.get(tab_e).ok();
        let branch = info.as_ref().map(|i| i.branch.clone()).unwrap_or_default();
        let base_ref = wt.map(|w| w.base_ref.clone()).unwrap_or_default();
        let mut leaves = Vec::new();
        collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut leaves);
        Some(TabBoundary {
            effective_dir: abbreviate_home(&path),
            source: match source {
                vmux_setting::DirSource::Tab => "tab",
                vmux_setting::DirSource::Space => "space",
                vmux_setting::DirSource::Global => "global",
            }
            .to_string(),
            is_git_repo: info.is_some(),
            is_worktree: info.as_ref().is_some_and(|i| i.is_worktree),
            branch,
            base_ref,
            uncommitted: info.as_ref().map(|i| i.uncommitted).unwrap_or(0),
            ahead: info.as_ref().map(|i| i.ahead).unwrap_or(0),
            pane_count: leaves.len() as u32,
        })
    });
    let payload = TabBoundaryEvent { boundary };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        TAB_BOUNDARY_EVENT,
        &payload,
    ));
    *last = ron_body;
}

#[allow(clippy::too_many_arguments)]
fn push_bookmarks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    pins: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Bookmark>,
        ),
        With<vmux_core::Pin>,
    >,
    folders: Query<
        (
            Entity,
            &vmux_core::Uuid,
            &Name,
            Option<&Children>,
            Has<vmux_core::Collapsed>,
            &vmux_core::BookmarkOrder,
            Option<&ChildOf>,
        ),
        With<vmux_core::Folder>,
    >,
    top_bookmarks: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Pin>,
        ),
        (With<vmux_core::Bookmark>, Without<ChildOf>),
    >,
    child_bookmarks: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Pin>,
        ),
        With<vmux_core::Bookmark>,
    >,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let row = |uuid: &vmux_core::Uuid, meta: &PageMetadata, bookmarked: bool, pinned: bool| {
        vmux_layout::event::BookmarkRow {
            uuid: uuid.0.clone(),
            metadata: meta.clone(),
            bookmarked,
            pinned,
        }
    };

    let mut pin_entries: Vec<(u32, vmux_layout::event::BookmarkRow)> = pins
        .iter()
        .map(|(u, m, o, bookmarked)| (o.0, row(u, m, bookmarked, true)))
        .collect();
    pin_entries.sort_by_key(|(order, _)| *order);
    let pin_rows: Vec<vmux_layout::event::BookmarkRow> =
        pin_entries.into_iter().map(|(_, r)| r).collect();

    let mut roots: Vec<(u32, vmux_layout::event::BookmarkNode)> = Vec::new();
    for (_, uuid, name, children, collapsed, order, parent) in folders.iter() {
        let mut kids = Vec::new();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok((uuid, meta, order, pinned)) = child_bookmarks.get(child) {
                    kids.push((order.0, row(uuid, meta, true, pinned)));
                }
            }
        }
        kids.sort_by_key(|(order, _)| *order);
        let parent = parent.and_then(|parent| {
            folders
                .get(parent.get())
                .ok()
                .map(|(_, uuid, _, _, _, _, _)| uuid.0.clone())
        });
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Folder(vmux_layout::event::FolderRow {
                uuid: uuid.0.clone(),
                name: name.as_str().to_string(),
                collapsed,
                parent,
                children: kids.into_iter().map(|(_, row)| row).collect(),
            }),
        ));
    }
    for (uuid, meta, order, pinned) in top_bookmarks.iter() {
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Entry(row(uuid, meta, true, pinned)),
        ));
    }
    roots.sort_by_key(|(o, _)| *o);
    let roots: Vec<vmux_layout::event::BookmarkNode> = roots.into_iter().map(|(_, n)| n).collect();

    let payload = vmux_layout::event::BookmarksHostEvent {
        pins: pin_rows,
        roots,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        vmux_layout::event::BOOKMARKS_EVENT,
        &payload,
    ));
    *last = body;
}

fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    tabs: Query<(Entity, &Tab, &LastActivatedAt)>,
    tab_q: Query<Entity, With<Tab>>,
    active_tab_param: vmux_layout::stack::ActiveTabParam,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<(&PageMetadata, Option<&OscTitle>), With<Browser>>,
    done_agents: Query<Entity, With<vmux_core::notify::AgentDoneUnseen>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let active_tab = active_tab_param.get();

    let done_tabs: std::collections::HashSet<Entity> = done_agents
        .iter()
        .filter_map(|agent| tab_of(agent, &child_of_q, &tab_q))
        .collect();

    let ordered = if let Some(anchor) = active_tab {
        vmux_layout::tab::active_tab_siblings(anchor, &child_of_q, &all_children, &tab_q)
    } else {
        Vec::new()
    };

    let rows: Vec<TabRow> = ordered
        .iter()
        .filter_map(|e| tabs.get(*e).ok())
        .map(|(entity, tab, _)| {
            let active_stack = active_stack_in_tab(
                entity,
                &all_children,
                &leaf_pane_q,
                &pane_children,
                &stack_ts,
            );
            let found =
                active_stack.and_then(|s| first_browser_meta(s, &stack_children, &browser_meta));
            let title = found
                .map(|(meta, osc)| effective_title(osc, &meta.title).to_string())
                .unwrap_or_default();
            let (url, icon, bg_color) = found
                .map(|(meta, _)| (meta.url.clone(), meta.icon.clone(), meta.bg_color.clone()))
                .unwrap_or_default();
            let name = if tab.name.is_empty() {
                "Tab".to_string()
            } else {
                tab.name.clone()
            };
            TabRow {
                id: entity.to_bits().to_string(),
                name,
                is_active: Some(entity) == active_tab,
                bg_color,
                title,
                url,
                icon,
                is_done_unseen: done_tabs.contains(&entity),
            }
        })
        .collect();

    let payload = TabsHostEvent { tabs: rows };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, TABS_EVENT, &payload));
    *last = body;
}

fn active_stack_in_tab(
    tab_e: Entity,
    all_children: &Query<&Children>,
    leaf_pane_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
) -> Option<Entity> {
    let mut leaves = Vec::new();
    collect_leaf_panes(tab_e, all_children, leaf_pane_q, &mut leaves);
    leaves
        .into_iter()
        .filter_map(|p| active_stack_in_pane(p, pane_children, stack_ts).map(|s| (s, p)))
        .filter_map(|(s, _)| stack_ts.get(s).ok())
        .max_by_key(|(_, ts)| ts.0)
        .map(|(e, _)| e)
}

fn effective_title<'a>(osc: Option<&'a OscTitle>, default: &'a str) -> &'a str {
    match osc {
        Some(OscTitle(t)) if !t.is_empty() => t,
        _ => default,
    }
}

fn first_browser_meta<'a>(
    stack: Entity,
    stack_children: &Query<&Children>,
    browser_meta: &'a Query<(&PageMetadata, Option<&OscTitle>), With<Browser>>,
) -> Option<(&'a PageMetadata, Option<&'a OscTitle>)> {
    let kids = stack_children.get(stack).ok()?;
    kids.iter().find_map(|c| browser_meta.get(c).ok())
}

fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut meta_q: Query<&mut PageMetadata, With<Browser>>,
    terminal_q: Query<(), With<Terminal>>,
    effective_startup_url: Option<Res<vmux_layout::settings::EffectiveStartupUrl>>,
    host_spawn: Res<HostSpawnRegistry>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut font_size_writer: MessageWriter<vmux_terminal::TerminalFontSizeCommand>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = cmd else {
            continue;
        };
        let (_, _, active_stack_opt) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(active) = active_stack_opt else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, co)| co.get() == active)
            .map(|(e, _)| e)
        else {
            continue;
        };
        let is_terminal = terminal_q.contains(webview);
        match browser_cmd {
            BrowserCommand::Navigation(nav) => match nav {
                BrowserNavigationCommand::PrevPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoBack { webview });
                    }
                }
                BrowserNavigationCommand::NextPage => {
                    if !is_terminal {
                        commands.trigger(RequestGoForward { webview });
                    }
                }
                BrowserNavigationCommand::Reload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReload { webview });
                    }
                }
                BrowserNavigationCommand::HardReload => {
                    if is_terminal {
                        commands.trigger(RestartPty { entity: webview });
                    } else {
                        commands.trigger(RequestReloadIgnoreCache { webview });
                    }
                }
                BrowserNavigationCommand::Stop => {}
            },
            #[allow(clippy::single_match)]
            BrowserCommand::Open(open_cmd) => match open_cmd {
                OpenCommand::InPlace { url } => {
                    let resolved = vmux_command::open::resolve_url(
                        url.as_deref(),
                        effective_startup_url.as_ref().map(|s| s.0.as_str()),
                    );
                    if resolved.is_empty() {
                        continue;
                    }
                    let resolved = normalize_vmux_url(&resolved);
                    let current_url = meta_q
                        .get(webview)
                        .map(|m| m.url.clone())
                        .unwrap_or_default();
                    if is_terminal
                        || host_spawn.needs_host_spawn(&current_url)
                        || host_spawn.needs_host_spawn(&resolved)
                    {
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(active),
                            url: resolved,
                            request_id: None,
                        });
                        continue;
                    }
                    if let Ok(mut meta) = meta_q.get_mut(webview) {
                        meta.url = resolved.clone();
                        meta.title = resolved.clone();
                        meta.icon = vmux_core::PageIcon::None;
                    }
                    commands
                        .entity(webview)
                        .insert(WebviewSource::new(&resolved));
                    commands.trigger(RequestNavigate {
                        webview,
                        url: resolved,
                    });
                }
                _ => {}
            },
            BrowserCommand::View(view) => match view {
                BrowserViewCommand::ZoomIn => {
                    if is_terminal {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Increase);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 += 0.5;
                    }
                }
                BrowserViewCommand::ZoomOut => {
                    if is_terminal {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Decrease);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 -= 0.5;
                    }
                }
                BrowserViewCommand::ZoomReset => {
                    if is_terminal {
                        font_size_writer.write(vmux_terminal::TerminalFontSizeCommand::Reset);
                    } else if let Ok(mut z) = zoom_q.get_mut(webview) {
                        z.0 = 0.0;
                    }
                }
                BrowserViewCommand::DevTools => {
                    commands.trigger(RequestShowDevTool { webview });
                }
                BrowserViewCommand::ViewSource => {}
                BrowserViewCommand::Print => {}
            },
            BrowserCommand::Bar(_) => {}
        }
    }
}

fn should_emit_update(
    current: &UpdateState,
    last: &Option<UpdateState>,
    page_ready_changed: bool,
) -> bool {
    last.as_ref() != Some(current) || (page_ready_changed && *current != UpdateState::Idle)
}

fn push_update_notice_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    state: Res<UpdateState>,
    mut last: Local<Option<UpdateState>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    if !should_emit_update(&state, &last, page_ready.is_changed()) {
        return;
    }
    match &*state {
        UpdateState::Idle => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_CLEARED_EVENT,
            &UpdateClearedEvent,
        )),
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: *downloaded,
                total: *total,
                installing: false,
            },
        )),
        UpdateState::Installing { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: 0,
                total: 0,
                installing: true,
            },
        )),
        UpdateState::Ready { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_READY_EVENT,
            &UpdateReadyEvent {
                version: version.clone(),
            },
        )),
    }
    *last = Some(state.clone());
}

fn on_debug_update_ready(
    trigger: On<BinReceive<DebugUpdateReady>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Ready {
        version: trigger.event().payload.version.clone(),
    };
}

fn on_debug_update_clear(
    _trigger: On<BinReceive<DebugUpdateClear>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Idle;
}

fn on_header_command_emit(
    trigger: On<BinReceive<HeaderCommandEvent>>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
) {
    let cmd = match trigger.event().payload.header_command.as_str() {
        "prev_page" => BrowserCommand::Navigation(BrowserNavigationCommand::PrevPage),
        "next_page" => BrowserCommand::Navigation(BrowserNavigationCommand::NextPage),
        "reload" => BrowserCommand::Navigation(BrowserNavigationCommand::Reload),
        "focus_address_bar" => BrowserCommand::Bar(BrowserBarCommand::OpenPageInCommandBar),
        _ => return,
    };
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let cmd = AppCommand::Browser(cmd);
    issued.write(vmux_command::CommandIssued {
        caller,
        command: cmd.clone(),
    });
    messages.write(cmd);
}

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    cef: Option<Single<Entity, (With<LayoutCef>, With<PageReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(cef) = cef else { return };
    let cef_e = *cef;
    if browsers.has_browser(cef_e) && browsers.host_emit_ready(&cef_e) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            RELOAD_EVENT,
            &ReloadEvent,
        ));
    }
}

fn knowledge_path_url(root: &Path, requested: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let metadata = std::fs::symlink_metadata(requested).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    let path = requested.canonicalize().ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let markdown = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        });
    if !path.is_dir() && !markdown {
        return None;
    }
    url::Url::from_file_path(path)
        .ok()
        .map(|url| url.to_string())
}

fn on_side_sheet_command_emit(
    trigger: On<BinReceive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut last_activated: Query<&mut LastActivatedAt>,
    child_of: Query<&ChildOf>,
    tabs: Query<(), With<Tab>>,
    section_states: Query<&SideSheetSectionsExpanded, With<Tab>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: MessageWriter<vmux_command::CommandIssued>,
    mut open_beside: MessageWriter<vmux_layout::OpenBesideRequest>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    let Ok(pane_id) = evt.pane_id.parse::<u64>() else {
        return;
    };
    let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let stack_entities: Vec<Entity> = children.iter().filter(|&e| stack_q.contains(e)).collect();

    match evt.command.as_str() {
        "activate_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };
            let activated_at = LastActivatedAt::now();
            if let Ok(mut value) = last_activated.get_mut(target_pane) {
                *value = activated_at;
            } else {
                commands.entity(target_pane).insert(activated_at);
            }
            if let Ok(mut value) = last_activated.get_mut(target_stack) {
                *value = activated_at;
            } else {
                commands.entity(target_stack).insert(activated_at);
            }

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
        }
        "close_stack" => {
            let Some(&target_stack) = stack_entities.get(evt.stack_index as usize) else {
                return;
            };
            commands.entity(target_pane).insert(LastActivatedAt::now());
            commands.entity(target_stack).insert(LastActivatedAt::now());
            let cmd = AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        "new_stack" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd =
                AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: None }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "collapse_card" => {
            commands
                .entity(target_pane)
                .insert(SideSheetCardCollapsed)
                .remove::<SideSheetPaneExpanded>();
        }
        "expand_card" => {
            commands
                .entity(target_pane)
                .remove::<SideSheetCardCollapsed>()
                .remove::<SideSheetPaneExpanded>();
        }
        "collapse_section" | "expand_section" => {
            let expanded = evt.command == "expand_section";
            if evt.path == "pane" {
                let mut pane = commands.entity(target_pane);
                if expanded {
                    pane.remove::<SideSheetCardCollapsed>()
                        .remove::<SideSheetPaneExpanded>();
                } else {
                    pane.insert(SideSheetCardCollapsed)
                        .remove::<SideSheetPaneExpanded>();
                }
                return;
            }
            let mut current = target_pane;
            let tab = loop {
                if tabs.contains(current) {
                    break Some(current);
                }
                let Ok(parent) = child_of.get(current) else {
                    break None;
                };
                current = parent.parent();
            };
            let Some(tab) = tab else {
                return;
            };
            let mut state = section_states.get(tab).copied().unwrap_or_default();
            if !state.set(&evt.path, expanded) {
                return;
            }
            if state.is_empty() {
                commands.entity(tab).remove::<SideSheetSectionsExpanded>();
            } else {
                commands.entity(tab).insert(state);
            }
        }
        "open_knowledge_path" => {
            let Some(mut url) =
                knowledge_path_url(&vmux_core::knowledge::knowledge_dir(), Path::new(&evt.path))
            else {
                return;
            };
            if evt.stack_index > 0 && !Path::new(&evt.path).is_dir() {
                url.push_str(&format!("#L{}", evt.stack_index));
            }
            open_beside.write(vmux_layout::OpenBesideRequest {
                pane: target_pane,
                direction: None,
                url,
                request_id: [0; 16],
                focus: true,
            });
        }
        "open_tools" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some("vmux://tools/".to_string()),
            }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "open_vault" => {
            commands.entity(target_pane).insert(LastActivatedAt::now());
            let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some("vmux://vault/".to_string()),
            }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        _ => {}
    }
}

fn spawn_visit_on_navigation(
    changed_tabs: Query<(Entity, &PageMetadata), (With<Stack>, Changed<PageMetadata>)>,
    mut last_urls: Local<std::collections::HashMap<u64, String>>,
    mut commands: Commands,
) {
    for (entity, meta) in &changed_tabs {
        if meta.url.is_empty() || meta.url == "about:blank" {
            continue;
        }

        let key = entity.to_bits();
        let is_new = last_urls
            .get(&key)
            .map(|prev| prev != &meta.url)
            .unwrap_or(true);

        if is_new {
            last_urls.insert(key, meta.url.clone());
            commands.spawn((Visit, meta.clone(), CreatedAt::now()));
        }
    }
}

fn sync_page_metadata_to_tab(
    browser_q: Query<(&PageMetadata, &ChildOf), (With<Browser>, Changed<PageMetadata>)>,
    tab_q: Query<Option<&PageMetadata>, With<Stack>>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, child_of) in &browser_q {
        let parent = child_of.get();
        let Ok(parent_meta) = tab_q.get(parent) else {
            continue;
        };
        if status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        let content_is_web = meta.url.starts_with("http://") || meta.url.starts_with("https://");
        let content_is_agent = meta.url.starts_with("vmux://agent/");
        if parent_meta
            .as_ref()
            .is_some_and(|m| m.url.starts_with("vmux://agent/"))
            && !content_is_web
            && !content_is_agent
        {
            continue;
        }
        if let Some(parent_url) = parent_meta.as_ref().map(|m| m.url.as_str())
            && parent_url.starts_with("vmux://")
            && (meta.url.starts_with("data:") || meta.url.is_empty())
        {
            continue;
        }
        if let Ok(mut ecmds) = commands.get_entity(parent) {
            ecmds.insert(meta.clone());
        }
    }
}

pub fn handle_browser_go_back_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoBackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_browser_target(s, &panes, &stacks),
            None => focus
                .pane
                .filter(|p| panes.contains(*p))
                .map(vmux_layout::target::BrowserTarget::Pane),
        };
        let Some(target) = target else { continue };
        let Some(webview) = vmux_layout::target::webview_for_target(
            target,
            &pane_children,
            &stack_ts,
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoBack { webview });
    }
}

pub fn handle_browser_go_forward_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoForwardRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_browser_target(s, &panes, &stacks),
            None => focus
                .pane
                .filter(|p| panes.contains(*p))
                .map(vmux_layout::target::BrowserTarget::Pane),
        };
        let Some(target) = target else { continue };
        let Some(webview) = vmux_layout::target::webview_for_target(
            target,
            &pane_children,
            &stack_ts,
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoForward { webview });
    }
}

pub fn handle_browser_open_history(
    mut reader: MessageReader<AppCommand>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mut writer: MessageWriter<PageOpenRequest>,
) {
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenHistory))
        ) {
            let Some(pane) = focus.pane else {
                continue;
            };
            writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url: "vmux://history/".to_string(),
                request_id: None,
            });
        }
    }
}

fn handle_page_open_requests(
    mut reader: MessageReader<PageOpenRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: Query<Entity, With<Stack>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack = match resolve_page_open_target(
            &request.target,
            &focus,
            &panes,
            &pane_children,
            &stack_ts,
            &stack_filter,
            &mut commands,
        ) {
            Ok(stack) => stack,
            Err(message) => {
                send_page_open_response(&service, request.request_id, Err(message));
                continue;
            }
        };
        let task = PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: normalize_vmux_url(&request.url),
            request_id: request.request_id,
        };
        if request.request_id.is_some() {
            commands.spawn((
                task,
                PageOpenAwaitSnapshot {
                    started: time.elapsed(),
                },
            ));
        } else {
            commands.spawn(task);
        }
    }
}

fn normalize_vmux_url(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("vmux://")
        && !rest.is_empty()
        && !rest.contains('/')
        && !rest.contains('?')
    {
        return format!("vmux://{rest}/");
    }
    url.to_string()
}

fn resolve_page_open_target(
    target: &PageOpenTarget,
    focus: &vmux_layout::stack::FocusedStack,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: &Query<Entity, With<Stack>>,
    commands: &mut Commands,
) -> Result<Entity, String> {
    match *target {
        PageOpenTarget::ActiveStack => focus
            .stack
            .or_else(|| {
                focus.pane.filter(|pane| panes.contains(*pane)).map(|pane| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                })
            })
            .ok_or_else(|| "page_open: no focused stack or pane".to_string()),
        PageOpenTarget::Stack(stack) => {
            if stack_filter.contains(stack) {
                Ok(stack)
            } else {
                Err("page_open: target stack does not exist".to_string())
            }
        }
        PageOpenTarget::ActiveStackInPane(pane) => {
            if !panes.contains(pane) {
                return Err("page_open: target pane does not exist".to_string());
            }
            Ok(active_stack_in_pane(pane, pane_children, stack_ts)
                .or_else(|| first_stack_in_pane(pane, pane_children, stack_filter))
                .unwrap_or_else(|| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                }))
        }
        PageOpenTarget::NewStackInPane(pane) => {
            if panes.contains(pane) {
                Ok(commands
                    .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id())
            } else {
                Err("page_open: target pane does not exist".to_string())
            }
        }
    }
}

fn attach_cef_page_requests(
    mut reader: MessageReader<CefPageAttachRequest>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        attach_cef_page_to_stack(
            request.stack,
            &request.url,
            &request.title,
            request.bg_color.clone(),
            &children_q,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
        );
    }
}

/// Marks a `PageOpenTask` the fallback has seen pending once. A `vmux://` scheme
/// owned by a `HandleKnownPages` handler can, under a rare command-visibility gap,
/// reach this fallback still pending in its first frame; this grace marker defers
/// the "unknown URL" verdict one run so the owning handler's mark becomes visible
/// before we error-claim (and permanently win the race for) an owned task.
#[derive(Component, Clone, Debug)]
struct PageOpenFallbackDeferred;

#[derive(Component, Clone, Debug)]
struct PageOpenAwaitSnapshot {
    started: std::time::Duration,
}

fn handle_unclaimed_page_open_tasks(
    mut tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenFallbackDeferred>,
        ),
        Without<PageOpenHandled>,
    >,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task, error, deferred_once) in &mut tasks {
        if let Some(error) = error {
            attach_error_page_to_stack(
                task.stack,
                &task.url,
                "Page failed to load",
                &error.message,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://error/") {
            attach_error_page_to_stack(
                task.stack,
                &task.url,
                "Page failed to load",
                &task.url,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://") {
            if deferred_once.is_none() {
                commands.entity(entity).insert(PageOpenFallbackDeferred);
                continue;
            }
            attach_error_page_to_stack(
                task.stack,
                &task.url,
                "Page not found",
                "",
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert((
                PageOpenHandled,
                PageOpenError {
                    message: format!("unknown vmux URL '{}'", task.url),
                },
            ));
        } else {
            attach_cef_page_to_stack(
                task.stack,
                &task.url,
                &task.url,
                None,
                &children_q,
                &mut commands,
                &mut meshes,
                &mut webview_mt,
            );
            commands.entity(entity).insert(PageOpenHandled);
        }
    }
}

fn respond_page_open_tasks(
    tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenAwaitSnapshot>,
        ),
        With<PageOpenHandled>,
    >,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    time: Res<Time>,
    children: Query<&Children>,
    browsers: Query<(), With<Browser>>,
    child_of: Query<&ChildOf>,
    mut pending_nav: ResMut<PendingNavSnapshots>,
    mut commands: Commands,
) {
    for (entity, task, error, await_snapshot) in &tasks {
        if let Some(error) = error {
            send_page_open_response(&service, task.request_id, Err(error.message.clone()));
            commands.entity(entity).despawn();
            continue;
        }
        let Some(await_snapshot) = await_snapshot else {
            send_page_open_response(&service, task.request_id, Ok(()));
            commands.entity(entity).despawn();
            continue;
        };
        let webview = children
            .get(task.stack)
            .ok()
            .and_then(|children| children.iter().find(|child| browsers.contains(*child)));
        if let (Some(webview), Some(request_id)) = (webview, task.request_id) {
            let pane = child_of
                .get(task.stack)
                .ok()
                .map(|child_of| child_of.get().to_bits().to_string());
            pending_nav.0.insert(
                webview,
                NavPending {
                    request_id,
                    started: await_snapshot.started,
                    saw_loading: false,
                    pane,
                },
            );
            commands.entity(entity).despawn();
        } else if time
            .elapsed()
            .saturating_sub(await_snapshot.started)
            .as_secs_f32()
            > 10.0
        {
            send_page_open_response(
                &service,
                task.request_id,
                Err("page opened without a snapshot-capable webview".to_string()),
            );
            commands.entity(entity).despawn();
        }
    }
}

fn send_page_open_response(
    service: &Option<Res<vmux_service::client::ServiceClient>>,
    request_id: Option<[u8; 16]>,
    result: Result<(), String>,
) {
    use vmux_service::protocol::{AgentCommandResult, AgentRequestId, ClientMessage};
    let (Some(service), Some(request_id)) = (service.as_ref(), request_id) else {
        return;
    };
    let result = match result {
        Ok(()) => AgentCommandResult::Ok,
        Err(message) => AgentCommandResult::Error(message),
    };
    service.0.send(ClientMessage::AgentCommandResponse {
        request_id: AgentRequestId(request_id),
        result,
    });
}

fn attach_cef_page_to_stack(
    stack: Entity,
    url: &str,
    title: &str,
    bg_color: Option<String>,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: url.to_string(),
        title: title.to_string(),
        bg_color,
        ..default()
    });
    let browser = commands
        .spawn((
            Browser::new_with_title(meshes, webview_mt, url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
    browser
}

fn attach_error_page_to_stack(
    stack: Entity,
    display_url: &str,
    title: &str,
    message: &str,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let source = error_page_source(title, message, display_url);
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: display_url.to_string(),
        title: title.to_string(),
        ..default()
    });
    let browser = commands
        .spawn((
            Browser::new_error(meshes, webview_mt, &source, display_url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

fn error_page_source(title: &str, message: &str, url: &str) -> String {
    format!(
        "vmux://error/?title={}&message={}&url={}",
        percent_encode(title),
        percent_encode(message),
        percent_encode(url),
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub fn handle_open_in_new_stack_requests(
    mut reader: MessageReader<vmux_layout::OpenInNewStackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
            continue;
        };
        page_open_writer.write(PageOpenRequest {
            target: PageOpenTarget::NewStackInPane(pane),
            url: request.url.clone(),
            request_id: None,
        });
    }
}

/// A pending agent-initiated in-place navigation, keyed by the target webview.
/// Populated by `handle_browser_navigate_requests`; drained in `vmux_desktop`
/// (`drive_pending_nav_snapshots`) once the page settles, so the navigation's
/// agent command returns the post-load snapshot inline.
pub struct NavPending {
    pub request_id: [u8; 16],
    pub started: std::time::Duration,
    pub saw_loading: bool,
    pub pane: Option<String>,
}

#[derive(Resource, Default)]
pub struct PendingNavSnapshots(pub std::collections::HashMap<Entity, NavPending>);

#[derive(Resource, Default)]
pub struct RecentBrowserInteraction {
    stack: Option<Entity>,
    at: Option<std::time::Instant>,
}

impl RecentBrowserInteraction {
    fn active(&self, stack: Entity) -> bool {
        self.stack == Some(stack)
            && self
                .at
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2))
    }
}

fn track_browser_interaction(
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut keyboard: MessageReader<KeyboardInput>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    browsers: Query<&ChildOf, With<Browser>>,
    mut recent: ResMut<RecentBrowserInteraction>,
) {
    let interacted = mouse_buttons
        .read()
        .any(|event| event.state == ButtonState::Pressed)
        || mouse_wheels.read().next().is_some()
        || keyboard
            .read()
            .any(|event| event.state == ButtonState::Pressed);
    if !interacted {
        return;
    }
    let Some(stack) = focus.stack else { return };
    if browsers.iter().any(|child_of| child_of.get() == stack) {
        recent.stack = Some(stack);
        recent.at = Some(std::time::Instant::now());
    }
}

pub fn handle_browser_navigate_requests(
    mut reader: MessageReader<vmux_layout::BrowserNavigateRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut commands: Commands,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
    mut pending_nav: ResMut<PendingNavSnapshots>,
    time: Res<Time>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &vmux_core::LastActivatedAt), With<vmux_layout::stack::Stack>>,
    recent_interaction: Res<RecentBrowserInteraction>,
    mut activate: MessageWriter<vmux_layout::active_panes::ActivatePane>,
) {
    for request in reader.read() {
        let vmux_layout::BrowserNavigateRequest {
            url,
            pane,
            request_id,
            new_stack,
            profile,
        } = request.clone();

        if let Some(s) = pane.as_deref() {
            if let Some(target) = vmux_layout::target::parse_pane_target(s, &panes) {
                if new_stack && !url.starts_with("vmux://") && !url.starts_with("file:") {
                    let active_stack =
                        vmux_layout::stack::active_stack_in_pane(target, &pane_children, &stack_ts);
                    let activate_new =
                        active_stack.is_none_or(|stack| !recent_interaction.active(stack));
                    let stack = commands
                        .spawn((
                            vmux_layout::stack::stack_bundle(),
                            if activate_new {
                                LastActivatedAt::now()
                            } else {
                                LastActivatedAt(0)
                            },
                            ChildOf(target),
                        ))
                        .id();
                    if let Some(profile) = profile {
                        activate.write(vmux_layout::active_panes::ActivatePane {
                            profile: vmux_layout::active_panes::ProfileId::Agent(profile),
                            active: vmux_layout::active_panes::ActiveStack {
                                tab: None,
                                pane: Some(target),
                                stack: Some(stack),
                                kind: None,
                            },
                        });
                    }
                    page_open_writer.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id,
                    });
                    continue;
                }
                let in_place = if url.starts_with("vmux://") || url.starts_with("file:") {
                    None
                } else {
                    vmux_layout::target::active_webview_for_tab(
                        vmux_layout::stack::active_stack_in_pane(target, &pane_children, &stack_ts),
                        &browsers,
                        &terminals,
                    )
                };
                if let Some(webview) = in_place {
                    commands.trigger(RequestNavigate {
                        webview,
                        url: url.clone(),
                    });
                    let displaced = match request_id {
                        Some(rid) => pending_nav.0.insert(
                            webview,
                            NavPending {
                                request_id: rid,
                                started: time.elapsed(),
                                saw_loading: false,
                                pane: Some(target.to_bits().to_string()),
                            },
                        ),
                        None => pending_nav.0.remove(&webview),
                    };
                    if let Some(old) = displaced {
                        send_page_open_response(&service, Some(old.request_id), Ok(()));
                    }
                    if request_id.is_none() {
                        send_page_open_response(&service, None, Ok(()));
                    }
                } else {
                    page_open_writer.write(PageOpenRequest {
                        target: PageOpenTarget::NewStackInPane(target),
                        url,
                        request_id,
                    });
                }
            } else {
                send_page_open_response(
                    &service,
                    request_id,
                    Err(format!("browser_navigate: invalid pane id '{s}'")),
                );
            }
        } else if let Some(webview) =
            vmux_layout::target::active_webview_for_tab(focus.stack, &browsers, &terminals)
        {
            if url.starts_with("vmux://") || url.starts_with("file:") {
                let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
                    send_page_open_response(
                        &service,
                        request_id,
                        Err("browser_navigate: no focused pane for vmux URL".to_string()),
                    );
                    continue;
                };
                page_open_writer.write(PageOpenRequest {
                    target: PageOpenTarget::NewStackInPane(pane),
                    url,
                    request_id,
                });
            } else {
                commands.trigger(RequestNavigate {
                    webview,
                    url: url.clone(),
                });
                let displaced = match request_id {
                    Some(rid) => pending_nav.0.insert(
                        webview,
                        NavPending {
                            request_id: rid,
                            started: time.elapsed(),
                            saw_loading: false,
                            pane: focus.pane.map(|p| p.to_bits().to_string()),
                        },
                    ),
                    None => pending_nav.0.remove(&webview),
                };
                if let Some(old) = displaced {
                    send_page_open_response(&service, Some(old.request_id), Ok(()));
                }
                if request_id.is_none() {
                    send_page_open_response(&service, None, Ok(()));
                }
            }
        } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
            page_open_writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url,
                request_id,
            });
        } else {
            send_page_open_response(
                &service,
                request_id,
                Err("browser_navigate: no focused pane".to_string()),
            );
        }
    }
}

fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::cef_cache_path()
}

#[cfg(test)]
#[path = "lib.appearance_bridge.test.rs"]
mod appearance_bridge_tests;
#[cfg(test)]
#[path = "lib.debug_update_observer.test.rs"]
mod debug_update_observer_tests;
#[cfg(test)]
#[path = "lib.error_page_source.test.rs"]
mod error_page_source_tests;
#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
#[cfg(test)]
#[path = "lib.update_notice.test.rs"]
mod update_notice_tests;
