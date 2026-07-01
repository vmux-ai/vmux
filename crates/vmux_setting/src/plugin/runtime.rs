use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};
use vmux_layout::settings::ConfirmCloseSettings;
pub use vmux_layout::settings::LayoutSettings;
#[cfg(test)]
pub use vmux_layout::settings::{
    FocusRingSettings, PaneSettings, SideSheetSettings, WindowSettings,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingsLoadSet;

#[derive(Clone, Debug, Deserialize, Serialize, Resource)]
pub struct AppSettings {
    #[serde(default = "default_browser_settings")]
    pub browser: BrowserSettings,
    #[serde(default)]
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default = "default_agent_settings")]
    pub agent: AgentSettings,
    #[serde(default)]
    pub spaces: std::collections::BTreeMap<String, SpaceOverrides>,
    #[serde(default)]
    pub recording: RecordingSettings,
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub appearance: AppearanceSettings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    Light,
    Dark,
    #[default]
    Device,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct AppearanceSettings {
    #[serde(default)]
    pub mode: ColorScheme,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EditorSettings {
    #[serde(default)]
    pub keymap: vmux_core::KeymapKind,
    #[serde(default)]
    pub lsp: LspSettings,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LspSettings {
    #[serde(default)]
    pub servers: std::collections::BTreeMap<String, LspServerOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspServerOverride {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub language_id: String,
    #[serde(default)]
    pub root_markers: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RecordingSettings {
    /// Output directory for screenshots and screen recordings. Absent falls back
    /// to the default `~/.vmux/recording` (see [`vmux_core::profile::recording_dir`]).
    #[serde(default)]
    pub output_dir: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SpaceOverrides {
    #[serde(default)]
    pub startup_url: Option<String>,
    #[serde(default)]
    pub startup_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentSettings {
    #[serde(default)]
    pub app_providers: Vec<AppProviderSettings>,
    /// When true (default), an agent reading/editing a file opens it in a
    /// `file://` follow-pane beside that agent.
    #[serde(default = "default_true")]
    pub follow_files: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        default_agent_settings()
    }
}

fn default_agent_settings() -> AgentSettings {
    AgentSettings {
        app_providers: vec![AppProviderSettings {
            provider: "stub".to_string(),
            kind: "vibe".to_string(),
            models: vec!["echo".to_string()],
        }],
        follow_files: true,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppProviderSettings {
    pub provider: String,
    #[serde(default = "default_provider_kind")]
    pub kind: String,
    pub models: Vec<String>,
}

fn default_provider_kind() -> String {
    "vibe".to_string()
}

fn normalize_space_key(key: &str) -> String {
    key.chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect::<String>()
        .to_lowercase()
}

fn space_override<'a>(settings: &'a AppSettings, space_id: &str) -> Option<&'a SpaceOverrides> {
    if let Some(overrides) = settings.spaces.get(space_id) {
        return Some(overrides);
    }
    let target = normalize_space_key(space_id);
    settings
        .spaces
        .iter()
        .find(|(key, _)| normalize_space_key(key) == target)
        .map(|(_, value)| value)
}

pub fn resolve_startup_url(settings: &AppSettings, space_id: &str) -> String {
    let per_space = space_override(settings, space_id)
        .and_then(|o| o.startup_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let chosen = per_space.unwrap_or_else(|| settings.browser.startup_url.trim());
    if chosen.is_empty() || chosen == "vmux://agent/" || chosen == "vmux://agent" {
        default_browser_startup_url()
    } else {
        chosen.to_string()
    }
}

pub fn resolve_startup_dir(settings: &AppSettings, space_id: &str) -> std::path::PathBuf {
    let pick = |opt: Option<&str>| -> Option<std::path::PathBuf> {
        opt.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
    };
    pick(space_override(settings, space_id).and_then(|o| o.startup_dir.as_deref()))
        .or_else(|| {
            pick(
                settings
                    .terminal
                    .as_ref()
                    .and_then(|t| t.startup_dir.as_deref()),
            )
        })
        .unwrap_or_else(|| vmux_core::profile::space_dir(space_id))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShortcutSettings {
    #[serde(default = "default_leader")]
    pub leader: KeyComboDef,
    #[serde(default = "default_chord_timeout_ms")]
    pub chord_timeout_ms: u64,
    #[serde(default)]
    pub bindings: Vec<ShortcutEntry>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            leader: default_leader(),
            chord_timeout_ms: default_chord_timeout_ms(),
            bindings: Vec::new(),
        }
    }
}

fn default_leader() -> KeyComboDef {
    KeyComboDef {
        key: "g".to_string(),
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    }
}

fn default_chord_timeout_ms() -> u64 {
    1000
}

fn default_auto_update() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShortcutEntry {
    pub command: String,
    pub binding: ShortcutDef,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ShortcutDef {
    Direct(KeyComboDef),
    Chord(KeyComboDef, KeyComboDef),
    Leader(KeyComboDef),
}

impl ShortcutDef {
    pub fn to_shortcut(&self) -> Option<vmux_command::shortcut::Shortcut> {
        match self {
            ShortcutDef::Direct(combo) => Some(vmux_command::shortcut::Shortcut::Direct(
                combo.to_key_combo()?,
            )),
            ShortcutDef::Chord(prefix, second) => Some(vmux_command::shortcut::Shortcut::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            ShortcutDef::Leader(_second) => None,
        }
    }

    pub fn to_shortcut_with_leader(
        &self,
        leader: &vmux_command::shortcut::KeyCombo,
    ) -> Option<vmux_command::shortcut::Shortcut> {
        match self {
            ShortcutDef::Direct(combo) => Some(vmux_command::shortcut::Shortcut::Direct(
                combo.to_key_combo()?,
            )),
            ShortcutDef::Chord(prefix, second) => Some(vmux_command::shortcut::Shortcut::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            ShortcutDef::Leader(second) => Some(vmux_command::shortcut::Shortcut::Chord(
                leader.clone(),
                second.to_key_combo()?,
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KeyComboDef {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub super_key: bool,
}

impl KeyComboDef {
    pub fn to_key_combo(&self) -> Option<vmux_command::shortcut::KeyCombo> {
        let resolved = vmux_command::shortcut::resolve_key(&self.key)?;
        Some(vmux_command::shortcut::KeyCombo {
            key: resolved.key,
            modifiers: vmux_command::shortcut::Modifiers {
                ctrl: self.ctrl,
                shift: self.shift || resolved.implicit_shift,
                alt: self.alt,
                super_key: self.super_key,
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowserSettings {
    #[serde(default = "default_browser_startup_url")]
    pub startup_url: String,
}

fn default_browser_settings() -> BrowserSettings {
    BrowserSettings {
        startup_url: default_browser_startup_url(),
    }
}

fn default_browser_startup_url() -> String {
    "vmux://start/".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalSettings {
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default = "default_theme_name")]
    pub default_theme: String,
    #[serde(default)]
    pub themes: Vec<TerminalTheme>,
    #[serde(default)]
    pub custom_themes: Vec<crate::themes::TerminalColorScheme>,
    #[serde(default = "default_true")]
    pub confirm_close: bool,
    #[serde(default)]
    pub startup_dir: Option<String>,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: None,
            font_family: None,
            default_theme: default_theme_name(),
            themes: Vec::new(),
            custom_themes: Vec::new(),
            confirm_close: true,
            startup_dir: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_theme_name() -> String {
    "default".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalTheme {
    pub name: String,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_terminal_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "default_padding")]
    pub padding: f32,
    #[serde(default = "default_cursor_style")]
    pub cursor_style: String,
    #[serde(default = "default_cursor_blink")]
    pub cursor_blink: bool,
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_color_scheme() -> String {
    "catppuccin-mocha".to_string()
}

fn default_font_size() -> f32 {
    14.0
}

fn default_line_height() -> f32 {
    1.2
}

fn default_padding() -> f32 {
    4.0
}

fn default_cursor_style() -> String {
    "block".to_string()
}

fn default_cursor_blink() -> bool {
    true
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

fn default_terminal_font_family() -> String {
    String::new()
}

impl TerminalSettings {
    pub fn resolve_theme(&self, name: &str) -> TerminalTheme {
        // Check explicit themes
        if let Some(t) = self.themes.iter().find(|t| t.name == name) {
            return t.clone();
        }
        // Fallback: build from legacy fields or defaults
        TerminalTheme {
            name: name.to_string(),
            color_scheme: default_color_scheme(),
            font_family: self
                .font_family
                .clone()
                .unwrap_or_else(default_terminal_font_family),
            font_size: default_font_size(),
            line_height: default_line_height(),
            padding: default_padding(),
            cursor_style: default_cursor_style(),
            cursor_blink: default_cursor_blink(),
            shell: self.shell.clone().unwrap_or_else(default_shell),
        }
    }
}

const DEFAULT_SETTINGS: &str = include_str!("../settings.ron");

#[derive(Resource)]
pub(crate) struct SettingsWatcher {
    rx: Mutex<mpsc::Receiver<()>>,
    path: std::path::PathBuf,
    _watcher: RecommendedWatcher,
}

#[derive(Deserialize, Default)]
struct PartialAppSettings {
    #[serde(default)]
    browser: Option<BrowserSettings>,
    #[serde(default)]
    layout: Option<LayoutSettings>,
    #[serde(default)]
    shortcuts: Option<ShortcutSettings>,
    #[serde(default)]
    terminal: Option<TerminalSettings>,
    #[serde(default)]
    auto_update: Option<bool>,
    #[serde(default)]
    agent: Option<AgentSettings>,
    #[serde(default)]
    spaces: Option<std::collections::BTreeMap<String, SpaceOverrides>>,
    #[serde(default)]
    recording: Option<RecordingSettings>,
    #[serde(default)]
    editor: Option<EditorSettings>,
    #[serde(default)]
    appearance: Option<AppearanceSettings>,
}

fn merge_over_embedded(partial: PartialAppSettings) -> AppSettings {
    let mut settings = load_embedded_settings();
    if let Some(browser) = partial.browser {
        settings.browser = browser;
    }
    if let Some(layout) = partial.layout {
        settings.layout = layout;
    }
    if let Some(shortcuts) = partial.shortcuts {
        settings.shortcuts = shortcuts;
    }
    if let Some(terminal) = partial.terminal {
        settings.terminal = Some(terminal);
    }
    if let Some(auto_update) = partial.auto_update {
        settings.auto_update = auto_update;
    }
    if let Some(agent) = partial.agent {
        settings.agent = agent;
    }
    if let Some(spaces) = partial.spaces {
        settings.spaces = spaces;
    }
    if let Some(recording) = partial.recording {
        settings.recording = recording;
    }
    if let Some(editor) = partial.editor {
        settings.editor = editor;
    }
    if let Some(appearance) = partial.appearance {
        settings.appearance = appearance;
    }
    settings
}

fn parse_settings(text: &str) -> Result<AppSettings, ron::error::SpannedError> {
    // IMPLICIT_SOME lets a sparse file write `browser: (..)` instead of
    // `browser: Some((..))` for the optional override sections.
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str::<PartialAppSettings>(text)
        .map(merge_over_embedded)
}

pub fn load_settings(mut commands: Commands) {
    // Resolve the active settings file: per-build override (~/.vmux/<profile>/
    // settings.ron) if present, else the shared ~/.vmux/settings.ron.
    let path = vmux_core::profile::settings_path();
    let parent_ready = path
        .parent()
        .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok());
    let (settings, config_path) = if parent_ready {
        let s = match std::fs::read_to_string(&path) {
            Ok(text) => match parse_settings(&text) {
                Ok(s) => s,
                Err(e) => {
                    bevy::log::warn!(
                        "Ignoring invalid config {}: {e}; using embedded defaults",
                        path.display()
                    );
                    load_embedded_settings()
                }
            },
            Err(_) => load_embedded_settings(),
        };
        (s, Some(path))
    } else {
        (load_embedded_settings(), None)
    };

    sync_layout_resources(&mut commands, &settings);
    commands.insert_resource(settings);

    // Start file watcher
    if let Some(path) = config_path {
        let (tx, rx) = mpsc::channel();
        let watch_path = path.clone();
        match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res
                && (event.kind.is_modify() || event.kind.is_create())
            {
                let _ = tx.send(());
            }
        }) {
            Ok(mut watcher) => {
                if let Err(e) =
                    watcher.watch(watch_path.parent().unwrap(), RecursiveMode::NonRecursive)
                {
                    bevy::log::warn!("Failed to watch settings dir: {e}");
                } else {
                    bevy::log::info!("Watching {} for changes", path.display());
                    commands.insert_resource(SettingsWatcher {
                        rx: Mutex::new(rx),
                        path,
                        _watcher: watcher,
                    });
                }
            }
            Err(e) => {
                bevy::log::warn!("Failed to create file watcher: {e}");
            }
        }
    }
}

pub(crate) fn reload_settings_on_change(
    watcher: Option<Res<SettingsWatcher>>,
    mut settings: ResMut<AppSettings>,
    mut layout_settings: ResMut<LayoutSettings>,
    mut confirm_close: ResMut<ConfirmCloseSettings>,
    last_hash: Res<LastSelfWriteHash>,
) {
    let Some(watcher) = watcher else { return };

    let rx = watcher.rx.lock().unwrap();
    let mut changed = false;
    while rx.try_recv().is_ok() {
        changed = true;
    }
    drop(rx);
    if !changed {
        return;
    }

    match std::fs::read_to_string(&watcher.path) {
        Ok(text) => {
            let current_hash = settings_content_hash(text.as_bytes());
            if last_hash.0 == Some(current_hash) {
                bevy::log::debug!("settings: skipping reload (matches last self-write)");
                return;
            }
            match parse_settings(&text) {
                Ok(new_settings) => {
                    bevy::log::info!("Settings reloaded from {}", watcher.path.display());
                    *layout_settings = new_settings.layout.clone();
                    confirm_close.enabled = new_settings
                        .terminal
                        .as_ref()
                        .is_none_or(|terminal| terminal.confirm_close);
                    *settings = new_settings;
                }
                Err(e) => {
                    bevy::log::warn!("Settings reload failed (parse error): {e}");
                }
            }
        }
        Err(e) => {
            bevy::log::warn!("Settings reload failed (read error): {e}");
        }
    }
}

fn load_embedded_settings() -> AppSettings {
    ron::de::from_str(DEFAULT_SETTINGS).expect("embedded settings.ron must parse")
}

fn sync_layout_resources(commands: &mut Commands, settings: &AppSettings) {
    commands.insert_resource(settings.layout.clone());
    commands.insert_resource(ConfirmCloseSettings {
        enabled: settings
            .terminal
            .as_ref()
            .is_none_or(|terminal| terminal.confirm_close),
    });
}

pub fn apply_settings_update(
    settings: &mut AppSettings,
    path: &str,
    value: serde_json::Value,
) -> Result<String, String> {
    let mut value_json =
        serde_json::to_value(&*settings).map_err(|e| format!("settings to JSON failed: {e}"))?;
    set_at_path(&mut value_json, path, value)?;
    let new_settings: AppSettings = serde_json::from_value(value_json)
        .map_err(|e| format!("invalid value for path '{path}': {e}"))?;
    let ron_bytes = sparse_settings_ron(&new_settings)?;
    *settings = new_settings;
    Ok(ron_bytes)
}

fn section_ron<T: Serialize>(value: &T) -> Result<String, String> {
    ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("RON serialize failed: {e}"))
}

/// Serialize only the top-level sections that differ from the embedded defaults,
/// so the on-disk `settings.ron` stays minimal (omitted sections fall back to
/// the embedded defaults on load via `merge_over_embedded`).
fn sparse_settings_ron(settings: &AppSettings) -> Result<String, String> {
    let default = load_embedded_settings();
    let cur =
        serde_json::to_value(settings).map_err(|e| format!("settings to JSON failed: {e}"))?;
    let def =
        serde_json::to_value(&default).map_err(|e| format!("settings to JSON failed: {e}"))?;
    let differs = |key: &str| cur.get(key) != def.get(key);
    let mut parts: Vec<String> = Vec::new();
    if differs("browser") {
        parts.push(format!("    browser: {},", section_ron(&settings.browser)?));
    }
    if differs("layout") {
        parts.push(format!("    layout: {},", section_ron(&settings.layout)?));
    }
    if differs("shortcuts") {
        parts.push(format!(
            "    shortcuts: {},",
            section_ron(&settings.shortcuts)?
        ));
    }
    if differs("terminal") {
        let terminal_ron = match &settings.terminal {
            Some(terminal) => sparse_terminal_ron(terminal, default.terminal.as_ref())?,
            None => section_ron(&settings.terminal)?,
        };
        parts.push(format!("    terminal: {terminal_ron},"));
    }
    if differs("auto_update") {
        parts.push(format!(
            "    auto_update: {},",
            section_ron(&settings.auto_update)?
        ));
    }
    if differs("agent") {
        parts.push(format!("    agent: {},", section_ron(&settings.agent)?));
    }
    if differs("spaces") {
        parts.push(format!("    spaces: {},", section_ron(&settings.spaces)?));
    }
    if differs("recording") {
        parts.push(format!(
            "    recording: {},",
            section_ron(&settings.recording)?
        ));
    }
    if differs("appearance") {
        parts.push(format!(
            "    appearance: {},",
            section_ron(&settings.appearance)?
        ));
    }
    if parts.is_empty() {
        return Ok("()\n".to_string());
    }
    Ok(format!("(\n{}\n)\n", parts.join("\n")))
}

fn leaf_ron<T: Serialize>(value: &T) -> Result<String, String> {
    ron::ser::to_string(value).map_err(|e| format!("RON serialize failed: {e}"))
}

fn sparse_terminal_ron(
    cur: &TerminalSettings,
    default: Option<&TerminalSettings>,
) -> Result<String, String> {
    let fallback;
    let def = match default {
        Some(d) => d,
        None => {
            fallback = TerminalSettings::default();
            &fallback
        }
    };
    let cur_json =
        serde_json::to_value(cur).map_err(|e| format!("settings to JSON failed: {e}"))?;
    let def_json =
        serde_json::to_value(def).map_err(|e| format!("settings to JSON failed: {e}"))?;
    let differs = |key: &str| cur_json.get(key) != def_json.get(key);

    let mut fields: Vec<String> = Vec::new();
    if differs("shell") {
        fields.push(format!("shell: {}", leaf_ron(&cur.shell)?));
    }
    if differs("font_family") {
        fields.push(format!("font_family: {}", leaf_ron(&cur.font_family)?));
    }
    if differs("default_theme") {
        fields.push(format!("default_theme: {}", leaf_ron(&cur.default_theme)?));
    }
    if differs("themes") {
        fields.push(format!(
            "themes: {}",
            sparse_themes_ron(&cur.themes, &def.themes)?
        ));
    }
    if differs("custom_themes") {
        fields.push(format!("custom_themes: {}", leaf_ron(&cur.custom_themes)?));
    }
    if differs("confirm_close") {
        fields.push(format!("confirm_close: {}", leaf_ron(&cur.confirm_close)?));
    }
    if differs("startup_dir") {
        fields.push(format!("startup_dir: {}", leaf_ron(&cur.startup_dir)?));
    }
    Ok(format!("({})", fields.join(", ")))
}

fn sparse_themes_ron(cur: &[TerminalTheme], default: &[TerminalTheme]) -> Result<String, String> {
    let mut items: Vec<String> = Vec::new();
    for theme in cur {
        let base = default.iter().find(|d| d.name == theme.name);
        items.push(sparse_theme_ron(theme, base)?);
    }
    Ok(format!("[{}]", items.join(", ")))
}

fn sparse_theme_ron(theme: &TerminalTheme, base: Option<&TerminalTheme>) -> Result<String, String> {
    let mut fields: Vec<String> = vec![format!("name: {}", leaf_ron(&theme.name)?)];
    if theme.color_scheme
        != base
            .map(|b| b.color_scheme.clone())
            .unwrap_or_else(default_color_scheme)
    {
        fields.push(format!("color_scheme: {}", leaf_ron(&theme.color_scheme)?));
    }
    if theme.font_family
        != base
            .map(|b| b.font_family.clone())
            .unwrap_or_else(default_terminal_font_family)
    {
        fields.push(format!("font_family: {}", leaf_ron(&theme.font_family)?));
    }
    if theme.font_size != base.map(|b| b.font_size).unwrap_or_else(default_font_size) {
        fields.push(format!("font_size: {}", leaf_ron(&theme.font_size)?));
    }
    if theme.line_height
        != base
            .map(|b| b.line_height)
            .unwrap_or_else(default_line_height)
    {
        fields.push(format!("line_height: {}", leaf_ron(&theme.line_height)?));
    }
    if theme.padding != base.map(|b| b.padding).unwrap_or_else(default_padding) {
        fields.push(format!("padding: {}", leaf_ron(&theme.padding)?));
    }
    if theme.cursor_style
        != base
            .map(|b| b.cursor_style.clone())
            .unwrap_or_else(default_cursor_style)
    {
        fields.push(format!("cursor_style: {}", leaf_ron(&theme.cursor_style)?));
    }
    if theme.cursor_blink
        != base
            .map(|b| b.cursor_blink)
            .unwrap_or_else(default_cursor_blink)
    {
        fields.push(format!("cursor_blink: {}", leaf_ron(&theme.cursor_blink)?));
    }
    if theme.shell != base.map(|b| b.shell.clone()).unwrap_or_else(default_shell) {
        fields.push(format!("shell: {}", leaf_ron(&theme.shell)?));
    }
    Ok(format!("({})", fields.join(", ")))
}

pub fn serialize_settings_to_json(settings: &AppSettings) -> String {
    serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string())
}

pub fn set_at_path(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("empty settings path".to_string());
    }
    let segments = parse_path_segments(path)?;
    let (last, parents) = segments
        .split_last()
        .ok_or_else(|| "empty settings path".to_string())?;

    let mut cursor = root;
    let mut walked = String::new();
    for segment in parents {
        append_segment(&mut walked, segment);
        cursor = descend(cursor, segment, &walked)?;
    }
    append_segment(&mut walked, last);
    set_leaf(cursor, last, &walked, value)
}

#[derive(Debug)]
enum PathSegment {
    Field(String),
    Index(usize),
}

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, String> {
    let mut out = Vec::new();
    for raw in path.split('.') {
        if raw.is_empty() {
            return Err(format!("empty segment in path: {path}"));
        }
        let mut chars = raw.chars();
        let mut name = String::new();
        for ch in chars.by_ref() {
            if ch == '[' {
                break;
            }
            name.push(ch);
        }
        if name.is_empty() {
            return Err(format!("missing field name before '[' in {raw}"));
        }
        out.push(PathSegment::Field(name));
        let mut tail: String = chars.collect();
        while !tail.is_empty() {
            let close = tail
                .find(']')
                .ok_or_else(|| format!("unclosed '[' in {raw}"))?;
            let idx_str = &tail[..close];
            let idx: usize = idx_str
                .parse()
                .map_err(|_| format!("non-integer index '[{idx_str}]' in {raw}"))?;
            out.push(PathSegment::Index(idx));
            tail = tail[close + 1..].to_string();
            if !tail.is_empty() && !tail.starts_with('[') {
                return Err(format!("unexpected text after ']' in {raw}: {tail}"));
            }
        }
    }
    Ok(out)
}

fn append_segment(walked: &mut String, segment: &PathSegment) {
    match segment {
        PathSegment::Field(name) => {
            if !walked.is_empty() {
                walked.push('.');
            }
            walked.push_str(name);
        }
        PathSegment::Index(i) => {
            walked.push_str(&format!("[{i}]"));
        }
    }
}

fn descend<'a>(
    cursor: &'a mut serde_json::Value,
    segment: &PathSegment,
    walked: &str,
) -> Result<&'a mut serde_json::Value, String> {
    match segment {
        PathSegment::Field(name) => cursor
            .get_mut(name.as_str())
            .ok_or_else(|| format!("unknown setting path: {walked}")),
        PathSegment::Index(i) => cursor
            .get_mut(*i)
            .ok_or_else(|| format!("unknown setting path: {walked}")),
    }
}

fn set_leaf(
    cursor: &mut serde_json::Value,
    segment: &PathSegment,
    walked: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    match segment {
        PathSegment::Field(name) => {
            let map = cursor
                .as_object_mut()
                .ok_or_else(|| format!("cannot index field on non-object at {walked}"))?;
            if !map.contains_key(name) {
                return Err(format!("unknown setting path: {walked}"));
            }
            map.insert(name.clone(), value);
            Ok(())
        }
        PathSegment::Index(i) => {
            let arr = cursor
                .as_array_mut()
                .ok_or_else(|| format!("cannot index by [{i}] on non-array at {walked}"))?;
            if *i >= arr.len() {
                return Err(format!("unknown setting path: {walked}"));
            }
            arr[*i] = value;
            Ok(())
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct LastSelfWriteHash(pub Option<u64>);

#[derive(Message, Debug, Clone)]
pub struct SettingsWriteRequest {
    pub ron_bytes: String,
}

fn settings_content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

const SETTINGS_SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// Request that the current in-memory settings be persisted to disk after a short
/// quiet period. Coalesces bursts of rapid edits (e.g. holding `cmd+`) into a single
/// write, keeping the main loop free of synchronous disk I/O per keystroke.
#[derive(Message, Debug, Clone)]
pub struct SettingsSaveRequest;

#[derive(Resource, Default)]
pub(crate) struct SettingsSaveDebounce {
    pub due: Option<Instant>,
}

pub(crate) fn request_settings_save(
    mut reader: MessageReader<SettingsSaveRequest>,
    mut debounce: ResMut<SettingsSaveDebounce>,
) {
    if reader.read().count() > 0 {
        debounce.due = Some(Instant::now() + SETTINGS_SAVE_DEBOUNCE);
    }
}

pub(crate) fn flush_settings_save(
    mut debounce: ResMut<SettingsSaveDebounce>,
    settings: Res<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
) {
    let Some(due) = debounce.due else {
        return;
    };
    if Instant::now() < due {
        return;
    }
    debounce.due = None;
    match sparse_settings_ron(&settings) {
        Ok(ron_bytes) => {
            writes.write(SettingsWriteRequest { ron_bytes });
        }
        Err(e) => bevy::log::warn!("settings: debounced save serialize failed: {e}"),
    }
}

pub(crate) fn persist_settings_to_disk(
    mut reader: MessageReader<SettingsWriteRequest>,
    watcher: Option<Res<SettingsWatcher>>,
    mut last_hash: ResMut<LastSelfWriteHash>,
) {
    for request in reader.read() {
        let Some(watcher) = watcher.as_deref() else {
            bevy::log::warn!("settings: no watcher path; cannot persist");
            continue;
        };
        let bytes = request.ron_bytes.as_bytes();
        let hash = settings_content_hash(bytes);
        last_hash.0 = Some(hash);
        if let Err(e) = atomic_write(&watcher.path, bytes) {
            bevy::log::warn!(
                "settings: failed to persist {}: {e}",
                watcher.path.display()
            );
        }
    }
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "settings path has no parent",
        )
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.persist(path)
        .map_err(|e| std::io::Error::other(format!("persist failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: default_browser_startup_url(),
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
            agent: crate::plugin::runtime::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn resolve_startup_url_returns_browser_override() {
        let mut s = base_settings();
        s.browser.startup_url = "vmux://services/".into();
        assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://services/");
    }

    #[test]
    fn resolve_startup_url_defaults_to_start() {
        let s = base_settings();
        assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
    }

    #[test]
    fn resolve_startup_url_uses_start_for_empty_browser_url() {
        let mut s = base_settings();
        s.browser.startup_url.clear();
        assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
    }

    #[test]
    fn resolve_startup_url_treats_legacy_agent_default_as_start() {
        let mut s = base_settings();
        s.browser.startup_url = "vmux://agent/".into();
        assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
    }

    #[test]
    fn resolve_startup_dir_matches_slug_variant_key() {
        let dir = std::env::temp_dir();
        let mut s = base_settings();
        s.spaces.insert(
            "mistralai-dashboard".to_string(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.to_string_lossy().to_string()),
            },
        );
        assert_eq!(resolve_startup_dir(&s, "mistralai/dashboard"), dir);
    }

    #[test]
    fn embedded_settings_default_to_start() {
        let s = load_embedded_settings();
        assert_eq!(resolve_startup_url(&s, "space-1"), "vmux://start/");
    }

    #[test]
    fn resolve_startup_url_prefers_per_space_override() {
        let mut s = base_settings();
        s.browser.startup_url = "https://global.example".into();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: None,
            },
        );
        assert_eq!(resolve_startup_url(&s, "work"), "https://work.example");
        assert_eq!(resolve_startup_url(&s, "other"), "https://global.example");
    }

    #[test]
    fn resolve_startup_url_blank_per_space_falls_to_global() {
        let mut s = base_settings();
        s.browser.startup_url = "https://global.example".into();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("   ".into()),
                startup_dir: None,
            },
        );
        assert_eq!(resolve_startup_url(&s, "work"), "https://global.example");
    }

    #[test]
    fn app_settings_roundtrips_through_json() {
        let original = base_settings();
        let value = serde_json::to_value(&original).expect("serialize");
        let recovered: AppSettings = serde_json::from_value(value).expect("deserialize");
        assert_eq!(
            recovered.layout.window.padding,
            original.layout.window.padding
        );
        assert_eq!(recovered.layout.pane.gap, original.layout.pane.gap);
        assert_eq!(
            recovered.shortcuts.chord_timeout_ms,
            original.shortcuts.chord_timeout_ms
        );
        assert_eq!(recovered.auto_update, original.auto_update);
    }

    #[test]
    fn set_at_path_replaces_nested_object_value() {
        let mut root = serde_json::json!({"layout": {"pane": {"gap": 8.0}}});
        set_at_path(&mut root, "layout.pane.gap", serde_json::json!(12.0)).unwrap();
        assert_eq!(root["layout"]["pane"]["gap"], serde_json::json!(12.0));
    }

    #[test]
    fn set_at_path_replaces_array_element_field() {
        let mut root = serde_json::json!({
            "terminal": {"themes": [{"name": "default", "font_size": 14.0}]}
        });
        set_at_path(
            &mut root,
            "terminal.themes[0].font_size",
            serde_json::json!(16.0),
        )
        .unwrap();
        assert_eq!(
            root["terminal"]["themes"][0]["font_size"],
            serde_json::json!(16.0)
        );
    }

    #[test]
    fn set_at_path_top_level_leaf() {
        let mut root = serde_json::json!({"auto_update": true});
        set_at_path(&mut root, "auto_update", serde_json::json!(false)).unwrap();
        assert_eq!(root["auto_update"], serde_json::json!(false));
    }

    #[test]
    fn set_at_path_unknown_key_errors() {
        let mut root = serde_json::json!({"layout": {}});
        let err = set_at_path(&mut root, "layout.nope", serde_json::json!(1)).unwrap_err();
        assert!(
            err.contains("layout.nope"),
            "error must mention path: {err}"
        );
    }

    #[test]
    fn set_at_path_array_out_of_bounds_errors() {
        let mut root = serde_json::json!({"themes": [{"font_size": 14.0}]});
        let err =
            set_at_path(&mut root, "themes[5].font_size", serde_json::json!(16.0)).unwrap_err();
        assert!(err.contains("themes[5]"), "error must mention path: {err}");
    }

    #[test]
    fn set_at_path_empty_path_errors() {
        let mut root = serde_json::json!({});
        assert!(set_at_path(&mut root, "", serde_json::json!(1)).is_err());
    }

    #[test]
    fn apply_settings_update_changes_pane_gap_and_returns_ron() {
        let mut settings = base_settings();
        let ron_bytes =
            apply_settings_update(&mut settings, "layout.pane.gap", serde_json::json!(16.0))
                .expect("apply ok");
        assert_eq!(settings.layout.pane.gap, 16.0);
        assert!(ron_bytes.contains("gap"));
        assert!(ron_bytes.contains("16"));
        let reparsed: AppSettings = ron::de::from_str(&ron_bytes).expect("RON parses");
        assert_eq!(reparsed.layout.pane.gap, 16.0);
    }

    #[test]
    fn apply_settings_update_changes_top_level_bool() {
        let mut settings = base_settings();
        apply_settings_update(&mut settings, "auto_update", serde_json::json!(true)).unwrap();
        assert!(settings.auto_update);
    }

    #[test]
    fn apply_settings_update_unknown_path_errors_without_mutating() {
        let mut settings = base_settings();
        let original_gap = settings.layout.pane.gap;
        let err =
            apply_settings_update(&mut settings, "layout.nope", serde_json::json!(1)).unwrap_err();
        assert!(err.contains("layout.nope"));
        assert_eq!(settings.layout.pane.gap, original_gap);
    }

    #[test]
    fn apply_settings_update_type_mismatch_errors_without_mutating() {
        let mut settings = base_settings();
        let original_auto = settings.auto_update;
        let err = apply_settings_update(&mut settings, "auto_update", serde_json::json!("yes"))
            .unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(settings.auto_update, original_auto);
    }

    #[test]
    fn content_hash_is_deterministic() {
        let h1 = settings_content_hash(b"hello");
        let h2 = settings_content_hash(b"hello");
        let h3 = settings_content_hash(b"world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn app_settings_spaces_roundtrip_through_ron() {
        let mut s = base_settings();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: Some("/tmp/work".into()),
            },
        );
        let ron = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: AppSettings = ron::de::from_str(&ron).unwrap();
        assert_eq!(
            back.spaces["work"].startup_url.as_deref(),
            Some("https://work.example")
        );
        assert_eq!(
            back.spaces["work"].startup_dir.as_deref(),
            Some("/tmp/work")
        );
    }

    #[test]
    fn embedded_settings_have_empty_spaces_and_no_global_startup_dir() {
        let s = load_embedded_settings();
        assert!(s.spaces.is_empty());
        assert!(
            s.terminal
                .as_ref()
                .and_then(|t| t.startup_dir.as_ref())
                .is_none()
        );
    }

    #[test]
    fn embedded_default_theme_shell_is_portable() {
        let s = load_embedded_settings();
        let terminal = s.terminal.expect("embedded settings define terminal");
        let shell = terminal.resolve_theme(&terminal.default_theme).shell;
        assert_eq!(shell, default_shell());
    }

    #[test]
    fn resolve_startup_dir_prefers_per_space_then_global_then_builtin() {
        let per = tempfile::tempdir().unwrap();
        let glob = tempfile::tempdir().unwrap();
        let mut s = base_settings();
        s.terminal = Some(TerminalSettings {
            startup_dir: Some(glob.path().to_string_lossy().into()),
            ..Default::default()
        });
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(per.path().to_string_lossy().into()),
            },
        );
        assert_eq!(resolve_startup_dir(&s, "work"), per.path());
        assert_eq!(resolve_startup_dir(&s, "other"), glob.path());
        s.terminal = None;
        assert_eq!(
            resolve_startup_dir(&s, "space-1"),
            vmux_core::profile::space_dir("space-1")
        );
    }

    #[test]
    fn resolve_startup_dir_invalid_per_space_cascades_to_valid_global() {
        let glob = tempfile::tempdir().unwrap();
        let mut s = base_settings();
        s.terminal = Some(TerminalSettings {
            startup_dir: Some(glob.path().to_string_lossy().into()),
            ..Default::default()
        });
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some("/no/such/dir/xyz-vmux".into()),
            },
        );
        assert_eq!(resolve_startup_dir(&s, "work"), glob.path());
    }

    #[test]
    fn resolve_startup_dir_all_invalid_falls_through_to_builtin() {
        let mut s = base_settings();
        s.terminal = Some(TerminalSettings {
            startup_dir: Some("/no/such/global/xyz-vmux".into()),
            ..Default::default()
        });
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some("/no/such/dir/xyz-vmux".into()),
            },
        );
        assert_eq!(
            resolve_startup_dir(&s, "work"),
            vmux_core::profile::space_dir("work")
        );
    }

    #[test]
    fn parse_settings_merges_sparse_over_embedded() {
        let s = parse_settings(r#"(browser: (startup_url: "https://x.example"))"#).unwrap();
        assert_eq!(s.browser.startup_url, "https://x.example");
        // omitted sections come from the embedded defaults, NOT the plainer serde
        // field defaults (embedded leader is "b"; serde default would be "g").
        assert_eq!(s.shortcuts.leader.key, "b");
        assert_eq!(s.layout.radius, 8.0);
    }

    #[test]
    fn parse_settings_empty_uses_embedded_defaults() {
        let s = parse_settings("()").unwrap();
        assert_eq!(s.shortcuts.leader.key, "b");
        assert_eq!(s.browser.startup_url, "vmux://start/");
    }

    #[test]
    fn apply_settings_update_writes_only_changed_section() {
        let mut settings = parse_settings("()").unwrap();
        let ron = apply_settings_update(
            &mut settings,
            "browser.startup_url",
            serde_json::json!("https://x.example"),
        )
        .unwrap();
        assert!(ron.contains("browser"));
        assert!(ron.contains("https://x.example"));
        // untouched heavy sections stay out of the file
        assert!(!ron.contains("shortcuts"));
        assert!(!ron.contains("themes"));
        // and reload merges them back from the embedded defaults
        let reloaded = parse_settings(&ron).unwrap();
        assert_eq!(reloaded.browser.startup_url, "https://x.example");
        assert_eq!(reloaded.shortcuts.leader.key, "b");
    }

    #[test]
    fn request_settings_save_sets_due() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SettingsSaveDebounce>()
            .add_message::<SettingsSaveRequest>()
            .add_systems(Update, request_settings_save);
        app.world_mut()
            .resource_mut::<Messages<SettingsSaveRequest>>()
            .write(SettingsSaveRequest);
        app.update();
        assert!(app.world().resource::<SettingsSaveDebounce>().due.is_some());
    }

    #[test]
    fn flush_writes_after_due_elapses() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(base_settings())
            .insert_resource(SettingsSaveDebounce {
                due: Some(Instant::now() - Duration::from_secs(1)),
            })
            .add_message::<SettingsWriteRequest>()
            .add_systems(Update, flush_settings_save);
        app.update();
        let writes = app
            .world_mut()
            .resource_mut::<Messages<SettingsWriteRequest>>()
            .drain()
            .count();
        assert_eq!(writes, 1);
        assert!(app.world().resource::<SettingsSaveDebounce>().due.is_none());
    }

    #[test]
    fn flush_skips_before_due() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(base_settings())
            .insert_resource(SettingsSaveDebounce {
                due: Some(Instant::now() + Duration::from_secs(60)),
            })
            .add_message::<SettingsWriteRequest>()
            .add_systems(Update, flush_settings_save);
        app.update();
        let writes = app
            .world_mut()
            .resource_mut::<Messages<SettingsWriteRequest>>()
            .drain()
            .count();
        assert_eq!(writes, 0);
        assert!(app.world().resource::<SettingsSaveDebounce>().due.is_some());
    }

    #[test]
    fn sparse_save_omits_terminal_when_unchanged() {
        let s = load_embedded_settings();
        let ron = sparse_settings_ron(&s).unwrap();
        assert!(
            !ron.contains("terminal"),
            "unchanged terminal must be omitted: {ron}"
        );
    }

    #[test]
    fn embedded_settings_bind_tab_nav_to_leader() {
        let s = load_embedded_settings();
        let leader_key = |cmd: &str| -> Option<String> {
            s.shortcuts.bindings.iter().find_map(|e| match &e.binding {
                ShortcutDef::Leader(combo) if e.command == cmd => Some(combo.key.clone()),
                _ => None,
            })
        };

        assert_eq!(
            leader_key("open_in_new_tab").as_deref(),
            Some("c"),
            "leader c must create a new tab"
        );
        assert_eq!(
            leader_key("next_tab").as_deref(),
            Some("n"),
            "leader n must select the next tab"
        );
        assert_eq!(
            leader_key("prev_tab").as_deref(),
            Some("p"),
            "leader p must select the previous tab"
        );
        assert_eq!(
            leader_key("open_in_new_stack"),
            None,
            "leader c is rebound from new stack to new tab"
        );
    }

    #[test]
    fn sparse_save_omits_default_equal_theme_fields() {
        let mut s = load_embedded_settings();
        s.terminal
            .as_mut()
            .unwrap()
            .themes
            .iter_mut()
            .find(|t| t.name == "default")
            .unwrap()
            .font_size = 12.0;

        let ron = sparse_settings_ron(&s).unwrap();
        assert!(ron.contains("font_size"), "changed field persisted: {ron}");
        assert!(ron.contains("12"), "changed value persisted: {ron}");
        assert!(
            !ron.contains("font_family"),
            "default-equal font_family must be omitted: {ron}"
        );
        assert!(
            !ron.contains("color_scheme"),
            "default-equal color_scheme must be omitted: {ron}"
        );
        assert!(
            !ron.contains("cursor_style"),
            "default-equal cursor_style must be omitted: {ron}"
        );

        let reloaded = parse_settings(&ron).unwrap();
        let theme = reloaded.terminal.unwrap().resolve_theme("default");
        assert_eq!(theme.font_size, 12.0);
        assert_eq!(theme.font_family, default_terminal_font_family());
    }

    #[test]
    fn sparse_save_keeps_genuinely_overridden_field() {
        let mut s = load_embedded_settings();
        s.terminal
            .as_mut()
            .unwrap()
            .themes
            .iter_mut()
            .find(|t| t.name == "default")
            .unwrap()
            .font_family = "Menlo".to_string();

        let ron = sparse_settings_ron(&s).unwrap();
        assert!(
            ron.contains("Menlo"),
            "explicit override must be persisted: {ron}"
        );
        let reloaded = parse_settings(&ron).unwrap();
        assert_eq!(
            reloaded
                .terminal
                .unwrap()
                .resolve_theme("default")
                .font_family,
            "Menlo"
        );
    }

    #[test]
    fn color_scheme_defaults_to_device() {
        assert_eq!(ColorScheme::default(), ColorScheme::Device);
    }

    #[test]
    fn appearance_absent_falls_back_to_device() {
        let s = parse_settings("()").expect("parse empty");
        assert_eq!(s.appearance.mode, ColorScheme::Device);
    }

    #[test]
    fn appearance_round_trips_through_ron() {
        let s = parse_settings("(appearance: (mode: light))").expect("parse light");
        assert_eq!(s.appearance.mode, ColorScheme::Light);
        let s = parse_settings("(appearance: (mode: dark))").expect("parse dark");
        assert_eq!(s.appearance.mode, ColorScheme::Dark);
    }

    #[test]
    fn sparse_omits_default_appearance_and_emits_changed() {
        let s = load_embedded_settings();
        assert!(!sparse_settings_ron(&s).unwrap().contains("appearance"));
        let mut s = s;
        s.appearance.mode = ColorScheme::Dark;
        let out = sparse_settings_ron(&s).unwrap();
        assert!(
            out.contains("appearance"),
            "changed appearance persisted: {out}"
        );
        assert!(out.contains("dark"), "mode value persisted: {out}");
        assert_eq!(
            parse_settings(&out).unwrap().appearance.mode,
            ColorScheme::Dark
        );
    }
}
