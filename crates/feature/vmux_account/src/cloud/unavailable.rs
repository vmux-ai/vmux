use bevy_app::{App, Update};
use bevy_ecs::prelude::*;

use crate::cloud::transport::NativePeer;
use crate::connection::{KeyRequest, Rejected};
use crate::control::{StoredKeyPrepared, StoredKeyRequest};
use crate::{Connection, Failure, Set};

pub(crate) struct UnavailablePlugin;

impl bevy_app::Plugin for UnavailablePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (refuse, refuse_stored).in_set(Set::Dispatch));
    }
}

fn refuse_stored(
    mut requests: MessageReader<StoredKeyRequest>,
    active: Query<(), With<NativePeer>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if active.contains(request.entity) {
            commands
                .entity(request.entity)
                .insert(StoredKeyPrepared(Some(Err(
                    "secure account credentials are unavailable on this platform".into(),
                ))));
        }
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
                .insert(Rejected(Failure::storage(
                    "secure account credentials are unavailable on this platform".into(),
                )));
        }
    }
}
