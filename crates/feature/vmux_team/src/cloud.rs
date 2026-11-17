use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use uuid::Uuid;
use vmux_account::{
    Account, Authorization, Binding, CancelConnectRequest, CloudAvailable, ConnectRequest,
    Connection, ConnectionError, DisconnectRequest, Disconnecting, Failure, FailureKind,
    MutationCompleted, Owner, Phase, Profile, Root, Set, SyncEnabled, SyncRequest,
};
use vmux_api::team::{
    ProfileRow, TeamAccountCancelRequest, TeamAccountConnectRequest, TeamAccountDisconnectRequest,
    TeamAccountError, TeamAccountState, TeamProfileSyncRequest,
};
use vmux_ecs::{Active, ProfileId, Ready, Revision, Uuid as Id};

use crate::host::Team;

pub(crate) struct AccountLinkPlugin;

impl bevy_app::Plugin for AccountLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            TeamAccountConnectRequest,
            TeamAccountDisconnectRequest,
            TeamAccountCancelRequest,
            TeamProfileSyncRequest,
        )>::default())
            .add_observer(connect)
            .add_observer(disconnect)
            .add_observer(cancel)
            .add_observer(select)
            .add_systems(Update, complete.after(Set::Reply).before(Set::Publish));
    }
}

#[derive(Component)]
struct PendingSync {
    operation_id: Uuid,
    profile_id: Uuid,
}

#[derive(SystemParam)]
pub(crate) struct Projection<'w, 's> {
    roots: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static ConnectionError>,
            Has<Disconnecting>,
            Option<&'static PendingSync>,
            Has<CloudAvailable>,
        ),
        (With<Root>, With<Active>, With<Ready>),
    >,
    accounts: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Id,
            &'static Revision,
            Option<&'static Binding>,
        ),
        With<Account>,
    >,
    profiles: Query<
        'w,
        's,
        (
            &'static Id,
            &'static ProfileId,
            &'static Owner,
            Option<&'static SyncEnabled>,
        ),
        With<Profile>,
    >,
    connections: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static Id,
            &'static Phase,
            Option<&'static Authorization>,
        ),
        With<Connection>,
    >,
}

impl Projection<'_, '_> {
    pub(crate) fn state(&self, active: Option<&ProfileRow>) -> Option<TeamAccountState> {
        let (root, error, disconnecting, pending_sync, available) = self.roots.single().ok()?;
        if !available {
            return None;
        }
        let (account, _, id, revision, binding) = self
            .accounts
            .iter()
            .find(|(_, owner, _, _, _)| owner.parent() == root)?;
        let mut state = TeamAccountState {
            account_id: id.0.clone(),
            cloud_available: available,
            binding_revision: revision.0,
            server_account_id: binding.and_then(|binding| binding.account_id.clone()),
            pending: disconnecting,
            sync_pending: pending_sync.is_some(),
            error: error.map(|error| match error.kind() {
                FailureKind::AccountMismatch => TeamAccountError::AccountMismatch,
                FailureKind::AccessDenied | FailureKind::NotAuthenticated => {
                    TeamAccountError::AccessDenied
                }
                FailureKind::AuthorizationExpired => TeamAccountError::Expired,
                FailureKind::NotLinked => TeamAccountError::NotLinked,
                FailureKind::Storage | FailureKind::InvalidStore => TeamAccountError::SaveFailed,
                _ => TeamAccountError::Unavailable,
            }),
            ..TeamAccountState::default()
        };
        for (owner, id, phase, authorization) in &self.connections {
            if owner.parent() != root {
                continue;
            }
            state.connected = *phase == Phase::Connected;
            state.pending |= *phase != Phase::Connected;
            state.connection_id = Some(id.0.clone());
            state.can_cancel = *phase != Phase::Saving && *phase != Phase::Connected;
            if let Some(authorization) = authorization {
                state.user_code = Some(authorization.user_code.clone());
                state.verification_uri = Some(authorization.verification_uri.clone());
            }
        }
        for (id, directory, owner, enabled) in &self.profiles {
            if owner.0 != account || active.is_none_or(|active| active.id != directory.0) {
                continue;
            }
            state.profile_id = Some(id.0.clone());
            state.sync_enabled = enabled.is_some_and(|enabled| enabled.0);
        }
        Some(state)
    }

    fn root(&self, expected: &str) -> Option<(Entity, Uuid)> {
        let (root, _, disconnecting, pending, _) = self.roots.single().ok()?;
        if disconnecting || pending.is_some() {
            return None;
        }
        for (_, owner, id, _, _) in &self.accounts {
            if owner.parent() == root && id.0 == expected {
                return Some((root, id.0.parse().ok()?));
            }
        }
        None
    }
}

fn connect(
    input: On<UiInput<TeamAccountConnectRequest>>,
    views: Query<(), With<Team>>,
    projection: Projection,
    mut requests: MessageWriter<ConnectRequest>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    if let Some((root, account_id)) = projection.root(&input.event().payload.account_id) {
        requests.write(ConnectRequest { root, account_id });
    }
}

fn disconnect(
    input: On<UiInput<TeamAccountDisconnectRequest>>,
    views: Query<(), With<Team>>,
    projection: Projection,
    mut requests: MessageWriter<DisconnectRequest>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    if let Some((root, account_id)) = projection.root(&input.event().payload.account_id) {
        requests.write(DisconnectRequest { root, account_id });
    }
}

fn cancel(
    input: On<UiInput<TeamAccountCancelRequest>>,
    views: Query<(), With<Team>>,
    projection: Projection,
    mut requests: MessageWriter<CancelConnectRequest>,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    let payload = &input.event().payload;
    let Some((root, account_id)) = projection.root(&payload.account_id) else {
        return;
    };
    let Ok(connection_id) = Uuid::parse_str(&payload.connection_id) else {
        return;
    };
    requests.write(CancelConnectRequest {
        root,
        account_id,
        connection_id,
    });
}

fn select(
    input: On<UiInput<TeamProfileSyncRequest>>,
    views: Query<(), With<Team>>,
    projection: Projection,
    mut requests: MessageWriter<SyncRequest>,
    mut commands: Commands,
) {
    if !views.contains(input.event().webview) {
        return;
    }
    let payload = &input.event().payload;
    let Some((root, account_id)) = projection.root(&payload.account_id) else {
        return;
    };
    let Ok(profile_id) = Uuid::parse_str(&payload.profile_id) else {
        return;
    };
    let operation_id = Uuid::new_v4();
    commands
        .entity(root)
        .insert(PendingSync {
            operation_id,
            profile_id,
        })
        .remove::<ConnectionError>();
    requests.write(SyncRequest {
        root,
        operation_id,
        account_id,
        binding_revision: payload.binding_revision,
        profile_id,
        enabled: payload.enabled,
    });
}

fn complete(
    mut results: MessageReader<MutationCompleted>,
    pending: Query<(Entity, &PendingSync), With<Root>>,
    mut commands: Commands,
) {
    for result in results.read() {
        for (root, pending) in &pending {
            if root != result.root || pending.operation_id != result.operation_id {
                continue;
            }
            commands.entity(root).remove::<PendingSync>();
            if result
                .result
                .as_ref()
                .is_ok_and(|receipt| receipt.entity_id != pending.profile_id)
            {
                commands
                    .entity(root)
                    .insert(ConnectionError(Failure::invalid_store()));
                continue;
            }
            if let Err(error) = &result.result {
                commands.entity(root).insert(ConnectionError(error.clone()));
            }
        }
    }
}
