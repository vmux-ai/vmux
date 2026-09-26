use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::page::PageReady;
use vmux_setting::{AppSettings, SettingsSaveRequest};

use crate::{Terminal, event::TermThemeEvent};

pub struct TerminalThemePlugin;

impl Plugin for TerminalThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::contract::TerminalContractPlugin)
            .add_systems(
                Update,
                handle_terminal_font_size.after(vmux_command::ReadCommandRequests),
            )
            .add_systems(Update, sync_terminal_theme.after(handle_terminal_font_size));
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFontSizeCommand {
    Increase,
    Decrease,
    Reset,
}

fn handle_terminal_font_size(
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
        let next = match cmd {
            TerminalFontSizeCommand::Increase => (current + 1.0).min(40.0),
            TerminalFontSizeCommand::Decrease => (current - 1.0).max(6.0),
            TerminalFontSizeCommand::Reset => 14.0,
        };
        if next == current {
            continue;
        }
        terminal.themes[idx].font_size = next;
        saves.write(SettingsSaveRequest);
    }
}

fn theme_signature(
    theme: &vmux_setting::TerminalTheme,
    colors: &vmux_setting::themes::TerminalColorScheme,
) -> u64 {
    use std::hash::{Hash, Hasher};
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

fn scheme_for_appearance(name: &str, dark: bool) -> &str {
    match (name, dark) {
        ("catppuccin-mocha" | "catppuccin-frappe" | "catppuccin-macchiato", false) => {
            "catppuccin-latte"
        }
        ("catppuccin-latte", true) => "catppuccin-mocha",
        ("solarized-dark", false) => "solarized-light",
        ("solarized-light", true) => "solarized-dark",
        (other, _) => other,
    }
}

fn sync_terminal_theme(
    terminals: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    newly_ready: Query<Entity, (With<Terminal>, Changed<PageReady>)>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    scheme: Option<Res<vmux_setting::ResolvedColorScheme>>,
    mut commands: Commands,
    mut last_theme_hash: Local<u64>,
) {
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let theme = terminal_settings.resolve_theme(&terminal_settings.default_theme);
    let dark = scheme
        .map(|scheme| matches!(scheme.0, vmux_setting::ResolvedScheme::Dark))
        .unwrap_or(true);
    let scheme_name = scheme_for_appearance(&theme.color_scheme, dark);
    let colors = vmux_setting::themes::resolve_theme(scheme_name, &terminal_settings.custom_themes);
    let hash = theme_signature(&theme, &colors);

    let theme_changed = hash != *last_theme_hash;
    if !theme_changed && new_terminals.is_empty() && newly_ready.is_empty() {
        return;
    }
    *last_theme_hash = hash;

    let event = TermThemeEvent {
        foreground: colors.foreground.into(),
        background: colors.background.into(),
        cursor: colors.cursor.into(),
        ansi: colors.ansi.into(),
        font_family: theme.font_family.clone(),
        font_size: theme.font_size,
        line_height: theme.line_height,
        padding: theme.padding,
        cursor_style: theme.cursor_style.clone(),
        cursor_blink: theme.cursor_blink,
    };
    let targets: Vec<Entity> = if theme_changed {
        terminals.iter().collect()
    } else {
        new_terminals.iter().chain(newly_ready.iter()).collect()
    };

    for entity in targets {
        if browsers.can_emit_to(&entity) {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_core::event::TerminalUiState,
            >::from_event(entity, &event));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_theme(font_size: f32) -> vmux_setting::TerminalTheme {
        vmux_setting::TerminalTheme {
            name: "default".to_string(),
            color_scheme: "catppuccin-mocha".to_string(),
            font_family: "JetBrainsMono Nerd Font".to_string(),
            font_size,
            line_height: 1.2,
            padding: 4.0,
            cursor_style: "block".to_string(),
            cursor_blink: true,
            shell: "/bin/sh".to_string(),
        }
    }

    fn settings_with_font(font_size: f32) -> AppSettings {
        let mut settings = AppSettings::embedded();
        settings.terminal = Some(vmux_setting::TerminalSettings {
            default_theme: "default".to_string(),
            themes: vec![terminal_theme(font_size)],
            ..Default::default()
        });
        settings
    }

    fn run_font_size_command(start: f32, command: TerminalFontSizeCommand) -> (f32, usize) {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings_with_font(start))
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<SettingsSaveRequest>()
            .add_systems(Update, handle_terminal_font_size);
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
            .themes[0]
            .font_size;
        let saves = app
            .world_mut()
            .resource_mut::<Messages<SettingsSaveRequest>>()
            .drain()
            .count();
        (size, saves)
    }

    #[test]
    fn font_size_materializes_missing_default_theme() {
        use bevy::ecs::message::Messages;
        let mut settings = AppSettings::embedded();
        settings.terminal = Some(vmux_setting::TerminalSettings {
            default_theme: "default".to_string(),
            themes: Vec::new(),
            ..Default::default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<SettingsSaveRequest>()
            .add_systems(Update, handle_terminal_font_size);
        app.world_mut()
            .resource_mut::<Messages<TerminalFontSizeCommand>>()
            .write(TerminalFontSizeCommand::Increase);
        app.update();

        let terminal = app
            .world()
            .resource::<AppSettings>()
            .terminal
            .clone()
            .unwrap();
        let theme = terminal
            .themes
            .iter()
            .find(|theme| theme.name == "default")
            .unwrap();
        assert_eq!(theme.font_size, 15.0);
        let saves = app
            .world_mut()
            .resource_mut::<Messages<SettingsSaveRequest>>()
            .drain()
            .count();
        assert_eq!(saves, 1);
    }

    #[test]
    fn font_size_increase_steps_up_and_persists() {
        let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Increase);
        assert_eq!(size, 15.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_decrease_steps_down_and_persists() {
        let (size, writes) = run_font_size_command(14.0, TerminalFontSizeCommand::Decrease);
        assert_eq!(size, 13.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn font_size_increase_clamps_at_40() {
        let (size, _) = run_font_size_command(40.0, TerminalFontSizeCommand::Increase);
        assert_eq!(size, 40.0);
    }

    #[test]
    fn font_size_decrease_clamps_at_6() {
        let (size, _) = run_font_size_command(6.0, TerminalFontSizeCommand::Decrease);
        assert_eq!(size, 6.0);
    }

    #[test]
    fn font_size_reset_returns_to_14() {
        let (size, writes) = run_font_size_command(20.0, TerminalFontSizeCommand::Reset);
        assert_eq!(size, 14.0);
        assert_eq!(writes, 1);
    }

    #[test]
    fn theme_signature_changes_with_font_size() {
        let colors = vmux_setting::themes::resolve_theme("catppuccin-mocha", &[]);
        let small = terminal_theme(14.0);
        let large = terminal_theme(15.0);
        assert_ne!(
            theme_signature(&small, &colors),
            theme_signature(&large, &colors)
        );
    }
}
