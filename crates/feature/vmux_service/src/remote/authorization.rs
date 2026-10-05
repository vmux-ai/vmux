use bevy::prelude::*;

pub(crate) use super::authorization_driver::RemoteAuthorizations;
use super::authorization_driver::{
    AuthenticateAuthorization, AuthorizationInbox, RevalidateAuthorization,
};
pub use super::authorization_driver::{
    AuthorizationOutcome, AuthorizedDevice, RelayToken, RemoteAuthorizationStore,
};

pub struct RemoteAuthorizationPlugin;

impl Plugin for RemoteAuthorizationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (receive_requests, ApplyDeferred, process_requests).chain(),
        );
    }
}

fn receive_requests(mut inbox: Single<&mut AuthorizationInbox>, mut commands: Commands) {
    while let Ok(request) = inbox.0.authenticate.try_recv() {
        commands.spawn(request);
    }
    while let Ok(request) = inbox.0.revalidate.try_recv() {
        commands.spawn(request);
    }
}

fn process_requests(
    store: Single<&RemoteAuthorizationStore>,
    mut authentications: Query<(Entity, &mut AuthenticateAuthorization)>,
    mut revalidations: Query<(Entity, &mut RevalidateAuthorization)>,
    mut commands: Commands,
) {
    for (entity, mut request) in &mut authentications {
        if let Some(response) = request.response.take() {
            let _ = response.send(store.authenticate(&request.client_id, &request.credential));
        }
        commands.entity(entity).despawn();
    }
    for (entity, mut request) in &mut revalidations {
        if let Some(response) = request.response.take() {
            let _ = response.send(store.authorizes(&request.client_id, &request.device_token));
        }
        commands.entity(entity).despawn();
    }
}
