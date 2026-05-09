use crate::Open;
use crate::header::Header;
use crate::side_sheet::SideSheet;
use bevy::prelude::*;
use vmux_command::{AppCommand, ReadAppCommands, ZenCommand};

#[derive(Resource, Default, Debug)]
pub struct ZenMode {
    pub active: bool,
}

pub struct ZenPlugin;

impl Plugin for ZenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZenMode>()
            .add_systems(Update, handle_zen_toggle.in_set(ReadAppCommands));
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
        if !matches!(cmd, AppCommand::Zen(ZenCommand::Toggle)) {
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
