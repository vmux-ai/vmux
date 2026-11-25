use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use vmux_ecs::{
    Generation, Operation, OperationRetained, OperationTarget, OperationTargetRevision,
};
use zeroize::Zeroizing;

use crate::state::{VaultSyncRequest, VaultUnlockRecoveryKeyRequest};
use crate::storage::{
    INDEX_FILE, LocalEntry, MANIFEST_FILE, OBJECTS_DIR, RECOVERY_DIR, RECOVERY_FILE,
    RecoveryEnvelope, RemoteManifest, VaultLocalState, VaultRecovery, VaultRepositoryPath,
    VaultSync,
};

use super::key::{
    AcceptedStorage, EcsKeyRequest, KeyAdmission, KeyAuthoritySet, KeyCanceled, KeyNamespace,
    KeyOutcome, KeyPermissions, KeyPort, KeyRequester, LoadKey, StoreKey, StoreRequest, VaultId,
    VaultKey, VaultNamespace,
};
use super::reconcile::{
    ReconcileCounts, ReconcileDone, ReconcileInputs, ReconcileLauncher, ReconcilePlugin,
    ReconcileRetire, ReconcileSet,
};
use super::vault_io::{
    ActiveVaultOperation, GitProcessTask, ProcessOutcome, RepoClaim, VaultOperationOutcome,
    VaultWake,
};
use super::{
    OperationSet, ReadyVaultOperation, VaultOperationOutput, VaultOperationRequest, VaultRegistry,
    VaultStatusProjection,
};

pub(super) struct SyncPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SyncSet;

impl Plugin for SyncPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ReconcilePlugin).add_systems(
            Update,
            (
                (
                    admit,
                    admit_recovery_unlock,
                    cancel,
                    claim_repo,
                    submit_recovery_decrypt,
                    drain_recovery_decrypt,
                    drain_recovery_store,
                    submit_check_repo,
                    drain_check_repo,
                    submit_check_origin,
                    drain_check_origin,
                    submit_check_branch,
                    drain_check_branch,
                    submit_key_load,
                    drain_key_load,
                    drain_recovery_check,
                    submit_baseline,
                    drain_baseline,
                )
                    .chain(),
                (
                    submit_fetch,
                    drain_fetch,
                    submit_remote_head,
                    drain_remote_head,
                    submit_verify_remote_ref,
                    drain_verify_remote_ref,
                    submit_list_remote_refs,
                    drain_list_remote_refs,
                    submit_validate_log,
                    drain_validate_log,
                    submit_validate_manifest,
                    drain_validate_manifest,
                    submit_merge_base,
                    drain_merge_base,
                    submit_reset_remote,
                    drain_reset_remote,
                )
                    .chain(),
                (
                    submit_decode_remote,
                    drain_decode_remote,
                    submit_collect_before,
                    drain_collect_before,
                    adopt_reconcile,
                    submit_collect_after,
                    drain_collect_after,
                    submit_write_snapshot,
                    drain_write_snapshot,
                    submit_git_add,
                    drain_git_add,
                    submit_git_status,
                    drain_git_status,
                    submit_git_commit,
                    drain_git_commit,
                    submit_push,
                    drain_push,
                    submit_local_state,
                    drain_local_state,
                    finish_canceled,
                )
                    .chain(),
            )
                .chain()
                .in_set(SyncSet)
                .after(KeyAuthoritySet)
                .after(ReconcileSet)
                .before(OperationSet),
        );
    }
}

#[derive(Component)]
pub(super) struct SyncActive;

#[derive(Component)]
pub(super) struct SyncPendingClaim;

#[derive(Component)]
pub(super) struct RecoveryUnlockRequest(pub(super) String);

#[derive(Component)]
struct RecoveryUnlockMode;

#[derive(Component)]
struct RecoveryDecrypt;

#[derive(Component)]
struct RecoveryStoreKey;

#[derive(Component)]
struct RecoveryDecryptTask(
    Task<
        Result<
            (
                RemoteManifest,
                Zeroizing<Vec<u8>>,
                BTreeMap<String, LocalEntry>,
            ),
            String,
        >,
    >,
);

#[derive(Component)]
struct SyncCanceling;

#[derive(Component)]
struct SyncCheckRepo;

#[derive(Component)]
struct SyncCheckOrigin;

#[derive(Component)]
struct SyncCheckBranch;

#[derive(Component)]
struct SyncAcquireKey;

#[derive(Component)]
struct SyncLoadBaseline;

#[derive(Component)]
struct SyncFetch;

#[derive(Component)]
struct SyncResolveRemoteHead;

#[derive(Component)]
struct SyncVerifyRemoteRef(Vec<String>);

#[derive(Component)]
struct SyncListRemoteRefs;

#[derive(Component)]
struct SyncValidateLog;

#[derive(Component)]
struct SyncValidateManifest;

#[derive(Component)]
struct SyncCheckMergeBase;

#[derive(Component)]
struct SyncResetRemote;

#[derive(Component)]
struct SyncDecodeRemote;

#[derive(Component)]
struct SyncCollectBefore;

#[derive(Component)]
struct SyncAwaitReconcile(Entity);

#[derive(Component)]
struct SyncCollectAfter;

#[derive(Component)]
struct SyncWriteSnapshot;

#[derive(Component)]
struct SyncGitAdd;

#[derive(Component)]
struct SyncGitStatus;

#[derive(Component)]
struct SyncGitCommit;

#[derive(Component)]
struct SyncPush;

#[derive(Component)]
struct SyncWriteLocalState;

#[derive(Component)]
struct SyncManifest(RemoteManifest);

#[derive(Component)]
struct SyncBranch(String);

#[derive(Component)]
pub(super) struct SyncRemoteBranch(pub(super) String);

#[derive(Component)]
struct SyncKeyRequest(Entity);

#[derive(Component)]
struct SyncKeyError(String);

#[derive(Component)]
struct SyncKeyLoan {
    key: Zeroizing<Vec<u8>>,
    owner: Option<Entity>,
    generation: Option<u64>,
}

#[derive(Component)]
struct SyncBaseline(BTreeMap<String, LocalEntry>);

#[derive(Component)]
struct SyncRemoteFiles(BTreeMap<String, LocalEntry>);

#[derive(Component)]
struct SyncLocalFiles(BTreeMap<String, LocalEntry>);

#[derive(Component)]
struct SyncOutcome(ReconcileCounts);

#[derive(Component)]
struct PushAttempt(u8);

#[derive(Component)]
struct ManifestReadTask(Task<Result<RemoteManifest, String>>);

#[derive(Component)]
struct RecoveryCheckTask(Task<bool>);

impl RecoveryCheckTask {
    fn spawn(wake: &VaultWake, repository: PathBuf) -> Self {
        Self(wake.spawn(move || {
            RecoveryEnvelope::read(&repository).is_ok_and(|envelope| envelope.is_some())
        }))
    }
}

#[derive(Component)]
struct BaselineReadTask(Task<Result<BTreeMap<String, LocalEntry>, String>>);

#[derive(Component)]
struct SnapshotLoadTask(Task<Result<BTreeMap<String, LocalEntry>, String>>);

#[derive(Component)]
struct CollectFilesTask(Task<Result<BTreeMap<String, LocalEntry>, String>>);

impl CollectFilesTask {
    fn spawn(wake: &VaultWake, root: PathBuf) -> Self {
        Self(wake.spawn(move || VaultSync::collect(&root)))
    }
}

#[derive(Component)]
struct SnapshotWriteTask(Task<Result<(), String>>);

impl SnapshotWriteTask {
    fn spawn(
        wake: &VaultWake,
        repository: PathBuf,
        vault_id: String,
        key: Zeroizing<Vec<u8>>,
        files: BTreeMap<String, LocalEntry>,
        remote: BTreeMap<String, LocalEntry>,
    ) -> Self {
        Self(wake.spawn(move || {
            VaultRepositoryPath::at(&repository).write_encrypted_snapshot(
                &vault_id,
                &key,
                &files,
                Some(&remote),
            )
        }))
    }
}

#[derive(Component)]
struct LocalStateWriteTask(Task<Result<(), String>>);

impl LocalStateWriteTask {
    fn spawn(wake: &VaultWake, root: PathBuf, repository: PathBuf) -> Self {
        Self(wake.spawn(move || VaultLocalState::write(&root, &repository)))
    }
}

#[derive(SystemParam)]
struct SyncValidity<'w, 's> {
    namespaces: Query<'w, 's, &'static Generation<VaultNamespace>, With<KeyNamespace>>,
    keys: Query<'w, 's, &'static Generation<VaultKey>, With<VaultKey>>,
}

