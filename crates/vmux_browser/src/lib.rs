//! Browser and page-open orchestration on top of `bevy_cef`: page resolution, CEF
//! backend management, and input forwarding between the native layout and embedded pages.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod appearance;
mod command;
mod extensions;
mod frame_rate;
mod host_focus;
mod input;
mod page_life;

mod native_bridge;
mod native_layout;
pub mod native_page;
mod navigation;
mod present;

use crate::page_life::spawn_popup_stacks;
use present::CommandBarWindowedFrame;
use vmux_command::command_bar::panel::CommandBarPanelActive;
mod page_open;
mod page_state;
mod scroll;
mod snapshot;
pub use host_focus::HostFocusIntent;

pub use native_bridge::NativeBridge;
/// Entry points for the AppKit monitors. Nothing in Rust calls them.
#[cfg(target_os = "macos")]
pub use native_bridge::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
pub use native_layout::NativeLayout;
#[cfg(target_os = "macos")]
pub use native_layout::NativeLayoutPointerMoveResult;

use bevy::{ecs::relationship::Relationship, input::mouse::MouseButton, prelude::*};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{CefEmbeddedHosts, CommandLineConfig};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use vmux_command::ReadAppCommands;
use vmux_command::command_bar::handler::PendingCommandBarReveal;
use vmux_core::{
    CefPageAttachRequest, HostSpawnRegistry, OscTitle, PageMetadata, PageOpenRequest, PageOpenSet,
    page::{PageManifest, PageReady},
};
use vmux_history::LastActivatedAt;
use vmux_layout::event::{RemoteCommandEvent, RemoteCopyEvent, SideSheetCommandEvent};
pub use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, Open, PendingWebviewReveal, UpdateState,
    bookmark::BookmarkContextMenuActive,
    event::{HeaderCommandEvent, StackRow},
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::{Stack, active_stack_in_pane, collect_leaf_panes},
    tab::Tab,
};

use vmux_flex::prelude::*;
use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;
use vmux_ui::theme::ThemeEvent;

/// Wires browser orchestration: resolves CEF embedded hosts from page manifests, manages
/// the CEF backend, and forwards pointer and cursor input between the layout and pages.
pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let profile = vmux_core::profile::active_profile_name();
        let startup_settings = vmux_setting::read_settings_from_disk();
        let startup_locale =
            Locale::requested(Some(&startup_settings.appearance.locale)).into_string();
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
        app.add_plugins((
            vmux_command::command_bar::CommandBarPlugin,
            native_page::NativePagesPlugin,
            extensions::ExtensionsPlugin,
            extensions::bridge_page::ExtensionBridgePagePlugin,
            extensions::broker::ExtensionBrokerPlugin,
            extensions::project::ExtensionProjectPlugin,
            extensions::windows::ExtensionWindowsPlugin,
        ));
        #[cfg(target_os = "macos")]
        app.add_plugins((
            native_page::NativePagePlugin::as_layout(&native_page::LAYOUT_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::START_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::HISTORY_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::TEAM_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::AGENTS_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::SETTINGS_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::SERVICES_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::SPACES_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::TOOLS_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::EXTENSIONS_PAGE),
            native_page::NativePagePlugin::in_pane(&native_page::ERROR_PAGE)
                .takes::<vmux_wire::error::ErrorPageData>(),
        ));
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        let cef_command_line = cef_command_line_config();
        configure_cef_backend_sync(app)
            .insert_resource(crate::extensions::load::PreparedExtensions(
                prepared_extensions,
            ))
            .insert_resource(extension_bridge)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_message::<PageOpenRequest>()
            .add_message::<CefPageAttachRequest>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
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
            .add_plugins((
                CefPlugin {
                    command_line_config: cef_command_line,
                    root_cache_path: cef_root_cache_path(),
                    locale: startup_locale,
                    accept_language_list: startup_accept_language_list,
                    embedded_hosts,
                    ..default()
                },
                BinEventEmitterPlugin::<(
                    HeaderCommandEvent,
                    SideSheetCommandEvent,
                    RemoteCommandEvent,
                    RemoteCopyEvent,
                )>::for_hosts(&["layout"]),
            ))
            .add_systems(Update, (vmux_layout::apply_cef_state_from_webview,))
            .add_systems(
                Update,
                vmux_layout::mirror_metadata_to_url
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .init_resource::<HostSpawnRegistry>()
            .add_plugins((
                host_focus::HostFocusPlugin,
                appearance::AppearancePlugin,
                page_life::PageLifePlugin,
                command::CommandPlugin,
                frame_rate::FrameRatePlugin,
                input::InputPlugin,
                navigation::NavigationPlugin,
                present::PresentPlugin,
                page_open::PageOpenPlugin,
                page_state::PageStatePlugin,
                snapshot::SnapshotPlugin,
                scroll::ScrollPlugin,
            ));
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
        BrowserSystems::SyncCefBackend.before(CefSystems::CreateAndResize),
    )
    .add_systems(
        Update,
        sync_cef_backend
            .in_set(BrowserSystems::SyncCefBackend)
            .after(PageOpenSet::Fallback)
            .after(spawn_popup_stacks),
    )
}

fn cef_command_line_config() -> CommandLineConfig {
    CommandLineConfig {
        switches: vmux_core::profile::cef_keychain_switches().to_vec(),
        switch_values: vec![("disable-features", "BackForwardCache")],
    }
}

