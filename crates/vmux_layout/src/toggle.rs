use crate::Open;
use crate::header::Header;
use crate::settings::LayoutSettings;
use crate::side_sheet::SideSheet;
use crate::window::VmuxWindow;
use bevy::prelude::*;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, ToggleLayoutCommand};

/// Tracks whether the layout CEF shell (header + side sheet) is currently hidden.
#[derive(Resource, Default, Debug)]
pub struct LayoutHidden(pub bool);

#[derive(Resource, Debug, Default)]
pub struct LayoutTransition {
    started_at: Option<std::time::Instant>,
}

impl LayoutTransition {
    pub fn is_animating(&self) -> bool {
        self.started_at
            .is_some_and(|started| transition_active(started, std::time::Instant::now()))
    }

    fn start(&mut self) {
        self.started_at = Some(std::time::Instant::now());
    }
}

pub const LAYOUT_TRANSITION_SECONDS: f32 = 0.24;

fn transition_active(started: std::time::Instant, now: std::time::Instant) -> bool {
    now.saturating_duration_since(started).as_secs_f32() < LAYOUT_TRANSITION_SECONDS
}

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutHidden>()
            .init_resource::<LayoutTransition>()
            .add_systems(Update, handle_toggle.in_set(ReadAppCommands));
    }
}

fn handle_toggle(
    mut reader: MessageReader<AppCommand>,
    mut hidden: ResMut<LayoutHidden>,
    mut transition: ResMut<LayoutTransition>,
    settings: Res<LayoutSettings>,
    header_q: Query<Entity, With<Header>>,
    sidesheet_q: Query<Entity, With<SideSheet>>,
    mut window_q: Query<&mut Node, With<VmuxWindow>>,
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
        transition.start();

        for mut node in &mut window_q {
            node.padding.top = Val::Px(if hidden.0 {
                settings.window.pad_top()
            } else {
                0.0
            });
            node.padding.left = Val::Px(if hidden.0 {
                settings.window.pad_left()
            } else {
                0.0
            });
        }

        for entity in header_q.iter().chain(sidesheet_q.iter()) {
            if hidden.0 {
                commands.entity(entity).remove::<Open>();
            } else {
                commands.entity(entity).insert(Open);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_window_expires() {
        let started = std::time::Instant::now();
        assert!(transition_active(
            started,
            started + std::time::Duration::from_millis(120)
        ));
        assert!(!transition_active(
            started,
            started + std::time::Duration::from_millis(240)
        ));
    }

    #[test]
    fn toggle_commits_layout_once_without_per_frame_geometry() {
        let source = include_str!("toggle.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or_default();

        assert!(!production.contains("animate_layout_transition"));
        assert!(!production.contains("time.delta_secs"));
        assert!(production.contains("node.padding.top"));
        assert!(production.contains("commands.entity(entity).remove::<Open>()"));
    }
}