impl SyncValidity<'_, '_> {
    fn valid(
        &self,
        target: &OperationTarget,
        accepted: &OperationTargetRevision,
        canceled: Option<&KeyCanceled>,
        loan: Option<&SyncKeyLoan>,
    ) -> bool {
        if canceled.is_some_and(|flag| flag.0.load(Ordering::Acquire)) {
            return false;
        }
        let Ok(generation) = self.namespaces.get(target.entity()) else {
            return false;
        };
        if generation.value() != accepted.0 {
            return false;
        }
        if let Some(loan) = loan
            && let (Some(owner), Some(expected)) = (loan.owner, loan.generation)
            && !self
                .keys
                .get(owner)
                .is_ok_and(|current| current.value() == expected)
        {
            return false;
        }
        true
    }
}

fn admit(
    operations: Query<
        Entity,
        (
            With<VaultOperationRequest<VaultSyncRequest>>,
            Added<ReadyVaultOperation>,
        ),
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &Generation<VaultNamespace>,
            &mut KeyAdmission,
            &AcceptedStorage,
        ),
        With<VaultRegistry>,
    >,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((namespace, access, generation, mut admission, captured)) = namespaces.single_mut()
    else {
        return;
    };
    let wake = VaultWake::from_proxy(proxy.as_deref());
    for entity in &operations {
        let storage = captured.0.clone();
        let (keys, ingress) = access.port(
            namespace,
            generation.value(),
            &mut admission,
            entity,
            wake.0.clone(),
        );
        let canceled = Arc::new(AtomicBool::new(false));
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.sync").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.value()),
                ActiveVaultOperation,
                SyncActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: false,
                    store: false,
                },
                wake.clone(),
                SyncPendingClaim,
            ));
        wake.notify();
    }
}

fn admit_recovery_unlock(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultUnlockRecoveryKeyRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &Generation<VaultNamespace>,
            &mut KeyAdmission,
            &AcceptedStorage,
        ),
        With<VaultRegistry>,
    >,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((namespace, access, generation, mut admission, captured)) = namespaces.single_mut()
    else {
        return;
    };
    let wake = VaultWake::from_proxy(proxy.as_deref());
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let (keys, ingress) = access.port(
            namespace,
            generation.value(),
            &mut admission,
            entity,
            wake.0.clone(),
        );
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.unlock_recovery_key")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.value()),
                ActiveVaultOperation,
                SyncActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: true,
                },
                wake.clone(),
                RecoveryUnlockRequest(request.0.recovery_key.clone()),
                RecoveryUnlockMode,
                SyncPendingClaim,
            ));
        wake.notify();
    }
}

fn cancel(
    validity: SyncValidity,
    ops: Query<
        (
            Entity,
            &OperationTarget,
            &OperationTargetRevision,
            Option<&KeyCanceled>,
            Option<&SyncKeyLoan>,
            &VaultWake,
        ),
        (
            With<SyncActive>,
            Without<SyncCanceling>,
            Without<VaultOperationOutcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, target, accepted, canceled, loan, wake) in &ops {
        if validity.valid(target, accepted, canceled, loan) {
            continue;
        }
        commands.entity(entity).insert(SyncCanceling);
        wake.notify();
    }
}

fn claim_repo(
    waiting: Query<
        (
            Entity,
            &AcceptedStorage,
            Has<RecoveryUnlockMode>,
            &VaultWake,
        ),
        (
            With<SyncPendingClaim>,
            Without<RepoClaim>,
            Without<SyncCanceling>,
        ),
    >,
    existing: Query<&RepoClaim>,
    mut commands: Commands,
) {
    let mut newly_claimed: Vec<RepoClaim> = Vec::new();
    for (entity, storage, recovery_unlock, wake) in &waiting {
        let claim = RepoClaim {
            root: storage.0.root().to_path_buf(),
            repository: storage.0.repository().to_path_buf(),
        };
        if existing.iter().any(|other| claim.conflicts(other))
            || newly_claimed.iter().any(|other| claim.conflicts(other))
        {
            continue;
        }
        newly_claimed.push(claim.clone());
        commands
            .entity(entity)
            .remove::<SyncPendingClaim>()
            .insert(claim);
        if recovery_unlock {
            commands.entity(entity).insert(RecoveryDecrypt);
        } else {
            commands.entity(entity).insert(SyncCheckRepo);
        }
        wake.notify();
    }
}

fn submit_recovery_decrypt(
    ops: Query<
        (Entity, &AcceptedStorage, &RecoveryUnlockRequest, &VaultWake),
        (
            With<RecoveryDecrypt>,
            Without<RecoveryDecryptTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, request, wake) in &ops {
        let recovery = VaultRecovery::from_storage(&storage.0);
        let key_text = request.0.clone();
        let task = wake.spawn(move || recovery.decrypt_for_unlock(&key_text));
        commands.entity(entity).insert(RecoveryDecryptTask(task));
    }
}

