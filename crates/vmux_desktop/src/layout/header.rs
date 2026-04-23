use crate::command::{AppCommand, HeaderCommand, ReadAppCommands};
use bevy::prelude::*;
use vmux_header::{Header, HEADER_HEIGHT_PX};

pub(crate) struct HeaderLayoutPlugin;

impl Plugin for HeaderLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HeaderOpen(true))
            .add_systems(Update, handle_header_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                sync_header_visibility.before(bevy::ui::UiSystems::Layout),
            );
    }
}

#[derive(Resource)]
pub(crate) struct HeaderOpen(pub bool);

fn handle_header_toggle(
    mut reader: MessageReader<AppCommand>,
    mut open: ResMut<HeaderOpen>,
) {
    for cmd in reader.read() {
        if matches!(cmd, AppCommand::Header(HeaderCommand::Toggle)) {
            open.0 = !open.0;
        }
    }
}

fn sync_header_visibility(
    open: Res<HeaderOpen>,
    mut header_q: Query<(&mut Visibility, &mut Node), With<Header>>,
) {
    if !open.is_changed() {
        return;
    }

    for (mut vis, mut node) in &mut header_q {
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.height = Val::Px(HEADER_HEIGHT_PX);
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}
