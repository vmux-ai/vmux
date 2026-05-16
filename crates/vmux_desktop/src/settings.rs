use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, mpsc};
use vmux_layout::settings::ConfirmCloseSettings;
pub use vmux_layout::settings::LayoutSettings;
#[cfg(test)]
pub use vmux_layout::settings::{
    FocusRingSettings, PaneSettings, SideSheetSettings, WindowSettings,
};

pub(crate) struct SettingsCorePlugin;

impl Plugin for SettingsCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastSelfWriteHash>()
            .add_message::<SettingsWriteRequest>()
            .configure_sets(
                Startup,
                SettingsLoadSet.before(vmux_layout::LayoutStartupSet::Window),
            )
            .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .add_systems(Startup, load_settings.in_set(SettingsLoadSet))
            .add_systems(
                Startup,
                update_effective_startup_url
                    .after(SettingsLoadSet)
                    .before(vmux_layout::LayoutStartupSet::Post),
            )
            .add_systems(
                Startup,
                register_app_agents_from_settings.after(SettingsLoadSet),
            )
            .add_systems(
                Update,
                (persist_settings_to_disk, reload_settings_on_change).chain(),
            )
            .add_systems(Update, update_effective_startup_url);
    }
}

fn register_app_agents_from_settings(
    settings: Option<Res<AppSettings>>,
    strategies: Option<ResMut<vmux_agent::strategy::AgentStrategies>>,
) {
    let Some(settings) = settings else { return };
    let Some(mut strategies) = strategies else {
        return;
    };
    for provider_settings in &settings.agent.app_providers {
        let kind = match provider_settings.kind.as_str() {
            "vibe" => vmux_agent::AgentKind::Vibe,
            "claude" => vmux_agent::AgentKind::Claude,
            "codex" => vmux_agent::AgentKind::Codex,
            other => {
                bevy::log::warn!(
                    "agent.app_providers: unknown kind '{other}' for provider '{}'; defaulting to vibe",
                    provider_settings.provider
                );
                vmux_agent::AgentKind::Vibe
            }
        };
        for model in &provider_settings.models {
            strategies.register_app(Box::new(vmux_agent::EchoAppStrategy::new(
                provider_settings.provider.clone(),
                model.clone(),
                kind,
            )));
        }
    }
}

