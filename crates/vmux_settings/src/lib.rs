//! Shared vmux defaults and [`VmuxAppSettings`] (persisted via [moonshine-save] with session).
//!
//! Bundled defaults are read from `settings.ron` next to this crate’s `Cargo.toml`.
//!
//! At runtime, if [`resolved_settings_path`] exists, it is loaded on **Startup** (after session
//! restore) and watched with [`notify`]. Edits to the file reload [`VmuxAppSettings`] without
//! restarting the app. Override the path with **`VMUX_SETTINGS_PATH`**.

use bevy::prelude::*;
use crossbeam_channel::Receiver;
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub mod bindings;

pub use bindings::{
    parse_key_code, preset_bindings, preset_from_env_or_default, BindingCommandId, ChordStep,
    GlobalBinding, PrefixChordSettings, PrefixSecondBinding, VmuxBindingGeneration,
    VmuxBindingSettings,
};

/// Tmux-style prefix: second key must arrive within this many seconds (matches [`crate::vmux_core`] default).
pub const PREFIX_TIMEOUT_SECS: f32 = 1.5;

/// Webview / URL defaults (user `settings.ron` → `browser:`).
#[derive(Clone, Debug, Reflect)]
#[reflect(Default)]
pub struct VmuxBrowserSettings {
    pub default_webview_url: String,
}

impl Default for VmuxBrowserSettings {
    fn default() -> Self {
        Self {
            default_webview_url: "https://www.google.com".to_string(),
        }
    }
}

/// Pane grid and window chrome insets (user `settings.ron` → `layout:`).
#[derive(Clone, Debug, Reflect)]
#[reflect(Default)]
pub struct VmuxLayoutSettings {
    /// Logical pixels between adjacent panes at each split (0 = flush). Named like tmux `pane-border-*` options.
    pub pane_border_spacing_px: f32,
    /// Inset from the window **left, right, and bottom** inner edges to the pane grid (layout px).
    pub window_padding_px: f32,
    /// Inset from the window **top** inner edge (layout px). Use a larger value than [`window_padding_px`]
    /// when the title bar / traffic lights overlap content (e.g. full-size content view on macOS).
    pub window_padding_top_px: f32,
    /// Corner radius for pane tiles in layout pixels (0 = square).
    pub pane_border_radius_px: f32,
}

impl Default for VmuxLayoutSettings {
    fn default() -> Self {
        Self {
            pane_border_spacing_px: 8.0,
            window_padding_px: 8.0,
            window_padding_top_px: 28.0,
            pane_border_radius_px: 8.0,
        }
    }
}

#[derive(Deserialize)]
struct NestedBundledRon {
    browser: BrowserRon,
    layout: LayoutRon,
    #[serde(default)]
    input: Option<VmuxBindingSettings>,
}

#[derive(Deserialize)]
struct BrowserRon {
    default_webview_url: String,
}

#[derive(Deserialize)]
struct LayoutRon {
    /// Logical pixels between adjacent panes at each split (0 = flush). Mirrors tmux `pane-border-*` naming.
    #[serde(alias = "pane_gap_px")]
    pane_border_spacing_px: f32,
    /// Inset from the window inner edge to the layout grid (logical px; 0 = edge-to-edge). Mirrors tmux `window-*` naming.
    #[serde(alias = "window_edge_gap_px")]
    window_padding_px: f32,
    /// Top inset (layout px). `0` means same as `window_padding_px` (RON serde does not accept bare floats for `Option`).
    #[serde(default)]
    window_padding_top_px: f32,
    /// Corner radius for pane tiles in layout pixels (0 = square).
    pane_border_radius_px: f32,
}

#[derive(Deserialize)]
struct FlatBundledSettings {
    default_webview_url: String,
    #[serde(alias = "pane_gap_px")]
    pane_border_spacing_px: f32,
    #[serde(alias = "window_edge_gap_px")]
    window_padding_px: f32,
    #[serde(default)]
    window_padding_top_px: f32,
    pane_border_radius_px: f32,
}

static BUNDLED_SETTINGS: OnceLock<VmuxAppSettings> = OnceLock::new();

#[inline]
fn resolve_window_padding_top_px(window_padding_px: f32, window_padding_top_px: f32) -> f32 {
    if window_padding_top_px > 0.0 {
        window_padding_top_px
    } else {
        window_padding_px
    }
}

/// Resolves legacy `window_padding_top_px: 0` to match horizontal padding (RON / flat session files).
#[inline]
pub fn resolve_window_padding_top_px_for_load(
    window_padding_px: f32,
    window_padding_top_px: f32,
) -> f32 {
    resolve_window_padding_top_px(window_padding_px, window_padding_top_px)
}

