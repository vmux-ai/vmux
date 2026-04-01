//! Command palette (Arc-style): ⌘T / Ctrl+T, glass panel, URL/search rows with **active pane** vs **new pane** targets.
//!
//! The palette’s UI camera uses the **full workspace viewport** (entire window) so the overlay is
//! centered globally, not clipped to the active pane (same logical basis as
//! [`vmux_layout::layout_viewport_for_workspace`]).
//!
//! Register [`CommandPlugin`] after [`vmux_input::InputPlugin`]. On [`Startup`], run
//! [`setup`] after the main scene camera exists (e.g. after `vmux`’s `spawn_camera`).

use bevy::app::AppExit;
use bevy::asset::AssetPath;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::image::{ImageFormatSetting, ImageLoaderSettings};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::picking::prelude::PointerButton;
use bevy::prelude::*;
use bevy::text::{TextLayout, TextLayoutInfo};
use bevy::ui::{
    widget::{text_system, ImageNode, NodeImageMode},
    BoxShadow, ComputedNode, Display, GlobalZIndex, Overflow, OverflowAxis, ScrollPosition,
    UiTargetCamera, UiSystems,
};
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::{RequestNavigate, WebviewExtendStandardMaterial, WebviewSource};
use leafwing_input_manager::prelude::ActionState;
use leafwing_input_manager::Actionlike;
use vmux_core::{
    Active, NavigationHistory, SessionSavePath, SessionSaveQueue, VMUX_PALETTE_ROW_COUNT,
    VmuxCommandPaletteState, VmuxPendingUiLibraryNavigation, VmuxPendingUiLibraryNavTarget,
    VmuxUiLibraryBaseUrl, VmuxWorldCamera,
    favicon_url_for_page_url, page_host_for_favicon_url,
};
use vmux_core::input_root::AppInputRoot;
use vmux_layout::{
    apply_pane_layout, layout_viewport_for_workspace, layout_workspace_pane_rects, try_cycle_pane_focus,
    try_kill_active_pane, try_mirror_pane_layout, try_rotate_window, try_select_pane_direction,
    try_split_active_pane, try_swap_active_pane, try_toggle_zoom_pane, History, LayoutAxis,
    Layout, LoadingBarMaterial, Pane, PaneChromeLoadingBar, PaneChromeOwner,
    PaneChromeStrip, PaneFocusIncoming, PaneLastUrl, PaneSwapDir, SessionLayoutSnapshot,
    VmuxAppSettings, Webview,
};
use vmux_settings::VmuxBindingSettings;

mod binding_id;

pub use binding_id::{app_command_from_binding_id, key_action_from_binding_id};

/// Stable name for keyboard-bound actions (same as [`KeyAction`]).
pub type BoundCommand = KeyAction;

/// Keyboard / [`InputManager`](leafwing_input_manager::InputManagerPlugin)-bound actions (small, [`Actionlike`]).
#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Quit,
    /// Centered command palette (⌘T on macOS, Ctrl+T elsewhere).
    ToggleCommandPalette,
    /// Command palette with the active pane’s current URL in the field (⌘L on macOS, Ctrl+L elsewhere).
    FocusCommandPaletteUrl,
    /// Focus or open the history pane (⌘Y on macOS, Ctrl+Shift+H elsewhere).
    OpenHistory,
    /// Split a new pane with the history UI (palette / custom binding).
    OpenHistoryInNewTab,
    SplitHorizontal,
    SplitVertical,
    CycleNextPane,
    SelectPane(PaneSwapDir),
    SwapPane(PaneSwapDir),
    ToggleZoom,
    MirrorLayout,
    RotateBackward,
    RotateForward,
    ClosePane,
}

impl KeyAction {
    /// Search keywords used by the command palette command filter.
    pub const fn palette_match_blob(self) -> &'static str {
        match self {
            KeyAction::Quit => "quit exit shutdown close q app",
            KeyAction::ToggleCommandPalette => "command palette search open launcher t",
            KeyAction::FocusCommandPaletteUrl => "focus url omnibox address bar l",
            KeyAction::OpenHistory => "history pane browse visited pages y open",
            KeyAction::OpenHistoryInNewTab => "history new tab pane split duplicate y",
            KeyAction::SplitHorizontal => "split horizontal pane side column percent tmux",
            KeyAction::SplitVertical => "split vertical pane row stack quote tmux",
            KeyAction::CycleNextPane => "cycle next pane focus window o alternate",
            KeyAction::SelectPane(PaneSwapDir::Left) => "focus pane left select move arrow",
            KeyAction::SelectPane(PaneSwapDir::Right) => "focus pane right select move arrow",
            KeyAction::SelectPane(PaneSwapDir::Up) => "focus pane up select move arrow",
            KeyAction::SelectPane(PaneSwapDir::Down) => "focus pane down select move arrow",
            KeyAction::SwapPane(PaneSwapDir::Left) => "swap pane left exchange position ctrl",
            KeyAction::SwapPane(PaneSwapDir::Right) => "swap pane right exchange position ctrl",
            KeyAction::SwapPane(PaneSwapDir::Up) => "swap pane up exchange position ctrl",
            KeyAction::SwapPane(PaneSwapDir::Down) => "swap pane down exchange position ctrl",
            KeyAction::ToggleZoom => "zoom maximize resize pane full z",
            KeyAction::MirrorLayout => "mirror flip swap halves layout m",
            KeyAction::RotateBackward => "rotate layout backward cycle panes bracket",
            KeyAction::RotateForward => "rotate layout forward cycle panes bracket",
            KeyAction::ClosePane => "close kill pane remove x",
        }
    }

}

/// All palette rows and deferred execution (vim-style: one command space — URL open, tab focus, and key-bound actions).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AppCommand {
    ClosePane,
    CycleNextPane,
    FocusCommandPaletteUrl,
    FocusPane(Entity),
    MirrorLayout,
    Noop,
    Omnibox { new_pane: bool },
    OpenHistory,
    OpenHistoryInNewTab,
    OpenUrl { url: String, new_pane: bool },
    /// Debug UI library (`VmuxUiLibraryBaseUrl`): navigate active pane to the bundle root.
    OpenUiLibrary,
    /// Same URL as [`Self::OpenUiLibrary`] after splitting a new pane (horizontal).
    OpenUiLibraryInNewPane,
    Quit,
    RotateBackward,
    RotateForward,
    SelectPane(PaneSwapDir),
    SplitHorizontal,
    SplitVertical,
    SwapPane(PaneSwapDir),
    ToggleCommandPalette,
    ToggleZoom,
    WebSearch { new_pane: bool },
}

impl From<KeyAction> for AppCommand {
    fn from(k: KeyAction) -> Self {
        match k {
            KeyAction::Quit => AppCommand::Quit,
            KeyAction::ToggleCommandPalette => AppCommand::ToggleCommandPalette,
            KeyAction::FocusCommandPaletteUrl => AppCommand::FocusCommandPaletteUrl,
            KeyAction::OpenHistory => AppCommand::OpenHistory,
            KeyAction::OpenHistoryInNewTab => AppCommand::OpenHistoryInNewTab,
            KeyAction::SplitHorizontal => AppCommand::SplitHorizontal,
            KeyAction::SplitVertical => AppCommand::SplitVertical,
            KeyAction::CycleNextPane => AppCommand::CycleNextPane,
            KeyAction::SelectPane(d) => AppCommand::SelectPane(d),
            KeyAction::SwapPane(d) => AppCommand::SwapPane(d),
            KeyAction::ToggleZoom => AppCommand::ToggleZoom,
            KeyAction::MirrorLayout => AppCommand::MirrorLayout,
            KeyAction::RotateBackward => AppCommand::RotateBackward,
            KeyAction::RotateForward => AppCommand::RotateForward,
            KeyAction::ClosePane => AppCommand::ClosePane,
        }
    }
}

impl AppCommand {
    /// If this command is a [`KeyAction`] variant, return it (palette `:` rows and chord deferral).
    pub fn as_key_action(&self) -> Option<KeyAction> {
        match self {
            AppCommand::Quit => Some(KeyAction::Quit),
            AppCommand::ToggleCommandPalette => Some(KeyAction::ToggleCommandPalette),
            AppCommand::FocusCommandPaletteUrl => Some(KeyAction::FocusCommandPaletteUrl),
            AppCommand::OpenHistory => Some(KeyAction::OpenHistory),
            AppCommand::OpenHistoryInNewTab => Some(KeyAction::OpenHistoryInNewTab),
            AppCommand::SplitHorizontal => Some(KeyAction::SplitHorizontal),
            AppCommand::SplitVertical => Some(KeyAction::SplitVertical),
            AppCommand::CycleNextPane => Some(KeyAction::CycleNextPane),
            AppCommand::SelectPane(d) => Some(KeyAction::SelectPane(*d)),
            AppCommand::SwapPane(d) => Some(KeyAction::SwapPane(*d)),
            AppCommand::ToggleZoom => Some(KeyAction::ToggleZoom),
            AppCommand::MirrorLayout => Some(KeyAction::MirrorLayout),
            AppCommand::RotateBackward => Some(KeyAction::RotateBackward),
            AppCommand::RotateForward => Some(KeyAction::RotateForward),
            AppCommand::ClosePane => Some(KeyAction::ClosePane),
            AppCommand::Omnibox { .. }
            | AppCommand::WebSearch { .. }
            | AppCommand::OpenUrl { .. }
            | AppCommand::OpenUiLibrary
            | AppCommand::OpenUiLibraryInNewPane
            | AppCommand::FocusPane(_)
            | AppCommand::Noop => None,
        }
    }
}

#[derive(Resource, Default)]
pub struct AppCommandRequestQueue {
    pub open_history_requested: bool,
    pub open_history_in_new_tab_requested: bool,
    /// Set from the command palette; same effect as the ⌘L / Ctrl+L shortcut.
    pub focus_command_palette_url_requested: bool,
}

/// Open panes (switch tab), then omnibox / web / GitHub or history / commands.
const MAX_PALETTE_TABS: usize = 8;
const MAX_GITHUB_REPO_SUGGESTIONS: usize = 4;
const GITHUB_SUGGEST_ROW_PAIRS: usize = MAX_GITHUB_REPO_SUGGESTIONS * 2;
/// History rows share the GitHub slot block (same row budget).
const MAX_HISTORY_SUGGEST_URLS: usize = 4;
const MAX_RECENT_HISTORY_WHEN_EMPTY: usize = 5;
const IDX_CMD_START: usize = MAX_PALETTE_TABS + 4 + GITHUB_SUGGEST_ROW_PAIRS;
/// One row per [`AppCommand`] variant (all exposed in `:` command mode).
const MAX_PALETTE_CMD_ROWS: usize = 22;
/// Total palette list rows (fixed grid); ends at the padded command block (use Esc / Toggle palette to dismiss).
const ROWS_MAX: usize = IDX_CMD_START + MAX_PALETTE_CMD_ROWS;
const _: () = assert!(ROWS_MAX == VMUX_PALETTE_ROW_COUNT);

const ROW_BG: Color = Color::srgba(0.12, 0.12, 0.14, 0.35);
const ROW_BG_HOVER: Color = Color::srgba(0.22, 0.22, 0.26, 0.55);
const ROW_BG_SELECTED: Color = Color::srgb(0.72, 0.42, 0.32);
const ROW_TEXT: Color = Color::srgba(0.92, 0.92, 0.94, 0.95);
const ROW_TEXT_SELECTED: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
/// Query field text selection (drawn behind [`CommandPaletteQueryText`]).
const QUERY_SELECTION_HIGHLIGHT: Color = Color::srgba(0.28, 0.42, 0.62, 0.55);
const ROW_SUBTEXT: Color = Color::srgba(0.55, 0.56, 0.62, 1.0);
const ROW_SUBTEXT_SELECTED: Color = Color::srgba(0.88, 0.88, 0.92, 0.95);
const PANEL_BG: Color = Color::srgba(0.11, 0.11, 0.12, 0.92);
const BORDER_SUBTLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Visible / invisible phase length for the command-palette text caret (seconds each).
const PALETTE_CARET_PHASE_SECS: f32 = 0.53;
/// Cap suggestion list height; additional rows scroll (wheel / trackpad).
const PALETTE_LIST_MAX_HEIGHT_PX: f32 = 384.0;
/// Single-column palette: allow roughly the vertical room of the old split Tabs + Links panes.
const PALETTE_LIST_COMBINED_MAX_HEIGHT_PX: f32 = PALETTE_LIST_MAX_HEIGHT_PX + 180.0;
/// Approx. row outer height plus `row_gap` (5) on the scroll list (used for scroll-into-view).
const PALETTE_LIST_ROW_STRIDE_PX: f32 = 42.0;
/// Horizontal line-scroll scale (palette list is Y-scroll only; kept for trackpads reporting line delta on X).
const PALETTE_SCROLL_LINE_HEIGHT_X_PX: f32 = 24.0;

