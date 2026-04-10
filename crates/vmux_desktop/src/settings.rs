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
}

#[derive(Clone, Debug, Deserialize)]
pub struct BrowserSettings {
    pub startup_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LayoutSettings {
    pub window: WindowSettings,
    pub pane: PaneSettings,
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
