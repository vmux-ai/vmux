use bevy::prelude::*;
use directories::ProjectDirs;
use serde::Deserialize;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_settings_file);
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
pub struct PaneSettings {
    pub gap: f32,
    pub radius: f32,
    #[serde(default = "default_pane_border")]
    pub border: f32,
}

fn default_pane_border() -> f32 {
    2.0
}

const DEFAULT_SETTINGS: &str = include_str!("settings.ron");

fn load_settings_file(mut commands: Commands) {
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
