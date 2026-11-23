use std::hash::{Hash, Hasher};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::page::PageReady;
use vmux_setting::{
    AppSettings, SettingsSaveRequest,
    themes::{TerminalColorScheme, TerminalColorSchemes},
};

use crate::{Terminal, event::TermThemeEvent};

pub struct TerminalThemePlugin;

impl Plugin for TerminalThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::contract::TerminalContractPlugin)
            .init_resource::<AppliedTerminalTheme>()
            .add_systems(Update, font_size.after(vmux_command::ReadCommandRequests))
            .add_systems(Update, sync.after(font_size));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFontSizeChange {
    Increase,
    Decrease,
    Reset,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalFontSizeCommand {
    pub change: TerminalFontSizeChange,
}

impl TerminalFontSizeCommand {
    pub fn increase() -> Self {
        Self {
            change: TerminalFontSizeChange::Increase,
        }
    }

    pub fn decrease() -> Self {
        Self {
            change: TerminalFontSizeChange::Decrease,
        }
    }

    pub fn reset() -> Self {
        Self {
            change: TerminalFontSizeChange::Reset,
        }
    }
}

#[derive(Resource, Default)]
struct AppliedTerminalTheme(Option<u64>);

impl AppliedTerminalTheme {
    fn signature(theme: &vmux_setting::TerminalTheme, colors: &TerminalColorScheme) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        colors.foreground.hash(&mut hasher);
        colors.background.hash(&mut hasher);
        colors.cursor.hash(&mut hasher);
        colors.ansi.hash(&mut hasher);
        theme.font_size.to_bits().hash(&mut hasher);
        theme.line_height.to_bits().hash(&mut hasher);
        theme.padding.to_bits().hash(&mut hasher);
        theme.font_family.hash(&mut hasher);
        theme.cursor_style.hash(&mut hasher);
        theme.cursor_blink.hash(&mut hasher);
        hasher.finish()
    }
}

fn font_size(
    mut reader: MessageReader<TerminalFontSizeCommand>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    for cmd in reader.read() {
        let Some(terminal) = settings.terminal.as_ref() else {
            continue;
        };
        let name = terminal.default_theme.clone();
        let idx = match terminal.themes.iter().position(|theme| theme.name == name) {
            Some(idx) => idx,
            None => {
                let resolved = terminal.resolve_theme(&name);
                let terminal = settings.terminal.as_mut().unwrap();
                terminal.themes.push(resolved);
                terminal.themes.len() - 1
            }
        };
        let terminal = settings.terminal.as_mut().unwrap();
        let current = terminal.themes[idx].font_size;
        let next = match cmd.change {
            TerminalFontSizeChange::Increase => (current + 1.0).min(40.0),
            TerminalFontSizeChange::Decrease => (current - 1.0).max(6.0),
            TerminalFontSizeChange::Reset => 14.0,
        };
        if next == current {
            continue;
        }
        terminal.themes[idx].font_size = next;
        saves.write(SettingsSaveRequest);
    }
}

