use crate::Open;
use crate::header::Header;
use crate::settings::LayoutSettings;
use crate::side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth};
use crate::window::VmuxWindow;
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, ToggleLayoutCommand};

/// Tracks whether the layout CEF shell (header + side sheet) is currently hidden.
#[derive(Resource, Default, Debug)]
pub struct LayoutHidden(pub bool);

#[derive(Resource, Debug)]
pub struct LayoutTransition {
    hidden_fraction: f32,
    target_hidden: bool,
}

impl Default for LayoutTransition {
    fn default() -> Self {
        Self {
            hidden_fraction: 0.0,
            target_hidden: false,
        }
    }
}

impl LayoutTransition {
    pub fn is_animating(&self) -> bool {
        self.hidden_fraction != if self.target_hidden { 1.0 } else { 0.0 }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LayoutChromeTransitionSet;

const LAYOUT_TRANSITION_SECONDS: f32 = 0.28;

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutHidden>()
            .init_resource::<LayoutTransition>()
            .add_systems(Update, handle_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                animate_layout_transition
                    .in_set(LayoutChromeTransitionSet)
                    .before(UiSystems::Layout),
            );
    }
}

fn animate_layout_transition(
    time: Res<Time>,
    mut transition: ResMut<LayoutTransition>,
    settings: Res<LayoutSettings>,
    mut window_q: Query<&mut Node, With<VmuxWindow>>,
    mut header_q: Query<
        (Entity, Has<Open>, &mut Node),
        (With<Header>, Without<VmuxWindow>, Without<SideSheet>),
    >,
    mut side_sheet_q: Query<
        (Entity, &SideSheetPosition, Has<Open>, &mut Node),
        (With<SideSheet>, Without<VmuxWindow>, Without<Header>),
    >,
    side_sheet_width: Res<SideSheetWidth>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let target = if transition.target_hidden { 1.0 } else { 0.0 };
    let was_animating = transition.hidden_fraction != target;
    transition.hidden_fraction = advance_transition(
        transition.hidden_fraction,
        target,
        time.delta_secs() / LAYOUT_TRANSITION_SECONDS,
    );
    let hidden = smootherstep(transition.hidden_fraction);
    let visible = 1.0 - hidden;

    for mut node in &mut window_q {
        node.padding.top = Val::Px(settings.window.pad_top() * hidden);
        node.padding.left = Val::Px(settings.window.pad_left() * hidden);
    }
    for (entity, open, mut node) in &mut header_q {
        node.height = Val::Px(crate::event::CEF_RESERVED_HEIGHT_PX * visible);
        if transition.target_hidden && transition.hidden_fraction >= 1.0 && open {
            commands.entity(entity).remove::<Open>();
        }
    }
    for (entity, position, open, mut node) in &mut side_sheet_q {
        if *position != SideSheetPosition::Left {
            continue;
        }
        node.width = Val::Px(side_sheet_width.0 * visible);
        if transition.target_hidden && transition.hidden_fraction >= 1.0 && open {
            commands.entity(entity).remove::<Open>();
        }
    }

    if was_animating && let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn advance_transition(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

fn smootherstep(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn handle_toggle(
    mut reader: MessageReader<AppCommand>,
    mut hidden: ResMut<LayoutHidden>,
    mut transition: ResMut<LayoutTransition>,
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
        transition.target_hidden = hidden.0;

        if !hidden.0 {
            for entity in header_q.iter().chain(sidesheet_q.iter()) {
                commands.entity(entity).insert(Open);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_transition_runs_before_ui_layout() {
        let source = include_str!("toggle.rs");
        let plugin_build = source
            .split("impl Plugin for TogglePlugin")
            .nth(1)
            .and_then(|tail| tail.split("fn animate_layout_transition").next())
            .unwrap_or_default();

        assert!(plugin_build.contains(".add_systems(\n                PostUpdate,"));
        assert!(plugin_build.contains("animate_layout_transition"));
        assert!(plugin_build.contains(".before(UiSystems::Layout)"));
    }

    #[test]
    fn transition_advances_and_reverses_without_jumping() {
        assert_eq!(advance_transition(0.25, 1.0, 0.1), 0.35);
        assert_eq!(advance_transition(0.35, 0.0, 0.1), 0.25);
        assert_eq!(advance_transition(0.95, 1.0, 0.1), 1.0);
        assert_eq!(advance_transition(0.05, 0.0, 0.1), 0.0);
    }

    #[test]
    fn smootherstep_keeps_transition_endpoints() {
        assert_eq!(smootherstep(0.0), 0.0);
        assert_eq!(smootherstep(1.0), 1.0);
        assert_eq!(smootherstep(0.5), 0.5);
    }
}