/// Remote favicon URLs (e.g. gstatic) have no path extension; default [`ImageLoaderSettings`] uses
/// [`ImageFormatSetting::FromExtension`] and panics. Guess format from magic bytes instead.
fn load_remote_favicon_image(asset_server: &AssetServer, url: impl Into<AssetPath<'static>>) -> Handle<Image> {
    asset_server.load_with_settings::<Image, ImageLoaderSettings>(url, |s| {
        s.format = ImageFormatSetting::Guess;
    })
}

fn pointer_top_entity(hover_map: &HoverMap, pointer_id: PointerId) -> Option<Entity> {
    let map = hover_map.get(&pointer_id)?;
    if map.is_empty() {
        return None;
    }
    let (&entity, _) = map
        .iter()
        .min_by(|(_, ha), (_, hb)| ha.depth.total_cmp(&hb.depth))?;
    Some(entity)
}

fn entity_to_palette_row_index(
    mut entity: Entity,
    row_q: &Query<&CommandPaletteRow>,
    parents: &Query<&ChildOf>,
) -> Option<usize> {
    for _ in 0..64 {
        if let Ok(CommandPaletteRow(i)) = row_q.get(entity) {
            return Some(*i as usize);
        }
        let Ok(parent) = parents.get(entity) else {
            return None;
        };
        entity = parent.parent();
    }
    None
}

fn note_palette_mouse_motion(
    mut palette: ResMut<VmuxCommandPaletteState>,
    mut reader: MessageReader<MouseMotion>,
) {
    if !palette.open {
        return;
    }
    if reader.read().next().is_some() {
        palette.pointer_row_selects = true;
    }
}

fn sync_command_palette_pointer_selection(
    mut palette: ResMut<VmuxCommandPaletteState>,
    hover_map: Res<HoverMap>,
    row_q: Query<&CommandPaletteRow>,
    parents: Query<&ChildOf>,
) {
    if !palette.open || !palette.pointer_row_selects {
        return;
    }
    let Some(ent) = pointer_top_entity(&hover_map, PointerId::Mouse) else {
        return;
    };
    let Some(idx) = entity_to_palette_row_index(ent, &row_q, &parents) else {
        return;
    };
    if idx < ROWS_MAX && palette.row_selectable_mask[idx] {
        palette.selection = idx;
    }
}

/// Wheel delta forwarded into the palette list; propagates to ancestors so rows can hover-scroll the list.
#[derive(EntityEvent, Debug)]
#[entity_event(propagate, auto_propagate)]
struct CommandPaletteScroll {
    entity: Entity,
    delta: Vec2,
}

#[derive(Resource, Default)]
struct PalettePendingAction(Option<AppCommand>);

/// Ordering for palette systems (input in [`Update`], [`submit`] / [`execute_palette_chord_pending`] in [`PostUpdate`]).
/// Those use large `SystemParam` lists; chaining them as tuples hits trait limits, so we use explicit sets.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum CommandPalettePipeline {
    InputChain,
    Submit,
    Chord,
    SyncVis,
    RefreshLabels,
    StyleRows,
}

#[derive(Component)]
struct CommandPaletteUiCamera;

#[derive(Component)]
struct CommandPaletteRoot;

#[derive(Component)]
struct CommandPaletteBackdrop;

#[derive(Component)]
struct CommandPaletteQueryText;

#[derive(Component)]
struct CommandPaletteQueryPlaceholder;

#[derive(Component)]
struct CommandPaletteQuerySelectionHighlight;

#[derive(Component)]
struct CommandPaletteCaret;

#[derive(Component)]
struct CommandPaletteListScroll;

#[derive(Component)]
struct CommandPaletteRow(u8);

#[derive(Component)]
struct PaletteRowIcon(u8);

#[derive(Component)]
struct PaletteRowFavicon(u8);

#[derive(Component)]
struct PaletteRowPrimary(u8);

#[derive(Component)]
struct PaletteRowSecondary(u8);

#[derive(Component)]
struct PaletteRowEnterHint(u8);

/// Right-side hint that is hidden until pointer hover (e.g. a shortcut string).
#[derive(Component)]
struct PaletteNavEnterHint;

/// Trims and strips a leading `:` used for **command mode** (see [`palette_in_command_mode`]).
fn palette_query_body(query: &str) -> &str {
    let s = query.trim();
    s.strip_prefix(':').map(str::trim).unwrap_or(s)
}

/// Leading `:` switches the palette to command-only suggestions (⌘T then `:`), matching common launcher UX.
fn palette_in_command_mode(query: &str) -> bool {
    query.trim_start().starts_with(':')
}

fn super_or_ctrl_held(keys: &ButtonInput<KeyCode>) -> bool {
    #[cfg(target_os = "macos")]
    {
        keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    }
}

/// Physical `KeyA` or logical `a`/`A` (layout-dependent) for ⌘A / Ctrl+A select-all.
fn palette_select_all_key(ev: &KeyboardInput) -> bool {
    if ev.key_code == KeyCode::KeyA {
        return true;
    }
    matches!(&ev.logical_key, Key::Character(s) if s.as_str().eq_ignore_ascii_case("a"))
}

/// Omnibox resolution: URL as-is, `host.tld` → https, else Google search.
fn omnibox_url(query: &str) -> Option<String> {
    let t = palette_query_body(query);
    if t.is_empty() {
        return None;
    }
    if t.contains("://") {
        return Some(t.to_string());
    }
    if t.contains('.') && !t.chars().any(char::is_whitespace) {
        return Some(format!("https://{t}"));
    }
    let q: String = t.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
    Some(format!("https://www.google.com/search?q={q}"))
}

fn normalized_palette_cmd_body(query: &str) -> String {
    palette_query_body(query)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Debug-only UI library: `:debug vmux ui` → embedded loopback base when [`VmuxUiLibraryBaseUrl`] is set.
fn ui_library_url_for_query(query: &str, base: Option<&str>) -> Option<String> {
    if normalized_palette_cmd_body(query) != "debug vmux ui" {
        return None;
    }
    let b = base?.trim();
    if b.is_empty() {
        return None;
    }
    let b = b.trim_end_matches('/');
    Some(format!("{b}/"))
}

/// Prefer resource / palette copy, then the same env overrides as [`vmux_ui::UiLibraryServerPlugin`].
fn ui_library_stored_base<'a>(
    ui_res: &'a VmuxUiLibraryBaseUrl,
    palette: &'a VmuxCommandPaletteState,
) -> Option<&'a str> {
    ui_res
        .0
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(palette.ui_library_base.as_deref().filter(|s| !s.trim().is_empty()))
}

/// Root URL for the embedded UI library (same trailing slash as `:debug vmux ui`).
fn ui_library_nav_url(stored: Option<&str>) -> Option<String> {
    let b = stored
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            std::env::var("VMUX_UI_LIBRARY_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| std::env::var("VMUX_UI_SHOWCASE_URL").ok().filter(|s| !s.trim().is_empty()))
        })?;
    let b = b.trim();
    if b.is_empty() {
        return None;
    }
    let b = b.trim_end_matches('/');
    Some(format!("{b}/"))
}

/// Same URL the palette rows show for the omnibox rows (UI library + standard omnibox).
fn resolve_omnibox_target(query: &str, ui_library_base: Option<&str>) -> Option<String> {
    ui_library_url_for_query(query, ui_library_base).or_else(|| omnibox_url(query))
}

/// Always a Google search URL for non-empty input (Arc’s second row).
fn web_search_url(query: &str) -> Option<String> {
    let t = palette_query_body(query);
    if t.is_empty() {
        return None;
    }
    let q: String = t.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
    Some(format!("https://www.google.com/search?q={q}"))
}

fn truncate_display(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{head}...")
}

fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
        || ('\u{100000}'..='\u{10fffd}').contains(&chr);
    !is_in_private_use_area && !chr.is_ascii_control()
}

fn query_len_chars(s: &str) -> usize {
    s.chars().count()
}

fn query_char_to_byte(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    match s.char_indices().nth(char_idx) {
        Some((i, _)) => i,
        None => s.len(),
    }
}

/// X offset (**physical** px, same space as [`TextLayoutInfo`] glyph positions) for a caret before
/// UTF-8 byte index `byte_pos`. Multiply by [`ComputedNode::inverse_scale_factor`] on the query text
/// entity to get **logical** px for [`Node::left`].
///
/// Uses glyph bounds only. [`TextLayoutInfo::size`] is scaled to logical in [`bevy_ui::widget::text_system`],
/// but per-glyph positions are not — callers must scale.
fn caret_x_from_text_layout(layout: &TextLayoutInfo, byte_pos: usize, query_len_bytes: usize) -> f32 {
    if layout.glyphs.is_empty() {
        return 0.0;
    }
    for g in &layout.glyphs {
        if byte_pos == g.byte_index {
            return g.position.x;
        }
        let end = g.byte_index + g.byte_length;
        if byte_pos > g.byte_index && byte_pos < end {
            return g.position.x;
        }
    }
    let mut x = 0.0f32;
    for g in &layout.glyphs {
        let end = g.byte_index + g.byte_length;
        if end <= byte_pos {
            x = (g.position.x + g.size.x).max(x);
        }
    }
    if byte_pos >= query_len_bytes {
        return x;
    }
    x
}

/// Horizontal span in layout space for selected characters `[start_char, end_char)` (same convention as [`delete_query_selection`]).
fn selection_highlight_range(
    layout: &TextLayoutInfo,
    query: &str,
    start_char: usize,
    end_char_exclusive: usize,
) -> Option<(f32, f32)> {
    if start_char >= end_char_exclusive {
        return None;
    }
    let qbytes = query.len();
    let start_b = query_char_to_byte(query, start_char);
    let end_b = query_char_to_byte(query, end_char_exclusive);
    let left = caret_x_from_text_layout(layout, start_b, qbytes);
    let right = caret_x_from_text_layout(layout, end_b, qbytes);
    let w = right - left;
    (w > 0.0).then_some((left, w))
}

fn normalized_selection(anchor: Option<usize>, caret: usize) -> Option<(usize, usize)> {
    let a = anchor?;
    if a == caret {
        None
    } else if a < caret {
        Some((a, caret))
    } else {
        Some((caret, a))
    }
}

fn set_query_caret(palette: &mut VmuxCommandPaletteState, next: usize) {
    let len = query_len_chars(&palette.input.query);
    palette.input.caret = next.min(len);
}

fn delete_query_selection(palette: &mut VmuxCommandPaletteState) -> bool {
    let Some((start, end)) =
        normalized_selection(palette.input.selection_anchor, palette.input.caret)
    else {
        return false;
    };
    let bs = query_char_to_byte(&palette.input.query, start);
    let be = query_char_to_byte(&palette.input.query, end);
    palette.input.query.replace_range(bs..be, "");
    palette.input.caret = start;
    palette.input.selection_anchor = None;
    true
}

/// `github.com/user` or `github.com/user/` with no further path segment → suggest repos (Arc-style).
fn github_owner_only(query: &str) -> Option<String> {
    let raw = palette_query_body(query);
    let t = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .unwrap_or(raw);
    let t = t.strip_prefix("www.").unwrap_or(t);
    let lower = t.to_ascii_lowercase();
    let prefix = "github.com/";
    if !lower.starts_with(prefix) {
        return None;
    }
    let rest = &t[prefix.len()..];
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() {
        return None;
    }
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() != 1 {
        return None;
    }
    let owner = parts[0];
    if owner.is_empty()
        || !owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(owner.to_string())
}

fn github_suggestion_rows(owner: &str) -> Vec<(String, String, String)> {
    let o = owner;
    vec![
        (
            format!("{o}/vmux"),
            format!("https://github.com/{o}/vmux"),
            format!("https://github.com/{o}/vmux"),
        ),
        (
            format!("{o}/vmux-legacy"),
            format!("https://github.com/{o}/vmux-legacy"),
            format!("https://github.com/{o}/vmux-legacy"),
        ),
        (
            format!("{o}"),
            "Profile on GitHub".to_string(),
            format!("https://github.com/{o}"),
        ),
        (
            "Search repositories".to_string(),
            format!("github.com/{o}"),
            format!("https://github.com/search?q=user%3A{o}&type=repositories"),
        ),
    ]
}

/// Icons: BMP-only (default UI fonts), no emoji — avoids “tofu” boxes.
const ICON_NAV: &str = ">";
const ICON_NEW_PANE: &str = "+";
const ICON_SEARCH: &str = ":";
const ICON_GH: &str = "#";
const ICON_HISTORY: &str = "@";
const ICON_CMD: &str = "!";

const ENTER_NAV: &str = "";
const ENTER_INERT: &str = "-";
const ENTER_TAB: &str = "Switch to Tab";

