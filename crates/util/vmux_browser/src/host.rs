mod agent;
mod agent_pane;

use crate::present::CommandBarWindowedFrame;
use bevy::{ecs::relationship::Relationship, input::mouse::MouseButton, prelude::*};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::CommandLineConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use vmux_command::command_bar::PendingCommandBarReveal;
use vmux_command::command_bar::panel::CommandBarPanelActive;
use vmux_core::{PageOpenSet, page::PageReady};
use vmux_flex::prelude::*;
use vmux_layout::{
    Browser, Header, Open, PendingWebviewReveal, UpdateState, bookmark::BookmarkContextMenuActive,
    overlay::LayoutOverlayActive, side_sheet::SideSheet,
};
use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;
use vmux_ui::theme::ThemeEvent;

pub(crate) use agent::{
    AgentBrowserGoBack, AgentBrowserGoForward, AgentBrowserHistorySearch,
    AgentBrowserInstallExtension, AgentBrowserNavigate, AgentBrowserPlugin, AgentBrowserScroll,
    AgentBrowserSnapshot,
};
pub use agent_pane::AgentBrowserResolve;

#[derive(Clone, Copy, Debug, Message)]
pub(crate) struct WebviewLoadCompleted {
    pub(crate) webview: Entity,
}

pub(crate) fn configure_cef_backend_sync(app: &mut App) -> &mut App {
    app.configure_sets(
        Update,
        crate::BrowserSystemSet::SyncCefBackend.before(CefSystems::CreateAndResize),
    )
    .add_systems(
        Update,
        sync_cef_backend
            .in_set(crate::BrowserSystemSet::SyncCefBackend)
            .after(PageOpenSet::Fallback)
            .after(crate::BrowserSystemSet::SpawnPopupStacks),
    )
}