fn theme_event(settings: &AppSettings) -> ThemeEvent {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    ThemeEvent {
        radius: settings.layout.radius,
        catalog: external_locale_catalog(locale.as_str()),
        locale: locale.into_string(),
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

#[cfg(test)]
mod accept_language_tests {
    use super::browser_accept_language_list;

    #[test]
    fn selected_locale_leads_browser_accept_language() {
        assert_eq!(
            browser_accept_language_list("ja"),
            "ja,en-US;q=0.9,en;q=0.8"
        );
        assert_eq!(
            browser_accept_language_list("pt-BR"),
            "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7"
        );
        assert_eq!(browser_accept_language_list("en-US"), "en-US,en;q=0.9");
    }
}

type CefPointerRegionRow<'a> = (
    Option<&'a Header>,
    Option<&'a SideSheet>,
    &'a Node,
    &'a ComputedNode,
    Option<&'a Visibility>,
    bool,
);

type CefPointerRegionQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Header>,
        Option<&'static SideSheet>,
        &'static Node,
        &'static ComputedNode,
        Option<&'static Visibility>,
        Has<Open>,
    ),
    Or<(With<Header>, With<SideSheet>)>,
>;

#[derive(Clone, Copy)]
struct CefPointerHitRect {
    rect: ComputedNode,
    interactive: bool,
}

static NATIVE_LAYOUT_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
static NATIVE_LAYOUT_ACTIVITY: AtomicBool = AtomicBool::new(false);

impl CefPointerHitRect {
    /// A region only takes the pointer while it is a header or sheet that is open, laid out and
    /// visible — anything else is a rectangle the cursor should fall straight through.
    fn of(row: CefPointerRegionRow<'_>) -> Self {
        let (header, side_sheet, node, &rect, visibility, open) = row;
        let interactive = (header.is_some() || side_sheet.is_some())
            && open
            && node.display != Display::None
            && !matches!(visibility, Some(Visibility::Hidden))
            && !rect.is_empty();
        Self { rect, interactive }
    }

    fn contains(self, point: Vec2) -> bool {
        self.interactive && self.rect.contains(point)
    }
}

pub fn set_native_layout_activity(active: bool) -> bool {
    NATIVE_LAYOUT_ACTIVITY.swap(active, Ordering::Relaxed) != active
}

fn native_layout_activity_active() -> bool {
    NATIVE_LAYOUT_ACTIVITY.load(Ordering::Relaxed)
}

fn cef_pointer_regions_contains(
    cursor_pos: Vec2,
    cef_regions: &CefPointerRegionQuery<'_, '_>,
) -> bool {
    for row in cef_regions.iter() {
        if CefPointerHitRect::of(row).contains(cursor_pos) {
            return true;
        }
    }
    false
}

fn pointer_button_from_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// Layout-page surfaces that float free of the laid-out regions and must own the whole window's
/// pointer input while they are up.
///
/// A context menu or the command bar panel can extend past any published hit rect, and both
/// dismiss on an outside click, so the layout webview has to see every move and click. A focused
/// bookmark field does not qualify — it stays inside the side sheet's own region.
pub(crate) type LayoutPointerCapture =
    Or<(With<BookmarkContextMenuActive>, With<CommandBarPanelActive>)>;

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