fn history_suggestion_urls(
    body: &str,
    history: &NavigationHistory,
    skip_url: Option<&str>,
) -> Vec<String> {
    let mut out = Vec::new();
    let skip = skip_url.map(str::trim);

    if body.is_empty() {
        for e in history.entries.iter() {
            let u = e.url.trim();
            if u.is_empty() || out.iter().any(|x: &String| x == u) {
                continue;
            }
            out.push(u.to_string());
            if out.len() >= MAX_RECENT_HISTORY_WHEN_EMPTY {
                break;
            }
        }
        return out;
    }

    let needle = body.to_ascii_lowercase();
    for e in &history.entries {
        if out.len() >= MAX_HISTORY_SUGGEST_URLS {
            break;
        }
        let u = e.url.trim();
        if u.is_empty() || skip.is_some_and(|s| s == u) {
            continue;
        }
        if !u.to_ascii_lowercase().contains(&needle) {
            continue;
        }
        if out.iter().any(|x: &String| x == u) {
            continue;
        }
        out.push(u.to_string());
    }
    out
}

#[derive(Clone)]
struct PaletteRowSpec {
    visible: bool,
    icon: &'static str,
    favicon: Option<String>,
    primary: String,
    secondary: String,
    /// Trailing column (shortcut, tab switch, `-`, …).
    enter: String,
    /// When true, [`PaletteNavEnterHint`] — show hint on pointer hover only.
    hint_hover_only: bool,
    /// If false, the row is informational only (no selection highlight; pointer does not select it).
    selectable: bool,
    action: AppCommand,
}

/// Prefer persisted [`NavigationHistoryEntry::favicon_url`], else compute from the page URL.
fn palette_favicon_for_url(history: &NavigationHistory, page_url: &str) -> Option<String> {
    if let Some(e) = history.entries.iter().find(|e| e.url == page_url) {
        if let Some(ref u) = e.favicon_url {
            return Some(u.clone());
        }
    }
    favicon_url_for_page_url(page_url)
}

fn palette_tab_title(url: &str) -> String {
    let t = url.trim();
    if t.is_empty() {
        return "New tab".to_string();
    }
    if let Some(host) = page_host_for_favicon_url(url) {
        return truncate_display(&host, 48);
    }
    truncate_display(t, 48)
}

fn collect_command_palette_tabs(
    tree: &Layout,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    main_webviews: &Query<Entity, With<Webview>>,
    default_url: &str,
) -> Vec<(Entity, String)> {
    let mut leaves = Vec::new();
    tree.root.collect_leaves(&mut leaves);
    let mut out = Vec::new();
    for e in leaves {
        if history_panes.contains(e) || !main_webviews.contains(e) {
            continue;
        }
        let mut url = pane_last.get(e).map(|p| p.0.clone()).unwrap_or_default();
        if url.trim().is_empty() {
            url = webview_src
                .get(e)
                .map(|src| match src {
                    WebviewSource::Url(u) | WebviewSource::InlineHtml(u) => u.clone(),
                })
                .unwrap_or_default();
        }
        if url.trim().is_empty() {
            url = default_url.to_string();
        }
        out.push((e, url));
    }
    out
}

fn palette_cmd_match_blob(cmd: &AppCommand) -> &'static str {
    match cmd {
        AppCommand::OpenUiLibrary => "open ui library showcase debug vmux",
        AppCommand::OpenUiLibraryInNewPane => "open ui library new pane split",
        _ => cmd
            .as_key_action()
            .map(KeyAction::palette_match_blob)
            .unwrap_or(""),
    }
}

fn palette_cmd_title_subtitle(cmd: &AppCommand, input: &VmuxBindingSettings) -> (String, String) {
    match cmd {
        AppCommand::OpenUiLibrary => ("Open UI library".to_string(), String::new()),
        AppCommand::OpenUiLibraryInNewPane => ("Open UI library in new pane".to_string(), String::new()),
        _ => cmd
            .as_key_action()
            .map(|ka| {
                let id = ka.to_binding_id();
                (id.palette_title().to_string(), input.shortcut_hint(id))
            })
            .unwrap_or_else(|| (String::new(), String::new())),
    }
}

fn palette_cmd_matches(cmd: &AppCommand, body_lower: &str, input: &VmuxBindingSettings) -> bool {
    if body_lower.is_empty() {
        return false;
    }
    let blob = palette_cmd_match_blob(cmd);
    let (title, sub) = palette_cmd_title_subtitle(cmd, input);
    let mut hay = String::with_capacity(blob.len() + title.len() + sub.len() + 4);
    hay.push_str(blob);
    hay.push(' ');
    hay.push_str(&title.to_ascii_lowercase());
    hay.push(' ');
    hay.push_str(&sub.to_ascii_lowercase());
    if hay.contains(body_lower) {
        return true;
    }
    body_lower
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .all(|t| hay.contains(t))
}

fn palette_cmd_row_order() -> &'static [AppCommand] {
    const ORDER: &[AppCommand] = &[
        AppCommand::Quit,
        AppCommand::ToggleCommandPalette,
        AppCommand::FocusCommandPaletteUrl,
        AppCommand::OpenHistory,
        AppCommand::OpenHistoryInNewTab,
        AppCommand::SplitHorizontal,
        AppCommand::SplitVertical,
        AppCommand::CycleNextPane,
        AppCommand::SelectPane(PaneSwapDir::Left),
        AppCommand::SelectPane(PaneSwapDir::Right),
        AppCommand::SelectPane(PaneSwapDir::Up),
        AppCommand::SelectPane(PaneSwapDir::Down),
        AppCommand::SwapPane(PaneSwapDir::Left),
        AppCommand::SwapPane(PaneSwapDir::Right),
        AppCommand::SwapPane(PaneSwapDir::Up),
        AppCommand::SwapPane(PaneSwapDir::Down),
        AppCommand::ToggleZoom,
        AppCommand::MirrorLayout,
        AppCommand::RotateBackward,
        AppCommand::RotateForward,
        AppCommand::ClosePane,
        AppCommand::OpenUiLibrary,
        AppCommand::OpenUiLibraryInNewPane,
    ];
    ORDER
}

/// When `command_mode` is true and the body after `:` is empty, lists popular commands in [`palette_cmd_row_order`].
fn palette_cmds_for_query(
    body: &str,
    command_mode: bool,
    input: &VmuxBindingSettings,
) -> Vec<AppCommand> {
    let order = palette_cmd_row_order();
    if command_mode && body.is_empty() {
        return order.iter().cloned().take(MAX_PALETTE_CMD_ROWS).collect();
    }
    if body.is_empty() {
        return Vec::new();
    }
    let b = body.to_ascii_lowercase();
    order
        .iter()
        .cloned()
        .filter(|c| palette_cmd_matches(c, &b, input))
        .take(MAX_PALETTE_CMD_ROWS)
        .collect()
}

/// Top/bottom Y within one scroll pane (`start..end` row indices).
fn selected_row_top_bottom_in_range(
    rows: &[PaletteRowSpec; ROWS_MAX],
    sel: usize,
    range: std::ops::Range<usize>,
) -> (f32, f32) {
    let mut y = 0.0;
    for i in range {
        if !rows[i].visible {
            continue;
        }
        if i == sel {
            return (y, y + PALETTE_LIST_ROW_STRIDE_PX);
        }
        y += PALETTE_LIST_ROW_STRIDE_PX;
    }
    (0.0, 0.0)
}

fn build_palette_rows(
    query: &str,
    history: &NavigationHistory,
    tabs: &[(Entity, String)],
    ui_library_base: Option<&str>,
    input: &VmuxBindingSettings,
) -> [PaletteRowSpec; ROWS_MAX] {
    let body = palette_query_body(query);
    let command_mode = palette_in_command_mode(query);
    let mut rows: Vec<PaletteRowSpec> = Vec::new();

    for slot in 0..MAX_PALETTE_TABS {
        if command_mode {
            rows.push(PaletteRowSpec {
                visible: false,
                icon: ICON_NAV,
                favicon: None,
                primary: String::new(),
                secondary: String::new(),
                enter: ENTER_INERT.to_string(),
                hint_hover_only: false,
                selectable: true,
                action: AppCommand::Noop,
            });
            continue;
        }
        if slot < tabs.len() {
            let (ent, url) = &tabs[slot];
            rows.push(PaletteRowSpec {
                visible: true,
                icon: ICON_NAV,
                favicon: palette_favicon_for_url(history, url),
                primary: palette_tab_title(url),
                secondary: truncate_display(url.as_str(), 52),
                enter: ENTER_TAB.to_string(),
                hint_hover_only: false,
                selectable: true,
                action: AppCommand::FocusPane(*ent),
            });
        } else {
            rows.push(PaletteRowSpec {
                visible: false,
                icon: ICON_NAV,
                favicon: None,
                primary: String::new(),
                secondary: String::new(),
                enter: ENTER_INERT.to_string(),
                hint_hover_only: false,
                selectable: true,
                action: AppCommand::Noop,
            });
        }
    }

    let omnibox_resolved = if body.is_empty() {
        None
    } else {
        resolve_omnibox_target(query, ui_library_base)
    };

    let omni_fav = omnibox_resolved
        .as_deref()
        .and_then(|u| palette_favicon_for_url(history, u));

    let (o0p, o0s, o0a) = if body.is_empty() {
        (
            "Enter a URL or search terms".to_string(),
            String::new(),
            AppCommand::Omnibox { new_pane: false },
        )
    } else if let Some(ref url) = omnibox_resolved {
        (
            truncate_display(url, 54),
            "Open in active pane".to_string(),
            AppCommand::Omnibox { new_pane: false },
        )
    } else {
        (
            String::new(),
            String::new(),
            AppCommand::Omnibox { new_pane: false },
        )
    };
    rows.push(PaletteRowSpec {
        visible: !command_mode,
        icon: ICON_NAV,
        favicon: omni_fav.clone(),
        primary: o0p,
        secondary: o0s,
        enter: ENTER_NAV.to_string(),
        hint_hover_only: true,
        selectable: true,
        action: o0a,
    });

    let show_omnibox_new = !command_mode && omnibox_resolved.is_some();
    let (o0np, o0ns, o0na) = if let Some(ref url) = omnibox_resolved {
        (
            truncate_display(url, 54),
            "Open in new pane".to_string(),
            AppCommand::Omnibox { new_pane: true },
        )
    } else {
        (String::new(), String::new(), AppCommand::Omnibox { new_pane: true })
    };
    rows.push(PaletteRowSpec {
        visible: show_omnibox_new,
        icon: ICON_NEW_PANE,
        favicon: omni_fav,
        primary: o0np,
        secondary: o0ns,
        enter: ENTER_NAV.to_string(),
        hint_hover_only: true,
        selectable: true,
        action: o0na,
    });

    let web_resolved = if body.is_empty() {
        None
    } else {
        web_search_url(query)
    };

    let web_redundant = omnibox_resolved.is_some()
        && web_resolved.as_ref() == omnibox_resolved.as_ref();

    let web_fav = web_resolved
        .as_deref()
        .and_then(|u| palette_favicon_for_url(history, u))
        .or_else(|| favicon_url_for_page_url("https://google.com"));

    let show_web_primary = body.is_empty() || (web_resolved.is_some() && !web_redundant);
    let (w0p, w0s, w0a) = if body.is_empty() {
        (
            "Search the web".to_string(),
            "Uses Google when you pick this row".to_string(),
            AppCommand::WebSearch { new_pane: false },
        )
    } else if let Some(ref url) = web_resolved {
        (
            format!("Search Google for \"{}\"", truncate_display(body, 36)),
            truncate_display(url, 58),
            AppCommand::WebSearch { new_pane: false },
        )
    } else {
        (
            String::new(),
            String::new(),
            AppCommand::WebSearch { new_pane: false },
        )
    };
    rows.push(PaletteRowSpec {
        visible: !command_mode && show_web_primary,
        icon: ICON_SEARCH,
        favicon: web_fav.clone(),
        primary: w0p,
        secondary: w0s,
        enter: ENTER_NAV.to_string(),
        hint_hover_only: true,
        selectable: true,
        action: w0a,
    });

    let show_web_new = !command_mode && web_resolved.is_some() && !web_redundant;
    let (w1p, w1s, w1a) = if web_resolved.is_some() {
        (
            format!("Search Google for \"{}\"", truncate_display(body, 36)),
            "Open in new pane".to_string(),
            AppCommand::WebSearch { new_pane: true },
        )
    } else {
        (String::new(), String::new(), AppCommand::WebSearch { new_pane: true })
    };
    rows.push(PaletteRowSpec {
        visible: show_web_new,
        icon: ICON_NEW_PANE,
        favicon: web_fav,
        primary: w1p,
        secondary: w1s,
        enter: ENTER_NAV.to_string(),
        hint_hover_only: true,
        selectable: true,
        action: w1a,
    });

    let gh_rows: Vec<_> = if command_mode {
        Vec::new()
    } else if let Some(owner) = github_owner_only(query) {
        github_suggestion_rows(&owner)
            .into_iter()
            .take(MAX_GITHUB_REPO_SUGGESTIONS)
            .flat_map(|(p, s, u)| {
                let fav = palette_favicon_for_url(history, &u);
                let u_clone = u.clone();
                [
                    PaletteRowSpec {
                        visible: true,
                        icon: ICON_GH,
                        favicon: fav.clone(),
                        primary: p.clone(),
                        secondary: s,
                        enter: ENTER_NAV.to_string(),
                        hint_hover_only: true,
                        selectable: true,
                        action: AppCommand::OpenUrl {
                            url: u_clone,
                            new_pane: false,
                        },
                    },
                    PaletteRowSpec {
                        visible: true,
                        icon: ICON_NEW_PANE,
                        favicon: fav,
                        primary: p,
                        secondary: "Open in new pane".to_string(),
                        enter: ENTER_NAV.to_string(),
                        hint_hover_only: true,
                        selectable: true,
                        action: AppCommand::OpenUrl { url: u, new_pane: true },
                    },
                ]
            })
            .collect()
    } else {
        history_suggestion_urls(body, history, omnibox_resolved.as_deref())
            .into_iter()
            .flat_map(|url| {
                let fav = palette_favicon_for_url(history, &url);
                let u2 = url.clone();
                [
                    PaletteRowSpec {
                        visible: true,
                        icon: ICON_HISTORY,
                        favicon: fav.clone(),
                        primary: truncate_display(&url, 54),
                        secondary: "From history · active pane".to_string(),
                        enter: ENTER_NAV.to_string(),
                        hint_hover_only: true,
                        selectable: true,
                        action: AppCommand::OpenUrl {
                            url,
                            new_pane: false,
                        },
                    },
                    PaletteRowSpec {
                        visible: true,
                        icon: ICON_NEW_PANE,
                        favicon: fav,
                        primary: truncate_display(&u2, 54),
                        secondary: "From history - new pane".to_string(),
                        enter: ENTER_NAV.to_string(),
                        hint_hover_only: true,
                        selectable: true,
                        action: AppCommand::OpenUrl { url: u2, new_pane: true },
                    },
                ]
            })
            .collect()
    };

    for g in gh_rows {
        rows.push(g);
    }
    while rows.len() < IDX_CMD_START {
        rows.push(PaletteRowSpec {
            visible: false,
            icon: ICON_GH,
            favicon: None,
            primary: String::new(),
            secondary: String::new(),
            enter: ENTER_INERT.to_string(),
            hint_hover_only: false,
            selectable: true,
            action: AppCommand::Noop,
        });
    }

    let palette_cmds = palette_cmds_for_query(body, command_mode, input);
    if command_mode && !body.is_empty() && palette_cmds.is_empty() {
        rows.push(PaletteRowSpec {
            visible: true,
            icon: ICON_CMD,
            favicon: None,
            primary: "No matching commands".to_string(),
            secondary: "Keep typing to refine, or delete text to browse the full list.".to_string(),
            enter: ENTER_INERT.to_string(),
            hint_hover_only: false,
            selectable: false,
            action: AppCommand::Noop,
        });
    }
    for cmd in palette_cmds {
        let (title, sub) = palette_cmd_title_subtitle(&cmd, input);
        rows.push(PaletteRowSpec {
            visible: true,
            icon: ICON_CMD,
            favicon: None,
            primary: title,
            secondary: String::new(),
            enter: if sub.is_empty() {
                ENTER_NAV.to_string()
            } else {
                sub
            },
            hint_hover_only: false,
            selectable: true,
            action: cmd,
        });
    }
    while rows.len() < ROWS_MAX {
        rows.push(PaletteRowSpec {
            visible: false,
            icon: ICON_CMD,
            favicon: None,
            primary: String::new(),
            secondary: String::new(),
            enter: ENTER_INERT.to_string(),
            hint_hover_only: false,
            selectable: true,
            action: AppCommand::Noop,
        });
    }

    debug_assert_eq!(
        rows.len(),
        ROWS_MAX,
        "palette row builder must produce ROWS_MAX rows"
    );
    rows.try_into().unwrap_or_else(|v: Vec<_>| {
        panic!("expected {ROWS_MAX} palette rows, got {}", v.len());
    })
}

