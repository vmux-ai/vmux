use bevy::prelude::*;
use directories::ProjectDirs;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::sync::{Mutex, mpsc};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_settings)
            .add_systems(Update, reload_settings_on_change);
    }
}

#[derive(Clone, Debug, Deserialize, Resource)]
pub struct AppSettings {
    pub browser: BrowserSettings,
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
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

#[derive(Clone, Debug, Deserialize)]
pub struct LayoutSettings {
    pub window: WindowSettings,
    pub pane: PaneSettings,
    #[serde(default)]
    pub side_sheet: SideSheetSettings,
    #[serde(default)]
    pub focus_ring: FocusRingSettings,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SideSheetSettings {
    #[serde(default = "default_side_sheet_width")]
    pub width: f32,
}

impl Default for SideSheetSettings {
    fn default() -> Self {
        Self {
            width: default_side_sheet_width(),
        }
    }
}

fn default_side_sheet_width() -> f32 {
    280.0
}

#[derive(Clone, Debug, Deserialize)]
pub struct WindowSettings {
    pub padding: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FocusRingColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Default for FocusRingColor {
    fn default() -> Self {
        Self {
            r: 0.52,
            g: 0.52,
            b: 0.58,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FocusRingGlow {
    #[serde(default = "default_outline_glow_spread")]
    pub spread: f32,
    #[serde(default = "default_outline_glow_intensity")]
    pub intensity: f32,
}

impl Default for FocusRingGlow {
    fn default() -> Self {
        Self {
            spread: default_outline_glow_spread(),
            intensity: default_outline_glow_intensity(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FocusRingGradient {
    #[serde(default = "default_outline_gradient_enabled")]
    pub enabled: bool,
    #[serde(default = "default_outline_gradient_speed")]
    pub speed: f32,
    #[serde(default = "default_outline_gradient_cycles")]
    pub cycles: f32,
    #[serde(default = "default_outline_gradient_accent")]
    pub accent: FocusRingColor,
}

impl Default for FocusRingGradient {
    fn default() -> Self {
        Self {
            enabled: default_outline_gradient_enabled(),
            speed: default_outline_gradient_speed(),
            cycles: default_outline_gradient_cycles(),
            accent: default_outline_gradient_accent(),
        }
    }
}

fn default_outline_gradient_enabled() -> bool {
    true
}

fn default_outline_gradient_speed() -> f32 {
    1.0
}

fn default_outline_gradient_cycles() -> f32 {
    2.0
}

fn default_outline_gradient_accent() -> FocusRingColor {
    FocusRingColor {
        r: 0.15,
        g: 0.55,
        b: 1.0,
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FocusRingSettings {
    #[serde(default = "default_outline_width")]
    pub width: f32,
    #[serde(default)]
    pub color: FocusRingColor,
    #[serde(default)]
    pub glow: FocusRingGlow,
    #[serde(default)]
    pub gradient: FocusRingGradient,
}

impl Default for FocusRingSettings {
    fn default() -> Self {
        Self {
            width: default_outline_width(),
            color: FocusRingColor::default(),
            glow: FocusRingGlow::default(),
            gradient: FocusRingGradient::default(),
        }
    }
}

fn default_outline_width() -> f32 {
    2.0
}

fn default_outline_glow_spread() -> f32 {
    3.0
}

fn default_outline_glow_intensity() -> f32 {
    0.35
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneSettings {
    pub gap: f32,
    pub radius: f32,
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

pub fn load_settings(mut commands: Commands) {
    let (settings, config_path) = if let Some(proj) = ProjectDirs::from("ai", "vmux", "desktop") {
        let dir = proj.config_dir();
        if std::fs::create_dir_all(dir).is_err() {
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

    commands.insert_resource(settings);

    // Start file watcher
    if let Some(path) = config_path {
        let (tx, rx) = mpsc::channel();
        let watch_path = path.clone();
        match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    let _ = tx.send(());
                }
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