fn bundled_settings() -> &'static VmuxAppSettings {
    BUNDLED_SETTINGS.get_or_init(|| {
        const EMBEDDED: &str = include_str!("../settings.ron");
        parse_settings_ron(EMBEDDED)
            .unwrap_or_else(|e| panic!("vmux_settings: invalid bundled settings.ron: {e}"))
    })
}

/// Bundled default webview URL from `settings.ron` (same string as [`VmuxAppSettings::default`] until overridden at runtime).
pub fn default_webview_url() -> &'static str {
    bundled_settings().browser.default_webview_url.as_str()
}

/// User-tunable app settings. Saved with [`SessionLayoutSnapshot`] in `last_session.ron` (moonshine).
///
/// Field names under [`VmuxLayoutSettings`] follow tmux’s hyphenated options as snake_case (`pane-border-*` → `pane_border_*`,
/// `window-*` → `window_*`). Older flat `settings.ron` keys `pane_gap_px` / `window_edge_gap_px` still
/// deserialize via serde aliases.
#[derive(Resource, Clone, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct VmuxAppSettings {
    pub browser: VmuxBrowserSettings,
    pub layout: VmuxLayoutSettings,
    /// Keybindings; when missing from file, derived from [`preset_from_env_or_default`] (see `VMUX_BINDING_PRESET`).
    pub input: VmuxBindingSettings,
}

impl Default for VmuxAppSettings {
    fn default() -> Self {
        bundled_settings().clone()
    }
}

/// Path to the reactive `settings.ron` file: `VMUX_SETTINGS_PATH` if set, else
/// `crates/vmux_settings/settings.ron` in the source tree (compile-time [`CARGO_MANIFEST_DIR`]).
pub fn resolved_settings_path() -> PathBuf {
    if let Ok(p) = std::env::var("VMUX_SETTINGS_PATH") {
        PathBuf::from(p)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("settings.ron")
    }
}

fn parse_settings_ron(s: &str) -> Result<VmuxAppSettings, ron::error::SpannedError> {
    if let Ok(nested) = ron::de::from_str::<NestedBundledRon>(s) {
        let input = nested
            .input
            .clone()
            .unwrap_or_else(|| preset_from_env_or_default(None));
        return Ok(VmuxAppSettings {
            browser: VmuxBrowserSettings {
                default_webview_url: nested.browser.default_webview_url,
            },
            layout: VmuxLayoutSettings {
                pane_border_spacing_px: nested.layout.pane_border_spacing_px,
                window_padding_px: nested.layout.window_padding_px,
                window_padding_top_px: resolve_window_padding_top_px(
                    nested.layout.window_padding_px,
                    nested.layout.window_padding_top_px,
                ),
                pane_border_radius_px: nested.layout.pane_border_radius_px,
            },
            input,
        });
    }
    let flat: FlatBundledSettings = ron::de::from_str(s)?;
    Ok(VmuxAppSettings {
        browser: VmuxBrowserSettings {
            default_webview_url: flat.default_webview_url,
        },
        layout: VmuxLayoutSettings {
            pane_border_spacing_px: flat.pane_border_spacing_px,
            window_padding_px: flat.window_padding_px,
            window_padding_top_px: resolve_window_padding_top_px(
                flat.window_padding_px,
                flat.window_padding_top_px,
            ),
            pane_border_radius_px: flat.pane_border_radius_px,
        },
        input: preset_from_env_or_default(None),
    })
}

fn load_settings_from_path(path: &Path) -> Option<VmuxAppSettings> {
    let s = std::fs::read_to_string(path).ok()?;
    parse_settings_ron(&s).ok()
}

#[derive(Resource)]
struct SettingsFileReloadChannel {
    path: PathBuf,
    rx: Receiver<()>,
}

fn event_targets_path(event: &notify::Event, path: &Path) -> bool {
    event.paths.iter().any(|p| {
        p == path
            || p.file_name()
                .is_some_and(|n| n == path.file_name().unwrap_or_default())
    })
}

fn run_watcher(path: PathBuf, tx: crossbeam_channel::Sender<()>) {
    let path_for_cb = path.clone();
    let mut watcher =
        match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else {
                return;
            };
            if !event.kind.is_modify() && !event.kind.is_create() && !event.kind.is_remove() {
                return;
            }
            if event_targets_path(&event, &path_for_cb) {
                let _ = tx.send(());
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                warn!("vmux_settings: could not create file watcher: {e}");
                return;
            }
        };

    let watch_res = if path.is_file() {
        watcher.watch(&path, RecursiveMode::NonRecursive)
    } else if let Some(parent) = path.parent() {
        watcher.watch(parent, RecursiveMode::NonRecursive)
    } else {
        return;
    };

    if let Err(e) = watch_res {
        warn!("vmux_settings: could not watch {:?}: {e}", path);
        return;
    }

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

