use crate::Open;
use crate::header::Header;
use crate::side_sheet::SideSheet;
use crate::window::VmuxWindow;
use bevy::prelude::*;
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
    mut window_q: Query<&mut Node, With<VmuxWindow>>,
) {
    let pad = crate::event::WINDOW_PAD_PX;
    let (top, left) = if hidden.0 { (pad, pad) } else { (0.0, 0.0) };
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
}