fn sync(
    terminals: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    newly_ready: Query<Entity, (With<Terminal>, Changed<PageReady>)>,
    browsers: Option<NonSend<Browsers>>,
    settings: Res<AppSettings>,
    color_schemes: Option<Res<TerminalColorSchemes>>,
    scheme: Option<Single<&vmux_setting::ResolvedColorScheme>>,
    mut applied: ResMut<AppliedTerminalTheme>,
    mut commands: Commands,
) {
    let (Some(browsers), Some(color_schemes)) = (browsers, color_schemes) else {
        return;
    };
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let theme = terminal_settings.resolve_theme(&terminal_settings.default_theme);
    let dark = scheme
        .map(|scheme| matches!(scheme.0, vmux_setting::ResolvedScheme::Dark))
        .unwrap_or(true);
    let colors = color_schemes.resolve(&theme.color_scheme, &terminal_settings.custom_themes);
    let colors = color_schemes.for_appearance(colors, dark, &terminal_settings.custom_themes);
    let hash = AppliedTerminalTheme::signature(&theme, &colors);

    let theme_changed = applied.0 != Some(hash);
    if !theme_changed && new_terminals.is_empty() && newly_ready.is_empty() {
        return;
    }
    applied.0 = Some(hash);

    let event = TermThemeEvent {
        foreground: colors.foreground,
        background: colors.background,
        cursor: colors.cursor,
        ansi: colors.ansi,
        font_family: theme.font_family.clone(),
        font_size: theme.font_size,
        line_height: theme.line_height,
        padding: theme.padding,
        cursor_style: theme.cursor_style,
        cursor_blink: theme.cursor_blink,
    };
    let targets: Vec<Entity> = if theme_changed {
        terminals.iter().collect()
    } else {
        new_terminals.iter().chain(newly_ready.iter()).collect()
    };

    for entity in targets {
        if browsers.can_emit_to(&entity) {
            commands.trigger(
                vmux_ecs::UiStateWrite::<vmux_ecs::event::TerminalUiState>::from_event(
                    entity, &event,
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;

    use super::*;

    struct ThemeFixture;

    impl ThemeFixture {
        fn terminal_theme(font_size: f32) -> vmux_setting::TerminalTheme {
            vmux_setting::TerminalTheme {
                name: "default".to_string(),
                color_scheme: "catppuccin-mocha".to_string(),
                font_family: "JetBrainsMono Nerd Font".to_string(),
                font_size,
                line_height: 1.2,
                padding: 4.0,
                cursor_style: vmux_api::terminal::CursorStyle::Block,
                cursor_blink: true,
                shell: "/bin/sh".to_string(),
            }
        }

        fn run_font_size(start: f32, command: TerminalFontSizeCommand) -> (f32, usize) {
            let settings = AppSettings {
                terminal: Some(vmux_setting::TerminalSettings {
                    default_theme: "default".to_string(),
                    themes: vec![Self::terminal_theme(start)],
                    ..Default::default()
                }),
                ..Default::default()
            };
            Self::run_with_settings(settings, command)
        }

        fn run_with_settings(
            settings: AppSettings,
            command: TerminalFontSizeCommand,
        ) -> (f32, usize) {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, TerminalThemePlugin))
                .insert_resource(settings)
                .add_message::<SettingsSaveRequest>();
            app.world_mut()
                .resource_mut::<Messages<TerminalFontSizeCommand>>()
                .write(command);
            app.update();
            let size = app
                .world()
                .resource::<AppSettings>()
                .terminal
                .as_ref()
                .unwrap()
                .themes
                .iter()
                .find(|theme| theme.name == "default")
                .unwrap()
                .font_size;
            let saves = app
                .world_mut()
                .resource_mut::<Messages<SettingsSaveRequest>>()
                .drain()
                .count();
            (size, saves)
        }
    }

    #[test]
    fn font_size_materializes_missing_default_theme() {
        let settings = AppSettings {
            terminal: Some(vmux_setting::TerminalSettings {
                default_theme: "default".to_string(),
                themes: Vec::new(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (size, saves) =
            ThemeFixture::run_with_settings(settings, TerminalFontSizeCommand::increase());
        assert_eq!(size, 15.0);
        assert_eq!(saves, 1);
    }

    #[test]
    fn font_size_increase_steps_up_and_persists() {
        let (size, writes) = ThemeFixture::run_font_size(14.0, TerminalFontSizeCommand::increase());
        assert_eq!(size, 15.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_decrease_steps_down_and_persists() {
        let (size, writes) = ThemeFixture::run_font_size(14.0, TerminalFontSizeCommand::decrease());
        assert_eq!(size, 13.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_increase_clamps_at_40() {
        let (size, _) = ThemeFixture::run_font_size(40.0, TerminalFontSizeCommand::increase());
        assert_eq!(size, 40.0);
    }

    #[test]
    fn font_size_decrease_clamps_at_6() {
        let (size, _) = ThemeFixture::run_font_size(6.0, TerminalFontSizeCommand::decrease());
        assert_eq!(size, 6.0);
    }

    #[test]
    fn font_size_reset_returns_to_14() {
        let (size, writes) = ThemeFixture::run_font_size(20.0, TerminalFontSizeCommand::reset());
        assert_eq!(size, 14.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn theme_signature_changes_with_font_size() {
        let colors = TerminalColorScheme {
            name: "test".to_string(),
            light: None,
            dark: None,
            foreground: [255, 255, 255].into(),
            background: [0, 0, 0].into(),
            cursor: [255, 255, 255].into(),
            ansi: [[0, 0, 0]; 16].into(),
        };
        let small = ThemeFixture::terminal_theme(14.0);
        let large = ThemeFixture::terminal_theme(15.0);
        assert_ne!(
            AppliedTerminalTheme::signature(&small, &colors),
            AppliedTerminalTheme::signature(&large, &colors)
        );
    }
}
