use std::time::{Duration, Instant};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate, Startup};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, NonSendMarker, Query, SystemParam};
use dioxus::prelude::*;
use vmux_api::session::ClientOpId;
use vmux_ecs::{Operation, OperationError, OperationRetained, OperationState};
use vmux_ui::i18n::translate;

use crate::actions::{
    ComposerDraft, ComposerEffectRevision, MediaQueryState, ModelPollState, TeamPollState,
};
use crate::catalog::{ReconcileAgents, ReconcileSessions};
use crate::credentials::{Credentials, CredentialsPath};
use crate::io::{
    CloseConnectionJob, CloseConnectionReceipt, ConnectionHandle, LoadCredentialsJob,
    LoadCredentialsReceipt, OpenConnectionJob, OpenConnectionReceipt, PersistCredentialsJob,
    PersistCredentialsReceipt, ReadAgentsJob, ReadAgentsReceipt, ReadSessionsJob,
    ReadSessionsReceipt, ReadUpgradedCredentialJob, ReadUpgradedCredentialReceipt,
    RemoveCredentialsJob, RemoveCredentialsReceipt,
};
use crate::presentation::{PublishedConnectionState, PublishedSessionState};
use crate::qr_scanner;
use crate::remote::ApiError;
use crate::session::{LeaveSession, MobileRoute, SessionGeneration, StreamConnected};
use crate::{AdmitSet, DecideSet, InvalidateSet, MaterializeSet};

pub(crate) struct MobileConnectionPlugin;

impl BevyPlugin for MobileConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PairLinkChanged>()
            .add_message::<PairRequest>()
            .add_message::<ScanRequest>()
            .add_message::<ScanCompleted>()
            .add_message::<CancelScan>()
            .add_message::<PairingFailure>()
            .add_message::<DisconnectRequest>()
            .add_message::<SourceUnauthorized>()
            .add_message::<SourceInvalidated>()
            .add_systems(Startup, spawn_root)
            .add_systems(
                PreUpdate,
                (change_link, admit_scan, admit_scan_completed, admit_failure)
                    .chain()
                    .in_set(AdmitSet),
            )
            .add_systems(
                PreUpdate,
                (
                    admit_pair,
                    admit_disconnect,
                    handle_unauthorized,
                    cancel_scan,
                )
                    .chain()
                    .in_set(InvalidateSet),
            )
            .add_systems(
                PreUpdate,
                (
                    adopt_loaded_credentials,
                    adopt_opened,
                    adopt_attempt_sessions,
                    adopt_refresh_sessions,
                    adopt_agents,
                    adopt_upgraded_credential,
                    adopt_persisted_credentials,
                    adopt_removed_credentials,
                    retire_closed,
                )
                    .in_set(MaterializeSet),
            )
            .add_systems(PreUpdate, schedule_refresh.in_set(DecideSet));
    }
}

#[derive(Component)]
#[require(MobileRoute)]
pub(crate) struct MobileRoot;

#[derive(Component)]
#[require(AuthStage, CredentialPersistence)]
pub(crate) struct ConnectionSource;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AuthStage {
    #[default]
    Loading,
    Paired,
    Unpaired,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SourceEpoch(pub(crate) u64);

impl SourceEpoch {
    pub(crate) fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[derive(Component, Default)]
pub(crate) struct PairDraft(pub(crate) String);

#[derive(Component, Default)]
pub(crate) struct ConnectionReachable(pub(crate) bool);

#[derive(Component, Default)]
pub(crate) struct ConnectionPairing(pub(crate) bool);

#[derive(Component, Default)]
pub(crate) struct ConnectionNotice(pub(crate) String);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CredentialPersistence {
    #[default]
    Unknown,
    Persisted,
    Failed,
}

#[derive(Component)]
pub(crate) struct RefreshDeadline(pub(crate) Instant);

impl RefreshDeadline {
    const INTERVAL: Duration = Duration::from_secs(3);
}

impl Default for RefreshDeadline {
    fn default() -> Self {
        Self(Instant::now() + Self::INTERVAL)
    }
}

#[derive(Component, Default)]
pub(crate) struct ActiveConnectionHandle(pub(crate) Option<ConnectionHandle>);

#[derive(Component, Default)]
pub(crate) struct ScanGeneration(pub(crate) u64);

impl ScanGeneration {
    fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScanTicket {
    pub(crate) source_epoch: u64,
    pub(crate) scan_generation: u64,
}

#[derive(Component, Default)]
pub(crate) struct ActiveScan(pub(crate) Option<ScanTicket>);

#[derive(Component, Default)]
pub(crate) struct ClientOperationSequence(u64);

impl ClientOperationSequence {
    pub(crate) fn next(&mut self) -> ClientOpId {
        self.0 = self.0.wrapping_add(1).max(1);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        ClientOpId::new(format!("mobile:{timestamp}:{}", self.0))
    }
}

#[derive(Message)]
pub(crate) struct PairLinkChanged(pub(crate) String);

#[derive(Message)]
pub(crate) struct PairRequest(pub(crate) String);

#[derive(Message)]
pub(crate) struct ScanRequest;

impl ScanRequest {
    fn unavailable() -> Option<String> {
        qr_scanner::ScannerSupport::detect(&Self).unavailable()
    }

