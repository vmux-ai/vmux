use bevy_app::{App, Update};
use bevy_ecs::prelude::*;

use crate::connection::{KeyRequest, Rejected};
use crate::{Connection, Failure, Set};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, refuse.in_set(Set::Dispatch));
    }
}

fn refuse(
    mut requests: MessageReader<KeyRequest>,
    active: Query<(), With<Connection>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if active.contains(request.entity) {
            commands
                .entity(request.entity)
                .insert(Rejected(Failure::Storage(
                    "secure account credentials are unavailable on this platform".into(),
                )));
        }
    }
}