/// Every CEF browser is windowed, and the native overlay markers belong to nobody.
///
/// Both used to vary. The layout was the one offscreen surface — it carried the overlay markers and
/// was excluded from `windowed` — and a camera whose transform drifted from the window's would drop
/// *everything* back to offscreen rendering as a safety net. The layout is served by wry now and
/// holds no `Browser` at all, so the exception has no subject, and the safety net leads nowhere:
/// there is no offscreen path left to fall back to.
fn sync_cef_backend(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        Has<WebviewNativeOverlay>,
    ), (With<Browser>, With<WebviewSource>)>();
    let entities: Vec<(Entity, bool)> = query.iter(world).collect();
    let mut recreate = Vec::new();
    {
        let browsers = world.non_send::<Browsers>();
        for &(entity, native_overlay) in &entities {
            let stale_backend = browsers
                .is_windowed(&entity)
                .is_some_and(|windowed| !windowed);
            let stale_overlay = browsers.has_browser(entity) && native_overlay;
            if stale_backend || stale_overlay {
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
    for (entity, native_overlay) in entities {
        let needs_recreate = recreate.contains(&entity);
        let settled =
            world.get::<WebviewWindowed>(entity).is_some() && !native_overlay && !needs_recreate;
        if settled {
            continue;
        }
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        entity_mut
            .insert(WebviewWindowed)
            .remove::<WebviewNativeOverlay>();
        if needs_recreate {
            entity_mut
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
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
        NativeLayout::clear_pointer_state();
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

#[derive(Default)]
struct WindowedHoverRefreshState {
    entity: Option<Entity>,
    position: Option<Vec2>,
}

const LAYOUT_INPUT_BURST: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
struct LayoutFrameRateState {
    native_sequence: u64,
    last_input: Option<std::time::Instant>,
    dragging_layout: bool,
}

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
static NATIVE_LEFT_MOUSE_DOWN: AtomicBool = AtomicBool::new(false);
static NATIVE_PAGE_OWNS_ESCAPE: AtomicBool = AtomicBool::new(false);

fn native_command_bar_route() -> CommandBarRoute {
    *NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn set_native_page_owns_escape(owns: bool) {
    NATIVE_PAGE_OWNS_ESCAPE.store(owns, Ordering::Relaxed);
}

/// Whether a page surface will answer Escape itself, so the host must not read it as a request to
/// leave fullscreen. True while a terminal holds the keyboard — it forwards Escape to the PTY —
/// and while the command bar owns input.
///
/// Read from the `NSEvent` monitor, which runs on the AppKit thread ahead of the ECS, hence the
/// static rather than a resource.
pub fn native_page_owns_escape() -> bool {
    NATIVE_PAGE_OWNS_ESCAPE.load(Ordering::Relaxed)
}

pub fn set_native_left_mouse_down(down: bool) {
    NATIVE_LEFT_MOUSE_DOWN.store(down, Ordering::Relaxed);
}

pub fn native_left_mouse_down() -> bool {
    NATIVE_LEFT_MOUSE_DOWN.load(Ordering::Relaxed)
}

fn command_bar_windowed_frame_contains(frame: CommandBarWindowedFrame, cursor: Vec2) -> bool {
    cursor.x >= frame.left_px
        && cursor.x <= frame.left_px + frame.width_px
        && cursor.y >= frame.top_px
        && cursor.y <= frame.top_px + frame.height_px
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

impl LayoutFixedOffsets {
    fn of(rect: &ComputedNode, window_width_px: f32) -> Option<Self> {
        if rect.is_empty() || window_width_px <= 0.0 {
            return None;
        }

        let logical = rect.to_logical();
        let window_width = window_width_px * rect.inverse_scale_factor.max(1.0e-6);

        Some(Self {
            left: logical.min().x,
            top: logical.min().y,
            right: window_width - logical.max().x,
            height: logical.size.y,
        })
    }
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

fn should_emit_update(
    current: &UpdateState,
    last: &Option<UpdateState>,
    page_ready_changed: bool,
) -> bool {
    last.as_ref() != Some(current) || (page_ready_changed && *current != UpdateState::Idle)
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
) -> Entity {
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: url.to_string(),
        title: title.to_string(),
        bg_color,
        ..default()
    });
    let browser = commands
        .spawn((Browser::new_with_title(url, title), ChildOf(stack)))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
    browser
}

/// Replace whatever is in a stack with the error page, and record on its view what to show.
///
/// The stack keeps the url that failed, because that is what the address bar is reporting; the
/// view is the error page itself, so it is named `vmux://error/` — the url the native surface is
/// claimed by — and carries the failure as a component for [`answer_error_data_request`] to read.
fn attach_error_page_to_stack(
    stack: Entity,
    failure: vmux_wire::error::ErrorPageData,
    children_q: &Query<&Children>,
    commands: &mut Commands,
) {
    clear_stack_children(stack, children_q, commands);
    commands.entity(stack).insert(PageMetadata {
        url: failure.url.clone(),
        title: failure.title.clone(),
        ..default()
    });
    commands.spawn((
        Browser::native_page(vmux_wire::error::ERROR_PAGE_URL, &failure.title),
        failure,
        ChildOf(stack),
    ));
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
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

fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::cef_cache_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::sync_appearance_to_cef;
    use vmux_core::overlay::WindowOverlay;

    #[test]
    fn cef_disables_bfcache_for_extension_ports() {
        assert!(
            cef_command_line_config()
                .switch_values
                .contains(&("disable-features", "BackForwardCache"))
        );
    }

    #[test]
    fn knowledge_paths_only_open_vault_markdown_and_directories() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("knowledge");
        let folder = vault.join("projects");
        std::fs::create_dir_all(&folder).unwrap();
        let note = folder.join("brief.md");
        let text = folder.join("brief.txt");
        let outside = temp.path().join("outside.md");
        std::fs::write(&note, "# Brief").unwrap();
        std::fs::write(&text, "Brief").unwrap();
        std::fs::write(&outside, "# Outside").unwrap();

        assert!(knowledge_path_url(&vault, &vault).is_some());
        assert!(knowledge_path_url(&vault, &folder).is_some());
        assert!(knowledge_path_url(&vault, &note).is_some());
        assert!(knowledge_path_url(&vault, &text).is_none());
        assert!(knowledge_path_url(&vault, &outside).is_none());
    }

    #[test]
    fn stored_tab_dir_is_sidebar_source_of_truth() {
        let tab = Tab {
            name: "test".into(),
            startup_dir: Some("/tmp/agent-checkout".into()),
        };
        let settings = test_app_settings_with_radius(0.0);

        assert_eq!(
            tab_boundary_dir(&tab, &settings, None),
            Some((
                std::path::PathBuf::from("/tmp/agent-checkout"),
                vmux_setting::DirSource::Tab,
            ))
        );
    }

    #[test]
    fn legacy_tab_boundary_uses_space_fallback_without_migration() {
        let dir = std::env::temp_dir();
        let record = vmux_space::model::bootstrap_space_record();
        let mut settings = test_app_settings_with_radius(0.0);
        settings.spaces.insert(
            record.id.clone(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.to_string_lossy().into_owned()),
            },
        );
        let tab = Tab::default();

        let (path, source) = tab_boundary_dir(
            &tab,
            &settings,
            Some(&vmux_space::spaces::ActiveSpace { record }),
        )
        .unwrap();

        assert_eq!(path, dir);
        assert_eq!(source, vmux_setting::DirSource::Space);
        assert_eq!(tab.startup_dir, None);
    }

    #[test]
    fn normalize_vmux_url_trims_and_adds_trailing_slash_to_bare_host() {
        assert_eq!(normalize_vmux_url("vmux://lsp"), "vmux://lsp/");
        assert_eq!(normalize_vmux_url("vmux://terminal"), "vmux://terminal/");
        assert_eq!(normalize_vmux_url("vmux://lsp/"), "vmux://lsp/");
        assert_eq!(
            normalize_vmux_url("vmux://agent/vibe/"),
            "vmux://agent/vibe/"
        );
        assert_eq!(
            normalize_vmux_url("vmux://error/?title=x"),
            "vmux://error/?title=x"
        );
        assert_eq!(
            normalize_vmux_url("file:///tmp/main.rs"),
            "file:///tmp/main.rs"
        );
        assert_eq!(
            normalize_vmux_url("  vmux://agent/codex/session-id  "),
            "vmux://agent/codex/session-id"
        );
    }

    #[test]
    fn effective_title_prefers_nonempty_osc() {
        use vmux_core::OscTitle;
        assert_eq!(
            effective_title(Some(&OscTitle("osc".to_string())), "def"),
            "osc"
        );
        assert_eq!(
            effective_title(Some(&OscTitle(String::new())), "def"),
            "def"
        );
        assert_eq!(effective_title(None, "def"), "def");
    }

    #[test]
    fn agent_cli_url_redirects_tab_to_session_id() {
        let mut app = App::new();
        app.add_systems(Update, crate::navigation::sync_page_metadata_to_tab);

        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "vmux://agent/vibe/".to_string(),
                    ..default()
                },
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://agent/vibe/".to_string(),
                    ..default()
                },
                ChildOf(stack),
            ))
            .id();

        app.update();

        app.world_mut().get_mut::<PageMetadata>(child).unwrap().url =
            "vmux://agent/vibe/abc-123".to_string();

        app.update();

        let stack_url = app.world().get::<PageMetadata>(stack).unwrap().url.clone();
        assert_eq!(stack_url, "vmux://agent/vibe/abc-123");
    }

    /// A closed or hidden region must not swallow a pointer that is geometrically over it, and an
    /// open one must still catch its own edges.
    #[test]
    fn a_pointer_hits_only_interactive_regions() {
        let rect = ComputedNode::from_origin(Vec2::new(100.0, 40.0));

        assert!(
            CefPointerHitRect {
                rect,
                interactive: true
            }
            .contains(Vec2::new(100.0, 40.0))
        );
        assert!(
            !CefPointerHitRect {
                rect,
                interactive: true
            }
            .contains(Vec2::new(100.1, 20.0))
        );
        assert!(
            !CefPointerHitRect {
                rect,
                interactive: false
            }
            .contains(Vec2::new(50.0, 20.0))
        );
    }

    #[test]
    fn layout_fixed_offsets_use_computed_header_rect() {
        let computed = ComputedNode {
            size: Vec2::new(1_544.0, 168.0),
            center: Vec2::new(788.0, 84.0),
            inverse_scale_factor: 0.5,
            ..default()
        };
        let offsets = LayoutFixedOffsets::of(&computed, 1_600.0).expect("offsets");

        assert_eq!(offsets.left, 8.0);
        assert_eq!(offsets.top, 0.0);
        assert_eq!(offsets.right, 20.0);
        assert_eq!(offsets.height, 84.0);
    }

    pub(crate) fn test_app_settings_with_radius(radius: f32) -> AppSettings {
        AppSettings {
            browser: vmux_setting::BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: vmux_layout::settings::LayoutSettings {
                radius,
                window: vmux_layout::settings::WindowSettings { padding: 0.0 },
                pane: vmux_layout::settings::PaneSettings { gap: 0.0 },
                side_sheet: vmux_layout::settings::SideSheetSettings::default(),
                focus_ring: vmux_layout::settings::FocusRingSettings::default(),
            },
            shortcuts: vmux_setting::ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn appearance_change_updates_cef_color_scheme() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_app_settings_with_radius(0.0))
            .init_resource::<CefColorScheme>()
            .add_systems(
                Update,
                sync_appearance_to_cef.run_if(resource_changed::<AppSettings>),
            );
        app.update();
        app.world_mut()
            .resource_mut::<AppSettings>()
            .appearance
            .mode = vmux_setting::ColorScheme::Light;
        app.update();
        assert_eq!(
            app.world().resource::<CefColorScheme>().0,
            CefColorMode::Light
        );
    }

    /// Windowed is the only backend. The camera-mismatch fallback that used to drop everything
    /// back to offscreen rendering is gone, so a browser that failed to be marked windowed would
    /// render nowhere at all rather than degrading.
    #[test]
    fn every_cef_browser_is_windowed_with_no_overlay_markers() {
        let mut app = App::new();
        app.world_mut().insert_non_send(Browsers::default());
        let page = app
            .world_mut()
            .spawn((Browser, WebviewSource::new("https://example.com")))
            .id();
        let modal = app
            .world_mut()
            .spawn((
                Browser,
                WindowOverlay,
                WebviewSource::new("vmux://command-bar/"),
            ))
            .id();

        sync_cef_backend(app.world_mut());

        for entity in [page, modal] {
            assert!(app.world().get::<WebviewWindowed>(entity).is_some());
            assert!(app.world().get::<WebviewNativeOverlay>(entity).is_none());
        }
    }

    #[test]
    fn native_layout_pointer_queue_retains_only_latest_sample() {
        let source = include_str!("native_layout/macos.rs");
        let queue = source
            .split("pub fn queue_pointer_move")
            .nth(1)
            .and_then(|tail| tail.split("pub fn flush_pointer_move").next())
            .unwrap_or_default();
        let flush = source
            .split("pub fn flush_pointer_move")
            .nth(1)
            .and_then(|tail| tail.split("pub fn forward_scroll").next())
            .unwrap_or_default();
        let sample = source
            .split("fn queue_sample")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .unwrap_or_default();

        assert!(sample.contains("self.position_px = Some(position)"));
        assert!(sample.contains("self.buttons = buttons"));
        assert!(source.contains("fn queue_sample"));
        assert!(sample.contains("sample_changed"));
        assert!(sample.contains("self.pending = true"));
        assert!(queue.contains("state.queue_sample("));
        assert!(flush.contains("state.pending = false"));
        assert!(flush.contains("presenter.send(position_px / state.scale"));
    }

    #[test]
    fn active_browser_url_wins_over_stale_new_stack_placeholder() {
        let stack = Entity::from_bits(1);
        let rows = [StackRow {
            title: "Google".to_string(),
            url: "https://www.google.com".to_string(),
            icon: vmux_core::PageIcon::None,
            is_active: true,
            bg_color: None,
        }];

        assert!(!should_emit_new_stack_placeholder(
            Some(stack),
            Some(stack),
            &rows
        ));
    }

    #[test]
    fn host_payload_emits_again_when_page_ready_changes() {
        assert!(should_emit_cached_payload("tabs", "tabs", true));
        assert!(should_emit_cached_payload("tabs-2", "tabs", false));
        assert!(!should_emit_cached_payload("tabs", "tabs", false));
    }

    #[test]
    fn layout_state_padding_reads_effective_window_node_padding() {
        let node = Node {
            padding: UiRect {
                top: Val::Px(10.0),
                right: Val::Px(11.0),
                bottom: Val::Px(12.0),
                left: Val::Px(13.0),
            },
            ..default()
        };

        assert_eq!(
            layout_window_padding_from_node(&node),
            LayoutWindowPadding {
                top: 10.0,
                right: 11.0,
                bottom: 12.0,
                left: 13.0,
            }
        );
    }

    mod browser_navigate_flow {
        use crate::input::RecentBrowserInteraction;
        use crate::{Browser, PendingNavSnapshots};
        use bevy::ecs::relationship::Relationship;
        use bevy::prelude::*;
        use vmux_agent::events::AgentCommandRequest;
        use vmux_agent::host::AgentSessionPlugin;
        use vmux_agent::strategy::AgentStrategies;
        use vmux_core::{
            CefPageAttachRequest, LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled,
            PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTask,
        };
        use vmux_layout::pane::Pane;
        use vmux_layout::settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        };
        use vmux_layout::stack::FocusedStack;
        use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId};
        use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};
        use vmux_terminal::Terminal;

        fn test_settings() -> AppSettings {
            AppSettings {
                browser: BrowserSettings {
                    startup_url: "about:blank".to_string(),
                    ..Default::default()
                },
                layout: LayoutSettings {
                    radius: 0.0,
                    window: WindowSettings { padding: 0.0 },
                    pane: PaneSettings { gap: 0.0 },
                    side_sheet: SideSheetSettings::default(),
                    focus_ring: FocusRingSettings::default(),
                },
                shortcuts: ShortcutSettings::default(),
                terminal: None,
                auto_update: false,
                agent: vmux_setting::AgentSettings::default(),
                spaces: Default::default(),
                recording: Default::default(),
                editor: Default::default(),
                appearance: Default::default(),
            }
        }

        struct ConsumerPlugin;

        impl Plugin for ConsumerPlugin {
            fn build(&self, app: &mut App) {
                app.add_plugins((
                    vmux_layout::LayoutContractPlugin,
                    vmux_terminal::TerminalContractPlugin,
                ))
                .add_message::<PageOpenRequest>()
                .add_message::<CefPageAttachRequest>()
                .add_message::<vmux_setting::SettingsWriteRequest>()
                .add_message::<vmux_space::SpaceCommandRequest>()
                .add_message::<vmux_history::query::HistoryOpenIntent>()
                .init_resource::<crate::PendingNavSnapshots>()
                .init_resource::<crate::input::RecentBrowserInteraction>()
                .configure_sets(
                    Update,
                    (
                        PageOpenSet::ResolveTarget,
                        PageOpenSet::HandleKnownPages,
                        PageOpenSet::Fallback,
                        PageOpenSet::Respond,
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        crate::navigation::handle_browser_navigate_requests
                            .before(PageOpenSet::ResolveTarget),
                        crate::page_open::handle_page_open_requests
                            .in_set(PageOpenSet::ResolveTarget),
                        handle_test_known_page_open.in_set(PageOpenSet::HandleKnownPages),
                        crate::page_open::attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                        crate::page_open::handle_unclaimed_page_open_tasks
                            .in_set(PageOpenSet::Fallback),
                        crate::page_open::respond_page_open_tasks.in_set(PageOpenSet::Respond),
                        vmux_terminal::handle_terminal_send_requests,
                        vmux_terminal::handle_run_shell_requests,
                    ),
                );
            }
        }

        type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

        fn handle_test_known_page_open(
            tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
            children_q: Query<&Children>,
            mut commands: Commands,
        ) {
            for (entity, task) in &tasks {
                if task.url.starts_with("vmux://terminal/") {
                    crate::clear_stack_children(task.stack, &children_q, &mut commands);
                    commands.spawn((Browser, Terminal, ChildOf(task.stack)));
                    commands.entity(entity).insert(PageOpenHandled);
                } else if task.url.starts_with("vmux://agent/") {
                    crate::clear_stack_children(task.stack, &children_q, &mut commands);
                    commands.entity(entity).insert(PageOpenHandled);
                }
            }
        }

        #[derive(Resource, Default)]
        struct CapturedNavigateUrls(Vec<String>);

        #[test]
        fn browser_navigate_triggers_request_navigate_with_url() {
            use bevy_cef::prelude::RequestNavigate;
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings())
                .init_resource::<CapturedNavigateUrls>();

            let pane = app.world_mut().spawn(Pane).id();
            let stack = app
                .world_mut()
                .spawn(vmux_layout::stack::stack_bundle())
                .insert(ChildOf(pane))
                .id();
            app.world_mut().spawn(Browser).insert(ChildOf(stack));

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

            app.add_observer(
                |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://example.com".to_string()]);
        }

        #[test]
        fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            app.world_mut().resource_mut::<FocusedStack>().stack = None;

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
            let tab_count_under_pane = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane)
                .count();
            assert_eq!(
                tab_count_under_pane, 1,
                "browser_navigate should have spawned exactly one tab in the focused pane"
            );

            let mut tab_metadata =
                world.query_filtered::<&PageMetadata, With<vmux_layout::stack::Stack>>();
            let tab_urls: Vec<String> = tab_metadata.iter(world).map(|p| p.url.clone()).collect();
            assert!(
                tab_urls.contains(&"https://example.com".to_string()),
                "tab entity should have PageMetadata with the URL; found {tab_urls:?}"
            );

            let mut browsers = world.query::<(&Browser, &PageMetadata)>();
            let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
            assert!(
                urls.contains(&"https://example.com".to_string()),
                "browser entity with the URL should exist; found {urls:?}"
            );
        }

        #[test]
        fn agent_browser_navigate_stacks_new_page_and_waits_for_snapshot() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ConsumerPlugin));
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            let first_stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    LastActivatedAt(1),
                    ChildOf(pane),
                ))
                .id();
            app.world_mut().spawn((Browser, ChildOf(first_stack)));
            let request_id = [7; 16];
            app.world_mut()
                .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
                .write(vmux_layout::BrowserNavigateRequest {
                    url: "https://second.example".into(),
                    pane: Some(pane.to_bits().to_string()),
                    request_id: Some(request_id),
                    new_stack: true,
                    profile: Some("agent-1".into()),
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut stacks = world.query_filtered::<
                (Entity, &PageMetadata, &LastActivatedAt),
                With<vmux_layout::stack::Stack>,
            >();
            let second = stacks
                .iter(world)
                .find(|(_, metadata, _)| metadata.url == "https://second.example")
                .map(|(entity, _, activated)| (entity, activated.0))
                .expect("new browser stack");
            assert_ne!(second.0, first_stack);
            assert!(second.1 > 1);
            assert_eq!(world.resource::<PendingNavSnapshots>().0.len(), 1);
            assert_eq!(
                world
                    .resource::<PendingNavSnapshots>()
                    .0
                    .values()
                    .next()
                    .unwrap()
                    .request_id,
                request_id
            );
        }

        #[test]
        fn agent_browser_navigate_does_not_raise_new_stack_during_user_interaction() {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ConsumerPlugin));
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            let first_stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    LastActivatedAt(10),
                    ChildOf(pane),
                ))
                .id();
            app.world_mut().spawn((Browser, ChildOf(first_stack)));
            app.insert_resource(RecentBrowserInteraction {
                stack: Some(first_stack),
                at: Some(std::time::Instant::now()),
            });
            app.world_mut()
                .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
                .write(vmux_layout::BrowserNavigateRequest {
                    url: "https://second.example".into(),
                    pane: Some(pane.to_bits().to_string()),
                    request_id: None,
                    new_stack: true,
                    profile: Some("agent-1".into()),
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut stacks = world
            .query_filtered::<(&PageMetadata, &LastActivatedAt), With<vmux_layout::stack::Stack>>();
            let activated = stacks
                .iter(world)
                .find(|(metadata, _)| metadata.url == "https://second.example")
                .map(|(_, activated)| activated.0)
                .expect("new browser stack");
            assert_eq!(activated, 0);
        }

        #[test]
        fn browser_navigate_targets_specific_pane_when_id_provided() {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane_a = app.world_mut().spawn(Pane).id();
            let pane_b = app.world_mut().spawn(Pane).id();

            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "https://example.com".to_string(),
                        pane: Some(pane_b.to_bits().to_string()),
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
            let tabs_in_b = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane_b)
                .count();
            let tabs_in_a = tabs
                .iter(world)
                .filter(|child_of| child_of.get() == pane_a)
                .count();
            assert_eq!(tabs_in_b, 1, "tab should be spawned in target pane B");
            assert_eq!(tabs_in_a, 0, "no tab should be spawned in focused pane A");
        }

        #[test]
        fn browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane() {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
            let request_id = AgentRequestId::new();

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id,
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://terminal/".to_string(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let terminal_count = world.query::<&Terminal>().iter(world).count();
            assert!(
                terminal_count >= 1,
                "terminal should be spawned in focused pane"
            );
            assert!(
                world
                    .resource::<PendingNavSnapshots>()
                    .0
                    .values()
                    .any(|pending| pending.request_id == request_id.0),
                "terminal navigation should wait for its snapshot"
            );
        }

        #[test]
        fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane_a = app.world_mut().spawn(Pane).id();
            let pane_b = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://terminal/".to_string(),
                        pane: Some(pane_b.to_bits().to_string()),
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let mut terminals = world.query_filtered::<&ChildOf, With<Terminal>>();
            let term_parents: Vec<Entity> = terminals.iter(world).map(|c| c.get()).collect();
            let mut found_in_b = 0;
            let mut found_in_a = 0;
            for tab in &term_parents {
                if let Some(co) = world.get::<ChildOf>(*tab) {
                    if co.get() == pane_b {
                        found_in_b += 1;
                    } else if co.get() == pane_a {
                        found_in_a += 1;
                    }
                }
            }
            assert_eq!(found_in_b, 1, "terminal should be in target pane B");
            assert_eq!(found_in_a, 0, "no terminal in focused pane A");
        }

        #[test]
        fn browser_navigate_with_unknown_vmux_url_errors() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://nonsense/".to_string(),
                        pane: None,
                    },
                });

            // One extra update vs. the other navigate tests: the fallback now grants
            // unknown `vmux://` URLs a one-frame grace before rendering the error page.
            app.update();
            app.update();
            app.update();

            let world = app.world_mut();
            let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
            let browser_titles: Vec<String> = browsers
                .iter(world)
                .map(|meta| meta.title.clone())
                .collect();
            let terminal_count = world.query::<&Terminal>().iter(world).count();
            assert_eq!(
                browser_titles,
                vec!["Page not found".to_string()],
                "unknown vmux URL should render an error page"
            );
            assert_eq!(
                terminal_count, 0,
                "no terminal should be spawned for unknown vmux URL"
            );
        }

        #[test]
        fn page_open_error_renders_error_page() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
            app.insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            let stack = app
                .world_mut()
                .spawn((
                    vmux_layout::stack::stack_bundle(),
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(pane),
                ))
                .id();

            app.world_mut().spawn((
                PageOpenTask {
                    id: PageOpenId::new(),
                    stack,
                    url: "vmux://terminal/bad".to_string(),
                    request_id: None,
                },
                PageOpenError {
                    message: "malformed terminal URL".to_string(),
                },
            ));

            app.update();
            app.update();

            let world = app.world_mut();
            let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
            let browser_titles: Vec<String> = browsers
                .iter(world)
                .map(|meta| meta.title.clone())
                .collect();
            assert_eq!(
                browser_titles,
                vec!["Page failed to load".to_string()],
                "page handler errors should render an error page"
            );
        }

        #[test]
        fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(vmux_agent::host::AgentExecutableOverride(
                    std::collections::HashMap::from([(vmux_core::agent::AgentKind::Claude, true)]),
                ))
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://agent/claude/cli/".into(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let standalone_browser_count = world
                .query_filtered::<&Browser, Without<Terminal>>()
                .iter(world)
                .count();
            assert_eq!(
                standalone_browser_count, 0,
                "claude URL should never spawn a standalone browser tab"
            );
        }

        #[test]
        fn browser_navigate_with_codex_url_does_not_spawn_standalone_browser() {
            use vmux_layout::Browser;

            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                AgentSessionPlugin,
                ConsumerPlugin,
            ));
            app.init_resource::<AgentStrategies>()
                .insert_resource(vmux_agent::host::AgentExecutableOverride(
                    std::collections::HashMap::from([(vmux_core::agent::AgentKind::Codex, true)]),
                ))
                .insert_resource(FocusedStack::default())
                .insert_resource(test_settings());

            let pane = app.world_mut().spawn(Pane).id();
            app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: vmux_service::agent_events::CommandOrigin::User,
                    command: ServiceAgentCommand::BrowserNavigate {
                        url: "vmux://agent/codex/cli/".into(),
                        pane: None,
                    },
                });

            app.update();
            app.update();

            let world = app.world_mut();
            let standalone_browser_count = world
                .query_filtered::<&Browser, Without<Terminal>>()
                .iter(world)
                .count();
            assert_eq!(
                standalone_browser_count, 0,
                "codex URL should never spawn a standalone browser tab"
            );
        }
    }

    mod open_in_place_flow {
        use bevy::ecs::message::Messages;
        use bevy::prelude::*;
        use bevy_cef::prelude::RequestNavigate;
        use vmux_command::open::OpenCommand;
        use vmux_command::{AppCommand, BrowserCommand, BrowserViewCommand};
        use vmux_core::{PageOpenRequest, PageOpenTarget};
        use vmux_history::LastActivatedAt;
        use vmux_layout::Browser;
        use vmux_layout::pane::Pane;
        use vmux_layout::stack::stack_bundle;
        use vmux_layout::tab::Tab;
        use vmux_terminal::Terminal;

        #[derive(Resource, Default)]
        struct CapturedNavigateUrls(Vec<String>);

        #[derive(Resource, Default)]
        struct CapturedPageOpenRequests(Vec<PageOpenRequest>);

        fn build_app() -> App {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_command::CommandPlugin,
                vmux_terminal::TerminalContractPlugin,
                crate::command::CommandPlugin,
            ))
            .add_message::<PageOpenRequest>()
            .add_systems(
                Update,
                capture_page_open_requests.after(vmux_command::ReadAppCommands),
            )
            .init_resource::<CapturedNavigateUrls>()
            .init_resource::<CapturedPageOpenRequests>()
            .add_observer(
                |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );
            for host in [
                "terminal", "agent", "services", "settings", "team", "spaces",
            ] {
                vmux_core::register_host_spawn(&mut app, host);
            }
            app
        }

        fn capture_page_open_requests(
            mut reader: MessageReader<PageOpenRequest>,
            mut captured: ResMut<CapturedPageOpenRequests>,
        ) {
            captured.0.extend(reader.read().cloned());
        }

        fn build_focused_stack(app: &mut App) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut().spawn(Browser).insert(ChildOf(stack));
        }

        fn build_focused_terminal_stack(app: &mut App) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut()
                .spawn((Browser, Terminal))
                .insert(ChildOf(stack));
        }

        fn build_focused_native_stack(app: &mut App, native_url: &str) {
            let space = app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, LastActivatedAt(1), ChildOf(space)))
                .id();
            let stack = app
                .world_mut()
                .spawn(stack_bundle())
                .insert((ChildOf(pane), LastActivatedAt(1)))
                .id();
            app.world_mut()
                .spawn((
                    Browser,
                    vmux_core::PageMetadata {
                        url: native_url.to_string(),
                        title: native_url.to_string(),
                        icon: vmux_core::PageIcon::None,
                        bg_color: None,
                    },
                ))
                .insert(ChildOf(stack));
        }

        #[test]
        fn in_place_with_explicit_url_triggers_request_navigate() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://example.com".into()),
                    },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://example.com".to_string()]);
        }

        #[test]
        fn in_place_with_vmux_url_routes_through_page_open() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://agent/vibe".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://agent/vibe");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_from_plain_vmux_to_web_navigates_in_place() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "vmux://history/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://mistral.ai".into()),
                    },
                )));

            app.update();

            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(navigates.0, vec!["https://mistral.ai".to_string()]);
        }

        #[test]
        fn in_place_from_web_to_plain_vmux_navigates_in_place() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://history/".into()),
                    },
                )));

            app.update();

            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(navigates.0, vec!["vmux://history/".to_string()]);
        }

        #[test]
        fn in_place_to_settings_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://settings/".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://settings/");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_to_terminal_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "vmux://settings/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("vmux://terminal/".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "vmux://terminal/");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_to_file_routes_through_page_open() {
            let mut app = build_app();
            build_focused_native_stack(&mut app, "https://example.com/");

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("file:///tmp/x".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "file:///tmp/x");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn in_place_from_terminal_to_web_routes_through_page_open() {
            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace {
                        url: Some("https://google.com".into()),
                    },
                )));

            app.update();

            let navigates = app.world().resource::<CapturedNavigateUrls>();
            assert!(navigates.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert_eq!(page_opens.0.len(), 1);
            assert_eq!(page_opens.0[0].url, "https://google.com");
            assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
        }

        #[test]
        fn zoom_in_on_terminal_emits_font_size_increase() {
            use bevy::ecs::message::Messages;

            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::View(
                    BrowserViewCommand::ZoomIn,
                )));

            app.update();

            let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
                .world_mut()
                .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
                .drain()
                .collect();
            assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Increase]);
        }

        #[test]
        fn zoom_reset_on_terminal_emits_font_size_reset() {
            use bevy::ecs::message::Messages;

            let mut app = build_app();
            build_focused_terminal_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::View(
                    BrowserViewCommand::ZoomReset,
                )));

            app.update();

            let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
                .world_mut()
                .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
                .drain()
                .collect();
            assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Reset]);
        }

        #[test]
        fn in_place_with_none_url_uses_startup_setting() {
            let mut app = build_app();
            app.insert_resource(vmux_core::EffectiveStartupUrl(
                "https://startup.example".into(),
            ));
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace { url: None },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert_eq!(captured.0, vec!["https://startup.example".to_string()]);
        }

        #[test]
        fn in_place_with_none_url_and_no_startup_does_not_navigate() {
            let mut app = build_app();
            build_focused_stack(&mut app);

            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InPlace { url: None },
                )));

            app.update();

            let captured = app.world().resource::<CapturedNavigateUrls>();
            assert!(captured.0.is_empty());
            let page_opens = app.world().resource::<CapturedPageOpenRequests>();
            assert!(page_opens.0.is_empty());
        }
    }
}

#[cfg(test)]
mod update_notice_tests {
    use super::should_emit_update;
    use vmux_layout::UpdateState;

    fn downloading(v: &str) -> UpdateState {
        UpdateState::Downloading {
            version: v.into(),
            downloaded: 1,
            total: 2,
        }
    }

    #[test]
    fn emits_on_change() {
        assert!(should_emit_update(
            &UpdateState::Ready {
                version: "v2".into()
            },
            &None,
            false
        ));
        assert!(should_emit_update(
            &UpdateState::Idle,
            &Some(downloading("v2")),
            false
        ));
    }

    #[test]
    fn no_emit_when_unchanged_and_no_page_ready() {
        assert!(!should_emit_update(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            false
        ));
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(!should_emit_update(&r, &Some(r.clone()), false));
    }

    #[test]
    fn re_emits_non_idle_on_page_ready() {
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(should_emit_update(&r, &Some(r.clone()), true));
        assert!(!should_emit_update(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            true
        ));
    }
}