fn load_settings_file_on_startup(
    mut settings: ResMut<VmuxAppSettings>,
    mut binding_gen: ResMut<VmuxBindingGeneration>,
) {
    let path = resolved_settings_path();
    if !path.is_file() {
        return;
    }
    if let Some(s) = load_settings_from_path(&path) {
        *settings = s;
        binding_gen.0 = binding_gen.0.wrapping_add(1);
    } else {
        warn!("vmux_settings: invalid settings.ron at {:?}", path);
    }
}

fn spawn_settings_file_watcher(mut commands: Commands) {
    let path = resolved_settings_path();
    let watchable = path.is_file() || path.parent().is_some_and(|p| p.is_dir());
    if !watchable {
        return;
    }

    let (tx, rx) = crossbeam_channel::unbounded();
    let path_thread = path.clone();
    match std::thread::Builder::new()
        .name("vmux-settings-watch".into())
        .spawn(move || run_watcher(path_thread, tx))
    {
        Ok(_) => {
            commands.insert_resource(SettingsFileReloadChannel { path, rx });
        }
        Err(e) => warn!("vmux_settings: could not spawn settings watcher thread: {e}"),
    }
}

fn apply_settings_file_reloads(
    mut settings: ResMut<VmuxAppSettings>,
    mut binding_gen: ResMut<VmuxBindingGeneration>,
    channel: Option<Res<SettingsFileReloadChannel>>,
) {
    let Some(channel) = channel else {
        return;
    };
    let mut any = false;
    while channel.rx.try_recv().is_ok() {
        any = true;
    }
    if !any {
        return;
    }
    let path = &channel.path;
    if let Some(s) = load_settings_from_path(path) {
        *settings = s;
        binding_gen.0 = binding_gen.0.wrapping_add(1);
    } else {
        warn!("vmux_settings: invalid settings.ron at {:?}", path);
    }
}

/// User-writable vmux cache directory (session, CEF sibling, etc.), inserted in [`PreStartup`](Schedule) by [`SettingsPlugin`].
#[derive(Resource, Clone, Debug, Default)]
pub struct VmuxCacheDir(pub Option<PathBuf>);

/// Runs before systems that read [`VmuxCacheDir`] (e.g. session save path).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VmuxCacheDirInitSet;

fn vmux_cache_base_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let local = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
        return local
            .or_else(|| {
                std::env::var("USERPROFILE").ok().map(|p| {
                    PathBuf::from(p).join("AppData").join("Local")
                })
            })
            .map(|p| p.join("vmux").join("cache"));
    }
    if cfg!(target_os = "macos") {
        return std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join("Library").join("Caches").join("vmux"));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".cache").join("vmux"))
}

fn init_vmux_cache_dir(mut commands: Commands) {
    commands.insert_resource(VmuxCacheDir(vmux_cache_base_dir()));
}

pub fn cef_root_cache_path() -> Option<String> {
    vmux_cache_base_dir()
        .map(|base| base.join("cef").to_string_lossy().into_owned())
        .or_else(|| {
            Some(
                std::env::temp_dir()
                    .join("vmux_cef")
                    .to_string_lossy()
                    .into_owned(),
            )
        })
}

/// Registers [`VmuxAppSettings`] for reflection (moonshine load/save) and [`VmuxCacheDir`] on startup.
#[derive(Default)]
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<VmuxBrowserSettings>()
            .register_type::<VmuxLayoutSettings>()
            .register_type::<BindingCommandId>()
            .register_type::<ChordStep>()
            .register_type::<GlobalBinding>()
            .register_type::<PrefixSecondBinding>()
            .register_type::<PrefixChordSettings>()
            .register_type::<VmuxBindingSettings>()
            .register_type::<VmuxAppSettings>()
            .init_resource::<VmuxBindingGeneration>()
            .init_resource::<VmuxAppSettings>()
            .configure_sets(PreStartup, VmuxCacheDirInitSet)
            .add_systems(PreStartup, init_vmux_cache_dir.in_set(VmuxCacheDirInitSet))
            .add_systems(
                Startup,
                (load_settings_file_on_startup, spawn_settings_file_watcher).chain(),
            )
            .add_systems(Update, apply_settings_file_reloads);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(SettingsPlugin);
    }

    #[test]
    fn bundled_settings_parse_includes_input_bindings() {
        let s = include_str!("../settings.ron");
        let app = parse_settings_ron(s).expect("parse");
        assert!(!app.input.global.is_empty());
        assert!(!app.input.prefix.second.is_empty());
    }

    #[test]
    fn vim_preset_changes_prefix_lead() {
        let vim = preset_bindings("vim");
        assert_eq!(vim.prefix.lead.key, "Backquote");
        let tmux = preset_bindings("tmux");
        assert_eq!(tmux.prefix.lead.key, "KeyB");
    }

}
