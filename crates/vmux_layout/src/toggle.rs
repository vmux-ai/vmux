use crate::Open;
use crate::header::Header;
use crate::settings::LayoutSettings;
use crate::side_sheet::SideSheet;
use crate::window::{VmuxWindow, window_uses_full_padding};
use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryWindow};
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, ToggleLayoutCommand};

/// Tracks whether the layout CEF shell (header + side sheet) is currently hidden.
#[derive(Resource, Default, Debug)]
pub struct LayoutHidden(pub bool);

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutHidden>()
            .add_systems(Update, handle_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                sync_window_padding_to_layout_hidden.before(bevy::ui::UiSystems::Layout),
            );
    }
}

/// When the CEF shell is hidden, the pane fills the full window. Apply
/// WINDOW_PAD_PX on all four sides to keep the pane off the system window
/// edge. When visible, keep top + left at 0 (pane is flush against the
/// CEF shell / system edge) and pad only the right + bottom corners.
fn sync_window_padding_to_layout_hidden(
    hidden: Res<LayoutHidden>,
    settings: Res<LayoutSettings>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    mut window_q: Query<&mut Node, With<VmuxWindow>>,
) {
    let fullscreen = primary_window
        .single()
        .ok()
        .is_some_and(|window| window_uses_full_padding(window, &monitors));
    let (top, left) = if hidden.0 || fullscreen {
        (settings.window.pad_top(), settings.window.pad_left())
    } else {
        (0.0, 0.0)
    };
    for mut node in &mut window_q {
        let want_top = Val::Px(top);
        let want_left = Val::Px(left);
        if node.padding.top != want_top || node.padding.left != want_left {
            node.padding.top = want_top;
            node.padding.left = want_left;
        }
    }
}

fn handle_toggle(
    mut reader: MessageReader<AppCommand>,
    mut hidden: ResMut<LayoutHidden>,
    header_q: Query<Entity, With<Header>>,
    sidesheet_q: Query<Entity, With<SideSheet>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        if !matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::ToggleLayout(ToggleLayoutCommand::Toggle))
        ) {
            continue;
        }
        hidden.0 = !hidden.0;

        if hidden.0 {
            for entity in header_q.iter().chain(sidesheet_q.iter()) {
                commands.entity(entity).remove::<Open>();
            }
        } else {
            for entity in header_q.iter().chain(sidesheet_q.iter()) {
                commands.entity(entity).insert(Open);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        },
        window::VmuxWindow,
    };
    use bevy::window::{Monitor, MonitorSelection, WindowMode};

    #[test]
    fn window_padding_sync_runs_before_ui_layout() {
        let source = include_str!("toggle.rs");
        let plugin_build = source
            .split("impl Plugin for TogglePlugin")
            .nth(1)
            .and_then(|tail| tail.split("/// When").next())
            .unwrap_or_default();

        assert!(plugin_build.contains(".add_systems(\n                PostUpdate,"));
        assert!(
            plugin_build.contains(
                "sync_window_padding_to_layout_hidden.before(bevy::ui::UiSystems::Layout)"
            )
        );
    }

    #[test]
    fn hidden_layout_padding_uses_layout_window_settings() {
        let source = include_str!("toggle.rs");
        let sync_fn = source
            .split("fn sync_window_padding_to_layout_hidden")
            .nth(1)
            .and_then(|tail| tail.split("fn handle_toggle").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("settings.window.pad_top()"));
        assert!(sync_fn.contains("settings.window.pad_left()"));
    }

    #[test]
    fn fullscreen_window_padding_uses_layout_window_settings() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutHidden(false))
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 16.0,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding_to_layout_hidden);
        app.world_mut().spawn((
            Window {
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                ..default()
            },
            PrimaryWindow,
        ));
        let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(16.0));
        assert_eq!(node.padding.left, Val::Px(16.0));
    }

    #[test]
    fn maximized_window_padding_uses_layout_window_settings() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutHidden(false))
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 16.0,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding_to_layout_hidden);
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn(Monitor {
            name: None,
            physical_width: 1200,
            physical_height: 800,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: None,
            scale_factor: 1.0,
            video_modes: Vec::new(),
        });
        let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(16.0));
        assert_eq!(node.padding.left, Val::Px(16.0));
    }
}
