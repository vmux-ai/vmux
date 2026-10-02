use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use bevy::window::{WindowTheme, WindowThemeChanged};

use crate::ColorScheme;

pub struct AppearancePlugin;

impl Plugin for AppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_message::<ColorSchemeChanged>()
            .add_systems(Update, (track_window_theme, resolve).chain());
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("System appearance"),
        SystemAppearance::default(),
        ResolvedColorScheme::default(),
    ));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedScheme {
    Light,
    Dark,
}

impl ResolvedScheme {
    pub fn resolve(mode: ColorScheme, system: Option<Self>) -> Self {
        match mode {
            ColorScheme::Light => Self::Light,
            ColorScheme::Dark => Self::Dark,
            ColorScheme::Device => system.unwrap_or(Self::Dark),
        }
    }
}

#[derive(Component, Default, Clone, Copy, Debug)]
pub struct SystemAppearance(pub Option<ResolvedScheme>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedColorScheme(pub ResolvedScheme);

impl Default for ResolvedColorScheme {
    fn default() -> Self {
        Self(ResolvedScheme::Dark)
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ColorSchemeChanged(pub ResolvedScheme);

fn track_window_theme(
    mut reader: MessageReader<WindowThemeChanged>,
    mut system: Single<&mut SystemAppearance>,
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

fn resolve(
    settings: Res<crate::AppSettings>,
    system: Single<Ref<SystemAppearance>>,
    mut resolved: Single<&mut ResolvedColorScheme>,
    mut changed: MessageWriter<ColorSchemeChanged>,
) {
    if !settings.is_changed() && !system.is_changed() {
        return;
    }
    let next = ResolvedScheme::resolve(settings.appearance.mode, system.0);
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
            ResolvedScheme::resolve(ColorScheme::Light, Some(ResolvedScheme::Dark)),
            ResolvedScheme::Light
        );
        assert_eq!(
            ResolvedScheme::resolve(ColorScheme::Dark, Some(ResolvedScheme::Light)),
            ResolvedScheme::Dark
        );
    }

    #[test]
    fn device_follows_os_and_defaults_dark() {
        assert_eq!(
            ResolvedScheme::resolve(ColorScheme::Device, Some(ResolvedScheme::Light)),
            ResolvedScheme::Light
        );
        assert_eq!(
            ResolvedScheme::resolve(ColorScheme::Device, Some(ResolvedScheme::Dark)),
            ResolvedScheme::Dark
        );
        assert_eq!(
            ResolvedScheme::resolve(ColorScheme::Device, None),
            ResolvedScheme::Dark
        );
    }
}
