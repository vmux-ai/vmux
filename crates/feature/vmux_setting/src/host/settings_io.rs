use std::path::{Path, PathBuf};

use vmux_ui::i18n::Locale;

use super::runtime::{AppSettings, AppearanceSettings};

pub(super) struct SettingsFileIo;

impl SettingsFileIo {
    pub(super) fn read_and_path(
        defaults: &AppSettings,
        path: PathBuf,
    ) -> (AppSettings, Option<PathBuf>) {
        let parent_ready = path
            .parent()
            .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok());
        if parent_ready {
            let settings = match std::fs::read_to_string(&path) {
                Ok(text) => match AppSettings::parse_with_defaults(&text, defaults) {
                    Ok(settings) => settings,
                    Err(error) => {
                        bevy::log::warn!(
                            "Ignoring invalid config {}: {error}; using feature defaults",
                            path.display()
                        );
                        defaults.clone()
                    }
                },
                Err(_) => defaults.clone(),
            };
            (settings, Some(path))
        } else {
            (defaults.clone(), None)
        }
    }

    pub(super) fn read_appearance() -> AppearanceSettings {
        let path = vmux_ecs::profile::ProfilePaths::current().settings();
        let Ok(source) = std::fs::read_to_string(path) else {
            return AppearanceSettings::default();
        };
        AppearanceSettings::parse_ron(&source)
    }

    pub(super) fn load_locale(locale: &Locale, directory: Option<&Path>) {
        let Some(directory) = directory else {
            return;
        };
        let tag = locale.as_str();
        for tag in [tag, tag.split('-').next().unwrap_or(tag)] {
            let Ok(source) = std::fs::read_to_string(directory.join(format!("{tag}.ftl"))) else {
                continue;
            };
            let _ = locale.register_catalog(&source);
            break;
        }
    }
}
