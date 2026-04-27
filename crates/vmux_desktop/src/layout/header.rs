use super::{HeaderState, Open};
use crate::command::{AppCommand, HeaderCommand, ReadAppCommands};
use bevy::prelude::*;
use vmux_header::{HEADER_HEIGHT_PX, Header};

pub(crate) struct HeaderLayoutPlugin;

impl Plugin for HeaderLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_header_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                sync_header_visibility.before(bevy::ui::UiSystems::Layout),
            );
    }
}

fn handle_header_toggle(
    mut reader: MessageReader<AppCommand>,
    header_q: Query<(Entity, Has<Open>), With<Header>>,
    state_q: Query<(Entity, Has<Open>), With<HeaderState>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        if matches!(cmd, AppCommand::Header(HeaderCommand::Toggle)) {
            for (entity, is_open) in &header_q {
                if is_open {
                    commands.entity(entity).remove::<Open>();
                } else {
                    commands.entity(entity).insert(Open);
                }
            }
            for (entity, is_open) in &state_q {
                if is_open {
                    commands.entity(entity).remove::<Open>();
                } else {
                    commands.entity(entity).insert(Open);
                }
            }
        }
    }
}

fn sync_header_visibility(
    mut header_q: Query<(&mut Visibility, &mut Node), With<Header>>,
    added: Query<Entity, (With<Header>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((mut vis, mut node)) = header_q.get_mut(entity) {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.height = Val::Px(HEADER_HEIGHT_PX);
        }
    }

    for entity in removed.read() {
        if let Ok((mut vis, mut node)) = header_q.get_mut(entity) {
            *vis = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}
