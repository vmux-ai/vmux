use crate::Open;
use crate::header::Header;
use crate::side_sheet::SideSheet;
use crate::window::VmuxWindow;
use bevy::prelude::*;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, ZenCommand};

#[derive(Resource, Default, Debug)]
pub struct ZenMode {
    pub active: bool,
}

pub struct ZenPlugin;

impl Plugin for ZenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZenMode>()
            .add_systems(Update, handle_zen_toggle.in_set(ReadAppCommands))
            .add_systems(Update, sync_window_padding_to_zen);
    }
}

/// In zen mode the chrome (header + side sheet) is hidden, so the pane fills
/// the full window. Apply WINDOW_PAD_PX on all four sides to keep the pane
/// off the system window edge. In normal mode keep top + left at 0 (pane
/// is flush against the chrome / system edge) and pad only the right +
/// bottom corners.
fn sync_window_padding_to_zen(zen: Res<ZenMode>, mut window_q: Query<&mut Node, With<VmuxWindow>>) {
    let pad = crate::event::WINDOW_PAD_PX;
    let (top, left) = if zen.active { (pad, pad) } else { (0.0, 0.0) };
    for mut node in &mut window_q {
        let want_top = Val::Px(top);
        let want_left = Val::Px(left);
        if node.padding.top != want_top || node.padding.left != want_left {
            node.padding.top = want_top;
            node.padding.left = want_left;
        }
    }
}

fn handle_zen_toggle(
    mut reader: MessageReader<AppCommand>,
    mut zen: ResMut<ZenMode>,
    header_q: Query<Entity, With<Header>>,
    sidesheet_q: Query<Entity, With<SideSheet>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        if !matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Zen(ZenCommand::Toggle))
        ) {
            continue;
        }
        zen.active = !zen.active;

        if zen.active {
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
