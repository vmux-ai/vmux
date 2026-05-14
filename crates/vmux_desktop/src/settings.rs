use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::sync::{Mutex, mpsc};
use vmux_layout::settings::ConfirmCloseSettings;
pub use vmux_layout::settings::LayoutSettings;
#[cfg(test)]
pub use vmux_layout::settings::{
    FocusRingSettings, PaneSettings, SideSheetSettings, WindowSettings,
};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
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
        .add_systems(Update, reload_settings_on_change)
        .add_systems(Update, update_effective_startup_url);
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

#[derive(Clone, Debug, Deserialize, Resource)]
pub struct AppSettings {
    #[allow(dead_code)]
    pub browser: BrowserSettings,
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub startup_url: Option<String>,
}

pub fn resolve_startup_url(settings: &AppSettings) -> String {
    settings
        .startup_url
        .clone()
        .unwrap_or_else(|| vmux_agent::AgentKind::Vibe.url_scheme().to_string())
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct ShortcutEntry {
    pub command: String,
    pub binding: ShortcutDef,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct BrowserSettings {
    #[allow(dead_code)]
    pub startup_url: String,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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
) {
    let Some(watcher) = watcher else { return };

    // Drain all pending notifications
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
        Ok(text) => match ron::de::from_str::<AppSettings>(&text) {
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
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
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
        assert_eq!(resolve_startup_url(&s), "vmux://vibe/");
    }
}
