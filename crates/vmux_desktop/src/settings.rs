use bevy::prelude::*;
use directories::ProjectDirs;
use serde::Deserialize;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_settings);
    }
}

#[derive(Clone, Debug, Deserialize, Resource)]
pub struct AppSettings {
    pub browser: BrowserSettings,
    pub layout: LayoutSettings,
    #[serde(default)]
    pub keybindings: KeyBindingSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct KeyBindingSettings {
    #[serde(default = "default_leader")]
    pub leader: KeyComboDef,
    #[serde(default = "default_chord_timeout_ms")]
    pub chord_timeout_ms: u64,
    #[serde(default)]
    pub bindings: Vec<KeyBindingEntry>,
}

impl Default for KeyBindingSettings {
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
pub struct KeyBindingEntry {
    pub command: String,
    pub binding: KeyBindingDef,
}

#[derive(Clone, Debug, Deserialize)]
pub enum KeyBindingDef {
    Direct(KeyComboDef),
    Chord(KeyComboDef, KeyComboDef),
    /// Chord binding that uses the configured leader key as prefix.
    Leader(KeyComboDef),
}

impl KeyBindingDef {
    pub fn to_key_binding(&self) -> Option<crate::keybinding::KeyBinding> {
        match self {
            KeyBindingDef::Direct(combo) => {
                Some(crate::keybinding::KeyBinding::Direct(combo.to_key_combo()?))
            }
            KeyBindingDef::Chord(prefix, second) => Some(crate::keybinding::KeyBinding::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            KeyBindingDef::Leader(_second) => {
                // Resolved in init_keybindings with the configured leader
                None
            }
        }
    }

    pub fn to_key_binding_with_leader(
        &self,
        leader: &crate::keybinding::KeyCombo,
    ) -> Option<crate::keybinding::KeyBinding> {
        match self {
            KeyBindingDef::Direct(combo) => {
                Some(crate::keybinding::KeyBinding::Direct(combo.to_key_combo()?))
            }
            KeyBindingDef::Chord(prefix, second) => Some(crate::keybinding::KeyBinding::Chord(
                prefix.to_key_combo()?,
                second.to_key_combo()?,
            )),
            KeyBindingDef::Leader(second) => Some(crate::keybinding::KeyBinding::Chord(
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
    pub fn to_key_combo(&self) -> Option<crate::keybinding::KeyCombo> {
        let resolved = crate::keybinding::resolve_key(&self.key)?;
        Some(crate::keybinding::KeyCombo {
            key: resolved.key,
            modifiers: crate::keybinding::Modifiers {
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
    pub shell: String,
    #[serde(default = "default_terminal_font_family")]
    pub font_family: String,
}

fn default_terminal_font_family() -> String {
    "JetBrainsMono Nerd Font".to_string()
}

#[derive(Clone, Debug, Deserialize)]
pub struct LayoutSettings {
    pub window: WindowSettings,
    pub pane: PaneSettings,
    #[serde(default)]
    pub side_sheet: SideSheetSettings,
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
pub struct PaneOutlineColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Default for PaneOutlineColor {
    fn default() -> Self {
        Self {
            r: 0.52,
            g: 0.52,
            b: 0.58,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneOutlineGlow {
    #[serde(default = "default_outline_glow_spread")]
    pub spread: f32,
    #[serde(default = "default_outline_glow_intensity")]
    pub intensity: f32,
}

impl Default for PaneOutlineGlow {
    fn default() -> Self {
        Self {
            spread: default_outline_glow_spread(),
            intensity: default_outline_glow_intensity(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneOutlineGradient {
    #[serde(default = "default_outline_gradient_enabled")]
    pub enabled: bool,
    #[serde(default = "default_outline_gradient_speed")]
    pub speed: f32,
    #[serde(default = "default_outline_gradient_cycles")]
    pub cycles: f32,
    #[serde(default = "default_outline_gradient_accent")]
    pub accent: PaneOutlineColor,
}

impl Default for PaneOutlineGradient {
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

fn default_outline_gradient_accent() -> PaneOutlineColor {
    PaneOutlineColor {
        r: 0.15,
        g: 0.55,
        b: 1.0,
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneOutlineSettings {
    #[serde(default = "default_outline_width")]
    pub width: f32,
    #[serde(default)]
    pub color: PaneOutlineColor,
    #[serde(default)]
    pub glow: PaneOutlineGlow,
    #[serde(default)]
    pub gradient: PaneOutlineGradient,
}

impl Default for PaneOutlineSettings {
    fn default() -> Self {
        Self {
            width: default_outline_width(),
            color: PaneOutlineColor::default(),
            glow: PaneOutlineGlow::default(),
            gradient: PaneOutlineGradient::default(),
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
    #[serde(default)]
    pub outline: PaneOutlineSettings,
}

const DEFAULT_SETTINGS: &str = include_str!("settings.ron");

pub fn load_settings(mut commands: Commands) {
    let settings = if let Some(proj) = ProjectDirs::from("dev", "vmux", "vmux-desktop-next") {
        let dir = proj.config_dir();
        if std::fs::create_dir_all(dir).is_err() {
            load_embedded_settings()
        } else {
            let path = dir.join("settings.ron");
            match std::fs::read_to_string(&path) {
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
            }
        }
    } else {
        load_embedded_settings()
    };

    commands.insert_resource(settings);
}

fn load_embedded_settings() -> AppSettings {
    ron::de::from_str(DEFAULT_SETTINGS).expect("embedded settings.ron must parse")
}