/// Secondary column on one line after primary (ASCII separator — Unicode `·`/`…` are not in the default UI font).
fn palette_secondary_display(secondary: &str) -> String {
    if secondary.is_empty() {
        String::new()
    } else {
        format!(" - {}", secondary)
    }
}

/// Map linear selection (0..visible-1) to row index — unused; we store index into ROWS_MAX directly.
fn row_index_from_visible_selection(rows: &[PaletteRowSpec; ROWS_MAX], visible_sel: usize) -> usize {
    let mut v = 0;
    for (i, r) in rows.iter().enumerate() {
        if !r.visible {
            continue;
        }
        if v == visible_sel {
            return i;
        }
        v += 1;
    }
    0
}

fn first_visible_selectable_row(rows: &[PaletteRowSpec; ROWS_MAX]) -> Option<usize> {
    for i in 0..ROWS_MAX {
        if rows[i].visible && rows[i].selectable {
            return Some(i);
        }
    }
    None
}

fn row_index_from_visible_selectable_selection(
    rows: &[PaletteRowSpec; ROWS_MAX],
    visible_sel: usize,
) -> usize {
    let mut v = 0;
    for (i, r) in rows.iter().enumerate() {
        if !r.visible || !r.selectable {
            continue;
        }
        if v == visible_sel {
            return i;
        }
        v += 1;
    }
    row_index_from_visible_selection(rows, 0)
}

fn visible_index_of_selectable_row(rows: &[PaletteRowSpec; ROWS_MAX], row_idx: usize) -> Option<usize> {
    let mut v = 0;
    for (i, r) in rows.iter().enumerate() {
        if !r.visible || !r.selectable {
            continue;
        }
        if i == row_idx {
            return Some(v);
        }
        v += 1;
    }
    None
}

fn visible_selectable_row_count(rows: &[PaletteRowSpec; ROWS_MAX]) -> usize {
    rows.iter().filter(|r| r.visible && r.selectable).count()
}

fn select_default_query_row(palette: &mut VmuxCommandPaletteState, rows: &[PaletteRowSpec; ROWS_MAX]) {
    palette.pointer_row_selects = false;
    // Query-first UX: when the user edits text, Enter should trigger navigation/search, not tab focus.
    if palette_in_command_mode(&palette.input.query) {
        if rows.get(IDX_CMD_START).is_some_and(|r| r.visible && r.selectable) {
            palette.selection = IDX_CMD_START;
            return;
        }
        if rows.get(IDX_CMD_START).is_some_and(|r| r.visible && !r.selectable) {
            if let Some(i) = first_visible_selectable_row(rows) {
                palette.selection = i;
                return;
            }
            palette.selection = IDX_CMD_START;
            return;
        }
    } else {
        let preferred = MAX_PALETTE_TABS;
        if rows.get(preferred).is_some_and(|r| r.visible) {
            palette.selection = preferred;
            return;
        }
    }
    palette.selection = row_index_from_visible_selection(rows, 0);
}

fn step_palette_visible_selection(
    palette: &mut VmuxCommandPaletteState,
    rows: &[PaletteRowSpec; ROWS_MAX],
    vis_count: usize,
    previous: bool,
) {
    if vis_count == 0 {
        return;
    }
    let cur_v = visible_index_of_selectable_row(rows, palette.selection).unwrap_or(0);
    let next_v = if previous {
        cur_v.saturating_sub(1)
    } else {
        (cur_v + 1).min(vis_count.saturating_sub(1))
    };
    palette.selection = row_index_from_visible_selectable_selection(rows, next_v);
}

macro_rules! spawn_command_palette_row {
    ($list:expr, $i:expr, $row:expr) => {{
        let i = $i;
        let row: &PaletteRowSpec = $row;
        let row_vis = if row.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        $list
            .spawn((
                CommandPaletteRow(i as u8),
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(10.0),
                    padding: UiRect::axes(px(10.0), px(8.0)),
                    border_radius: BorderRadius::all(px(10.0)),
                    min_height: px(36.0),
                    display: if row_vis == Visibility::Visible {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    ..default()
                },
                BackgroundColor(ROW_BG),
                row_vis,
            ))
            .observe({
                let row_idx = i as usize;
                move |mut press: On<Pointer<Press>>, mut palette: ResMut<VmuxCommandPaletteState>| {
                    if !palette.open || press.event().button != PointerButton::Primary {
                        return;
                    }
                    if row_idx >= ROWS_MAX || !palette.row_selectable_mask[row_idx] {
                        return;
                    }
                    palette.selection = row_idx;
                    palette.pointer_row_selects = true;
                    palette.pending_pointer_submit = true;
                    press.propagate(false);
                }
            })
            .with_children(|r| {
                r.spawn((
                    Node {
                        width: px(22.0),
                        height: px(22.0),
                        flex_shrink: 0.0,
                        position_type: PositionType::Relative,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                ))
                .with_children(|slot| {
                    slot.spawn((
                        PaletteRowIcon(i as u8),
                        Text::new(row.icon),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(ROW_TEXT),
                    ));
                    slot.spawn((
                        PaletteRowFavicon(i as u8),
                        ImageNode {
                            image_mode: NodeImageMode::Stretch,
                            ..default()
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            width: px(18.0),
                            height: px(18.0),
                            left: px(2.0),
                            top: px(2.0),
                            ..default()
                        },
                        Visibility::Hidden,
                    ));
                });
                r.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(8.0),
                        flex_grow: 1.0,
                        flex_shrink: 1.0,
                        min_width: px(0.0),
                        overflow: Overflow::clip_x(),
                        ..default()
                    },
                ))
                .with_children(|text_row| {
                    text_row.spawn((
                        PaletteRowPrimary(i as u8),
                        Text::new(row.primary.as_str()),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(ROW_TEXT),
                        TextLayout::new_with_no_wrap(),
                        Node {
                            flex_shrink: 1.0,
                            min_width: px(0.0),
                            ..default()
                        },
                    ));
                    text_row.spawn((
                        PaletteRowSecondary(i as u8),
                        Text::new(palette_secondary_display(row.secondary.as_str())),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(ROW_SUBTEXT),
                        TextLayout::new_with_no_wrap(),
                        Node {
                            flex_shrink: 1.0,
                            min_width: px(0.0),
                            ..default()
                        },
                    ));
                });
                r.spawn((
                    Node {
                        flex_shrink: 0.0,
                        margin: UiRect::left(px(4.0)),
                        ..default()
                    },
                ))
                .with_children(|hint| {
                    if row.hint_hover_only {
                        hint.spawn((
                            PaletteRowEnterHint(i as u8),
                            PaletteNavEnterHint,
                            Visibility::Hidden,
                            Text::new(row.enter.as_str()),
                            TextFont {
                                font_size: 12.5,
                                ..default()
                            },
                            TextColor(Color::srgba(0.5, 0.51, 0.55, 1.0)),
                            TextLayout::new_with_no_wrap(),
                        ));
                    } else {
                        hint.spawn((
                            PaletteRowEnterHint(i as u8),
                            Text::new(row.enter.as_str()),
                            TextFont {
                                font_size: 12.5,
                                ..default()
                            },
                            TextColor(Color::srgba(0.5, 0.51, 0.55, 1.0)),
                            TextLayout::new_with_no_wrap(),
                        ));
                    }
                });
            });
    }};
}

/// Spawns the palette UI camera and root. Run after the main [`vmux_core::VmuxWorldCamera`] exists.
pub fn setup(mut commands: Commands, hist: Res<NavigationHistory>, settings: Res<VmuxAppSettings>) {
    let camera = commands
        .spawn((
            CommandPaletteUiCamera,
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                is_active: false,
                ..default()
            },
            IsDefaultUiCamera,
        ))
        .id();

    let initial = build_palette_rows("", &hist, &[], None, &settings.input);

    commands
        .spawn((
            CommandPaletteRoot,
            UiTargetCamera(camera),
            Node {
                width: percent(100.0),
                height: percent(100.0),
                position_type: PositionType::Relative,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(1),
        ))
        .with_children(|root| {
            root.spawn((
                CommandPaletteBackdrop,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    right: px(0.0),
                    top: px(0.0),
                    bottom: px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.42)),
            ))
            .observe(
                |_: On<Pointer<Press>>, mut palette: ResMut<VmuxCommandPaletteState>| {
                    palette.open = false;
                },
            );

            root.spawn((
                Node {
                    width: percent(92.0),
                    max_width: px(920.0),
                    min_width: px(300.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(12.0)),
                    row_gap: px(5.0),
                    border_radius: BorderRadius::all(px(16.0)),
                    border: UiRect::all(px(1.0)),
                    ..default()
                },
                GlobalZIndex(2),
                BackgroundColor(PANEL_BG),
                BorderColor::all(BORDER_SUBTLE),
                BoxShadow::new(
                    Color::srgba(0.0, 0.0, 0.0, 0.55),
                    px(0.0),
                    px(20.0),
                    px(0.0),
                    px(32.0),
                ),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: px(10.0),
                            padding: UiRect::axes(px(10.0), px(9.0)),
                            border_radius: BorderRadius::all(px(12.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.28)),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                flex_grow: 1.0,
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                min_width: px(0.0),
                                position_type: PositionType::Relative,
                                ..default()
                            },
                        ))
                        .with_children(|query| {
                            query.spawn((
                                CommandPaletteQuerySelectionHighlight,
                                Node {
                                    position_type: PositionType::Absolute,
                                    height: px(16.0),
                                    width: px(0.0),
                                    left: px(0.0),
                                    top: percent(50.0),
                                    margin: UiRect::top(Val::Px(-8.0)),
                                    border_radius: BorderRadius::all(px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(QUERY_SELECTION_HIGHLIGHT),
                                Visibility::Hidden,
                            ));
                            query.spawn((
                                CommandPaletteQueryText,
                                Text::new(""),
                                TextFont {
                                    font_size: 16.0,
                                    ..default()
                                },
                                TextColor(ROW_TEXT),
                                TextLayout::new_with_no_wrap(),
                                Node {
                                    flex_grow: 1.0,
                                    flex_shrink: 1.0,
                                    min_width: px(0.0),
                                    ..default()
                                },
                            ));
                            query.spawn((
                                CommandPaletteQueryPlaceholder,
                                Text::new("Search or enter URL..."),
                                TextFont {
                                    font_size: 16.0,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.62, 0.63, 0.68, 1.0)),
                            ));
                            query.spawn((
                                CommandPaletteCaret,
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: px(2.0),
                                    height: px(16.0),
                                    left: px(0.0),
                                    top: percent(50.0),
                                    margin: UiRect::top(Val::Px(-8.0)),
                                    ..default()
                                },
                                BackgroundColor(ROW_TEXT),
                                Visibility::Hidden,
                            ));
                        });
                    });

                panel
                    .spawn((
                        CommandPaletteListScroll,
                        ScrollPosition::default(),
                        Node {
                            width: percent(100.0),
                            flex_grow: 1.0,
                            flex_shrink: 1.0,
                            min_height: px(0.0),
                            max_height: px(PALETTE_LIST_COMBINED_MAX_HEIGHT_PX),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(5.0),
                            overflow: Overflow::scroll_y(),
                            border_radius: BorderRadius::all(px(10.0)),
                            padding: UiRect::all(px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.2)),
                    ))
                    .with_children(|list| {
                        for i in 0..ROWS_MAX {
                            spawn_command_palette_row!(list, i, &initial[i]);
                        }
                    });
            });
        });
}