    pub(crate) fn permitted(&self) -> bool {
        vmux_app::host::Product::camera()
    }
}

#[derive(Message)]
pub(crate) struct ScanCompleted {
    pub(crate) ticket: ScanTicket,
    pub(crate) result: Result<String, String>,
}

#[derive(Message)]
pub(crate) struct CancelScan;

#[derive(Message)]
pub(crate) struct PairingFailure(pub(crate) String);

#[derive(Message)]
pub(crate) struct DisconnectRequest;

#[derive(Message)]
pub(crate) struct SourceUnauthorized {
    pub(crate) source: Entity,
    pub(crate) epoch: u64,
}

#[derive(Message)]
pub(crate) struct SourceInvalidated {
    pub(crate) source: Entity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionAttemptOrigin {
    Restore,
    Pair,
}

#[derive(Component)]
struct CredentialsLoadOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct ConnectionAttempt {
    source: Entity,
    epoch: u64,
    origin: ConnectionAttemptOrigin,
    handle: Option<ConnectionHandle>,
}

#[derive(Component)]
struct CatalogRefreshOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct CatalogAgentsOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct CredentialUpgradeOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct CredentialPersistOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct CredentialRemoveOp {
    source: Entity,
    epoch: u64,
}

#[derive(Component)]
struct ConnectionCloseOp;

#[derive(SystemParam)]
struct ConnectionOps<'w, 's> {
    commands: Commands<'w, 's>,
}

impl ConnectionOps<'_, '_> {
    fn spawn_attempt(
        &mut self,
        source: Entity,
        epoch: u64,
        origin: ConnectionAttemptOrigin,
        credentials: Credentials,
    ) {
        self.commands.spawn((
            Operation::bundle("mobile.connection.open")
                .unwrap()
                .running(),
            OperationRetained,
            ConnectionAttempt {
                source,
                epoch,
                origin,
                handle: None,
            },
            OpenConnectionJob { credentials },
        ));
    }

    fn spawn_post_connect_reads(&mut self, source: Entity, epoch: u64, handle: ConnectionHandle) {
        self.commands.spawn((
            Operation::bundle("mobile.catalog.agents")
                .unwrap()
                .running(),
            OperationRetained,
            CatalogAgentsOp { source, epoch },
            ReadAgentsJob {
                handle: handle.clone(),
            },
        ));
        self.commands.spawn((
            Operation::bundle("mobile.credentials.upgrade")
                .unwrap()
                .running(),
            OperationRetained,
            CredentialUpgradeOp { source, epoch },
            ReadUpgradedCredentialJob { handle },
        ));
    }

    fn schedule_close(&mut self, handle: ConnectionHandle) {
        handle.seal();
        self.commands.spawn((
            Operation::bundle("mobile.connection.close")
                .unwrap()
                .running(),
            OperationRetained,
            ConnectionCloseOp,
            CloseConnectionJob { handle },
        ));
    }

    fn schedule_remove_credentials(&mut self, source: Entity, epoch: u64, path: CredentialsPath) {
        self.commands.spawn((
            Operation::bundle("mobile.credentials.remove")
                .unwrap()
                .running(),
            OperationRetained,
            CredentialRemoveOp { source, epoch },
            RemoveCredentialsJob { path },
        ));
    }