pub(crate) fn cef_command_line_config() -> CommandLineConfig {
    CommandLineConfig {
        switches: vmux_core::profile::Profile::current()
            .cef_keychain_switches()
            .to_vec(),
        switch_values: vec![("disable-features", "BackForwardCache")],
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn cef_os_crypt_key_provider() -> Option<bevy_cef::CefOsCryptKeyProvider> {
    Some(cef_os_crypt_key)
}

#[cfg(target_os = "macos")]
fn cef_os_crypt_key() -> Result<bevy_cef::CefOsCryptKey, String> {
    let key = vmux_core::profile::safe_storage::SafeStorage::browser_key()
        .map_err(|error| error.to_string())?;
    Ok(bevy_cef::CefOsCryptKey::new(*key.as_bytes()))
}

pub(crate) fn theme_event(settings: &AppSettings) -> ThemeEvent {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    ThemeEvent {
        radius: settings.layout.radius,
        catalog: external_locale_catalog(locale.as_str()),
        locale: locale.into_string(),
    }
}

pub(crate) fn browser_accept_language_list(locale: &str) -> String {
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
    let directory = vmux_core::profile::ProfilePaths::current()
        .config()
        .join("locales");
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

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CefPointerRegions<'w, 's> {
    rows: CefPointerRegionQuery<'w, 's>,
}

impl CefPointerRegions<'_, '_> {
    pub(crate) fn contains(&self, cursor: Vec2) -> bool {
        for row in self.rows.iter() {
            if CefPointerHitRect::from_row(row).contains(cursor) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CefPointerHitRect {
    pub(crate) rect: ComputedNode,
    pub(crate) interactive: bool,
}

pub(crate) static NATIVE_LAYOUT_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);

impl CefPointerHitRect {
    fn from_row(row: CefPointerRegionRow<'_>) -> Self {
        let (header, side_sheet, node, &rect, visibility, open) = row;
        let interactive = (header.is_some() || side_sheet.is_some())
            && open
            && node.display != Display::None
            && !matches!(visibility, Some(Visibility::Hidden))
            && !rect.is_empty();
        Self { rect, interactive }
    }

    pub(crate) fn contains(self, point: Vec2) -> bool {
        self.interactive && self.rect.contains(point)
    }
}

pub(crate) fn pointer_button_from_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

pub(crate) type LayoutPointerCapture = Or<(
    With<BookmarkContextMenuActive>,
    With<CommandBarPanelActive>,
    With<LayoutOverlayActive>,
)>;

fn sync_cef_backend(
    browser_entities: Query<Entity, With<Browser>>,
    webviews: Query<
        (Entity, Has<WebviewNativeOverlay>, Has<WebviewWindowed>),
        (With<Browser>, With<WebviewSource>),
    >,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut browsers: NonSendMut<Browsers>,
    mut commands: Commands,
) {
    let mut moved = Vec::new();
    for entity in &browser_entities {
        let mut current = entity;
        let mut inherited = None;
        while let Ok(parent) = child_of.get(current).map(Relationship::get) {
            if let Ok(host) = host_windows.get(parent) {
                inherited = Some(*host);
                break;
            }
            current = parent;
        }
        let Some(inherited) = inherited else {
            continue;
        };
        if host_windows.get(entity).ok() != Some(&inherited) {
            commands.entity(entity).insert(inherited);
            moved.push(entity);
        }
    }

    let mut recreate = Vec::new();
    for (entity, native_overlay, _) in &webviews {
        let stale_backend = browsers
            .is_windowed(&entity)
            .is_some_and(|windowed| !windowed);
        let stale_overlay = browsers.has_browser(entity) && native_overlay;
        if stale_backend || stale_overlay || moved.contains(&entity) {
            recreate.push(entity);
        }
    }
    for entity in &recreate {
        browsers.close(entity);
    }
    for (entity, native_overlay, windowed) in &webviews {
        let needs_recreate = recreate.contains(&entity);
        let settled = windowed && !native_overlay && !needs_recreate;
        if settled {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity
            .insert(WebviewWindowed)
            .remove::<WebviewNativeOverlay>();
        if needs_recreate {
            entity
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
}

pub(crate) fn agent_ring_rgb(key: &str) -> [f32; 3] {
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

pub(crate) const CLAUDE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/claude.png");
pub(crate) const CODEX_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/codex.png");
pub(crate) const VIBE_LOGO_PNG: &[u8] = include_bytes!("../assets/agent-logos/vibe.png");

pub(crate) struct LogoBitmap {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn decode_premultiplied(png: &[u8]) -> Option<LogoBitmap> {
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

pub(crate) fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub(crate) struct LayoutHoverRefreshState {
    pub(crate) sequence: u64,
    pub(crate) position: Option<Vec2>,
    pub(crate) in_region: bool,
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn reset_layout_cef_hover(
    browsers: &Browsers,
    buttons: &ButtonInput<MouseButton>,
    layout: Entity,
    state: &mut LayoutHoverRefreshState,
) {
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

#[derive(Default)]
pub(crate) struct WindowedHoverRefreshState {
    pub(crate) entity: Option<Entity>,
    pub(crate) position: Option<Vec2>,
}

pub(crate) const LAYOUT_INPUT_BURST: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Component, Default)]
pub(crate) struct LayoutFrameRateState {
    pub(crate) native_sequence: u64,
    pub(crate) last_input: Option<std::time::Instant>,
    pub(crate) last_emit: Option<std::time::Instant>,
    pub(crate) dragging_layout: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CommandBarRoute {
    pub(crate) generation: u64,
    pub(crate) owns_input: bool,
    pub(crate) frame: Option<CommandBarWindowedFrame>,
    pub(crate) scale: f32,
}

pub(crate) static NATIVE_COMMAND_BAR_ROUTE: LazyLock<Mutex<CommandBarRoute>> =
    LazyLock::new(|| Mutex::new(CommandBarRoute::default()));
static NATIVE_LEFT_MOUSE_DOWN: AtomicBool = AtomicBool::new(false);

#[cfg(any(target_os = "macos", test))]
pub(crate) fn native_command_bar_route() -> CommandBarRoute {
    *NATIVE_COMMAND_BAR_ROUTE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn set_native_left_mouse_down(down: bool) {
    NATIVE_LEFT_MOUSE_DOWN.store(down, Ordering::Relaxed);
}

pub fn native_left_mouse_down() -> bool {
    NATIVE_LEFT_MOUSE_DOWN.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
pub(crate) fn command_bar_windowed_frame_contains(
    frame: CommandBarWindowedFrame,
    cursor: Vec2,
) -> bool {
    cursor.x >= frame.left_px
        && cursor.x <= frame.left_px + frame.width_px
        && cursor.y >= frame.top_px
        && cursor.y <= frame.top_px + frame.height_px
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutWindowPadding {
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
    pub(crate) left: f32,
}

fn val_px(value: Val) -> f32 {
    match value {
        Val::Px(px) => px,
        _ => 0.0,
    }
}

pub(crate) fn layout_window_padding_from_node(node: &Node) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: val_px(node.padding.top),
        right: val_px(node.padding.right),
        bottom: val_px(node.padding.bottom),
        left: val_px(node.padding.left),
    }
}

pub(crate) fn layout_window_padding_from_settings(settings: &AppSettings) -> LayoutWindowPadding {
    LayoutWindowPadding {
        top: settings.layout.window.pad_top(),
        right: settings.layout.window.pad_right(),
        bottom: settings.layout.window.pad_bottom(),
        left: settings.layout.window.pad_left(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutFixedOffsets {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) height: f32,
}

impl LayoutFixedOffsets {
    pub(crate) fn from_node(rect: &ComputedNode, window_width_px: f32) -> Option<Self> {
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

pub(crate) fn should_emit_cached_payload(body: &str, last: &str, page_ready_changed: bool) -> bool {
    page_ready_changed || body != last
}

pub(crate) fn should_emit_update(
    current: &UpdateState,
    last: &Option<UpdateState>,
    page_ready_changed: bool,
) -> bool {
    last.as_ref() != Some(current) || (page_ready_changed && *current != UpdateState::Idle)
}

#[derive(Component, Clone, Debug)]
pub(crate) struct PageOpenFallbackDeferred;

#[derive(Component, Clone, Debug)]
pub(crate) struct PageOpenAwaitSnapshot {
    pub(crate) started: std::time::Duration,
}

pub(crate) fn page_open_response(
    request_id: Option<[u8; 16]>,
    result: Result<(), String>,
) -> Option<vmux_core::service::ServiceRequest> {
    use vmux_api::protocol::{AgentCommandResult, AgentRequestId, ClientMessage};
    use vmux_core::service::ServiceRequest;
    let request_id = request_id?;
    let result = match result {
        Ok(()) => AgentCommandResult::Ok,
        Err(message) => AgentCommandResult::Error(message),
    };
    Some(ServiceRequest(ClientMessage::AgentCommandResponse {
        request_id: AgentRequestId(request_id),
        result,
    }))
}

#[derive(Component, Clone)]
pub(crate) struct PendingNavigationSnapshot {
    pub(crate) webview: Entity,
    pub request_id: [u8; 16],
    pub started: std::time::Duration,
    pub saw_loading: bool,
    pub pane: Option<String>,
}

#[derive(Message)]
pub(crate) struct PendingNavigationUpdate {
    pub(crate) webview: Entity,
    pub(crate) pending: Option<PendingNavigationSnapshot>,
}

impl PendingNavigationUpdate {
    pub(crate) fn set(
        webview: Entity,
        request_id: [u8; 16],
        started: std::time::Duration,
        pane: Option<String>,
    ) -> Self {
        Self {
            webview,
            pending: Some(PendingNavigationSnapshot {
                webview,
                request_id,
                started,
                saw_loading: false,
                pane,
            }),
        }
    }

    pub(crate) fn clear(webview: Entity) -> Self {
        Self {
            webview,
            pending: None,
        }
    }
}

pub(crate) fn cef_root_cache_path() -> Option<String> {
    vmux_core::profile::ProfilePaths::current().cef_cache()
}