/// Size the palette [`Camera`] viewport to the full workspace (entire window), not the active pane.
fn sync_command_palette_camera_viewport(
    palette: Res<VmuxCommandPaletteState>,
    window: Query<&Window, With<PrimaryWindow>>,
    world_cam: Query<&Camera, (With<VmuxWorldCamera>, Without<CommandPaletteUiCamera>)>,
    mut palette_cam: Query<&mut Camera, (With<CommandPaletteUiCamera>, Without<VmuxWorldCamera>)>,
) {
    let Ok(mut cam) = palette_cam.single_mut() else {
        return;
    };
    if !palette.open {
        cam.viewport = None;
        cam.is_active = false;
        return;
    }
    cam.is_active = true;
    let Ok(window) = window.single() else {
        cam.viewport = None;
        return;
    };
    let Ok(world_cam) = world_cam.single() else {
        return;
    };
    let Some((vw, vh)) = layout_viewport_for_workspace(window, world_cam) else {
        cam.viewport = None;
        return;
    };

    let scale = window.scale_factor();
    let phys_x = 0u32;
    let phys_y = 0u32;
    let phys_w = ((vw * scale).round() as u32).max(1);
    let phys_h = ((vh * scale).round() as u32).max(1);

    cam.viewport = Some(Viewport {
        physical_position: UVec2::new(phys_x, phys_y),
        physical_size: UVec2::new(phys_w, phys_h),
        depth: 0.0..1.0,
    });
}

fn clamp_palette_list_scroll_to_selection(
    palette: Res<VmuxCommandPaletteState>,
    hist: Res<NavigationHistory>,
    layout_q: Query<&Layout, With<vmux_layout::Window>>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    main_webviews: Query<Entity, With<Webview>>,
    settings: Res<VmuxAppSettings>,
    mut list_scroll: Query<(&mut ScrollPosition, &ComputedNode), With<CommandPaletteListScroll>>,
    mut last_clamp_sig: Local<Option<(String, usize, u64, Option<String>)>>,
) {
    if !palette.open {
        *last_clamp_sig = None;
        return;
    }
    let sig = (
        palette.input.query.clone(),
        palette.selection,
        hist.revision,
        palette.ui_library_base.clone(),
    );
    if last_clamp_sig.as_ref() == Some(&sig) {
        return;
    }
    let default_url = settings.browser.default_webview_url.as_str();
    let tabs = if let Ok(tree) = layout_q.single() {
        collect_command_palette_tabs(
            &*tree,
            &pane_last,
            &webview_src,
            &history_panes,
            &main_webviews,
            default_url,
        )
    } else {
        Vec::new()
    };
    let rows = build_palette_rows(
        &palette.input.query,
        &hist,
        &tabs,
        palette.ui_library_base.as_deref(),
        &settings.input,
    );
    let sel = palette.selection;
    if sel >= ROWS_MAX || !rows[sel].visible {
        return;
    }
    *last_clamp_sig = Some(sig);
    let apply_scroll = |sp: &mut ScrollPosition, computed: &ComputedNode, y_top: f32, y_bot: f32| {
        // `ScrollPosition` is in logical pixels; `ComputedNode` sizes are physical (see Bevy `ui_node`).
        // Match `on_command_palette_scroll` so scroll-into-view works at non-1.0 scale factors.
        let inv = computed.inverse_scale_factor();
        let view_h = computed.size.y * inv;
        if view_h <= 0.0 {
            return;
        }
        let max_scroll = (computed.content_size.y - computed.size.y).max(0.0) * inv;
        let mut y = sp.0.y;
        if y_top < y {
            y = y_top;
        } else if y_bot > y + view_h {
            y = y_bot - view_h;
        }
        y = y.clamp(0.0, max_scroll);
        sp.0.y = y;
    };
    let (y_top, y_bot) = selected_row_top_bottom_in_range(&rows, sel, 0..ROWS_MAX);
    if let Ok((mut sp, computed)) = list_scroll.single_mut() {
        apply_scroll(&mut sp, &computed, y_top, y_bot);
    }
}

fn send_command_palette_scroll_events(
    palette: Res<VmuxCommandPaletteState>,
    mut mouse_wheel_reader: MessageReader<MouseWheel>,
    hover_map: Res<HoverMap>,
    mut commands: Commands,
) {
    if !palette.open {
        return;
    }
    for mouse_wheel in mouse_wheel_reader.read() {
        let mut delta = -Vec2::new(mouse_wheel.x, mouse_wheel.y);
        if mouse_wheel.unit == MouseScrollUnit::Line {
            delta.x *= PALETTE_SCROLL_LINE_HEIGHT_X_PX;
            // Match one list row per notch (~same step as ↑/↓ moving selection & scroll-into-view).
            delta.y *= PALETTE_LIST_ROW_STRIDE_PX;
        }
        for pointer_entities in hover_map.values() {
            let Some((&entity, _)) = pointer_entities
                .iter()
                .min_by(|(_, ha), (_, hb)| ha.depth.total_cmp(&hb.depth))
            else {
                continue;
            };
            commands.trigger(CommandPaletteScroll { entity, delta });
        }
    }
}

fn on_command_palette_scroll(
    mut scroll: On<CommandPaletteScroll>,
    mut query: Query<(&mut ScrollPosition, &Node, &ComputedNode)>,
) {
    let Ok((mut scroll_position, node, computed)) = query.get_mut(scroll.entity) else {
        return;
    };

    let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
    let delta = &mut scroll.delta;
    if node.overflow.x == OverflowAxis::Scroll && delta.x != 0. {
        let max = if delta.x > 0. {
            scroll_position.x >= max_offset.x
        } else {
            scroll_position.x <= 0.
        };
        if !max {
            scroll_position.x += delta.x;
            delta.x = 0.;
        }
    }
    if node.overflow.y == OverflowAxis::Scroll && delta.y != 0. {
        let max = if delta.y > 0. {
            scroll_position.y >= max_offset.y
        } else {
            scroll_position.y <= 0.
        };
        if !max {
            scroll_position.y += delta.y;
            delta.y = 0.;
        }
    }
    if *delta == Vec2::ZERO {
        scroll.propagate(false);
    }
}

fn sync_ui_library_url_into_palette(
    src: Res<VmuxUiLibraryBaseUrl>,
    mut dst: ResMut<VmuxCommandPaletteState>,
) {
    if dst.ui_library_base != src.0 {
        dst.ui_library_base = src.0.clone();
    }
}

fn sync_visibility(
    palette: Res<VmuxCommandPaletteState>,
    mut q: Query<&mut Visibility, With<CommandPaletteRoot>>,
) {
    let Ok(mut vis) = q.single_mut() else {
        return;
    };
    let next = if palette.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if *vis != next {
        *vis = next;
    }
}

fn toggle_hotkey(
    state: Query<&ActionState<KeyAction>, With<AppInputRoot>>,
    time: Res<Time>,
    mut palette: ResMut<VmuxCommandPaletteState>,
    mut list_scroll: Query<&mut ScrollPosition, With<CommandPaletteListScroll>>,
) {
    let Ok(s) = state.single() else {
        return;
    };
    if s.just_pressed(&KeyAction::ToggleCommandPalette) {
        palette.open = !palette.open;
        if palette.open {
            palette.pointer_row_selects = true;
            palette.pending_pointer_submit = false;
            palette.input.query.clear();
            palette.input.caret = 0;
            palette.input.selection_anchor = None;
            palette.selection = 0;
            palette.input.caret_blink_t0 = time.elapsed_secs();
            if let Ok(mut sp) = list_scroll.single_mut() {
                sp.0 = Vec2::ZERO;
            }
        }
    }
}

/// Opens the palette with the active pane’s URL in the field (⌘L / Ctrl+L behavior).
fn focus_command_palette_on_active_pane_url(
    palette: &mut VmuxCommandPaletteState,
    time: &Time,
    active: &Query<Entity, (With<Pane>, With<Active>)>,
    pane_last: &Query<&PaneLastUrl>,
    list_scroll: &mut Query<&mut ScrollPosition, With<CommandPaletteListScroll>>,
) {
    let Ok(ent) = active.single() else {
        return;
    };
    let url = pane_last
        .get(ent)
        .map(|p| p.0.clone())
        .unwrap_or_default();
    palette.open = true;
    palette.pointer_row_selects = true;
    palette.pending_pointer_submit = false;
    palette.input.query = url;
    let end = query_len_chars(&palette.input.query);
    palette.input.caret = end;
    palette.input.selection_anchor = Some(0);
    palette.selection = MAX_PALETTE_TABS;
    palette.input.caret_blink_t0 = time.elapsed_secs();
    if let Ok(mut sp) = list_scroll.single_mut() {
        sp.0 = Vec2::ZERO;
    }
}

fn focus_url_hotkey(
    state: Query<&ActionState<KeyAction>, With<AppInputRoot>>,
    time: Res<Time>,
    mut palette: ResMut<VmuxCommandPaletteState>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    pane_last: Query<&PaneLastUrl>,
    mut list_scroll: Query<&mut ScrollPosition, With<CommandPaletteListScroll>>,
) {
    let Ok(s) = state.single() else {
        return;
    };
    if !s.just_pressed(&KeyAction::FocusCommandPaletteUrl) {
        return;
    }
    focus_command_palette_on_active_pane_url(
        &mut palette,
        &time,
        &active,
        &pane_last,
        &mut list_scroll,
    );
}

fn apply_focus_command_palette_url_request(
    mut requests: ResMut<AppCommandRequestQueue>,
    time: Res<Time>,
    mut palette: ResMut<VmuxCommandPaletteState>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    pane_last: Query<&PaneLastUrl>,
    mut list_scroll: Query<&mut ScrollPosition, With<CommandPaletteListScroll>>,
) {
    if !requests.focus_command_palette_url_requested {
        return;
    }
    requests.focus_command_palette_url_requested = false;
    focus_command_palette_on_active_pane_url(
        &mut palette,
        &time,
        &active,
        &pane_last,
        &mut list_scroll,
    );
}