fn update_effective_startup_url(
    settings: Option<Res<AppSettings>>,
    mut effective: ResMut<vmux_layout::settings::EffectiveStartupUrl>,
) {
    if let Some(settings) = settings.as_ref()
        && (settings.is_changed() || effective.0.is_empty())
    {
        effective.0 = resolve_startup_url(settings);
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingsLoadSet;

#[derive(Clone, Debug, Deserialize, Serialize, Resource)]
pub struct AppSettings {
    #[allow(dead_code)]
    pub browser: BrowserSettings,
    #[serde(default)]
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub startup_url: Option<String>,
    #[serde(default = "default_agent_settings")]
    pub agent: AgentSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentSettings {
    #[serde(default)]
    pub app_providers: Vec<AppProviderSettings>,
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

pub fn resolve_startup_url(settings: &AppSettings) -> String {
    settings
        .startup_url
        .clone()
        .unwrap_or_else(|| vmux_agent::AgentKind::Vibe.cli_url_prefix())
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
    /// Chord binding that uses the configured leader key as prefix.
    Leader(KeyComboDef),
}

impl ShortcutDef {
    pub fn to_shortcut(&self) -> Option<crate::shortcut::Shortcut> {
        match self {
            ShortcutDef::Direct(combo) => {
                Some(crate::shortcut::Shortcut::Direct(combo.to_key_combo()?))
            }
            ShortcutDef::Chord(prefix, second) => Some(crate::shortcut::Shortcut::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            ShortcutDef::Leader(_second) => {
                // Resolved in init_shortcuts with the configured leader
                None
            }
        }
    }

    pub fn to_shortcut_with_leader(
        &self,
        leader: &crate::shortcut::KeyCombo,
    ) -> Option<crate::shortcut::Shortcut> {
        match self {
            ShortcutDef::Direct(combo) => {
                Some(crate::shortcut::Shortcut::Direct(combo.to_key_combo()?))
            }
            ShortcutDef::Chord(prefix, second) => Some(crate::shortcut::Shortcut::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            ShortcutDef::Leader(second) => Some(crate::shortcut::Shortcut::Chord(
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
    pub fn to_key_combo(&self) -> Option<crate::shortcut::KeyCombo> {
        let resolved = crate::shortcut::resolve_key(&self.key)?;
        Some(crate::shortcut::KeyCombo {
            key: resolved.key,
            modifiers: crate::shortcut::Modifiers {
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
    #[allow(dead_code)]
    pub startup_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalSettings {
    // Legacy fields for backward compatibility
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    // New fields
    #[serde(default = "default_theme_name")]
    pub default_theme: String,
    #[serde(default)]
    pub themes: Vec<TerminalTheme>,
    #[serde(default)]
    pub custom_themes: Vec<crate::themes::TerminalColorScheme>,
    #[serde(default = "default_true")]
    pub confirm_close: bool,
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
    "JetBrainsMono Nerd Font".to_string()
}

impl TerminalSettings {
    /// Get the effective profile, migrating legacy fields if needed.
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

const DEFAULT_SETTINGS: &str = include_str!("settings.ron");

/// Holds the file watcher and channel for settings hot-reload.
#[derive(Resource)]
struct SettingsWatcher {
    rx: Mutex<mpsc::Receiver<()>>,
    path: std::path::PathBuf,
    // Keep watcher alive -- dropping it stops watching.
    _watcher: RecommendedWatcher,
}

/// Returns the Vmux data directory (~/Library/Application Support/Vmux on macOS).
/// Matches the paths used by persistence, browser profiles, and the service.
fn data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join("Library/Application Support/Vmux"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(std::env::temp_dir().join("Vmux"))
    }
}

pub fn load_settings(mut commands: Commands) {
    let (settings, config_path) = if let Some(dir) = data_dir() {
        if std::fs::create_dir_all(&dir).is_err() {
            (load_embedded_settings(), None)
        } else {
            let path = dir.join("settings.ron");
            let s = match std::fs::read_to_string(&path) {
                Ok(text) => match ron::de::from_str::<AppSettings>(&text) {
                    Ok(s) => s,
                    Err(e) => {
                        bevy::log::warn!(
                            "Ignoring invalid config {}: {e}; using embedded defaults",
                            path.display()
                        );
                        load_embedded_settings()
                    }
                },
                Err(_) => {
                    let _ = std::fs::write(&path, DEFAULT_SETTINGS);
                    load_embedded_settings()
                }
            };
            (s, Some(path))
        }
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

fn reload_settings_on_change(
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
            match ron::de::from_str::<AppSettings>(&text) {
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

pub(crate) fn apply_settings_update(
    settings: &mut AppSettings,
    path: &str,
    value: serde_json::Value,
) -> Result<String, String> {
    let mut value_json =
        serde_json::to_value(&*settings).map_err(|e| format!("settings to JSON failed: {e}"))?;
    set_at_path(&mut value_json, path, value)?;
    let new_settings: AppSettings = serde_json::from_value(value_json)
        .map_err(|e| format!("invalid value for path '{path}': {e}"))?;
    let ron_bytes = ron::ser::to_string_pretty(&new_settings, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("RON serialize failed: {e}"))?;
    *settings = new_settings;
    Ok(ron_bytes)
}

pub(crate) fn serialize_settings_to_json(settings: &AppSettings) -> String {
    serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn set_at_path(
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

fn persist_settings_to_disk(
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
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
            agent: crate::settings::AgentSettings::default(),
        }
    }

    #[test]
    fn resolve_startup_url_returns_user_override() {
        let mut s = base_settings();
        s.startup_url = Some("vmux://services/".into());
        assert_eq!(resolve_startup_url(&s), "vmux://services/");
    }

    #[test]
    fn resolve_startup_url_defaults_to_vibe() {
        let s = base_settings();
        assert_eq!(resolve_startup_url(&s), "vmux://agent/vibe/");
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
}