fn drain_recovery_decrypt(
    mut ops: Query<
        (
            Entity,
            &KeyPort,
            &VaultWake,
            Has<SyncCanceling>,
            &mut RecoveryDecryptTask,
        ),
        With<RecoveryDecrypt>,
    >,
    mut commands: Commands,
) {
    for (entity, port, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(RecoveryDecryptTask, RecoveryDecrypt)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok((manifest, key, remote_files)) => {
                let request = commands
                    .spawn((
                        Operation::bundle("vault.key.store").unwrap().running(),
                        OperationRetained,
                        KeyRequester(port.origin),
                        VaultId(manifest.vault_id.clone()),
                        StoreKey(key),
                        StoreRequest,
                        EcsKeyRequest,
                    ))
                    .id();
                commands.entity(entity).insert((
                    SyncManifest(manifest),
                    SyncBaseline(BTreeMap::new()),
                    SyncRemoteFiles(remote_files),
                    SyncKeyRequest(request),
                    RecoveryStoreKey,
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(SyncActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn drain_recovery_store(
    ops: Query<(Entity, &SyncKeyRequest, &VaultWake, Has<SyncCanceling>), With<RecoveryStoreKey>>,
    outcomes: Query<&KeyOutcome>,
    mut commands: Commands,
) {
    for (entity, request, wake, canceling) in &ops {
        let Ok(outcome) = outcomes.get(request.0) else {
            continue;
        };
        commands.entity(request.0).despawn();
        commands
            .entity(entity)
            .remove::<(SyncKeyRequest, RecoveryStoreKey)>();
        if canceling {
            wake.notify();
            continue;
        }
        match &outcome.result {
            Ok(key) => {
                commands.entity(entity).insert((
                    SyncKeyLoan {
                        key: Zeroizing::new(key.to_vec()),
                        owner: outcome.owner,
                        generation: outcome.generation,
                    },
                    SyncCollectBefore,
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(SyncActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error.clone())));
            }
        }
        wake.notify();
    }
}

fn submit_check_repo(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncCheckRepo>,
            Without<ManifestReadTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let task = wake.spawn(move || {
            if !repository.join(".git").is_dir() {
                return Err("Vault is not connected to Git".to_string());
            }
            VaultRepositoryPath::at(&repository).manifest()
        });
        commands.entity(entity).insert(ManifestReadTask(task));
    }
}

fn drain_check_repo(
    mut ops: Query<(
        Entity,
        &VaultWake,
        Has<SyncCanceling>,
        &mut ManifestReadTask,
    )>,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(ManifestReadTask, SyncCheckRepo)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(manifest) => {
                commands
                    .entity(entity)
                    .insert((SyncManifest(manifest), SyncCheckOrigin));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<RepoClaim>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_check_origin(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncCheckOrigin>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["remote".into(), "get-url".into(), "origin".into()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_check_origin(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncCheckOrigin>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncCheckOrigin)>();
        if canceling {
            wake.notify();
            continue;
        }
        let origin = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default();
        if origin.is_empty() {
            commands
                .entity(entity)
                .remove::<RepoClaim>()
                .insert(VaultOperationOutcome(Err(
                    "Vault has no origin remote".to_string()
                )));
        } else {
            commands.entity(entity).insert(SyncCheckBranch);
        }
        wake.notify();
    }
}

fn submit_check_branch(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncCheckBranch>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["branch".into(), "--show-current".into()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_check_branch(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncCheckBranch>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncCheckBranch)>();
        if canceling {
            wake.notify();
            continue;
        }
        let branch = result.and_then(ProcessOutcome::text);
        match branch {
            Ok(branch) if !branch.is_empty() => {
                commands
                    .entity(entity)
                    .insert((SyncBranch(branch), SyncAcquireKey));
            }
            Ok(_) => {
                commands
                    .entity(entity)
                    .remove::<RepoClaim>()
                    .insert(VaultOperationOutcome(Err(
                        "Vault has no current branch".to_string()
                    )));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<RepoClaim>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_key_load(
    ops: Query<
        (Entity, &KeyPort, &SyncManifest, &VaultWake),
        (
            With<SyncAcquireKey>,
            Without<SyncKeyRequest>,
            Without<RecoveryCheckTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, port, manifest, wake) in &ops {
        let request = commands
            .spawn((
                Operation::bundle("vault.key.load").unwrap().running(),
                OperationRetained,
                KeyRequester(port.origin),
                VaultId(manifest.0.vault_id.clone()),
                LoadKey,
                EcsKeyRequest,
            ))
            .id();
        commands.entity(entity).insert(SyncKeyRequest(request));
        wake.notify();
    }
}

fn drain_key_load(
    ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &SyncKeyRequest,
            &VaultWake,
            Has<SyncCanceling>,
        ),
        With<SyncAcquireKey>,
    >,
    outcomes: Query<&KeyOutcome>,
    mut commands: Commands,
) {
    for (entity, storage, request, wake, canceling) in &ops {
        let Ok(outcome) = outcomes.get(request.0) else {
            continue;
        };
        commands.entity(request.0).despawn();
        commands.entity(entity).remove::<SyncKeyRequest>();
        if canceling {
            commands.entity(entity).remove::<SyncAcquireKey>();
            wake.notify();
            continue;
        }
        match &outcome.result {
            Ok(key) => {
                commands.entity(entity).remove::<SyncAcquireKey>().insert((
                    SyncKeyLoan {
                        key: Zeroizing::new(key.to_vec()),
                        owner: outcome.owner,
                        generation: outcome.generation,
                    },
                    SyncLoadBaseline,
                ));
            }
            Err(error) => {
                let repository = storage.0.repository().to_path_buf();
                let task = RecoveryCheckTask::spawn(wake, repository);
                commands
                    .entity(entity)
                    .insert((SyncKeyError(error.clone()), task));
            }
        }
        wake.notify();
    }
}

fn drain_recovery_check(
    mut ops: Query<
        (
            Entity,
            &SyncKeyError,
            &VaultWake,
            Has<SyncCanceling>,
            &mut RecoveryCheckTask,
        ),
        With<SyncAcquireKey>,
    >,
    mut commands: Commands,
) {
    for (entity, error, wake, canceling, mut task) in &mut ops {
        let Some(has_recovery) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(RecoveryCheckTask, SyncKeyError, SyncAcquireKey)>();
        if canceling {
            wake.notify();
            continue;
        }
        let message = if !has_recovery {
            "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key."
                .to_string()
        } else {
            error.0.clone()
        };
        commands
            .entity(entity)
            .remove::<RepoClaim>()
            .insert(VaultOperationOutcome(Err(message)));
        wake.notify();
    }
}

fn submit_baseline(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncKeyLoan, &VaultWake),
        (
            With<SyncLoadBaseline>,
            Without<BaselineReadTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, loan, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let key = Zeroizing::new(loan.key.to_vec());
        let task = wake.spawn(move || {
            let files = VaultLocalState::baseline(&repository).unwrap_or_else(|_| {
                VaultRepositoryPath::at(&repository)
                    .load_encrypted_snapshot(&key)
                    .map(|(_, files)| files)
                    .unwrap_or_default()
            });
            Ok(files)
        });
        commands.entity(entity).insert(BaselineReadTask(task));
    }
}

fn drain_baseline(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<SyncCanceling>,
            &mut BaselineReadTask,
        ),
        With<SyncLoadBaseline>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(BaselineReadTask, SyncLoadBaseline)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(baseline) => {
                commands
                    .entity(entity)
                    .insert((SyncBaseline(baseline), PushAttempt(0), SyncFetch));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_fetch(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncFetch>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["fetch".into(), "origin".into()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_fetch(
    mut ops: Query<(Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask), With<SyncFetch>>,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncFetch)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands
                    .entity(entity)
                    .remove::<SyncRemoteBranch>()
                    .insert(SyncResolveRemoteHead);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_remote_head(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncResolveRemoteHead>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "symbolic-ref".into(),
                "--short".into(),
                "refs/remotes/origin/HEAD".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_remote_head(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncResolveRemoteHead>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncResolveRemoteHead)>();
        if canceling {
            wake.notify();
            continue;
        }
        let mut candidates = Vec::new();
        if let Ok(outcome) = result
            && let Ok(symbolic) = outcome.text()
            && symbolic.starts_with("origin/")
        {
            candidates.push(symbolic);
        }
        for fallback in ["origin/main", "origin/master"] {
            if !candidates.iter().any(|c| c == fallback) {
                candidates.push(fallback.to_string());
            }
        }
        commands
            .entity(entity)
            .insert(SyncVerifyRemoteRef(candidates));
        wake.notify();
    }
}

fn submit_verify_remote_ref(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncVerifyRemoteRef, &VaultWake),
        (Without<GitProcessTask>, Without<SyncCanceling>),
    >,
    mut commands: Commands,
) {
    for (entity, storage, candidates, wake) in &ops {
        let Some(candidate) = candidates.0.first() else {
            commands
                .entity(entity)
                .remove::<SyncVerifyRemoteRef>()
                .insert(SyncListRemoteRefs);
            wake.notify();
            continue;
        };
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["rev-parse".into(), "--verify".into(), candidate.clone()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_verify_remote_ref(
    mut ops: Query<(
        Entity,
        &mut SyncVerifyRemoteRef,
        &VaultWake,
        Has<SyncCanceling>,
        &mut GitProcessTask,
    )>,
    mut commands: Commands,
) {
    for (entity, mut candidates, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<GitProcessTask>();
        if canceling {
            commands.entity(entity).remove::<SyncVerifyRemoteRef>();
            wake.notify();
            continue;
        }
        let checked = if candidates.0.is_empty() {
            String::new()
        } else {
            candidates.0.remove(0)
        };
        if result.is_ok_and(|outcome| outcome.success) && !checked.is_empty() {
            commands
                .entity(entity)
                .remove::<SyncVerifyRemoteRef>()
                .insert((SyncRemoteBranch(checked), SyncValidateLog));
        } else if candidates.0.is_empty() {
            commands
                .entity(entity)
                .remove::<SyncVerifyRemoteRef>()
                .insert(SyncListRemoteRefs);
        }
        wake.notify();
    }
}

fn submit_list_remote_refs(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncListRemoteRefs>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "for-each-ref".into(),
                "--format=%(refname:short)".into(),
                "refs/remotes/origin".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_list_remote_refs(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncListRemoteRefs>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncListRemoteRefs)>();
        if canceling {
            wake.notify();
            continue;
        }
        let branches: Vec<String> = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default()
            .lines()
            .filter(|branch| *branch != "origin/HEAD" && !branch.is_empty())
            .map(str::to_string)
            .collect();
        if branches.len() == 1 {
            commands
                .entity(entity)
                .insert((SyncRemoteBranch(branches[0].clone()), SyncValidateLog));
        } else {
            commands.entity(entity).insert(SyncDecodeRemote);
        }
        wake.notify();
    }
}

fn submit_validate_log(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncRemoteBranch, &VaultWake),
        (
            With<SyncValidateLog>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "log".into(),
                remote.0.clone(),
                "--name-only".into(),
                "--pretty=format:".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_validate_log(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncValidateLog>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncValidateLog)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(entries) => {
                let valid = entries
                    .lines()
                    .map(str::trim)
                    .all(|entry| entry.is_empty() || SyncRemoteBranch::is_vault_path(entry));
                if valid {
                    commands.entity(entity).insert(SyncValidateManifest);
                } else {
                    commands
                        .entity(entity)
                        .remove::<(RepoClaim, SyncKeyLoan)>()
                        .insert(VaultOperationOutcome(Err(
                            "selected repository contains plaintext or non-Vault history"
                                .to_string(),
                        )));
                }
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

impl SyncRemoteBranch {
    pub(super) fn is_vault_path(path: &str) -> bool {
        if path == MANIFEST_FILE || path == INDEX_FILE {
            return true;
        }
        if let Some(rest) = path
            .strip_prefix(RECOVERY_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            && rest == RECOVERY_FILE
        {
            return true;
        }
        let Some(name) = path
            .strip_prefix(OBJECTS_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return false;
        };
        name.len() == 64 && !name.contains('/')
    }
}

fn submit_validate_manifest(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncRemoteBranch, &VaultWake),
        (
            With<SyncValidateManifest>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let spec = format!("{}:{MANIFEST_FILE}", remote.0);
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["show".into(), spec],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_validate_manifest(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncValidateManifest>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncValidateManifest)>();
        if canceling {
            wake.notify();
            continue;
        }
        let manifest = result
            .and_then(ProcessOutcome::text)
            .and_then(|source| RemoteManifest::try_from(source.as_bytes()));
        match manifest {
            Ok(_) => {
                commands.entity(entity).insert(SyncCheckMergeBase);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_merge_base(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncRemoteBranch, &VaultWake),
        (
            With<SyncCheckMergeBase>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["merge-base".into(), "HEAD".into(), remote.0.clone()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_merge_base(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncCheckMergeBase>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncCheckMergeBase)>();
        if canceling {
            wake.notify();
            continue;
        }
        if result.is_ok_and(|outcome| outcome.success) {
            commands.entity(entity).insert(SyncResetRemote);
        } else {
            commands
                .entity(entity)
                .remove::<(RepoClaim, SyncKeyLoan)>()
                .insert(VaultOperationOutcome(Err(
                    "Vault remote has unrelated history".to_string(),
                )));
        }
        wake.notify();
    }
}

fn submit_reset_remote(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncRemoteBranch, &VaultWake),
        (
            With<SyncResetRemote>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["reset".into(), "--hard".into(), remote.0.clone()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_reset_remote(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncResetRemote>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncResetRemote)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(SyncDecodeRemote);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_decode_remote(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncKeyLoan, &VaultWake),
        (
            With<SyncDecodeRemote>,
            Without<SnapshotLoadTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, loan, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let key = Zeroizing::new(loan.key.to_vec());
        let task = wake.spawn(move || {
            VaultRepositoryPath::at(&repository)
                .load_encrypted_snapshot(&key)
                .map(|(_, files)| files)
        });
        commands.entity(entity).insert(SnapshotLoadTask(task));
    }
}

fn drain_decode_remote(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<SyncCanceling>,
            &mut SnapshotLoadTask,
        ),
        With<SyncDecodeRemote>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(SnapshotLoadTask, SyncDecodeRemote)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(remote_files) => {
                commands
                    .entity(entity)
                    .insert((SyncRemoteFiles(remote_files), SyncCollectBefore));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_collect_before(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncCollectBefore>,
            Without<CollectFilesTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        commands
            .entity(entity)
            .insert(CollectFilesTask::spawn(wake, root));
    }
}

fn drain_collect_before(
    mut ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &SyncBaseline,
            &SyncRemoteFiles,
            &VaultWake,
            Has<SyncCanceling>,
            &mut CollectFilesTask,
        ),
        With<SyncCollectBefore>,
    >,
    mut reconcile: ReconcileLauncher,
    mut commands: Commands,
) {
    for (entity, storage, baseline, remote, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(CollectFilesTask, SyncCollectBefore)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(local) => {
                let session = reconcile.spawn(
                    entity,
                    storage.0.root().to_path_buf(),
                    wake.clone(),
                    ReconcileInputs {
                        baseline: baseline.0.clone(),
                        local,
                        remote: remote.0.clone(),
                    },
                );
                commands.entity(entity).insert(SyncAwaitReconcile(session));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn adopt_reconcile(
    ops: Query<(
        Entity,
        &SyncAwaitReconcile,
        Has<RecoveryUnlockMode>,
        &VaultWake,
        Has<SyncCanceling>,
    )>,
    sessions: Query<&ReconcileDone>,
    mut commands: Commands,
) {
    for (entity, wait, recovery_unlock, wake, canceling) in &ops {
        let Ok(done) = sessions.get(wait.0) else {
            continue;
        };
        commands.entity(wait.0).insert(ReconcileRetire);
        commands.entity(entity).remove::<SyncAwaitReconcile>();
        if canceling {
            wake.notify();
            continue;
        }
        match &done.0 {
            Ok(counts) if recovery_unlock => {
                commands
                    .entity(entity)
                    .insert((SyncOutcome(*counts), SyncWriteLocalState));
            }
            Ok(counts) => {
                commands
                    .entity(entity)
                    .insert((SyncOutcome(*counts), SyncCollectAfter));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(SyncActive, RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error.clone())));
            }
        }
        wake.notify();
    }
}

fn submit_collect_after(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncCollectAfter>,
            Without<CollectFilesTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        commands
            .entity(entity)
            .insert(CollectFilesTask::spawn(wake, root));
    }
}

fn drain_collect_after(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<SyncCanceling>,
            &mut CollectFilesTask,
        ),
        With<SyncCollectAfter>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(CollectFilesTask, SyncCollectAfter)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(files) => {
                commands
                    .entity(entity)
                    .insert((SyncLocalFiles(files), SyncWriteSnapshot));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_write_snapshot(
    ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &SyncManifest,
            &SyncKeyLoan,
            &SyncLocalFiles,
            &SyncRemoteFiles,
            &VaultWake,
        ),
        (
            With<SyncWriteSnapshot>,
            Without<SnapshotWriteTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, manifest, loan, files, remote, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let vault_id = manifest.0.vault_id.clone();
        let key = Zeroizing::new(loan.key.to_vec());
        let files = files.0.clone();
        let remote = remote.0.clone();
        commands.entity(entity).insert(SnapshotWriteTask::spawn(
            wake, repository, vault_id, key, files, remote,
        ));
    }
}

fn drain_write_snapshot(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<SyncCanceling>,
            &mut SnapshotWriteTask,
        ),
        With<SyncWriteSnapshot>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(SnapshotWriteTask, SyncWriteSnapshot, SyncLocalFiles)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                commands.entity(entity).insert(SyncGitAdd);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_git_add(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncGitAdd>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["add".into(), "--all".into()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_git_add(
    mut ops: Query<(Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask), With<SyncGitAdd>>,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncGitAdd)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(SyncGitStatus);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_git_status(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncGitStatus>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec!["status".into(), "--porcelain".into()],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_git_status(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncGitStatus>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncGitStatus)>();
        if canceling {
            wake.notify();
            continue;
        }
        let status = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default();
        if status.is_empty() {
            commands.entity(entity).insert(SyncPush);
        } else {
            commands.entity(entity).insert(SyncGitCommit);
        }
        wake.notify();
    }
}

fn submit_git_commit(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncGitCommit>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "-c".into(),
                "commit.gpgSign=false".into(),
                "commit".into(),
                "-m".into(),
                "Sync vmux Vault".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_git_commit(
    mut ops: Query<
        (Entity, &VaultWake, Has<SyncCanceling>, &mut GitProcessTask),
        With<SyncGitCommit>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncGitCommit)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(SyncPush);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_push(
    ops: Query<
        (Entity, &AcceptedStorage, &SyncBranch, &VaultWake),
        (
            With<SyncPush>,
            Without<GitProcessTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, branch, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "push".into(),
                "-u".into(),
                "origin".into(),
                branch.0.clone(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_push(
    validity: SyncValidity,
    mut ops: Query<
        (
            Entity,
            &OperationTarget,
            &OperationTargetRevision,
            Option<&KeyCanceled>,
            Option<&SyncKeyLoan>,
            &mut PushAttempt,
            &VaultWake,
            Has<SyncCanceling>,
            &mut GitProcessTask,
        ),
        With<SyncPush>,
    >,
    mut commands: Commands,
) {
    for (entity, target, accepted, canceled, loan, mut attempt, wake, canceling, mut task) in
        &mut ops
    {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, SyncPush)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(outcome) if outcome.success => {
                commands.entity(entity).insert(SyncWriteLocalState);
            }
            Ok(outcome) if outcome.is_remote_change_refusal() => {
                if attempt.0 < 2 {
                    if !validity.valid(target, accepted, canceled, loan) {
                        commands.entity(entity).insert(SyncCanceling);
                    } else {
                        attempt.0 += 1;
                        commands.entity(entity).insert(SyncFetch);
                    }
                } else {
                    commands
                        .entity(entity)
                        .remove::<(RepoClaim, SyncKeyLoan)>()
                        .insert(VaultOperationOutcome(Err(
                            "Vault remote kept changing during sync".to_string(),
                        )));
                }
            }
            Ok(outcome) => {
                let error = outcome
                    .text()
                    .err()
                    .unwrap_or_else(|| "Vault push failed".to_string());
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(RepoClaim, SyncKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_local_state(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<SyncWriteLocalState>,
            Without<LocalStateWriteTask>,
            Without<SyncCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        let repository = storage.0.repository().to_path_buf();
        commands
            .entity(entity)
            .insert(LocalStateWriteTask::spawn(wake, root, repository));
    }
}

fn drain_local_state(
    mut ops: Query<
        (
            Entity,
            Option<&SyncOutcome>,
            Has<RecoveryUnlockMode>,
            &VaultWake,
            Has<SyncCanceling>,
            &mut LocalStateWriteTask,
        ),
        With<SyncWriteLocalState>,
    >,
    mut commands: Commands,
) {
    for (entity, counts, recovery_unlock, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<(
            LocalStateWriteTask,
            SyncWriteLocalState,
            SyncActive,
            RepoClaim,
            SyncKeyLoan,
        )>();
        if canceling {
            wake.notify();
            continue;
        }
        let outcome = match result {
            Ok(()) => {
                let message = if recovery_unlock {
                    "Vault unlocked".to_string()
                } else {
                    counts.map_or_else(|| "Vault synced".to_string(), |counts| counts.0.message())
                };
                Ok(VaultOperationOutput::message(message))
            }
            Err(error) => Err(error),
        };
        commands
            .entity(entity)
            .insert(VaultOperationOutcome(outcome));
        wake.notify();
    }
}

fn finish_canceled(
    ops: Query<
        (Entity, &VaultWake),
        (
            With<SyncCanceling>,
            Without<RecoveryDecryptTask>,
            Without<ManifestReadTask>,
            Without<GitProcessTask>,
            Without<SyncKeyRequest>,
            Without<RecoveryCheckTask>,
            Without<BaselineReadTask>,
            Without<SnapshotLoadTask>,
            Without<CollectFilesTask>,
            Without<SyncAwaitReconcile>,
            Without<SnapshotWriteTask>,
            Without<LocalStateWriteTask>,
            Without<VaultOperationOutcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, wake) in &ops {
        commands
            .entity(entity)
            .remove::<(
                SyncActive,
                SyncCanceling,
                SyncPendingClaim,
                RecoveryDecrypt,
                RecoveryStoreKey,
                SyncCheckRepo,
                SyncCheckOrigin,
                SyncCheckBranch,
                SyncAcquireKey,
                SyncLoadBaseline,
                SyncFetch,
                SyncResolveRemoteHead,
            )>()
            .remove::<(
                SyncVerifyRemoteRef,
                SyncListRemoteRefs,
                SyncValidateLog,
                SyncValidateManifest,
                SyncCheckMergeBase,
                SyncResetRemote,
                SyncDecodeRemote,
                SyncCollectBefore,
                SyncCollectAfter,
                SyncWriteSnapshot,
                SyncGitAdd,
                SyncGitStatus,
            )>()
            .remove::<(
                SyncGitCommit,
                SyncPush,
                SyncWriteLocalState,
                RepoClaim,
                SyncKeyLoan,
            )>()
            .insert(VaultOperationOutcome(
                Err("Vault sync canceled".to_string()),
            ));
        wake.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::connect::{
        ConnectActive, ConnectFolderMode, ConnectInitOnly, ConnectPendingClaim, ConnectRemoteMode,
        ConnectionPlugin,
    };
    use crate::host::key::{KeyAuthorityPlugin, LockVault, RepositoryDirectory};
    use crate::host::reconcile::{
        ReconcileInputs, ReconcileRoot, ReconcileSession, ReconcileSpawning,
    };
    use crate::storage::keys::{KeyWriteReceipt, NativeKeyAccess, NativeKeyFixture};
    use crate::storage::{GeneratedRecoveryKey, RonMap, VaultStorage};
    use bevy::tasks::{IoTaskPool, TaskPool};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    struct IsolatedKeyFixture {
        directory: PathBuf,
        fixed_key: Option<Vec<u8>>,
        stored: parking_lot::Mutex<Option<Vec<u8>>>,
    }

    impl IsolatedKeyFixture {
        fn fixed(directory: PathBuf, byte: u8) -> Arc<Self> {
            Arc::new(Self {
                directory,
                fixed_key: Some(vec![byte; 32]),
                stored: parking_lot::Mutex::new(None),
            })
        }

        fn empty(directory: PathBuf) -> Arc<Self> {
            Arc::new(Self {
                directory,
                fixed_key: None,
                stored: parking_lot::Mutex::new(None),
            })
        }

        fn stored_bytes(&self) -> Option<Vec<u8>> {
            self.stored.lock().clone()
        }
    }

    impl NativeKeyFixture for IsolatedKeyFixture {
        fn path(&self, vault_id: &str) -> PathBuf {
            self.directory.join(vault_id)
        }

        fn read(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
            if let Some(key) = self.stored.lock().clone() {
                return Ok(Zeroizing::new(key));
            }
            if let Some(key) = &self.fixed_key {
                return Ok(Zeroizing::new(key.clone()));
            }
            Err("key unavailable".to_string())
        }

        fn observe(&self, _vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
            if let Some(key) = self.stored.lock().clone() {
                return Ok(Some(Zeroizing::new(key)));
            }
            Ok(self.fixed_key.clone().map(Zeroizing::new))
        }

        fn generate(&self) -> Result<Zeroizing<Vec<u8>>, String> {
            if let Some(key) = &self.fixed_key {
                return Ok(Zeroizing::new(key.clone()));
            }
            Ok(Zeroizing::new(vec![19; 32]))
        }

        fn write(&self, _vault_id: &str, key: &[u8]) -> Result<KeyWriteReceipt, String> {
            let mut guard = self.stored.lock();
            let created = guard.is_none();
            *guard = Some(key.to_vec());
            Ok(KeyWriteReceipt { created })
        }
    }

    struct VaultHarness {
        app: App,
        namespace: Entity,
        storage: VaultStorage,
        _key_dir: tempfile::TempDir,
    }

    impl VaultHarness {
        fn new(root: &Path, repository: PathBuf, fixture: Arc<IsolatedKeyFixture>) -> Self {
            IoTaskPool::get_or_init(TaskPool::new);
            let key_dir = tempfile::tempdir().unwrap();
            let storage = VaultStorage {
                root: root.to_path_buf(),
                repository: repository.clone(),
            };
            let mut app = App::new();
            app.add_plugins((KeyAuthorityPlugin, SyncPlugin, ConnectionPlugin));
            let namespace = app
                .world_mut()
                .spawn((
                    Name::new("Vault harness"),
                    vmux_ecs::Cwd(root.to_path_buf()),
                    RepositoryDirectory(repository),
                    AcceptedStorage(storage.clone()),
                    KeyNamespace(NativeKeyAccess::fixture(
                        key_dir.path().to_path_buf(),
                        fixture,
                    )),
                    Generation::<VaultNamespace>::default(),
                    KeyAdmission::default(),
                ))
                .id();
            Self {
                app,
                namespace,
                storage,
                _key_dir: key_dir,
            }
        }

        fn spawn_base(&mut self, name: &'static str, permissions: KeyPermissions) -> Entity {
            let canceled = Arc::new(AtomicBool::new(false));
            let op = self
                .app
                .world_mut()
                .spawn((
                    Operation::bundle(name).unwrap().running(),
                    OperationRetained,
                    OperationTarget::new(self.namespace),
                    ActiveVaultOperation,
                    AcceptedStorage(self.storage.clone()),
                    KeyCanceled(canceled),
                    permissions,
                    VaultWake(None),
                ))
                .id();
            let world = self.app.world_mut();
            let mut query = world.query::<(
                &KeyNamespace,
                &Generation<VaultNamespace>,
                &mut KeyAdmission,
            )>();
            let (access, generation, mut admission) = query.get_mut(world, self.namespace).unwrap();
            let gen_value = generation.value();
            let (port, ingress) = access.port(self.namespace, gen_value, &mut admission, op, None);
            world
                .entity_mut(op)
                .insert((OperationTargetRevision(gen_value), port, ingress));
            op
        }

        fn run_until_outcome(&mut self, op: Entity) -> Result<String, String> {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                self.app.update();
                if let Some(outcome) = self
                    .app
                    .world_mut()
                    .entity_mut(op)
                    .take::<VaultOperationOutcome>()
                {
                    self.app.world_mut().despawn(op);
                    self.app.update();
                    return outcome.0.map(|output| output.message);
                }
                assert!(Instant::now() < deadline, "vault operation did not finish");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn initialize(&mut self) -> Result<String, String> {
            let op = self.spawn_base(
                "test.vault.init",
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
            );
            self.app.world_mut().entity_mut(op).insert((
                ConnectActive,
                ConnectInitOnly,
                ConnectPendingClaim,
            ));
            self.run_until_outcome(op)
        }

        fn connect_remote(&mut self, remote: &str) -> Result<String, String> {
            let op = self.spawn_base(
                "test.vault.connect",
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
            );
            self.app.world_mut().entity_mut(op).insert((
                ConnectActive,
                ConnectRemoteMode(remote.to_string()),
                ConnectPendingClaim,
            ));
            self.run_until_outcome(op)
        }

        fn connect_folder(&mut self, folder: &Path) -> Result<String, String> {
            let op = self.spawn_base(
                "test.vault.folder",
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
            );
            self.app.world_mut().entity_mut(op).insert((
                ConnectActive,
                ConnectFolderMode {
                    folder: folder.to_path_buf(),
                    require_existing: false,
                },
                ConnectPendingClaim,
            ));
            self.run_until_outcome(op)
        }

        fn sync(&mut self) -> Result<String, String> {
            let op = self.spawn_sync_op();
            self.run_until_outcome(op)
        }

        fn spawn_sync_op(&mut self) -> Entity {
            let op = self.spawn_base(
                "test.vault.sync",
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: false,
                    store: false,
                },
            );
            self.app
                .world_mut()
                .entity_mut(op)
                .insert((SyncActive, SyncPendingClaim));
            op
        }

        fn unlock_recovery(&mut self, recovery_key: &str) -> Result<String, String> {
            let op = self.spawn_base(
                "test.vault.unlock",
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: true,
                },
            );
            self.app.world_mut().entity_mut(op).insert((
                SyncActive,
                RecoveryUnlockRequest(recovery_key.to_string()),
                RecoveryUnlockMode,
                SyncPendingClaim,
            ));
            self.run_until_outcome(op)
        }

        fn prepare_repo(root: &Path) -> PathBuf {
            let repo = root.join(".vmux-vault");
            VaultRepositoryPath::at(&repo).ensure().unwrap();
            Self::git(&repo, &["config", "user.name", "Vmux Test"]).unwrap();
            Self::git(&repo, &["config", "user.email", "vmux@example.com"]).unwrap();
            Self::git(&repo, &["config", "commit.gpgSign", "false"]).unwrap();
            repo
        }

        fn create_bare(path: &Path) {
            let output = Command::new("git")
                .args(["init", "--bare", path.to_string_lossy().as_ref()])
                .output()
                .unwrap();
            assert!(output.status.success());
        }

        fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
            let output = Command::new("git")
                .current_dir(dir)
                .env("GIT_TERMINAL_PROMPT", "0")
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            ProcessOutcome::from_output(output).text()
        }
    }

    #[test]
    fn initialization_commits_only_encrypted_paths_and_content() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
        std::fs::create_dir_all(root.path().join("workspace/project")).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(secret: true)\n").unwrap();
        std::fs::write(root.path().join("knowledge/private.md"), "# Private\n").unwrap();
        std::fs::write(
            root.path().join("workspace/project/secret.txt"),
            "ignored\n",
        )
        .unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let fixture = IsolatedKeyFixture::fixed(root.path().join("keys"), 3);
        let mut harness = VaultHarness::new(root.path(), repo.clone(), fixture);

        harness.initialize().unwrap();

        let tree = VaultHarness::git(&repo, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap();
        assert!(tree.lines().any(|p| p == MANIFEST_FILE));
        assert!(tree.lines().any(|p| p == INDEX_FILE));
        assert!(tree.lines().any(|p| p.starts_with("objects/")));
        assert!(!tree.contains("settings.ron"));
        assert!(!tree.contains("private.md"));
        let bytes = std::fs::read(repo.join(INDEX_FILE)).unwrap();
        assert!(!bytes.windows(7).any(|w| w == b"Private"));
        let (_, files) = VaultRepositoryPath::at(&repo)
            .load_encrypted_snapshot(&[3; 32])
            .unwrap();
        assert_eq!(files["settings.ron"].data, b"(secret: true)\n");
        assert_eq!(files["knowledge/private.md"].data, b"# Private\n");
        assert!(!files.contains_key("workspace/project/secret.txt"));
    }

    #[test]
    fn initialization_rejects_unencrypted_staging_files() {
        let root = tempfile::tempdir().unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        std::fs::write(repo.join("plaintext.txt"), "secret\n").unwrap();
        let fixture = IsolatedKeyFixture::fixed(root.path().join("keys"), 11);
        let mut harness = VaultHarness::new(root.path(), repo.clone(), fixture);

        let error = harness.initialize().unwrap_err();
        assert!(error.contains("unencrypted"));
        assert!(VaultHarness::git(&repo, &["rev-parse", "--verify", "HEAD"]).is_err());
    }

    #[test]
    fn empty_remote_receives_encrypted_initial_and_followup_syncs() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let fixture = IsolatedKeyFixture::fixed(root.path().join("keys"), 4);
        let mut harness = VaultHarness::new(root.path(), repo.clone(), fixture);

        harness.connect_remote(remote.to_str().unwrap()).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(changed: true)\n").unwrap();
        harness.sync().unwrap();

        assert_eq!(
            VaultHarness::git(&repo, &["rev-list", "--count", "origin/main"]).unwrap(),
            "2"
        );
        let tree =
            VaultHarness::git(&repo, &["ls-tree", "-r", "--name-only", "origin/main"]).unwrap();
        assert!(!tree.contains("settings.ron"));
    }

    #[test]
    fn stale_encrypted_commit_is_discarded_and_regenerated_from_plaintext() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        let path = root.path().join("settings.ron");
        std::fs::write(&path, "(value: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let fixture = IsolatedKeyFixture::fixed(root.path().join("keys"), 15);
        let mut harness = VaultHarness::new(root.path(), repo.clone(), fixture);
        harness.connect_remote(remote.to_str().unwrap()).unwrap();
        std::fs::write(&path, "(value: 2)\n").unwrap();
        std::fs::write(repo.join(INDEX_FILE), b"stale encrypted commit").unwrap();
        VaultHarness::git(&repo, &["add", INDEX_FILE]).unwrap();
        VaultHarness::git(&repo, &["commit", "-m", "Stale local snapshot"]).unwrap();

        harness.sync().unwrap();

        let (_, files) = VaultRepositoryPath::at(&repo)
            .load_encrypted_snapshot(&[15; 32])
            .unwrap();
        assert_eq!(files["settings.ron"].data, b"(value: 2)\n");
        assert_eq!(
            VaultHarness::git(&repo, &["status", "--short"]).unwrap(),
            ""
        );
    }

    #[test]
    fn existing_encrypted_vault_merges_non_conflicting_local_files() {
        let seed = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
        let seed_repo = VaultHarness::prepare_repo(seed.path());
        let mut first = VaultHarness::new(
            seed.path(),
            seed_repo,
            IsolatedKeyFixture::fixed(seed.path().join("keys"), 5),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
        std::fs::write(root.path().join("knowledge/local.md"), "# Local\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut second = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 5),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        assert_eq!(
            std::fs::read_to_string(root.path().join("settings.ron")).unwrap(),
            "(remote: true)\n"
        );
        let (_, files) = VaultRepositoryPath::at(&repo)
            .load_encrypted_snapshot(&[5; 32])
            .unwrap();
        assert!(files.contains_key("settings.ron"));
        assert!(files.contains_key("knowledge/local.md"));
    }

    #[test]
    fn existing_encrypted_vault_merges_structured_files_by_key() {
        let seed = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
        let seed_repo = VaultHarness::prepare_repo(seed.path());
        let mut first = VaultHarness::new(
            seed.path(),
            seed_repo,
            IsolatedKeyFixture::fixed(seed.path().join("keys"), 6),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::write(root.path().join("settings.ron"), "(local: true)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut second = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 6),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        let source = std::fs::read_to_string(root.path().join("settings.ron")).unwrap();
        let ron::Value::Map(settings) = ron::from_str::<ron::Value>(&source).unwrap() else {
            panic!("settings must remain a RON map");
        };
        assert_eq!(
            RonMap(&settings).get(&ron::Value::String("local".to_string())),
            Some(&ron::Value::Bool(true))
        );
        assert_eq!(
            RonMap(&settings).get(&ron::Value::String("remote".to_string())),
            Some(&ron::Value::Bool(true))
        );
    }

    #[test]
    fn same_structured_key_prefers_the_local_value() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(first_dir.path().join("settings.ron"), "(value: 1)\n").unwrap();
        let first_repo = VaultHarness::prepare_repo(first_dir.path());
        let mut first = VaultHarness::new(
            first_dir.path(),
            first_repo,
            IsolatedKeyFixture::fixed(first_dir.path().join("keys"), 10),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        let second_repo = VaultHarness::prepare_repo(second_dir.path());
        let mut second = VaultHarness::new(
            second_dir.path(),
            second_repo,
            IsolatedKeyFixture::fixed(second_dir.path().join("keys"), 10),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::write(first_dir.path().join("settings.ron"), "(value: 2)\n").unwrap();
        first.sync().unwrap();
        std::fs::write(second_dir.path().join("settings.ron"), "(value: 3)\n").unwrap();

        let result = second.sync().unwrap();
        second.sync().unwrap();

        assert!(result.contains("automatic merge"));
        let source = std::fs::read_to_string(second_dir.path().join("settings.ron")).unwrap();
        let ron::Value::Map(settings) = ron::from_str::<ron::Value>(&source).unwrap() else {
            panic!("settings must remain a RON map");
        };
        let value = RonMap(&settings)
            .get(&ron::Value::String("value".to_string()))
            .cloned()
            .unwrap()
            .into_rust::<i64>()
            .unwrap();
        assert_eq!(value, 3);
    }

    #[test]
    fn markdown_changes_from_two_devices_merge_automatically() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::create_dir_all(first_dir.path().join("knowledge")).unwrap();
        std::fs::write(
            first_dir.path().join("knowledge/note.md"),
            "# Note\n\nAlpha\n\nOmega\n",
        )
        .unwrap();
        let first_repo = VaultHarness::prepare_repo(first_dir.path());
        let mut first = VaultHarness::new(
            first_dir.path(),
            first_repo,
            IsolatedKeyFixture::fixed(first_dir.path().join("keys"), 12),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        let second_repo = VaultHarness::prepare_repo(second_dir.path());
        let mut second = VaultHarness::new(
            second_dir.path(),
            second_repo,
            IsolatedKeyFixture::fixed(second_dir.path().join("keys"), 12),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::write(
            first_dir.path().join("knowledge/note.md"),
            "# Note\n\nAlpha from first\n\nOmega\n",
        )
        .unwrap();
        first.sync().unwrap();
        std::fs::write(
            second_dir.path().join("knowledge/note.md"),
            "# Note\n\nAlpha\n\nOmega from second\n",
        )
        .unwrap();

        let result = second.sync().unwrap();
        assert!(result.contains("automatic merge"));
        assert_eq!(
            std::fs::read_to_string(second_dir.path().join("knowledge/note.md")).unwrap(),
            "# Note\n\nAlpha from first\n\nOmega from second\n"
        );
    }

    #[test]
    fn toml_changes_from_two_devices_merge_by_key() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::create_dir_all(first_dir.path().join("tools")).unwrap();
        std::fs::write(
            first_dir.path().join("tools/tools.toml"),
            "[values]\nfirst = 1\nsecond = 1\n",
        )
        .unwrap();
        let first_repo = VaultHarness::prepare_repo(first_dir.path());
        let mut first = VaultHarness::new(
            first_dir.path(),
            first_repo,
            IsolatedKeyFixture::fixed(first_dir.path().join("keys"), 13),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        let second_repo = VaultHarness::prepare_repo(second_dir.path());
        let mut second = VaultHarness::new(
            second_dir.path(),
            second_repo,
            IsolatedKeyFixture::fixed(second_dir.path().join("keys"), 13),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::write(
            first_dir.path().join("tools/tools.toml"),
            "[values]\nfirst = 2\nsecond = 1\n",
        )
        .unwrap();
        first.sync().unwrap();
        std::fs::write(
            second_dir.path().join("tools/tools.toml"),
            "[values]\nfirst = 1\nsecond = 2\n",
        )
        .unwrap();

        second.sync().unwrap();
        let merged = toml::from_str::<toml::Value>(
            &std::fs::read_to_string(second_dir.path().join("tools/tools.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(merged["values"]["first"].as_integer(), Some(2));
        assert_eq!(merged["values"]["second"].as_integer(), Some(2));
    }

    #[test]
    fn opaque_conflicts_keep_remote_and_create_one_local_copy() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::create_dir_all(first_dir.path().join("knowledge")).unwrap();
        std::fs::write(first_dir.path().join("knowledge/data.bin"), b"baseline").unwrap();
        let first_repo = VaultHarness::prepare_repo(first_dir.path());
        let mut first = VaultHarness::new(
            first_dir.path(),
            first_repo,
            IsolatedKeyFixture::fixed(first_dir.path().join("keys"), 14),
        );
        first.connect_remote(remote.to_str().unwrap()).unwrap();

        let second_repo = VaultHarness::prepare_repo(second_dir.path());
        let mut second = VaultHarness::new(
            second_dir.path(),
            second_repo,
            IsolatedKeyFixture::fixed(second_dir.path().join("keys"), 14),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        std::fs::write(first_dir.path().join("knowledge/data.bin"), b"remote").unwrap();
        first.sync().unwrap();
        std::fs::write(second_dir.path().join("knowledge/data.bin"), b"local").unwrap();

        let result = second.sync().unwrap();
        second.sync().unwrap();

        assert!(result.contains("1 conflicted copy"));
        assert_eq!(
            std::fs::read(second_dir.path().join("knowledge/data.bin")).unwrap(),
            b"remote"
        );
        let copies: Vec<_> = std::fs::read_dir(second_dir.path().join("knowledge"))
            .unwrap()
            .flatten()
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with("data (Conflicted copy ") && name.ends_with(".bin")
            })
            .collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(std::fs::read(copies[0].path()).unwrap(), b"local");
    }

    #[test]
    fn plaintext_remote_history_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let seed = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        VaultHarness::git(seed.path(), &["init", "-b", "main"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "user.name", "Vmux Test"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "user.email", "vmux@example.com"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "commit.gpgSign", "false"]).unwrap();
        std::fs::write(seed.path().join("settings.ron"), "()\n").unwrap();
        VaultHarness::git(seed.path(), &["add", "--all"]).unwrap();
        VaultHarness::git(seed.path(), &["commit", "-m", "Plaintext"]).unwrap();
        VaultHarness::git(
            seed.path(),
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        VaultHarness::git(seed.path(), &["push", "-u", "origin", "main"]).unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 7),
        );

        let error = harness
            .connect_remote(remote.to_str().unwrap())
            .unwrap_err();
        assert!(error.contains("plaintext"));
        assert!(VaultHarness::git(&repo, &["remote", "get-url", "origin"]).is_err());
    }

    #[test]
    fn existing_vault_uses_the_remote_default_branch() {
        let seed = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
        let seed_repo = VaultHarness::prepare_repo(seed.path());
        let mut first = VaultHarness::new(
            seed.path(),
            seed_repo.clone(),
            IsolatedKeyFixture::fixed(seed.path().join("keys"), 8),
        );
        first.initialize().unwrap();
        VaultHarness::git(&seed_repo, &["branch", "--move", "trunk"]).unwrap();
        VaultHarness::git(
            &seed_repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        VaultHarness::git(&seed_repo, &["push", "-u", "origin", "trunk"]).unwrap();
        let output = Command::new("git")
            .args([
                "--git-dir",
                remote.to_str().unwrap(),
                "symbolic-ref",
                "HEAD",
                "refs/heads/trunk",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());

        let repo = VaultHarness::prepare_repo(root.path());
        let mut second = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 8),
        );
        second.connect_remote(remote.to_str().unwrap()).unwrap();

        assert_eq!(
            VaultHarness::git(&repo, &["branch", "--show-current"]).unwrap(),
            "trunk"
        );
        assert_eq!(
            VaultHarness::git(&repo, &["rev-list", "--count", "origin/trunk"]).unwrap(),
            "1"
        );
    }

    #[test]
    fn cloud_folder_creates_an_encrypted_bare_vault_repository() {
        let root = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();
        let mut harness = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 9),
        );

        let remote = harness.connect_folder(cloud.path()).unwrap();

        assert_eq!(
            VaultHarness::git(&repo, &["remote", "get-url", "origin"]).unwrap(),
            remote
        );
        let output = Command::new("git")
            .args(["--git-dir", &remote, "rev-parse", "--is-bare-repository"])
            .output()
            .unwrap();
        assert_eq!(ProcessOutcome::from_output(output).text().unwrap(), "true");
        let tree =
            VaultHarness::git(&repo, &["ls-tree", "-r", "--name-only", "origin/main"]).unwrap();
        assert!(!tree.contains("settings.ron"));
    }

    #[test]
    fn recovery_key_unlocks_knowledge_and_tools_on_a_new_device() {
        let first_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(first_dir.path().join("knowledge")).unwrap();
        std::fs::create_dir_all(first_dir.path().join("tools")).unwrap();
        std::fs::write(first_dir.path().join("knowledge/private.md"), "# Private\n").unwrap();
        std::fs::write(
            first_dir.path().join("tools/tools.toml"),
            "version = 1\n[homebrew]\npackages = [\"ripgrep\"]\n",
        )
        .unwrap();
        std::fs::write(
            first_dir.path().join("tools/Brewfile"),
            "brew \"ripgrep\"\n",
        )
        .unwrap();
        let first_repo = VaultHarness::prepare_repo(first_dir.path());
        let first_fixture = IsolatedKeyFixture::fixed(first_dir.path().join("keys"), 43);
        let mut first = VaultHarness::new(first_dir.path(), first_repo.clone(), first_fixture);
        first.initialize().unwrap();
        let generated = GeneratedRecoveryKey::generate().unwrap();
        let recovery_key = generated.display().to_string();
        let recovery = VaultRecovery::from_storage(&first.storage);
        let op = first.spawn_base(
            "test.vault.create_recovery",
            KeyPermissions {
                load: true,
                observe: false,
                create: false,
                store: false,
            },
        );
        let port = first.app.world().get::<KeyPort>(op).unwrap().clone();
        let handle = std::thread::spawn(move || recovery.create(generated, &port));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !handle.is_finished() {
            first.app.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        handle.join().unwrap().unwrap();
        first.app.world_mut().despawn(op);

        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        VaultHarness::git(
            &first_repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        VaultHarness::git(&first_repo, &["push", "-u", "origin", "main"]).unwrap();

        let second_dir = tempfile::tempdir().unwrap();
        let second_repo = VaultHarness::prepare_repo(second_dir.path());
        let second_fixture = IsolatedKeyFixture::empty(second_dir.path().join("keys"));
        let mut second = VaultHarness::new(second_dir.path(), second_repo, second_fixture.clone());
        second.connect_remote(remote.to_str().unwrap()).unwrap();
        assert!(!second_dir.path().join("knowledge/private.md").exists());

        second.unlock_recovery(&recovery_key).unwrap();
        assert_eq!(
            std::fs::read_to_string(second_dir.path().join("knowledge/private.md")).unwrap(),
            "# Private\n"
        );
        assert_eq!(
            std::fs::read_to_string(second_dir.path().join("tools/Brewfile")).unwrap(),
            "brew \"ripgrep\"\n"
        );
        assert_eq!(
            second_fixture.stored_bytes().as_deref(),
            Some(&[43; 32][..])
        );
        assert!(
            second
                .unlock_recovery(
                    "vmux-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000"
                )
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_retries_exactly_twice_on_remote_change_refusal() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "(v: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 21),
        );
        harness.connect_remote(remote.to_str().unwrap()).unwrap();

        let count_file = remote_dir.path().join("push-count");
        let hook_path = remote.join("hooks/pre-receive");
        let script = format!(
            "#!/bin/sh\nc=0\nif [ -f '{0}' ]; then c=$(cat '{0}'); fi\nc=$((c+1))\necho $c > '{0}'\nif [ $c -le 2 ]; then echo 'rejected: non-fast-forward' >&2; exit 1; fi\nexit 0\n",
            count_file.display()
        );
        std::fs::write(&hook_path, script).unwrap();
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(root.path().join("settings.ron"), "(v: 2)\n").unwrap();
        harness.sync().unwrap();
        assert_eq!(std::fs::read_to_string(&count_file).unwrap().trim(), "3");

        let count_file_fail = remote_dir.path().join("push-count-fail");
        let fail_script = format!(
            "#!/bin/sh\nc=0\nif [ -f '{0}' ]; then c=$(cat '{0}'); fi\nc=$((c+1))\necho $c > '{0}'\necho 'rejected: fetch first' >&2\nexit 1\n",
            count_file_fail.display()
        );
        std::fs::write(&hook_path, fail_script).unwrap();
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(root.path().join("settings.ron"), "(v: 3)\n").unwrap();
        let error = harness.sync().unwrap_err();
        assert_eq!(error, "Vault remote kept changing during sync");
        assert_eq!(
            std::fs::read_to_string(&count_file_fail).unwrap().trim(),
            "3"
        );
    }

    #[cfg(unix)]
    #[test]
    fn fatal_push_refusal_does_not_retry() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "(v: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 22),
        );
        harness.connect_remote(remote.to_str().unwrap()).unwrap();

        let count_file = remote_dir.path().join("fatal-count");
        let hook_path = remote.join("hooks/pre-receive");
        let script = format!(
            "#!/bin/sh\nc=0\nif [ -f '{0}' ]; then c=$(cat '{0}'); fi\nc=$((c+1))\necho $c > '{0}'\necho 'authentication permission denied' >&2\nexit 1\n",
            count_file.display()
        );
        std::fs::write(&hook_path, script).unwrap();
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(root.path().join("settings.ron"), "(v: 2)\n").unwrap();
        let error = harness.sync().unwrap_err();
        assert!(error.contains("permission denied"));
        assert_eq!(std::fs::read_to_string(&count_file).unwrap().trim(), "1");
    }

    #[test]
    fn concurrent_local_edit_is_preserved_without_overwrite() {
        IoTaskPool::get_or_init(TaskPool::new);
        let root = tempfile::tempdir().unwrap();
        let target_path = root.path().join("settings.ron");
        std::fs::write(&target_path, "(v: 1)\n").unwrap();
        let local_captured = VaultSync::collect(root.path()).unwrap();
        let baseline = local_captured.clone();
        let mut remote = local_captured.clone();
        remote.insert(
            "settings.ron".to_string(),
            remote["settings.ron"].with_data(b"(v: 99)\n".to_vec()),
        );
        std::fs::write(&target_path, "(user_edit: true, long_content: 12345)\n").unwrap();

        let mut app = App::new();
        app.add_plugins(ReconcilePlugin);
        let owner = app.world_mut().spawn_empty().id();
        let session = app
            .world_mut()
            .spawn((
                Operation::bundle("vault.reconcile").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                ReconcileSession,
                ReconcileRoot(root.path().to_path_buf()),
                VaultWake(None),
                ReconcileInputs {
                    baseline,
                    local: local_captured,
                    remote,
                },
                ReconcileCounts::default(),
                ReconcileSpawning,
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            app.update();
            if let Some(done) = app.world().get::<ReconcileDone>(session) {
                break done.0.clone();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(result.unwrap_err().contains("changed during sync"));
        assert_eq!(
            std::fs::read_to_string(&target_path).unwrap(),
            "(user_edit: true, long_content: 12345)\n"
        );
    }

    #[test]
    fn stale_origin_compensation_leaves_replacement_origin_intact() {
        let root = tempfile::tempdir().unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        VaultHarness::git(&repo, &["remote", "add", "origin", "/previous/origin.git"]).unwrap();
        let mut harness = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 24),
        );
        let missing_remote = root.path().join("nonexistent-remote.git");
        let error = harness
            .connect_remote(missing_remote.to_str().unwrap())
            .unwrap_err();
        assert!(!error.is_empty());
        assert_eq!(
            VaultHarness::git(&repo, &["remote", "get-url", "origin"]).unwrap(),
            "/previous/origin.git"
        );

        let seed = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        VaultHarness::git(seed.path(), &["init", "-b", "main"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "user.name", "Vmux Test"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "user.email", "vmux@example.com"]).unwrap();
        VaultHarness::git(seed.path(), &["config", "commit.gpgSign", "false"]).unwrap();
        std::fs::write(seed.path().join("plaintext.txt"), "bad\n").unwrap();
        VaultHarness::git(seed.path(), &["add", "--all"]).unwrap();
        VaultHarness::git(seed.path(), &["commit", "-m", "Bad"]).unwrap();
        VaultHarness::git(
            seed.path(),
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        VaultHarness::git(seed.path(), &["push", "-u", "origin", "main"]).unwrap();

        let op = harness.spawn_base(
            "test.vault.stale_comp",
            KeyPermissions {
                load: true,
                observe: false,
                create: true,
                store: true,
            },
        );
        harness.app.world_mut().entity_mut(op).insert((
            ConnectActive,
            ConnectRemoteMode(remote.to_string_lossy().into_owned()),
            ConnectPendingClaim,
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut replaced = false;
        let outcome = loop {
            harness.app.update();
            if !replaced
                && VaultHarness::git(&repo, &["remote", "get-url", "origin"])
                    .ok()
                    .as_deref()
                    == Some(remote.to_string_lossy().as_ref())
            {
                VaultHarness::git(&repo, &["remote", "set-url", "origin", "/newer/owner.git"])
                    .unwrap();
                replaced = true;
            }
            if let Some(outcome) = harness
                .app
                .world_mut()
                .entity_mut(op)
                .take::<VaultOperationOutcome>()
            {
                break outcome.0;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(replaced);
        assert!(outcome.is_err());
        assert_eq!(
            VaultHarness::git(&repo, &["remote", "get-url", "origin"]).unwrap(),
            "/newer/owner.git"
        );
    }

    #[test]
    fn canceled_before_claim_never_runs_sync() {
        let root = tempfile::tempdir().unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 25),
        );
        let op = harness.spawn_sync_op();
        harness
            .app
            .world()
            .get::<KeyCanceled>(op)
            .unwrap()
            .0
            .store(true, Ordering::Release);
        let error = harness.run_until_outcome(op).unwrap_err();
        assert!(error.contains("canceled"));
    }

    #[test]
    fn durable_local_state_failure_is_never_acknowledged_as_success() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "(v: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo.clone(),
            IsolatedKeyFixture::fixed(root.path().join("keys"), 26),
        );
        harness.connect_remote(remote.to_str().unwrap()).unwrap();
        let state_path = VaultRepositoryPath::at(&repo).state_path();
        std::fs::remove_file(&state_path).unwrap();
        std::fs::create_dir(&state_path).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(v: 2)\n").unwrap();

        assert!(harness.sync().is_err());
    }

    #[test]
    fn locking_vault_mid_sync_refuses_replacement_and_drains() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "(v: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 27),
        );
        harness.connect_remote(remote.to_str().unwrap()).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(v: 2)\n").unwrap();
        let op = harness.spawn_sync_op();
        let owner = harness
            .app
            .world_mut()
            .query_filtered::<Entity, With<VaultKey>>()
            .single(harness.app.world())
            .unwrap();
        harness.app.world_mut().write_message(LockVault(owner));
        let error = harness.run_until_outcome(op).unwrap_err();
        assert!(!error.is_empty());
    }

    #[test]
    fn concurrent_sync_operations_serialize_on_repo_claim() {
        let root = tempfile::tempdir().unwrap();
        let remote_dir = tempfile::tempdir().unwrap();
        let remote = remote_dir.path().join("vault.git");
        VaultHarness::create_bare(&remote);
        std::fs::write(root.path().join("settings.ron"), "(v: 1)\n").unwrap();
        let repo = VaultHarness::prepare_repo(root.path());
        let mut harness = VaultHarness::new(
            root.path(),
            repo,
            IsolatedKeyFixture::fixed(root.path().join("keys"), 28),
        );
        harness.connect_remote(remote.to_str().unwrap()).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(v: 2)\n").unwrap();
        let first = harness.spawn_sync_op();
        let second = harness.spawn_sync_op();
        harness.app.update();
        let claims = harness
            .app
            .world_mut()
            .query::<&RepoClaim>()
            .iter(harness.app.world())
            .count();
        assert_eq!(claims, 1);
        assert!(harness.run_until_outcome(first).is_ok());
        assert!(harness.run_until_outcome(second).is_ok());
    }
}