    fn retire(&mut self, entity: Entity, state: OperationState) {
        self.commands
            .entity(entity)
            .remove::<OperationRetained>()
            .insert(state);
    }
}

fn spawn_root(
    roots: Query<Entity, With<MobileRoot>>,
    sources: Query<(Entity, &SourceEpoch, &CredentialsPath), With<ConnectionSource>>,
    mut commands: Commands,
) {
    let root = match roots.single() {
        Ok(entity) => entity,
        Err(_) => commands
            .spawn((
                MobileRoot,
                ClientOperationSequence::default(),
                MobileRoute::default(),
                SessionGeneration::default(),
                StreamConnected::default(),
                PublishedConnectionState::default(),
                PublishedSessionState::default(),
                ComposerDraft::default(),
                ComposerEffectRevision::default(),
                MediaQueryState::default(),
                ModelPollState::default(),
                TeamPollState::default(),
            ))
            .id(),
    };
    let (source, epoch, path) = match sources.single() {
        Ok((entity, epoch, path)) => (entity, epoch.0, path.clone()),
        Err(_) => {
            let path = CredentialsPath::default();
            let source = commands
                .spawn((
                    ChildOf(root),
                    ConnectionSource,
                    AuthStage::Loading,
                    SourceEpoch(1),
                    PairDraft::default(),
                    ConnectionReachable::default(),
                    ConnectionPairing::default(),
                    ConnectionNotice::default(),
                    CredentialPersistence::Unknown,
                    RefreshDeadline::default(),
                    ActiveConnectionHandle::default(),
                    ScanGeneration::default(),
                    ActiveScan::default(),
                    path.clone(),
                ))
                .id();
            (source, 1, path)
        }
    };
    commands.spawn((
        Operation::bundle("mobile.credentials.load")
            .unwrap()
            .running(),
        OperationRetained,
        CredentialsLoadOp { source, epoch },
        LoadCredentialsJob { path },
    ));
}

fn change_link(
    mut requests: MessageReader<PairLinkChanged>,
    mut sources: Query<(&mut PairDraft, &mut ConnectionNotice), With<ConnectionSource>>,
) {
    let Ok((mut draft, mut notice)) = sources.single_mut() else {
        return;
    };
    for request in requests.read() {
        draft.0.clone_from(&request.0);
        notice.0.clear();
    }
}

fn admit_scan(
    _main_thread: NonSendMarker,
    mut requests: MessageReader<ScanRequest>,
    mut sources: Query<
        (Entity, &SourceEpoch, &mut ScanGeneration, &mut ActiveScan),
        With<ConnectionSource>,
    >,
    mut failures: MessageWriter<PairingFailure>,
) {
    let Ok((source, epoch, mut scan_gen, mut active_scan)) = sources.single_mut() else {
        return;
    };
    for request in requests.read() {
        let ticket = ScanTicket {
            source_epoch: epoch.0,
            scan_generation: scan_gen.advance(),
        };
        active_scan.0 = Some(ticket);
        if let Err(message) = request.open_with_ticket(source, ticket) {
            active_scan.0 = None;
            failures.write(PairingFailure(message));
        }
    }
}

fn admit_scan_completed(
    mut completed: MessageReader<ScanCompleted>,
    mut sources: Query<(&SourceEpoch, &mut ActiveScan), With<ConnectionSource>>,
    mut pairs: MessageWriter<PairRequest>,
    mut failures: MessageWriter<PairingFailure>,
) {
    let Ok((epoch, mut active_scan)) = sources.single_mut() else {
        return;
    };
    for event in completed.read() {
        let valid = active_scan.0 == Some(event.ticket) && event.ticket.source_epoch == epoch.0;
        if !valid {
            continue;
        }
        active_scan.0 = None;
        match &event.result {
            Ok(link) => {
                pairs.write(PairRequest(link.clone()));
            }
            Err(message) => {
                failures.write(PairingFailure(message.clone()));
            }
        }
    }
}

fn cancel_scan(
    _main_thread: NonSendMarker,
    mut cancels: MessageReader<CancelScan>,
    mut sources: Query<(&mut ScanGeneration, &mut ActiveScan), With<ConnectionSource>>,
) {
    if cancels.read().count() == 0 {
        return;
    }
    let Ok((mut scan_gen, mut active_scan)) = sources.single_mut() else {
        return;
    };
    scan_gen.advance();
    active_scan.0 = None;
    CancelScan::cancel_native();
}

fn admit_failure(
    mut failures: MessageReader<PairingFailure>,
    mut sources: Query<(&mut ConnectionNotice, &mut ConnectionPairing), With<ConnectionSource>>,
) {
    let Ok((mut notice, mut pairing)) = sources.single_mut() else {
        return;
    };
    for failure in failures.read() {
        notice.0.clone_from(&failure.0);
        pairing.0 = false;
    }
}

type PairSourceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut SourceEpoch,
        &'static mut AuthStage,
        &'static mut PairDraft,
        &'static mut ConnectionNotice,
        &'static mut ConnectionReachable,
        &'static mut ConnectionPairing,
        &'static mut CredentialPersistence,
        &'static mut ActiveConnectionHandle,
        &'static mut ScanGeneration,
        &'static mut ActiveScan,
    ),
    With<ConnectionSource>,
>;

fn admit_pair(
    mut requests: MessageReader<PairRequest>,
    mut sources: PairSourceQuery,
    mut leaves: MessageWriter<LeaveSession>,
    mut invalidated: MessageWriter<SourceInvalidated>,
    mut ops_cmd: ConnectionOps,
) {
    let Ok((
        source,
        mut epoch,
        mut auth,
        mut draft,
        mut notice,
        mut reachable,
        mut pairing,
        mut persistence,
        mut handle,
        mut scan_gen,
        mut active_scan,
    )) = sources.single_mut()
    else {
        return;
    };
    for request in requests.read() {
        let next_epoch = epoch.advance();
        scan_gen.advance();
        active_scan.0 = None;
        CancelScan::cancel_native();
        leaves.write(LeaveSession);
        invalidated.write(SourceInvalidated { source });
        if let Some(displaced) = handle.0.take() {
            ops_cmd.schedule_close(displaced);
        }
        *auth = AuthStage::Unpaired;
        draft.0.clone_from(&request.0);
        notice.0.clear();
        reachable.0 = false;
        *persistence = CredentialPersistence::Unknown;
        let credentials = match Credentials::parse(&request.0) {
            Ok(credentials) => credentials,
            Err(message) => {
                pairing.0 = false;
                notice.0 = message;
                continue;
            }
        };
        pairing.0 = true;
        ops_cmd.spawn_attempt(
            source,
            next_epoch,
            ConnectionAttemptOrigin::Pair,
            credentials,
        );
    }
}

type DisconnectSourceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut SourceEpoch,
        &'static mut AuthStage,
        &'static mut PairDraft,
        &'static mut ConnectionNotice,
        &'static mut ConnectionReachable,
        &'static mut ConnectionPairing,
        &'static mut CredentialPersistence,
        &'static mut ActiveConnectionHandle,
        &'static mut ScanGeneration,
        &'static mut ActiveScan,
        &'static CredentialsPath,
    ),
    With<ConnectionSource>,
>;

fn admit_disconnect(
    mut requests: MessageReader<DisconnectRequest>,
    mut sources: DisconnectSourceQuery,
    mut leaves: MessageWriter<LeaveSession>,
    mut invalidated: MessageWriter<SourceInvalidated>,
    mut ops_cmd: ConnectionOps,
) {
    if requests.read().count() == 0 {
        return;
    }
    let Ok((
        source,
        mut epoch,
        mut auth,
        mut draft,
        mut notice,
        mut reachable,
        mut pairing,
        mut persistence,
        mut handle,
        mut scan_gen,
        mut active_scan,
        path,
    )) = sources.single_mut()
    else {
        return;
    };
    let next_epoch = epoch.advance();
    scan_gen.advance();
    active_scan.0 = None;
    CancelScan::cancel_native();
    leaves.write(LeaveSession);
    invalidated.write(SourceInvalidated { source });
    if let Some(displaced) = handle.0.take() {
        ops_cmd.schedule_close(displaced);
    }
    *auth = AuthStage::Unpaired;
    draft.0.clear();
    notice.0.clear();
    reachable.0 = false;
    pairing.0 = false;
    *persistence = CredentialPersistence::Unknown;
    ops_cmd.schedule_remove_credentials(source, next_epoch, path.clone());
}

fn handle_unauthorized(
    mut unauthorized: MessageReader<SourceUnauthorized>,
    mut sources: DisconnectSourceQuery,
    mut leaves: MessageWriter<LeaveSession>,
    mut invalidated: MessageWriter<SourceInvalidated>,
    mut ops_cmd: ConnectionOps,
) {
    for event in unauthorized.read() {
        let Ok((
            source,
            mut epoch,
            mut auth,
            mut draft,
            mut notice,
            mut reachable,
            mut pairing,
            mut persistence,
            mut handle,
            mut scan_gen,
            mut active_scan,
            path,
        )) = sources.get_mut(event.source)
        else {
            continue;
        };
        if epoch.0 != event.epoch {
            continue;
        }
        let next_epoch = epoch.advance();
        scan_gen.advance();
        active_scan.0 = None;
        CancelScan::cancel_native();
        leaves.write(LeaveSession);
        invalidated.write(SourceInvalidated { source });
        if let Some(displaced) = handle.0.take() {
            ops_cmd.schedule_close(displaced);
        }
        *auth = AuthStage::Unpaired;
        draft.0.clear();
        reachable.0 = false;
        pairing.0 = false;
        *persistence = CredentialPersistence::Unknown;
        notice.0 = translate("mobile-error-pairing-expired");
        ops_cmd.schedule_remove_credentials(source, next_epoch, path.clone());
    }
}

fn adopt_loaded_credentials(
    mut ops: Query<(Entity, &CredentialsLoadOp, &mut LoadCredentialsReceipt)>,
    mut sources: Query<
        (
            &mut SourceEpoch,
            &mut AuthStage,
            &mut PairDraft,
            &mut ConnectionNotice,
        ),
        With<ConnectionSource>,
    >,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<LoadCredentialsReceipt>()
            .remove::<OperationRetained>()
            .insert(OperationState::Succeeded);
        let Ok((mut epoch, mut auth, mut draft, mut notice)) = sources.get_mut(op.source) else {
            continue;
        };
        if epoch.0 != op.epoch {
            continue;
        }
        match &receipt.0 {
            Ok(Some(credentials)) => {
                draft.0 = credentials.pairing_url();
                let next_epoch = epoch.advance();
                ops_cmd.spawn_attempt(
                    op.source,
                    next_epoch,
                    ConnectionAttemptOrigin::Restore,
                    credentials.clone(),
                );
            }
            Ok(None) => {
                *auth = AuthStage::Unpaired;
            }
            Err(error) => {
                *auth = AuthStage::Unpaired;
                notice.0 = error.clone();
            }
        }
    }
}

fn adopt_opened(
    mut ops: Query<(Entity, &mut ConnectionAttempt, &OpenConnectionReceipt)>,
    mut sources: Query<
        (
            &SourceEpoch,
            &mut AuthStage,
            &mut ConnectionPairing,
            &mut ConnectionNotice,
            &CredentialsPath,
        ),
        With<ConnectionSource>,
    >,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, mut attempt, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<OpenConnectionReceipt>();
        let Ok((epoch, mut auth, mut pairing, mut notice, path)) = sources.get_mut(attempt.source)
        else {
            if let Ok(handle) = &receipt.0 {
                ops_cmd.schedule_close(handle.clone());
            }
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != attempt.epoch {
            if let Ok(handle) = &receipt.0 {
                ops_cmd.schedule_close(handle.clone());
            }
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        match &receipt.0 {
            Ok(handle) => {
                attempt.handle = Some(handle.clone());
                ops_cmd.commands.entity(entity).insert(ReadSessionsJob {
                    handle: handle.clone(),
                });
            }
            Err(error) => {
                pairing.0 = false;
                *auth = AuthStage::Unpaired;
                notice.0 = error.to_string();
                ops_cmd.schedule_remove_credentials(attempt.source, attempt.epoch, path.clone());
                ops_cmd.retire(entity, OperationState::Failed);
            }
        }
    }
}

type AttemptSourceQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SourceEpoch,
        &'static mut AuthStage,
        &'static mut PairDraft,
        &'static mut ConnectionNotice,
        &'static mut ConnectionReachable,
        &'static mut ConnectionPairing,
        &'static mut RefreshDeadline,
        &'static mut ActiveConnectionHandle,
        &'static CredentialsPath,
    ),
    With<ConnectionSource>,
>;

fn adopt_attempt_sessions(
    mut ops: Query<(Entity, &mut ConnectionAttempt, &ReadSessionsReceipt)>,
    mut sources: AttemptSourceQuery,
    mut reconcile: MessageWriter<ReconcileSessions>,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, mut attempt, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<ReadSessionsReceipt>();
        let candidate = attempt.handle.take();
        let Ok((
            epoch,
            mut auth,
            mut draft,
            mut notice,
            mut reachable,
            mut pairing,
            mut refresh_at,
            mut active_handle,
            path,
        )) = sources.get_mut(attempt.source)
        else {
            if let Some(handle) = candidate {
                ops_cmd.schedule_close(handle);
            }
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != attempt.epoch {
            if let Some(handle) = candidate {
                ops_cmd.schedule_close(handle);
            }
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        pairing.0 = false;
        let Some(handle) = candidate else {
            ops_cmd.retire(entity, OperationState::Failed);
            continue;
        };
        match &receipt.0 {
            Ok(sessions) => {
                if let Some(displaced) = active_handle.0.replace(handle.clone()) {
                    ops_cmd.schedule_close(displaced);
                }
                *auth = AuthStage::Paired;
                draft.0.clear();
                notice.0.clear();
                reachable.0 = true;
                refresh_at.0 = Instant::now() + RefreshDeadline::INTERVAL;
                reconcile.write(ReconcileSessions {
                    source: attempt.source,
                    epoch: attempt.epoch,
                    sessions: sessions.clone(),
                });
                ops_cmd.spawn_post_connect_reads(attempt.source, attempt.epoch, handle);
                ops_cmd.retire(entity, OperationState::Succeeded);
            }
            Err(ApiError::Unauthorized) => {
                ops_cmd.schedule_close(handle);
                ops_cmd.schedule_remove_credentials(attempt.source, attempt.epoch, path.clone());
                *auth = AuthStage::Unpaired;
                notice.0 = match attempt.origin {
                    ConnectionAttemptOrigin::Restore => translate("mobile-error-pairing-expired"),
                    ConnectionAttemptOrigin::Pair => translate("mobile-error-token-rejected"),
                };
                ops_cmd.retire(entity, OperationState::Failed);
            }
            Err(error) => match attempt.origin {
                ConnectionAttemptOrigin::Restore => {
                    if let Some(displaced) = active_handle.0.replace(handle) {
                        ops_cmd.schedule_close(displaced);
                    }
                    *auth = AuthStage::Paired;
                    reachable.0 = false;
                    notice.0 = error.to_string();
                    refresh_at.0 = Instant::now() + RefreshDeadline::INTERVAL;
                    ops_cmd.retire(entity, OperationState::Failed);
                }
                ConnectionAttemptOrigin::Pair => {
                    ops_cmd.schedule_close(handle);
                    *auth = AuthStage::Unpaired;
                    notice.0 = error.to_string();
                    ops_cmd.retire(entity, OperationState::Failed);
                }
            },
        }
    }
}

fn schedule_refresh(
    sources: Query<
        (
            Entity,
            &AuthStage,
            &SourceEpoch,
            &RefreshDeadline,
            &ActiveConnectionHandle,
        ),
        With<ConnectionSource>,
    >,
    refreshes: Query<&CatalogRefreshOp>,
    mut commands: Commands,
) {
    let Ok((source, auth, epoch, deadline, active_handle)) = sources.single() else {
        return;
    };
    if *auth != AuthStage::Paired || Instant::now() < deadline.0 {
        return;
    }
    if refreshes
        .iter()
        .any(|refresh| refresh.source == source && refresh.epoch == epoch.0)
    {
        return;
    }
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    commands.spawn((
        Operation::bundle("mobile.catalog.refresh")
            .unwrap()
            .running(),
        OperationRetained,
        CatalogRefreshOp {
            source,
            epoch: epoch.0,
        },
        ReadSessionsJob { handle },
    ));
}

fn adopt_refresh_sessions(
    mut ops: Query<(Entity, &CatalogRefreshOp, &ReadSessionsReceipt)>,
    mut sources: Query<
        (
            &SourceEpoch,
            &mut RefreshDeadline,
            &mut ConnectionReachable,
            &mut ConnectionNotice,
        ),
        With<ConnectionSource>,
    >,
    mut reconcile: MessageWriter<ReconcileSessions>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<ReadSessionsReceipt>();
        let Ok((epoch, mut refresh_at, mut reachable, mut notice)) = sources.get_mut(op.source)
        else {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        refresh_at.0 = Instant::now() + RefreshDeadline::INTERVAL;
        match &receipt.0 {
            Ok(sessions) => {
                reachable.0 = true;
                notice.0.clear();
                reconcile.write(ReconcileSessions {
                    source: op.source,
                    epoch: op.epoch,
                    sessions: sessions.clone(),
                });
                ops_cmd.retire(entity, OperationState::Succeeded);
            }
            Err(ApiError::Unauthorized) => {
                unauthorized.write(SourceUnauthorized {
                    source: op.source,
                    epoch: op.epoch,
                });
                ops_cmd.retire(entity, OperationState::Failed);
            }
            Err(error) => {
                reachable.0 = false;
                notice.0 = error.to_string();
                ops_cmd.retire(entity, OperationState::Failed);
            }
        }
    }
}

fn adopt_agents(
    mut ops: Query<(Entity, &CatalogAgentsOp, &ReadAgentsReceipt)>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut reconcile: MessageWriter<ReconcileAgents>,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<ReadAgentsReceipt>();
        let Ok(epoch) = sources.get(op.source) else {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        if let Ok(agents) = &receipt.0 {
            reconcile.write(ReconcileAgents {
                source: op.source,
                epoch: op.epoch,
                agents: agents.clone(),
            });
            ops_cmd.retire(entity, OperationState::Succeeded);
        } else {
            ops_cmd.retire(entity, OperationState::Failed);
        }
    }
}

fn adopt_upgraded_credential(
    mut ops: Query<(Entity, &CredentialUpgradeOp, &ReadUpgradedCredentialReceipt)>,
    sources: Query<(&SourceEpoch, &CredentialsPath), With<ConnectionSource>>,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<ReadUpgradedCredentialReceipt>();
        let Ok((epoch, path)) = sources.get(op.source) else {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        if let Some(credentials) = &receipt.0 {
            ops_cmd.commands.spawn((
                Operation::bundle("mobile.credentials.persist")
                    .unwrap()
                    .running(),
                OperationRetained,
                CredentialPersistOp {
                    source: op.source,
                    epoch: op.epoch,
                },
                PersistCredentialsJob {
                    path: path.clone(),
                    credentials: credentials.clone(),
                },
            ));
        }
        ops_cmd.retire(entity, OperationState::Succeeded);
    }
}

fn adopt_persisted_credentials(
    mut ops: Query<(Entity, &CredentialPersistOp, &PersistCredentialsReceipt)>,
    mut sources: Query<
        (
            &SourceEpoch,
            &mut CredentialPersistence,
            &mut ConnectionNotice,
        ),
        With<ConnectionSource>,
    >,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<PersistCredentialsReceipt>();
        let Ok((epoch, mut persistence, mut notice)) = sources.get_mut(op.source) else {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        match &receipt.0 {
            Ok(()) => {
                *persistence = CredentialPersistence::Persisted;
                ops_cmd.retire(entity, OperationState::Succeeded);
            }
            Err(error) => {
                *persistence = CredentialPersistence::Failed;
                notice.0 = error.clone();
                if let Ok(op_error) = OperationError::new(error.clone()) {
                    ops_cmd.commands.entity(entity).insert(op_error);
                }
                ops_cmd.retire(entity, OperationState::Failed);
            }
        }
    }
}

fn adopt_removed_credentials(
    mut ops: Query<(Entity, &CredentialRemoveOp, &RemoveCredentialsReceipt)>,
    mut sources: Query<
        (
            &SourceEpoch,
            &mut CredentialPersistence,
            &mut ConnectionNotice,
        ),
        With<ConnectionSource>,
    >,
    mut ops_cmd: ConnectionOps,
) {
    for (entity, op, receipt) in &mut ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<RemoveCredentialsReceipt>();
        let Ok((epoch, mut persistence, mut notice)) = sources.get_mut(op.source) else {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        };
        if epoch.0 != op.epoch {
            ops_cmd.retire(entity, OperationState::Canceled);
            continue;
        }
        match &receipt.0 {
            Ok(()) => {
                ops_cmd.retire(entity, OperationState::Succeeded);
            }
            Err(error) => {
                *persistence = CredentialPersistence::Failed;
                if notice.0.is_empty() {
                    notice.0 = error.clone();
                }
                if let Ok(op_error) = OperationError::new(error.clone()) {
                    ops_cmd.commands.entity(entity).insert(op_error);
                }
                ops_cmd.retire(entity, OperationState::Failed);
            }
        }
    }
}

fn retire_closed(
    ops: Query<Entity, (With<ConnectionCloseOp>, With<CloseConnectionReceipt>)>,
    mut ops_cmd: ConnectionOps,
) {
    for entity in &ops {
        ops_cmd
            .commands
            .entity(entity)
            .remove::<CloseConnectionReceipt>();
        ops_cmd.retire(entity, OperationState::Succeeded);
    }
}

#[derive(Props, Clone, PartialEq)]
pub(crate) struct PairCardProps {
    pub(crate) value: String,
    pub(crate) error: String,
    pub(crate) pairing: bool,
    pub(crate) on_value: EventHandler<String>,
    pub(crate) on_pair: EventHandler<()>,
    pub(crate) on_scan: EventHandler<()>,
}

#[component]
pub(crate) fn PairCard(props: PairCardProps) -> Element {
    let unavailable = use_hook(ScanRequest::unavailable);
    let mut show_link = use_signal(|| unavailable.is_some() || !props.value.trim().is_empty());

    rsx! {
        div { class: "w-full",
            div { class: "mb-5 text-center",
                h2 { class: "text-base font-semibold text-foreground", {translate("mobile-pair-title")} }
                p { class: "mt-1 text-xs leading-5 text-muted-foreground", {translate("mobile-pair-subtitle")} }
            }
            button {
                class: "vmux-primary-action flex h-14 w-full items-center justify-center gap-2.5 rounded-2xl text-sm font-semibold shadow-xl shadow-black/20 disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none",
                r#type: "button",
                disabled: unavailable.is_some(),
                onclick: move |_| props.on_scan.call(()),
                svg {
                    class: "h-5 w-5",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 5a2 2 0 0 1 2-2h2" }
                    path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                    path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                    path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                    rect { width: "5", height: "5", x: "7", y: "7", rx: "1" }
                    path { d: "M17 7v.01" }
                    path { d: "M17 12v5" }
                    path { d: "M12 17h5" }
                }
                {translate("mobile-pair-scan")}
            }
            button {
                class: "mx-auto mt-4 block rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground active:bg-accent active:text-accent-foreground",
                r#type: "button",
                onclick: move |_| show_link.set(!show_link()),
                {if show_link() { translate("mobile-pair-hide-link") } else { translate("mobile-pair-show-link") }}
            }
            if let Some(reason) = unavailable.clone() {
                p { class: "mt-3 text-center text-xs leading-5 text-muted-foreground", "{reason}" }
            }
            if show_link() {
                form {
                    class: "mt-2 flex items-center gap-2 rounded-2xl border border-border bg-muted p-1.5",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 bg-transparent px-3 font-mono text-base text-foreground outline-none placeholder:text-muted-foreground",
                        r#type: "url",
                        autofocus: unavailable.is_some(),
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: translate("mobile-pair-link-placeholder"),
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    button {
                        class: "flex h-10 shrink-0 items-center gap-2 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground disabled:opacity-70 active:bg-secondary/80",
                        r#type: "submit",
                        disabled: props.pairing,
                        if props.pairing {
                            span { class: "size-3.5 animate-spin rounded-full border-2 border-secondary-foreground/25 border-t-secondary-foreground motion-reduce:animate-none" }
                        }
                        {if props.pairing { translate("mobile-pair-connecting") } else { translate("mobile-pair-connect") }}
                    }
                }
            }
            if !props.error.is_empty() {
                p { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
            }
        }
    }
}
