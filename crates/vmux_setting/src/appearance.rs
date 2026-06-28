use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use bevy::window::{WindowTheme, WindowThemeChanged};

use crate::ColorScheme;

/// Concrete light/dark choice after resolving [`ColorScheme`] against the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedScheme {
    Light,
    Dark,
}

/// The OS appearance as last observed by the host. `None` until known.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SystemAppearance(pub Option<ResolvedScheme>);

/// The resolved app scheme driving host-generated colors (editor, terminal).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedColorScheme(pub ResolvedScheme);

impl Default for ResolvedColorScheme {
    fn default() -> Self {
        Self(ResolvedScheme::Dark)
    }
}

/// Sent whenever the resolved scheme changes.
#[derive(Message, Clone, Copy, Debug)]
pub struct ColorSchemeChanged(pub ResolvedScheme);

/// Pure resolution: explicit Light/Dark win; Device follows the OS, defaulting
/// to Dark when the OS appearance is unknown.
pub fn resolve(mode: ColorScheme, system: Option<ResolvedScheme>) -> ResolvedScheme {
    match mode {
        ColorScheme::Light => ResolvedScheme::Light,
        ColorScheme::Dark => ResolvedScheme::Dark,
        ColorScheme::Device => system.unwrap_or(ResolvedScheme::Dark),
    }
}

/// Track winit OS theme changes into [`SystemAppearance`].
pub fn track_window_theme(
    mut reader: MessageReader<WindowThemeChanged>,
    mut system: ResMut<SystemAppearance>,
) {
    for ev in reader.read() {
        let scheme = match ev.theme {
            WindowTheme::Light => ResolvedScheme::Light,
            WindowTheme::Dark => ResolvedScheme::Dark,
        };
        if system.0 != Some(scheme) {
            system.0 = Some(scheme);
        }
    }
}

/// Recompute [`ResolvedColorScheme`] from the setting + OS; emit on change.
pub fn update_resolved_color_scheme(
    settings: Res<crate::AppSettings>,
    system: Res<SystemAppearance>,
    mut resolved: ResMut<ResolvedColorScheme>,
    mut changed: MessageWriter<ColorSchemeChanged>,
) {
    if !settings.is_changed() && !system.is_changed() {
        return;
    }
    let next = resolve(settings.appearance.mode, system.0);
    if resolved.0 != next {
        resolved.0 = next;
        changed.write(ColorSchemeChanged(next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_modes_ignore_os() {
        assert_eq!(
            resolve(ColorScheme::Light, Some(ResolvedScheme::Dark)),
            ResolvedScheme::Light
        );
        assert_eq!(
            resolve(ColorScheme::Dark, Some(ResolvedScheme::Light)),
            ResolvedScheme::Dark
        );
    }

    #[test]
    fn device_follows_os_and_defaults_dark() {
        assert_eq!(
            resolve(ColorScheme::Device, Some(ResolvedScheme::Light)),
            ResolvedScheme::Light
        );
        assert_eq!(
            resolve(ColorScheme::Device, Some(ResolvedScheme::Dark)),
            ResolvedScheme::Dark
        );
        assert_eq!(resolve(ColorScheme::Device, None), ResolvedScheme::Dark);
    }
}