fn sync_command_palette_caret(
    palette: Res<VmuxCommandPaletteState>,
    time: Res<Time>,
    mut q: Query<&mut Visibility, With<CommandPaletteCaret>>,
) {
    let Ok(mut vis) = q.single_mut() else {
        return;
    };
    if !palette.open {
        *vis = Visibility::Hidden;
        return;
    }
    if normalized_selection(palette.input.selection_anchor, palette.input.caret).is_some() {
        *vis = Visibility::Hidden;
        return;
    }
    let t = time.elapsed_secs() - palette.input.caret_blink_t0;
    let show = ((t / PALETTE_CARET_PHASE_SECS) as u32) % 2 == 0;
    *vis = if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

/// Caret position + selection highlight after [`widget::text_system`] so [`TextLayoutInfo`] matches the query [`Text`].
fn position_command_palette_query_layout(
    palette: Res<VmuxCommandPaletteState>,
    mut caret_q: Query<
        &mut Node,
        (With<CommandPaletteCaret>, Without<CommandPaletteQuerySelectionHighlight>),
    >,
    mut sel_q: Query<
        (&mut Node, &mut Visibility),
        (With<CommandPaletteQuerySelectionHighlight>, Without<CommandPaletteCaret>),
    >,
    text_q: Query<(&TextLayoutInfo, &ComputedNode), With<CommandPaletteQueryText>>,
) {
    if !palette.open {
        return;
    }
    let Ok((layout, computed)) = text_q.single() else {
        return;
    };
    let inv = computed.inverse_scale_factor();
    let query = palette.input.query.as_str();
    let qbytes = query.len();
    let caret_b = query_char_to_byte(query, palette.input.caret);
    let x_caret = caret_x_from_text_layout(layout, caret_b, qbytes) * inv;
    if let Ok(mut caret_node) = caret_q.single_mut() {
        caret_node.left = Val::Px(x_caret);
    }
    let range = normalized_selection(palette.input.selection_anchor, palette.input.caret)
        .and_then(|(a, b)| selection_highlight_range(layout, query, a, b));
    if let Ok((mut sel_node, mut sel_vis)) = sel_q.single_mut() {
        if let Some((left, width)) = range {
            sel_node.left = Val::Px(left * inv);
            sel_node.width = Val::Px(width * inv);
            *sel_vis = Visibility::Visible;
        } else {
            sel_node.width = Val::Px(0.0);
            *sel_vis = Visibility::Hidden;
        }
    }
}

fn handle_keyboard(
    mut palette: ResMut<VmuxCommandPaletteState>,
    mut reader: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut list_scroll: Query<&mut ScrollPosition, With<CommandPaletteListScroll>>,
    hist: Res<NavigationHistory>,
    layout_q: Query<&Layout, With<vmux_layout::Window>>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    main_webviews: Query<Entity, With<Webview>>,
    settings: Res<VmuxAppSettings>,
) {
    if !palette.open {
        for ev in reader.read() {
            if !ev.state.is_pressed() {
                continue;
            }
            if ev.text.as_deref() != Some(":") {
                continue;
            }
            if super_or_ctrl_held(&keys) {
                continue;
            }
            palette.open = true;
            palette.pointer_row_selects = false;
            palette.input.query.clear();
            palette.input.query.push(':');
            palette.input.caret = 1;
            palette.input.selection_anchor = None;
            palette.selection = IDX_CMD_START;
            palette.input.caret_blink_t0 = time.elapsed_secs();
            if let Ok(mut sp) = list_scroll.single_mut() {
                sp.0 = Vec2::ZERO;
            }
        }
        return;
    }

    let default_url = settings.browser.default_webview_url.as_str();
    let tabs = if let Ok(tree) = layout_q.single() {
        collect_command_palette_tabs(
            &*tree,
            &pane_last,
            &webview_src,
            &history_panes,
            &main_webviews,
            default_url,
        )
    } else {
        Vec::new()
    };
    let rows = build_palette_rows(
        &palette.input.query,
        &hist,
        &tabs,
        palette.ui_library_base.as_deref(),
        &settings.input,
    );
    let vis_count = visible_selectable_row_count(&rows);

    for ev in reader.read() {
        if !ev.state.is_pressed() {
            continue;
        }

        if ev.key_code == KeyCode::Escape {
            palette.open = false;
            return;
        }

        let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let shortcut_mod_held = super_or_ctrl_held(&keys);
        let ctrl_held =
            keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        let query_len = query_len_chars(&palette.input.query);

        // Arrow / Ctrl+N / Ctrl+P: driven by KeyboardInput (includes `repeat: true`) so OS hold-to-repeat works.
        match ev.key_code {
            KeyCode::ArrowUp => {
                palette.pointer_row_selects = false;
                palette.input.selection_anchor = None;
                step_palette_visible_selection(&mut palette, &rows, vis_count, true);
            }
            KeyCode::ArrowDown => {
                palette.pointer_row_selects = false;
                palette.input.selection_anchor = None;
                step_palette_visible_selection(&mut palette, &rows, vis_count, false);
            }
            KeyCode::KeyP if ctrl_held => {
                palette.pointer_row_selects = false;
                palette.input.selection_anchor = None;
                step_palette_visible_selection(&mut palette, &rows, vis_count, true);
            }
            KeyCode::KeyN if ctrl_held => {
                palette.pointer_row_selects = false;
                palette.input.selection_anchor = None;
                step_palette_visible_selection(&mut palette, &rows, vis_count, false);
            }
            KeyCode::ArrowLeft => {
                if shift_held {
                    if palette.input.selection_anchor.is_none() {
                        palette.input.selection_anchor = Some(palette.input.caret);
                    }
                    let next = palette.input.caret.saturating_sub(1);
                    set_query_caret(&mut palette, next);
                } else if let Some((start, _)) =
                    normalized_selection(palette.input.selection_anchor, palette.input.caret)
                {
                    palette.input.caret = start;
                    palette.input.selection_anchor = None;
                } else {
                    let next = palette.input.caret.saturating_sub(1);
                    set_query_caret(&mut palette, next);
                    palette.input.selection_anchor = None;
                }
            }
            KeyCode::ArrowRight => {
                if shift_held {
                    if palette.input.selection_anchor.is_none() {
                        palette.input.selection_anchor = Some(palette.input.caret);
                    }
                    let next = (palette.input.caret + 1).min(query_len);
                    set_query_caret(&mut palette, next);
                } else if let Some((_, end)) =
                    normalized_selection(palette.input.selection_anchor, palette.input.caret)
                {
                    palette.input.caret = end;
                    palette.input.selection_anchor = None;
                } else {
                    let next = (palette.input.caret + 1).min(query_len);
                    set_query_caret(&mut palette, next);
                    palette.input.selection_anchor = None;
                }
            }
            KeyCode::Home => {
                if shift_held {
                    if palette.input.selection_anchor.is_none() {
                        palette.input.selection_anchor = Some(palette.input.caret);
                    }
                } else {
                    palette.input.selection_anchor = None;
                }
                palette.input.caret = 0;
            }
            KeyCode::End => {
                if shift_held {
                    if palette.input.selection_anchor.is_none() {
                        palette.input.selection_anchor = Some(palette.input.caret);
                    }
                } else {
                    palette.input.selection_anchor = None;
                }
                palette.input.caret = query_len;
            }
            _ => {
                if shortcut_mod_held && palette_select_all_key(ev) {
                    palette.input.selection_anchor = Some(0);
                    palette.input.caret = query_len;
                    continue;
                }
                if matches!(&ev.logical_key, Key::Backspace) {
                    let deleted_selection = delete_query_selection(&mut palette);
                    if deleted_selection || palette.input.caret > 0 {
                        if !deleted_selection {
                        let prev = palette.input.caret - 1;
                        let bs = query_char_to_byte(&palette.input.query, prev);
                        let be = query_char_to_byte(&palette.input.query, palette.input.caret);
                        palette.input.query.replace_range(bs..be, "");
                        palette.input.caret = prev;
                        }
                        let updated_rows = build_palette_rows(
                            &palette.input.query,
                            &hist,
                            &tabs,
                            palette.ui_library_base.as_deref(),
                            &settings.input,
                        );
                        select_default_query_row(&mut palette, &updated_rows);
                    }
                    palette.input.selection_anchor = None;
                    continue;
                }
                if matches!(&ev.logical_key, Key::Delete) {
                    let deleted_selection = delete_query_selection(&mut palette);
                    if deleted_selection || palette.input.caret < query_len {
                        if !deleted_selection {
                            let bs = query_char_to_byte(&palette.input.query, palette.input.caret);
                            let be = query_char_to_byte(&palette.input.query, palette.input.caret + 1);
                            palette.input.query.replace_range(bs..be, "");
                        }
                        let updated_rows = build_palette_rows(
                            &palette.input.query,
                            &hist,
                            &tabs,
                            palette.ui_library_base.as_deref(),
                            &settings.input,
                        );
                        select_default_query_row(&mut palette, &updated_rows);
                    }
                    palette.input.selection_anchor = None;
                    continue;
                }
                if shortcut_mod_held {
                    continue;
                }
                if ctrl_held {
                    continue;
                }

                match (&ev.logical_key, &ev.text) {
                    (_, Some(t)) if !t.is_empty() => {
                        delete_query_selection(&mut palette);
                        let mut query_edited = false;
                        for ch in t.chars() {
                            if is_printable_char(ch) {
                                let b = query_char_to_byte(&palette.input.query, palette.input.caret);
                                palette.input.query.insert(b, ch);
                                palette.input.caret += 1;
                                query_edited = true;
                            }
                        }
                        if query_edited {
                            let updated_rows = build_palette_rows(
                                &palette.input.query,
                                &hist,
                                &tabs,
                                palette.ui_library_base.as_deref(),
                                &settings.input,
                            );
                            select_default_query_row(&mut palette, &updated_rows);
                        }
                        palette.input.selection_anchor = None;
                    }
                    _ => {}
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
const PENDING_UI_LIBRARY_NAV_TIMEOUT_FRAMES: u32 = 300;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_pending_ui_library_navigation(
    mut pending: ResMut<VmuxPendingUiLibraryNavigation>,
    ui_library_res: Res<VmuxUiLibraryBaseUrl>,
    palette: Res<VmuxCommandPaletteState>,
    mut commands: Commands,
    (
        active,
        mut layout_q,
        mut meshes,
        mut materials,
        mut loading_bar_materials,
        mut snapshot,
        pane_last,
        webview_src,
        history_panes,
    ): (
        Query<Entity, (With<Pane>, With<Active>, With<Webview>, Without<History>)>,
        Query<&mut Layout, With<vmux_layout::Window>>,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<WebviewExtendStandardMaterial>>,
        ResMut<Assets<LoadingBarMaterial>>,
        ResMut<SessionLayoutSnapshot>,
        Query<&PaneLastUrl>,
        Query<&WebviewSource>,
        Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    ),
    (path, mut session_queue, settings): (
        Option<Res<SessionSavePath>>,
        ResMut<SessionSaveQueue>,
        Res<VmuxAppSettings>,
    ),
) {
    if pending.inner.is_none() {
        return;
    }
    let Some(url) = ui_library_nav_url(ui_library_stored_base(&ui_library_res, &palette)) else {
        pending.wait_frames = pending.wait_frames.saturating_add(1);
        if pending.wait_frames >= PENDING_UI_LIBRARY_NAV_TIMEOUT_FRAMES {
            bevy::log::warn!(
                "vmux: UI library base URL not available after waiting; run `cargo build -p vmux_ui` to build dist/, or set VMUX_UI_LIBRARY_URL."
            );
            pending.inner = None;
            pending.wait_frames = 0;
        }
        return;
    };
    let Some(target) = pending.inner.take() else {
        return;
    };
    pending.wait_frames = 0;
    let default_url = settings.browser.default_webview_url.as_str();
    navigate_palette_url(
        &mut commands,
        url,
        target.new_pane,
        &active,
        &mut layout_q,
        &mut meshes,
        &mut materials,
        &mut loading_bar_materials,
        &mut snapshot,
        &pane_last,
        &webview_src,
        &history_panes,
        path.as_ref(),
        &mut session_queue,
        default_url,
    );
}

#[allow(clippy::too_many_arguments)]
fn navigate_palette_url(
    commands: &mut Commands,
    url: String,
    new_pane: bool,
    active: &Query<Entity, (With<Pane>, With<Active>, With<Webview>, Without<History>)>,
    layout_q: &mut Query<&mut Layout, With<vmux_layout::Window>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    loading_bar_materials: &mut ResMut<Assets<LoadingBarMaterial>>,
    snapshot: &mut SessionLayoutSnapshot,
    pane_last: &Query<&PaneLastUrl>,
    webview_src: &Query<&WebviewSource>,
    history_panes: &Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    path: Option<&Res<SessionSavePath>>,
    session_queue: &mut SessionSaveQueue,
    default_webview_url: &str,
) {
    if !new_pane {
        if let Ok(ent) = active.single() {
            commands.trigger(RequestNavigate { webview: ent, url });
        }
        return;
    }
    let Ok(before) = active.single() else {
        return;
    };
    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };
    try_split_active_pane(
        commands,
        &mut *tree,
        before,
        LayoutAxis::Horizontal,
        meshes,
        materials,
        loading_bar_materials,
        snapshot,
        pane_last,
        webview_src,
        history_panes,
        path,
        session_queue,
        default_webview_url,
    );
    if let Ok(after) = active.single() {
        if after != before {
            commands.trigger(RequestNavigate { webview: after, url });
        }
    }
}

/// Nested tuples keep this under Bevy’s single-tuple `System` size; `#[derive(SystemParam)]` with
/// `Query<'w, 's, &'s _>` hits lifetime errors on recent Rust + Bevy 0.18.
#[allow(clippy::type_complexity)]
fn execute_palette_chord_pending(
    mut pending: ResMut<PalettePendingAction>,
    mut app_action_requests: ResMut<AppCommandRequestQueue>,
    (
        mut commands,
        mut meshes,
        mut materials,
        mut loading_bar_materials,
        mut layout_q,
        active,
        mut snapshot,
        pane_last,
        webview_src,
        path,
        mut session_queue,
        settings,
        window,
        camera,
        panes,
        pane_focus_incoming,
    ): (
        Commands,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<WebviewExtendStandardMaterial>>,
        ResMut<Assets<LoadingBarMaterial>>,
        Query<&mut Layout, With<vmux_layout::Window>>,
        Query<Entity, (With<Pane>, With<Active>)>,
        ResMut<SessionLayoutSnapshot>,
        Query<&PaneLastUrl>,
        Query<&WebviewSource>,
        Option<Res<SessionSavePath>>,
        ResMut<SessionSaveQueue>,
        Res<VmuxAppSettings>,
        Query<&Window, With<PrimaryWindow>>,
        Query<&Camera, With<VmuxWorldCamera>>,
        Query<Entity, With<Pane>>,
        Res<PaneFocusIncoming>,
    ),
    (
        history_panes,
        chrome_or_border,
        mut app_exit,
    ): (
        Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
        Query<
            (Entity, &PaneChromeOwner),
            Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
        >,
        MessageWriter<AppExit>,
    ),
) {
    let cmd = pending.0.take();
    let Some(cmd) = cmd else {
        return;
    };
    let default_url = settings.browser.default_webview_url.as_str();

    match cmd {
        AppCommand::Quit => {
            app_exit.write(AppExit::Success);
        }
        AppCommand::ToggleCommandPalette => {
            // [`submit`] already closed the palette; nothing else to do.
        }
        AppCommand::FocusCommandPaletteUrl => {
            app_action_requests.focus_command_palette_url_requested = true;
        }
        AppCommand::OpenHistory => {
            app_action_requests.open_history_requested = true;
        }
        AppCommand::OpenHistoryInNewTab => {
            app_action_requests.open_history_in_new_tab_requested = true;
        }
        AppCommand::SplitHorizontal => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_split_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                LayoutAxis::Horizontal,
                &mut meshes,
                &mut materials,
                &mut loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::SplitVertical => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_split_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                LayoutAxis::Vertical,
                &mut meshes,
                &mut materials,
                &mut loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::CycleNextPane => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(cur) = active.single() else {
                return;
            };
            try_cycle_pane_focus(&mut commands, &mut tree, cur);
        }
        AppCommand::SelectPane(dir) => {
            let Ok(window) = window.single() else {
                return;
            };
            let Ok(camera) = camera.single() else {
                return;
            };
            let Some((vw, vh)) = layout_viewport_for_workspace(window, camera) else {
                return;
            };
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            let rects = layout_workspace_pane_rects(vw, vh, &tree, &settings, |e| panes.get(e).is_ok());
            let prefer = pane_focus_incoming.0.get(&active_ent).copied();
            try_select_pane_direction(
                &mut commands,
                &mut tree,
                active_ent,
                dir,
                &rects,
                prefer,
            );
        }
        AppCommand::SwapPane(dir) => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_swap_active_pane(
                &mut tree,
                active_ent,
                dir,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::ToggleZoom => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_toggle_zoom_pane(&mut tree, active_ent);
        }
        AppCommand::MirrorLayout => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_mirror_pane_layout(
                &mut tree,
                active_ent,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::RotateBackward => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_rotate_window(
                &mut commands,
                &mut tree,
                active_ent,
                true,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::RotateForward => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_rotate_window(
                &mut commands,
                &mut tree,
                active_ent,
                false,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        AppCommand::ClosePane => {
            let Ok(mut tree) = layout_q.single_mut() else {
                return;
            };
            let Ok(active_ent) = active.single() else {
                return;
            };
            try_kill_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                &mut meshes,
                &mut materials,
                &mut loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                &chrome_or_border,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        _ => {}
    }
}

/// Nested tuples keep this under Bevy’s `IntoSystemConfigs` / `SystemParam` arity limits (same idea
/// as [`execute_palette_chord_pending`]).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn submit(
    mut commands: Commands,
    (keys, mut palette): (Res<ButtonInput<KeyCode>>, ResMut<VmuxCommandPaletteState>),
    ui_library_res: Res<VmuxUiLibraryBaseUrl>,
    mut pending_ui_library_nav: ResMut<VmuxPendingUiLibraryNavigation>,
    (
        active,
        panes,
        mut layout_q,
        mut meshes,
        mut materials,
        mut loading_bar_materials,
        mut snapshot,
        pane_last,
        webview_src,
        history_panes,
        main_webviews,
    ): (
        Query<Entity, (With<Pane>, With<Active>, With<Webview>, Without<History>)>,
        Query<Entity, With<Pane>>,
        Query<&mut Layout, With<vmux_layout::Window>>,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<WebviewExtendStandardMaterial>>,
        ResMut<Assets<LoadingBarMaterial>>,
        ResMut<SessionLayoutSnapshot>,
        Query<&PaneLastUrl>,
        Query<&WebviewSource>,
        Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
        Query<Entity, With<Webview>>,
    ),
    (path, mut session_queue, settings, hist, mut chord_pending): (
        Option<Res<SessionSavePath>>,
        ResMut<SessionSaveQueue>,
        Res<VmuxAppSettings>,
        Res<NavigationHistory>,
        ResMut<PalettePendingAction>,
    ),
) {
    let enter = keys.just_pressed(KeyCode::Enter);
    let click_submit = palette.pending_pointer_submit;
    if !palette.open || (!enter && !click_submit) {
        return;
    }
    if click_submit {
        palette.pending_pointer_submit = false;
    }
    let default_url = settings.browser.default_webview_url.as_str();
    let tabs = if let Ok(tree) = layout_q.single() {
        collect_command_palette_tabs(
            &*tree,
            &pane_last,
            &webview_src,
            &history_panes,
            &main_webviews,
            default_url,
        )
    } else {
        Vec::new()
    };
    let rows = build_palette_rows(
        &palette.input.query,
        &hist,
        &tabs,
        palette.ui_library_base.as_deref(),
        &settings.input,
    );
    if palette.selection >= ROWS_MAX {
        return;
    }
    let row = &rows[palette.selection];
    if !row.visible || !row.selectable {
        return;
    }
    let action = row.action.clone();

    match action {
        AppCommand::Omnibox { new_pane } => {
            if let Some(url) =
                resolve_omnibox_target(&palette.input.query, palette.ui_library_base.as_deref())
            {
                navigate_palette_url(
                    &mut commands,
                    url,
                    new_pane,
                    &active,
                    &mut layout_q,
                    &mut meshes,
                    &mut materials,
                    &mut loading_bar_materials,
                    &mut snapshot,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    path.as_ref(),
                    &mut session_queue,
                    default_url,
                );
                palette.open = false;
            }
        }
        AppCommand::WebSearch { new_pane } => {
            if let Some(url) = web_search_url(&palette.input.query) {
                navigate_palette_url(
                    &mut commands,
                    url,
                    new_pane,
                    &active,
                    &mut layout_q,
                    &mut meshes,
                    &mut materials,
                    &mut loading_bar_materials,
                    &mut snapshot,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    path.as_ref(),
                    &mut session_queue,
                    default_url,
                );
                palette.open = false;
            }
        }
        AppCommand::OpenUrl { url, new_pane } => {
            navigate_palette_url(
                &mut commands,
                url,
                new_pane,
                &active,
                &mut layout_q,
                &mut meshes,
                &mut materials,
                &mut loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
            palette.open = false;
        }
        AppCommand::OpenUiLibrary => {
            if let Some(url) = ui_library_nav_url(ui_library_stored_base(&ui_library_res, &palette)) {
                navigate_palette_url(
                    &mut commands,
                    url,
                    false,
                    &active,
                    &mut layout_q,
                    &mut meshes,
                    &mut materials,
                    &mut loading_bar_materials,
                    &mut snapshot,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    path.as_ref(),
                    &mut session_queue,
                    default_url,
                );
                palette.open = false;
            } else {
                pending_ui_library_nav.inner = Some(VmuxPendingUiLibraryNavTarget {
                    new_pane: false,
                });
                pending_ui_library_nav.wait_frames = 0;
                palette.open = false;
            }
        }
        AppCommand::OpenUiLibraryInNewPane => {
            if let Some(url) = ui_library_nav_url(ui_library_stored_base(&ui_library_res, &palette)) {
                navigate_palette_url(
                    &mut commands,
                    url,
                    true,
                    &active,
                    &mut layout_q,
                    &mut meshes,
                    &mut materials,
                    &mut loading_bar_materials,
                    &mut snapshot,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    path.as_ref(),
                    &mut session_queue,
                    default_url,
                );
                palette.open = false;
            } else {
                pending_ui_library_nav.inner = Some(VmuxPendingUiLibraryNavTarget { new_pane: true });
                pending_ui_library_nav.wait_frames = 0;
                palette.open = false;
            }
        }
        AppCommand::FocusPane(target) => {
            for e in panes.iter() {
                commands.entity(e).remove::<Active>();
            }
            commands.entity(target).insert(Active);
            palette.open = false;
        }
        AppCommand::Noop => {}
        cmd => {
            chord_pending.0 = Some(cmd);
            palette.open = false;
        }
    }
}

type DisjointQueryText = (
    With<CommandPaletteQueryText>,
    Without<CommandPaletteQueryPlaceholder>,
    Without<CommandPaletteQuerySelectionHighlight>,
    Without<CommandPaletteCaret>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
);

type DisjointQueryPlaceholderText = (
    With<CommandPaletteQueryPlaceholder>,
    Without<CommandPaletteRow>,
    Without<PaletteRowFavicon>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
);

type DisjointPrimaryText = (
    With<PaletteRowPrimary>,
    Without<CommandPaletteQueryText>,
    Without<PaletteRowIcon>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
);

type DisjointSecondaryText = (
    With<PaletteRowSecondary>,
    Without<CommandPaletteQueryText>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowEnterHint>,
);

type DisjointEnterHintText = (
    With<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
);

type DisjointCommandPaletteRowNode = (
    With<CommandPaletteRow>,
    Without<PaletteRowIcon>,
    Without<PaletteRowFavicon>,
);

type DisjointPaletteRowFaviconLabel = (
    With<PaletteRowFavicon>,
    Without<PaletteRowIcon>,
    Without<CommandPaletteRow>,
);

fn refresh_labels(
    mut palette: ResMut<VmuxCommandPaletteState>,
    hist: Res<NavigationHistory>,
    layout_q: Query<&Layout, With<vmux_layout::Window>>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    main_webviews: Query<Entity, With<Webview>>,
    settings: Res<VmuxAppSettings>,
    asset_server: Res<AssetServer>,
    (mut q_text, mut q_placeholder): (
        Query<&mut Text, DisjointQueryText>,
        Query<&mut Visibility, DisjointQueryPlaceholderText>,
    ),
    mut primary: Query<(&PaletteRowPrimary, &mut Text), DisjointPrimaryText>,
    mut secondary: Query<(&PaletteRowSecondary, &mut Text), DisjointSecondaryText>,
    mut enter_hints: Query<(&PaletteRowEnterHint, &mut Text), DisjointEnterHintText>,
    mut icons: Query<(&PaletteRowIcon, &mut Text, &mut Visibility), DisjointIconText>,
    mut row_nodes: Query<(&CommandPaletteRow, &mut Visibility, &mut Node), DisjointCommandPaletteRowNode>,
    mut fav: Query<
        (&PaletteRowFavicon, &mut ImageNode, &mut Visibility),
        DisjointPaletteRowFaviconLabel,
    >,
) {
    if !palette.open {
        return;
    }

    let default_url = settings.browser.default_webview_url.as_str();
    let tabs = if let Ok(tree) = layout_q.single() {
        collect_command_palette_tabs(
            &*tree,
            &pane_last,
            &webview_src,
            &history_panes,
            &main_webviews,
            default_url,
        )
    } else {
        Vec::new()
    };
    let rows = build_palette_rows(
        &palette.input.query,
        &hist,
        &tabs,
        palette.ui_library_base.as_deref(),
        &settings.input,
    );
    let mut sel = palette.selection;
    if !rows.get(sel).map(|r| r.visible).unwrap_or(false) {
        sel = row_index_from_visible_selection(&rows, 0);
    }
    if rows.get(sel).is_some_and(|r| r.visible && !r.selectable) {
        if let Some(i) = first_visible_selectable_row(&rows) {
            sel = i;
        }
    }
    palette.selection = sel;

    for i in 0..ROWS_MAX {
        palette.row_selectable_mask[i] = rows[i].selectable;
    }

    let qlen = query_len_chars(&palette.input.query);
    if palette.input.caret > qlen {
        palette.input.caret = qlen;
    }
    if palette
        .input
        .selection_anchor
        .is_some_and(|a| a > qlen || a == palette.input.caret)
    {
        palette.input.selection_anchor = None;
    }
    if let Ok(mut t) = q_text.single_mut() {
        *t = Text::new(palette.input.query.as_str());
    }
    if let Ok(mut vis) = q_placeholder.single_mut() {
        *vis = if palette.input.query.is_empty() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for (CommandPaletteRow(i), mut vis, mut node) in &mut row_nodes {
        let idx = *i as usize;
        if idx < ROWS_MAX {
            let show = rows[idx].visible;
            *vis = if show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            node.display = if show {
                Display::Flex
            } else {
                Display::None
            };
        }
    }

    for (tag, mut text) in &mut primary {
        let i = tag.0 as usize;
        if i < ROWS_MAX {
            *text = Text::new(rows[i].primary.as_str());
        }
    }
    for (tag, mut text) in &mut secondary {
        let i = tag.0 as usize;
        if i < ROWS_MAX {
            *text = Text::new(palette_secondary_display(rows[i].secondary.as_str()));
        }
    }
    for (tag, mut text) in &mut enter_hints {
        let i = tag.0 as usize;
        if i < ROWS_MAX {
            *text = Text::new(rows[i].enter.as_str());
        }
    }
    for (tag, mut text, mut gvis) in &mut icons {
        let i = tag.0 as usize;
        if i < ROWS_MAX {
            *text = Text::new(rows[i].icon);
            match &rows[i].favicon {
                Some(url) => {
                    let h: Handle<Image> = load_remote_favicon_image(&asset_server, url.clone());
                    // Hide placeholder only after the texture is ready (see favicon loop below).
                    *gvis = if asset_server.load_state(&h).is_loaded() {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    };
                }
                None => {
                    *gvis = Visibility::Visible;
                }
            }
        }
    }

    for (PaletteRowFavicon(i), mut img, mut vis) in &mut fav {
        let i = *i as usize;
        if i >= ROWS_MAX {
            continue;
        }
        if let Some(url) = rows[i].favicon.clone() {
            img.image = load_remote_favicon_image(&asset_server, url);
            *vis = if asset_server.load_state(&img.image).is_loaded() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

type DisjointIconText = (
    With<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
    Without<CommandPaletteRow>,
    Without<PaletteRowFavicon>,
);

type DisjointIconColor = (
    With<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
);

type DisjointPrimaryColor = (
    With<PaletteRowPrimary>,
    Without<PaletteRowIcon>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
);

type DisjointSecondaryColor = (
    With<PaletteRowSecondary>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
);

type DisjointHintColor = (
    With<PaletteRowEnterHint>,
    Without<PaletteNavEnterHint>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<CommandPaletteQueryText>,
);

type DisjointHintNavColor = (
    With<PaletteRowEnterHint>,
    With<PaletteNavEnterHint>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<CommandPaletteQueryText>,
);

type DisjointFaviconImage = (
    With<PaletteRowFavicon>,
    Without<PaletteRowIcon>,
    Without<PaletteRowPrimary>,
    Without<PaletteRowSecondary>,
    Without<PaletteRowEnterHint>,
    Without<CommandPaletteQueryText>,
);

fn style_rows(
    palette: Res<VmuxCommandPaletteState>,
    hover_map: Res<HoverMap>,
    row_q: Query<&CommandPaletteRow>,
    parents: Query<&ChildOf>,
    mut row_vs_nav_hint_vis: ParamSet<(
        Query<(&CommandPaletteRow, &mut BackgroundColor, &Visibility)>,
        Query<(&PaletteRowEnterHint, &mut TextColor, &mut Visibility), DisjointHintNavColor>,
    )>,
    mut icons: Query<(&PaletteRowIcon, &mut TextColor), DisjointIconColor>,
    mut primary: Query<(&PaletteRowPrimary, &mut TextColor), DisjointPrimaryColor>,
    mut secondary: Query<(&PaletteRowSecondary, &mut TextColor), DisjointSecondaryColor>,
    mut hints: Query<(&PaletteRowEnterHint, &mut TextColor), DisjointHintColor>,
    mut fav_imgs: Query<(&PaletteRowFavicon, &mut ImageNode), DisjointFaviconImage>,
) {
    if !palette.is_changed() && !palette.open {
        return;
    }
    if !palette.open {
        return;
    }

    let hover_idx = pointer_top_entity(&hover_map, PointerId::Mouse)
        .and_then(|e| entity_to_palette_row_index(e, &row_q, &parents));

    for (CommandPaletteRow(i), mut bg, vis) in row_vs_nav_hint_vis.p0().iter_mut() {
        if *vis == Visibility::Hidden {
            continue;
        }
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        let hover = hover_idx == Some(i) && !sel;
        *bg = if sel {
            ROW_BG_SELECTED.into()
        } else if hover {
            ROW_BG_HOVER.into()
        } else {
            ROW_BG.into()
        };
    }

    for (PaletteRowIcon(i), mut tc) in &mut icons {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        *tc = TextColor(if sel { ROW_TEXT_SELECTED } else { ROW_TEXT });
    }
    for (PaletteRowPrimary(i), mut tc) in &mut primary {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        *tc = TextColor(if sel { ROW_TEXT_SELECTED } else { ROW_TEXT });
    }
    for (PaletteRowSecondary(i), mut tc) in &mut secondary {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        *tc = TextColor(if sel { ROW_SUBTEXT_SELECTED } else { ROW_SUBTEXT });
    }
    for (PaletteRowEnterHint(i), mut tc) in &mut hints {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        let hover = hover_idx == Some(i) && !sel;
        *tc = TextColor(if sel {
            ROW_TEXT_SELECTED
        } else if hover {
            Color::srgba(0.62, 0.63, 0.68, 1.0)
        } else {
            Color::srgba(0.5, 0.51, 0.55, 1.0)
        });
    }
    for (PaletteRowEnterHint(i), mut tc, mut vis) in row_vs_nav_hint_vis.p1().iter_mut() {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        *vis = if sel {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        *tc = TextColor(if sel {
            ROW_TEXT_SELECTED
        } else {
            Color::srgba(0.5, 0.51, 0.55, 1.0)
        });
    }
    for (PaletteRowFavicon(i), mut img) in &mut fav_imgs {
        let i = *i as usize;
        let sel = i == palette.selection && palette.row_selectable_mask[i];
        let hover = hover_idx == Some(i) && !sel;
        img.color = if sel {
            Color::WHITE
        } else if hover {
            Color::srgba(0.94, 0.95, 0.96, 0.98)
        } else {
            Color::srgba(0.88, 0.89, 0.92, 0.92)
        };
    }
}

/// Command palette resource and systems (palette input in [`Update`], submit + deferred chords in [`PostUpdate`]). Add [`setup`] on [`Startup`] after the world camera.
#[derive(Default)]
pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavigationHistory>()
            .init_resource::<PalettePendingAction>()
            .init_resource::<AppCommandRequestQueue>()
            .init_resource::<VmuxCommandPaletteState>()
            .init_resource::<VmuxUiLibraryBaseUrl>()
            .init_resource::<VmuxPendingUiLibraryNavigation>();
        app.add_observer(on_command_palette_scroll);
        app.add_systems(
            PostUpdate,
            (
                sync_command_palette_camera_viewport.after(apply_pane_layout),
                clamp_palette_list_scroll_to_selection.after(UiSystems::Layout),
                position_command_palette_query_layout.after(text_system),
            ),
        );
        app.configure_sets(
            Update,
            (
                CommandPalettePipeline::InputChain,
                CommandPalettePipeline::SyncVis,
                CommandPalettePipeline::RefreshLabels,
                CommandPalettePipeline::StyleRows,
            )
                .chain(),
        );
        app.configure_sets(
            PostUpdate,
            (CommandPalettePipeline::Submit, CommandPalettePipeline::Chord).chain(),
        );
        app.add_systems(
            Update,
            (
                sync_ui_library_url_into_palette,
                toggle_hotkey,
                focus_url_hotkey,
                sync_command_palette_caret,
                note_palette_mouse_motion,
                handle_keyboard,
                sync_command_palette_pointer_selection,
            )
                .chain()
                .in_set(CommandPalettePipeline::InputChain),
        );
        app.add_systems(Update, send_command_palette_scroll_events);
        app.add_systems(
            PostUpdate,
            (
                submit.in_set(CommandPalettePipeline::Submit),
                apply_pending_ui_library_navigation,
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                execute_palette_chord_pending,
                apply_focus_command_palette_url_request,
            )
                .chain()
                .in_set(CommandPalettePipeline::Chord),
        );
        app.add_systems(
            Update,
            sync_visibility.in_set(CommandPalettePipeline::SyncVis),
        );
        app.add_systems(
            Update,
            refresh_labels.in_set(CommandPalettePipeline::RefreshLabels),
        );
        app.add_systems(
            Update,
            style_rows.in_set(CommandPalettePipeline::StyleRows),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;
    use bevy::input::keyboard::Key;
    use std::time::Duration;

    #[test]
    fn sync_visibility_system_tracks_open_state() {
        let mut app = App::new();
        app.init_resource::<VmuxCommandPaletteState>();
        let root = app.world_mut().spawn((CommandPaletteRoot, Visibility::Visible)).id();
        app.add_systems(Update, sync_visibility);

        app.world_mut().resource_mut::<VmuxCommandPaletteState>().open = false;
        app.update();
        assert_eq!(*app.world().entity(root).get::<Visibility>().unwrap(), Visibility::Hidden);

        app.world_mut().resource_mut::<VmuxCommandPaletteState>().open = true;
        app.update();
        assert_eq!(
            *app.world().entity(root).get::<Visibility>().unwrap(),
            Visibility::Visible
        );
    }

    #[test]
    fn sync_command_palette_caret_system_blinks() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.insert_resource(VmuxCommandPaletteState {
            open: true,
            input: vmux_core::command_palette::CommandPaletteInputState {
                caret_blink_t0: 0.0,
                ..Default::default()
            },
            ..Default::default()
        });
        let caret = app
            .world_mut()
            .spawn((CommandPaletteCaret, Visibility::Hidden))
            .id();
        app.add_systems(Update, sync_command_palette_caret);

        app.update();
        assert_eq!(
            *app.world().entity(caret).get::<Visibility>().unwrap(),
            Visibility::Visible
        );

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(PALETTE_CARET_PHASE_SECS));
        app.update();
        assert_eq!(
            *app.world().entity(caret).get::<Visibility>().unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn style_rows_system_highlights_selected_row() {
        let mut app = App::new();
        app.init_resource::<HoverMap>();
        app.insert_resource(VmuxCommandPaletteState {
            open: true,
            selection: 1,
            ..Default::default()
        });

        let row0 = app
            .world_mut()
            .spawn((CommandPaletteRow(0), BackgroundColor(ROW_BG), Visibility::Visible))
            .id();
        let row1 = app
            .world_mut()
            .spawn((CommandPaletteRow(1), BackgroundColor(ROW_BG), Visibility::Visible))
            .id();
        let icon0 = app
            .world_mut()
            .spawn((PaletteRowIcon(0), TextColor(ROW_TEXT)))
            .id();
        let icon1 = app
            .world_mut()
            .spawn((PaletteRowIcon(1), TextColor(ROW_TEXT)))
            .id();

        app.add_systems(Update, style_rows);
        app.update();

        assert_eq!(app.world().entity(row0).get::<BackgroundColor>().unwrap().0, ROW_BG);
        assert_eq!(
            app.world().entity(row1).get::<BackgroundColor>().unwrap().0,
            ROW_BG_SELECTED
        );
        assert_eq!(app.world().entity(icon0).get::<TextColor>().unwrap().0, ROW_TEXT);
        assert_eq!(
            app.world().entity(icon1).get::<TextColor>().unwrap().0,
            ROW_TEXT_SELECTED
        );
    }

    #[test]
    fn handle_keyboard_typing_reselects_query_row() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<Messages<KeyboardInput>>();
        app.init_resource::<Time>();
        app.init_resource::<NavigationHistory>();
        app.init_resource::<VmuxAppSettings>();
        app.insert_resource(VmuxCommandPaletteState {
            open: true,
            selection: 0,
            ..Default::default()
        });
        app.add_systems(Update, handle_keyboard);

        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyV,
            logical_key: Key::Character("v".into()),
            state: ButtonState::Pressed,
            text: Some("v".into()),
            repeat: false,
            window: Entity::PLACEHOLDER,
        });

        app.update();

        let palette = app.world().resource::<VmuxCommandPaletteState>();
        assert_eq!(palette.input.query, "v");
        assert_eq!(
            palette.selection, MAX_PALETTE_TABS,
            "typing in palette should move selection to omnibox/search row"
        );
    }

    #[test]
    fn ui_library_phrase_resolves_with_base() {
        let base = Some("http://127.0.0.1:54321/");
        assert_eq!(
            super::ui_library_url_for_query(":debug vmux ui", base).as_deref(),
            Some("http://127.0.0.1:54321/")
        );
        assert_eq!(
            super::ui_library_url_for_query("  :debug   vmux  ui  ", base).as_deref(),
            Some("http://127.0.0.1:54321/")
        );
        assert!(super::ui_library_url_for_query("debug vmux ui", None).is_none());
    }

    #[test]
    fn resolve_omnibox_prefers_ui_library_when_base_set() {
        assert_eq!(
            super::resolve_omnibox_target(":debug vmux ui", Some("http://127.0.0.1:1/")).as_deref(),
            Some("http://127.0.0.1:1/")
        );
    }
}
