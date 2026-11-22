use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use vmux_ecs::{Operation, OperationRetained, OperationTarget, OperationTargetRevision};
use zeroize::Zeroizing;

use crate::state::{
    VaultChooseCloudFolderRequest, VaultConnectFolderRequest, VaultConnectRequest,
    VaultCreateCloudFolderRequest, VaultCreateRequest,
};
use crate::storage::{
    Hex, LocalEntry, MANIFEST_FILE, RecoveryEnvelope, RemoteManifest, RepositoryVisibility,
    VaultLocalState, VaultRepositoryPath, VaultSync,
};

use super::key::{
    AcceptedStorage, CreateKey, EcsKeyRequest, KeyAdmission, KeyAuthoritySet, KeyCanceled,
    KeyGeneration, KeyNamespace, KeyOutcome, KeyPermissions, KeyPort, KeyRequester, LoadKey,
    NamespaceGeneration, VaultId, VaultKey,
};
use super::reconcile::{ReconcileDone, ReconcileInputs, ReconcileLauncher, ReconcileRetire};
use super::sync::{SyncRemoteBranch, SyncSet};
use super::vault_io::{
    ActiveVaultOperation, GitProcessTask, ProcessOutcome, RepoClaim, VaultOperationOutcome,
    VaultWake,
};
use super::{
    OperationSet, ReadyVaultOperation, VaultOperationOutput, VaultOperationRequest, VaultRegistry,
    VaultStatusProjection,
};

pub(super) struct ConnectionPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ConnectionSet;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    admit_create,
                    admit_remote,
                    admit_folder,
                    admit_create_cloud_folder,
                    admit_choose_cloud_folder,
                    cancel,
                    claim_repo,
                    drain_folder_dialog,
                    submit_folder_bare_check,
                    drain_folder_bare_check,
                    submit_folder_init_bare,
                    drain_folder_init_bare,
                    submit_ensure_repo,
                    drain_ensure_repo,
                    submit_resolve_url,
                    drain_resolve_url,
                )
                    .chain(),
                (
                    submit_read_previous_origin,
                    drain_read_previous_origin,
                    submit_install_origin,
                    drain_install_origin,
                    submit_fetch,
                    drain_fetch,
                    submit_set_head,
                    drain_set_head,
                    submit_remote_head,
                    drain_remote_head,
                    submit_verify_remote_ref,
                    drain_verify_remote_ref,
                    submit_list_remote_refs,
                    drain_list_remote_refs,
                    submit_validate_log,
                    drain_validate_log,
                    submit_read_remote_manifest,
                    drain_read_remote_manifest,
                )
                    .chain(),
                (
                    submit_remote_key_load,
                    drain_remote_key_load,
                    submit_check_recipients,
                    drain_check_recipients,
                    submit_checkout_branch,
                    drain_checkout_branch,
                    submit_set_upstream,
                    drain_set_upstream,
                    submit_decode_remote,
                    drain_decode_remote,
                    submit_collect_before,
                    drain_collect_before,
                    adopt_reconcile,
                    submit_collect_after,
                    drain_collect_after,
                )
                    .chain(),
                (
                    submit_init_check,
                    drain_init_check,
                    submit_init_key,
                    drain_init_key,
                    drain_init_recovery_check,
                    submit_init_load_previous,
                    drain_init_load_previous,
                    submit_write_snapshot,
                    drain_write_snapshot,
                    submit_git_add,
                    drain_git_add,
                    submit_git_status,
                    drain_git_status,
                    submit_git_commit,
                    drain_git_commit,
                )
                    .chain(),
                (
                    submit_check_create_origin,
                    drain_check_create_origin,
                    submit_gh_repo_create,
                    drain_gh_repo_create,
                    submit_read_current_branch,
                    drain_read_current_branch,
                    submit_push,
                    drain_push,
                    submit_local_state,
                    drain_local_state,
                    submit_restore_origin_check,
                    drain_restore_origin_check,
                    submit_restore_origin_apply,
                    drain_restore_origin_apply,
                    finish_canceled,
                )
                    .chain(),
            )
                .chain()
                .in_set(ConnectionSet)
                .after(KeyAuthoritySet)
                .after(SyncSet)
                .before(OperationSet),
        );
    }
}

#[derive(Component)]
pub(super) struct ConnectActive;

#[derive(Component)]
pub(super) struct ConnectPendingClaim;

#[derive(Component)]
struct ConnectCanceling;

#[derive(Component)]
pub(super) struct ConnectCreateMode {
    pub(super) repository: String,
    pub(super) visibility: RepositoryVisibility,
}

#[derive(Component)]
pub(super) struct ConnectRemoteMode(pub(super) String);

#[derive(Component)]
pub(super) struct ConnectFolderMode {
    pub(super) folder: PathBuf,
    pub(super) require_existing: bool,
}

#[derive(Component)]
struct ConnectResultMessage(String);

#[derive(Component)]
struct ConnectFolderTarget(PathBuf);

#[derive(Component)]
struct ConnectRemoteUrl(String);

#[derive(Component)]
struct OriginMutation {
    previous: Option<String>,
    installed: String,
}

#[derive(Component)]
struct ConnectRemoteBranch(String);

#[derive(Component)]
struct ConnectLocalBranch(String);

#[derive(Component)]
struct ConnectVaultId(String);

#[derive(Component)]
struct ConnectLockedRemote;

#[derive(Component)]
struct ConnectAfterInitPush;

#[derive(Component)]
struct ConnectAfterInitGhCreate;

#[derive(Component)]
pub(super) struct ConnectInitOnly;

#[derive(Component)]
struct ConnectCommitMessage(String);

#[derive(Component)]
struct ConnectKeyLoan {
    key: Zeroizing<Vec<u8>>,
    owner: Option<Entity>,
    generation: Option<u64>,
}

#[derive(Component)]
struct ConnectKeyRequest(Entity);

#[derive(Component)]
struct ConnectKeyError(String);

#[derive(Component)]
struct ConnectRemoteFiles(BTreeMap<String, LocalEntry>);

#[derive(Component)]
struct ConnectLocalFiles(BTreeMap<String, LocalEntry>);

#[derive(Component)]
struct ConnectAwaitReconcile(Entity);

#[derive(Component)]
struct ConnectPendingFailure(String);

#[derive(Component)]
struct ConnectFolderCheckBare;

#[derive(Component)]
struct ConnectFolderInitBare;

#[derive(Component)]
struct ConnectEnsureRepo;

#[derive(Component)]
struct ConnectResolveUrl;

#[derive(Component)]
struct ConnectReadPreviousOrigin;

#[derive(Component)]
struct ConnectInstallOrigin;

#[derive(Component)]
struct ConnectFetch;

#[derive(Component)]
struct ConnectSetHead;

#[derive(Component)]
struct ConnectResolveRemoteHead;

#[derive(Component)]
struct ConnectVerifyRemoteRef(Vec<String>);

#[derive(Component)]
struct ConnectListRemoteRefs;

#[derive(Component)]
struct ConnectValidateLog;

#[derive(Component)]
struct ConnectReadRemoteManifest;

#[derive(Component)]
struct ConnectLoadRemoteKey;

#[derive(Component)]
struct ConnectCheckRecipients;

#[derive(Component)]
struct ConnectCheckoutBranch;

#[derive(Component)]
struct ConnectSetUpstream;

#[derive(Component)]
struct ConnectDecodeRemote;

#[derive(Component)]
struct ConnectCollectBefore;

#[derive(Component)]
struct ConnectCollectAfter;

#[derive(Component)]
struct ConnectInitCheck;

#[derive(Component)]
struct ConnectInitAcquireKey;

#[derive(Component)]
struct ConnectInitCreateKey;

#[derive(Component)]
struct ConnectInitLoadPrevious;

#[derive(Component)]
struct ConnectWriteSnapshot;

#[derive(Component)]
struct ConnectGitAdd;

#[derive(Component)]
struct ConnectGitStatus;

#[derive(Component)]
struct ConnectGitCommit;

#[derive(Component)]
struct ConnectCheckCreateOrigin;

#[derive(Component)]
struct ConnectGhRepoCreate;

#[derive(Component)]
struct ConnectReadCurrentBranch;

#[derive(Component)]
struct ConnectPush;

#[derive(Component)]
struct ConnectWriteLocalState;

#[derive(Component)]
struct ConnectRestoreOriginCheck;

#[derive(Component)]
struct ConnectRestoreOriginApply;

#[derive(Component)]
struct FolderDialogTask(Task<Result<PathBuf, String>>);

#[derive(Component)]
struct FsUnitTask(Task<Result<(), String>>);

#[derive(Component)]
struct InitCheckTask(Task<Result<Option<RemoteManifest>, String>>);

#[derive(Component)]
struct RecoveryCheckTask(Task<bool>);

#[derive(Component)]
struct SnapshotLoadTask(Task<Result<BTreeMap<String, LocalEntry>, String>>);

#[derive(Component)]
struct CollectFilesTask(Task<Result<BTreeMap<String, LocalEntry>, String>>);

#[derive(Component)]
struct SnapshotWriteTask(Task<Result<(), String>>);

#[derive(Component)]
struct LocalStateWriteTask(Task<Result<(), String>>);

#[derive(SystemParam)]
struct ConnectValidity<'w, 's> {
    namespaces: Query<'w, 's, &'static NamespaceGeneration, With<KeyNamespace>>,
    keys: Query<'w, 's, &'static KeyGeneration, With<VaultKey>>,
}

impl ConnectValidity<'_, '_> {
    fn valid(
        &self,
        target: &OperationTarget,
        accepted: &OperationTargetRevision,
        canceled: Option<&KeyCanceled>,
        loan: Option<&ConnectKeyLoan>,
    ) -> bool {
        if canceled.is_some_and(|flag| flag.0.load(Ordering::Acquire)) {
            return false;
        }
        let Ok(generation) = self.namespaces.get(target.entity()) else {
            return false;
        };
        if generation.0 != accepted.0 {
            return false;
        }
        if let Some(loan) = loan
            && let (Some(owner), Some(expected)) = (loan.owner, loan.generation)
            && !self
                .keys
                .get(owner)
                .is_ok_and(|current| current.0 == expected)
        {
            return false;
        }
        true
    }
}

#[derive(SystemParam)]
struct FailureRouter<'w, 's> {
    commands: Commands<'w, 's>,
}

impl FailureRouter<'_, '_> {
    fn fail(&mut self, entity: Entity, has_origin: bool, error: String) {
        if has_origin {
            self.commands
                .entity(entity)
                .insert((ConnectPendingFailure(error), ConnectRestoreOriginCheck));
        } else {
            self.commands
                .entity(entity)
                .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan)>()
                .insert(VaultOperationOutcome(Err(error)));
        }
    }
}

fn admit_create(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultCreateRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
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
            generation.0,
            &mut admission,
            entity,
            wake.0.clone(),
        );
        let visibility = if request.0.private {
            RepositoryVisibility::Private
        } else {
            RepositoryVisibility::Public
        };
        let repository = if request.0.repository.trim().is_empty() {
            "vmux-vault".to_string()
        } else {
            request.0.repository.trim().to_string()
        };
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.create").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.0),
                ActiveVaultOperation,
                ConnectActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                wake.clone(),
                ConnectCreateMode {
                    repository: repository.clone(),
                    visibility,
                },
                ConnectResultMessage(repository),
                ConnectPendingClaim,
            ));
        wake.notify();
    }
}

fn admit_remote(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectRequest>),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
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
            generation.0,
            &mut admission,
            entity,
            wake.0.clone(),
        );
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.0),
                ActiveVaultOperation,
                ConnectActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                wake.clone(),
                ConnectRemoteMode(request.0.repository.clone()),
                ConnectPendingClaim,
            ));
        wake.notify();
    }
}

fn admit_folder(
    operations: Query<
        Entity,
        (
            With<VaultOperationRequest<VaultConnectFolderRequest>>,
            Added<ReadyVaultOperation>,
        ),
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
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
            generation.0,
            &mut admission,
            entity,
            wake.0.clone(),
        );
        let dialog_wake = wake.0.clone();
        let dialog = IoTaskPool::get().spawn(async move {
            let initial_dir = std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join("Library/CloudStorage"))
                .filter(|path| path.is_dir())
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
            let mut dialog = rfd::AsyncFileDialog::new();
            if let Some(initial_dir) = initial_dir {
                dialog = dialog.set_directory(initial_dir);
            }
            let result = match dialog.pick_folder().await {
                Some(folder) => Ok(folder.path().to_path_buf()),
                None => Err(String::new()),
            };
            if let Some(wake) = dialog_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect_folder").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.0),
                ActiveVaultOperation,
                ConnectActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                wake.clone(),
                FolderDialogTask(dialog),
            ));
    }
}

fn admit_create_cloud_folder(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultCreateCloudFolderRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
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
            generation.0,
            &mut admission,
            entity,
            wake.0.clone(),
        );
        let folder = Path::new(&request.0.root).join(&request.0.folder_name);
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.create_cloud_folder")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.0),
                ActiveVaultOperation,
                ConnectActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                wake.clone(),
                ConnectFolderMode {
                    folder,
                    require_existing: false,
                },
                ConnectPendingClaim,
            ));
        wake.notify();
    }
}

fn admit_choose_cloud_folder(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultChooseCloudFolderRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
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
            generation.0,
            &mut admission,
            entity,
            wake.0.clone(),
        );
        let initial_root = PathBuf::from(&request.0.root);
        let dialog_wake = wake.0.clone();
        let dialog = IoTaskPool::get().spawn(async move {
            let mut dialog = rfd::AsyncFileDialog::new();
            if initial_root.is_dir() {
                dialog = dialog.set_directory(initial_root);
            }
            let result = match dialog.pick_folder().await {
                Some(folder) => Ok(folder.path().to_path_buf()),
                None => Err(String::new()),
            };
            if let Some(wake) = dialog_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(namespace).remove::<VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.choose_cloud_folder")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                OperationTargetRevision(generation.0),
                ActiveVaultOperation,
                ConnectActive,
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(Arc::new(AtomicBool::new(false))),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                wake.clone(),
                FolderDialogTask(dialog),
            ));
    }
}

fn cancel(
    validity: ConnectValidity,
    ops: Query<
        (
            Entity,
            &OperationTarget,
            &OperationTargetRevision,
            Option<&KeyCanceled>,
            Option<&ConnectKeyLoan>,
            &VaultWake,
        ),
        (
            With<ConnectActive>,
            Without<ConnectCanceling>,
            Without<VaultOperationOutcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, target, accepted, canceled, loan, wake) in &ops {
        if validity.valid(target, accepted, canceled, loan) {
            continue;
        }
        commands.entity(entity).insert(ConnectCanceling);
        wake.notify();
    }
}

fn drain_folder_dialog(
    mut ops: Query<(
        Entity,
        &VaultWake,
        Has<ConnectCanceling>,
        Has<VaultOperationRequest<VaultChooseCloudFolderRequest>>,
        &mut FolderDialogTask,
    )>,
    mut commands: Commands,
) {
    for (entity, wake, canceling, require_existing, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<FolderDialogTask>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(folder) => {
                commands.entity(entity).insert((
                    ConnectFolderMode {
                        folder,
                        require_existing,
                    },
                    ConnectPendingClaim,
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<ConnectActive>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn claim_repo(
    waiting: Query<
        (
            Entity,
            &AcceptedStorage,
            Option<&ConnectCreateMode>,
            Option<&ConnectRemoteMode>,
            Option<&ConnectFolderMode>,
            Has<ConnectInitOnly>,
            &VaultWake,
        ),
        (
            With<ConnectPendingClaim>,
            Without<RepoClaim>,
            Without<ConnectCanceling>,
        ),
    >,
    existing: Query<&RepoClaim>,
    mut commands: Commands,
) {
    let mut newly_claimed: Vec<RepoClaim> = Vec::new();
    for (entity, storage, create_mode, remote_mode, folder_mode, init_only, wake) in &waiting {
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
            .remove::<ConnectPendingClaim>()
            .insert(claim);
        if init_only {
            commands.entity(entity).insert((
                ConnectCommitMessage("Initialize vmux Vault".to_string()),
                ConnectEnsureRepo,
            ));
        } else if let Some(folder_mode) = folder_mode {
            let remote = if folder_mode
                .folder
                .extension()
                .is_some_and(|ext| ext == "git")
            {
                folder_mode.folder.clone()
            } else {
                folder_mode.folder.join("vmux-vault.git")
            };
            if folder_mode.require_existing && !remote.exists() {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(
                        "selected folder does not contain a Vault".to_string(),
                    )));
            } else if remote.exists() {
                commands
                    .entity(entity)
                    .insert((ConnectFolderTarget(remote), ConnectFolderCheckBare));
            } else {
                commands
                    .entity(entity)
                    .insert((ConnectFolderTarget(remote), ConnectFolderInitBare));
            }
        } else if create_mode.is_some() {
            commands.entity(entity).insert((
                ConnectAfterInitGhCreate,
                ConnectCommitMessage("Initialize vmux Vault".to_string()),
                ConnectEnsureRepo,
            ));
        } else if let Some(remote_mode) = remote_mode {
            let trimmed = remote_mode.0.trim();
            if trimmed.is_empty() {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(
                        "repository is required".to_string()
                    )));
            } else {
                commands.entity(entity).insert(ConnectEnsureRepo);
            }
        }
        wake.notify();
    }
}

fn submit_folder_bare_check(
    ops: Query<
        (Entity, &ConnectFolderTarget, &VaultWake),
        (
            With<ConnectFolderCheckBare>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, remote, wake) in &ops {
        let task = GitProcessTask::spawn_global(
            wake,
            vec![
                "--git-dir".into(),
                remote.0.to_string_lossy().into_owned(),
                "rev-parse".into(),
                "--is-bare-repository".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_folder_bare_check(
    mut ops: Query<
        (
            Entity,
            &ConnectFolderTarget,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectFolderCheckBare>,
    >,
    mut commands: Commands,
) {
    for (entity, remote, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectFolderCheckBare)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(bare) if bare == "true" => {
                let remote_str = remote.0.to_string_lossy().into_owned();
                commands.entity(entity).insert((
                    ConnectRemoteMode(remote_str.clone()),
                    ConnectResultMessage(remote_str),
                    ConnectEnsureRepo,
                ));
            }
            Ok(_) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(
                        "selected folder is not a Vault repository".to_string(),
                    )));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_folder_init_bare(
    ops: Query<
        (Entity, &ConnectFolderMode, &ConnectFolderTarget, &VaultWake),
        (
            With<ConnectFolderInitBare>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mode, remote, wake) in &ops {
        let folder = mode.folder.clone();
        let remote_arg = remote.0.to_string_lossy().into_owned();
        let task = GitProcessTask(wake.spawn(move || {
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            let output = std::process::Command::new("git")
                .env("GIT_TERMINAL_PROMPT", "0")
                .args(["init", "--bare", &remote_arg])
                .output()
                .map_err(|error| format!("failed to run git: {error}"))?;
            Ok(ProcessOutcome::from_output(output))
        }));
        commands.entity(entity).insert(task);
    }
}

fn drain_folder_init_bare(
    mut ops: Query<
        (
            Entity,
            &ConnectFolderTarget,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectFolderInitBare>,
    >,
    mut commands: Commands,
) {
    for (entity, remote, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectFolderInitBare)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                let remote_str = remote.0.to_string_lossy().into_owned();
                commands.entity(entity).insert((
                    ConnectRemoteMode(remote_str.clone()),
                    ConnectResultMessage(remote_str),
                    ConnectEnsureRepo,
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_ensure_repo(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectEnsureRepo>,
            Without<FsUnitTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let task = wake.spawn(move || VaultRepositoryPath::at(&repository).ensure());
        commands.entity(entity).insert(FsUnitTask(task));
    }
}

fn drain_ensure_repo(
    mut ops: Query<
        (
            Entity,
            Has<ConnectAfterInitGhCreate>,
            Has<ConnectInitOnly>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut FsUnitTask,
        ),
        With<ConnectEnsureRepo>,
    >,
    mut commands: Commands,
) {
    for (entity, is_create, init_only, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(FsUnitTask, ConnectEnsureRepo)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                if is_create || init_only {
                    commands.entity(entity).insert(ConnectInitCheck);
                } else {
                    commands.entity(entity).insert(ConnectResolveUrl);
                }
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_resolve_url(
    ops: Query<
        (Entity, &ConnectRemoteMode, &VaultWake),
        (
            With<ConnectResolveUrl>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mode, wake) in &ops {
        let repository = mode.0.trim();
        if repository.contains("://")
            || repository.starts_with("git@")
            || Path::new(repository).is_absolute()
        {
            commands
                .entity(entity)
                .remove::<ConnectResolveUrl>()
                .insert((
                    ConnectRemoteUrl(repository.to_string()),
                    ConnectReadPreviousOrigin,
                ));
            wake.notify();
            continue;
        }
        let task = GitProcessTask::spawn_gh(
            wake,
            None,
            vec![
                "repo".into(),
                "view".into(),
                repository.to_string(),
                "--json".into(),
                "url".into(),
                "--jq".into(),
                ".url".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_resolve_url(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectResolveUrl>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectResolveUrl)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(url) => {
                commands
                    .entity(entity)
                    .insert((ConnectRemoteUrl(url), ConnectReadPreviousOrigin));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_read_previous_origin(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectReadPreviousOrigin>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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

fn drain_read_previous_origin(
    mut ops: Query<
        (
            Entity,
            &ConnectRemoteUrl,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectReadPreviousOrigin>,
    >,
    mut commands: Commands,
) {
    for (entity, url, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectReadPreviousOrigin)>();
        if canceling {
            wake.notify();
            continue;
        }
        let previous = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .filter(|value| !value.is_empty());
        commands.entity(entity).insert((
            OriginMutation {
                previous,
                installed: url.0.clone(),
            },
            ConnectInstallOrigin,
        ));
        wake.notify();
    }
}

fn submit_install_origin(
    ops: Query<
        (Entity, &AcceptedStorage, &OriginMutation, &VaultWake),
        (
            With<ConnectInstallOrigin>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, mutation, wake) in &ops {
        let subcommand = if mutation.previous.is_some() {
            "set-url"
        } else {
            "add"
        };
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "remote".into(),
                subcommand.into(),
                "origin".into(),
                mutation.installed.clone(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_install_origin(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectInstallOrigin>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectInstallOrigin)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectFetch);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim, OriginMutation)>()
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
            With<ConnectFetch>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectFetch>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectFetch)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectSetHead);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_set_head(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectSetHead>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "remote".into(),
                "set-head".into(),
                "origin".into(),
                "--auto".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_set_head(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectSetHead>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        if future::block_on(future::poll_once(&mut task.0)).is_none() {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectSetHead)>();
        if !canceling {
            commands.entity(entity).insert(ConnectResolveRemoteHead);
        }
        wake.notify();
    }
}

fn submit_remote_head(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectResolveRemoteHead>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectResolveRemoteHead>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectResolveRemoteHead)>();
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
            .insert(ConnectVerifyRemoteRef(candidates));
        wake.notify();
    }
}

fn submit_verify_remote_ref(
    ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &ConnectVerifyRemoteRef,
            &VaultWake,
        ),
        (Without<GitProcessTask>, Without<ConnectCanceling>),
    >,
    mut commands: Commands,
) {
    for (entity, storage, candidates, wake) in &ops {
        let Some(candidate) = candidates.0.first() else {
            commands
                .entity(entity)
                .remove::<ConnectVerifyRemoteRef>()
                .insert(ConnectListRemoteRefs);
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
        &mut ConnectVerifyRemoteRef,
        &VaultWake,
        Has<ConnectCanceling>,
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
            commands.entity(entity).remove::<ConnectVerifyRemoteRef>();
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
                .remove::<ConnectVerifyRemoteRef>()
                .insert((ConnectRemoteBranch(checked), ConnectValidateLog));
        } else if candidates.0.is_empty() {
            commands
                .entity(entity)
                .remove::<ConnectVerifyRemoteRef>()
                .insert(ConnectListRemoteRefs);
        }
        wake.notify();
    }
}

fn submit_list_remote_refs(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectListRemoteRefs>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectListRemoteRefs>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectListRemoteRefs)>();
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
                .insert((ConnectRemoteBranch(branches[0].clone()), ConnectValidateLog));
        } else {
            commands.entity(entity).insert((
                ConnectAfterInitPush,
                ConnectCommitMessage("Initialize vmux Vault".to_string()),
                ConnectInitCheck,
            ));
        }
        wake.notify();
    }
}

fn submit_validate_log(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectRemoteBranch, &VaultWake),
        (
            With<ConnectValidateLog>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectValidateLog>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectValidateLog)>();
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
                    commands.entity(entity).insert(ConnectReadRemoteManifest);
                } else {
                    failures.fail(
                        entity,
                        has_origin,
                        "selected repository contains plaintext or non-Vault history".to_string(),
                    );
                }
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_read_remote_manifest(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectRemoteBranch, &VaultWake),
        (
            With<ConnectReadRemoteManifest>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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

fn drain_read_remote_manifest(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectReadRemoteManifest>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectReadRemoteManifest)>();
        if canceling {
            wake.notify();
            continue;
        }
        let manifest = result
            .and_then(ProcessOutcome::text)
            .and_then(|source| RemoteManifest::try_from(source.as_bytes()));
        match manifest {
            Ok(manifest) => {
                commands
                    .entity(entity)
                    .insert((ConnectVaultId(manifest.vault_id), ConnectLoadRemoteKey));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_remote_key_load(
    ops: Query<
        (Entity, &KeyPort, &ConnectVaultId, &VaultWake),
        (
            With<ConnectLoadRemoteKey>,
            Without<ConnectKeyRequest>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, port, vault_id, wake) in &ops {
        let request = commands
            .spawn((
                Operation::bundle("vault.key.load").unwrap().running(),
                OperationRetained,
                KeyRequester(port.origin),
                VaultId(vault_id.0.clone()),
                LoadKey,
                EcsKeyRequest,
            ))
            .id();
        commands.entity(entity).insert(ConnectKeyRequest(request));
        wake.notify();
    }
}

fn drain_remote_key_load(
    ops: Query<
        (
            Entity,
            &ConnectKeyRequest,
            &VaultWake,
            Has<ConnectCanceling>,
        ),
        With<ConnectLoadRemoteKey>,
    >,
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
            .remove::<(ConnectKeyRequest, ConnectLoadRemoteKey)>();
        if canceling {
            wake.notify();
            continue;
        }
        match &outcome.result {
            Ok(key) => {
                commands.entity(entity).insert((
                    ConnectKeyLoan {
                        key: Zeroizing::new(key.to_vec()),
                        owner: outcome.owner,
                        generation: outcome.generation,
                    },
                    ConnectCheckoutBranch,
                ));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ConnectKeyError(error.clone()), ConnectCheckRecipients));
            }
        }
        wake.notify();
    }
}

fn submit_check_recipients(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectRemoteBranch, &VaultWake),
        (
            With<ConnectCheckRecipients>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "ls-tree".into(),
                "-r".into(),
                "--name-only".into(),
                remote.0.clone(),
                "keys".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_check_recipients(
    mut ops: Query<
        (
            Entity,
            &ConnectKeyError,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectCheckRecipients>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, key_error, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectCheckRecipients, ConnectKeyError)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(keys) if !keys.is_empty() => {
                commands
                    .entity(entity)
                    .insert((ConnectLockedRemote, ConnectCheckoutBranch));
            }
            Ok(_) => failures.fail(entity, has_origin, key_error.0.clone()),
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_checkout_branch(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectRemoteBranch, &VaultWake),
        (
            With<ConnectCheckoutBranch>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let branch = remote
            .0
            .strip_prefix("origin/")
            .unwrap_or(&remote.0)
            .to_string();
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "checkout".into(),
                "-B".into(),
                branch.clone(),
                remote.0.clone(),
            ],
        );
        commands
            .entity(entity)
            .insert((ConnectLocalBranch(branch), task));
    }
}

fn drain_checkout_branch(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectCheckoutBranch>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectCheckoutBranch)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectSetUpstream);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_set_upstream(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectRemoteBranch, &VaultWake),
        (
            With<ConnectSetUpstream>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "branch".into(),
                "--set-upstream-to".into(),
                remote.0.clone(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_set_upstream(
    mut ops: Query<
        (
            Entity,
            Option<&ConnectResultMessage>,
            Option<&ConnectRemoteUrl>,
            Has<ConnectLockedRemote>,
            Has<OriginMutation>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectSetUpstream>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, result_message, url, locked, has_origin, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectSetUpstream)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) if locked => {
                let message = result_message
                    .map(|m| m.0.clone())
                    .or_else(|| url.map(|u| u.0.clone()))
                    .unwrap_or_default();
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim, OriginMutation)>()
                    .insert(VaultOperationOutcome(Ok(VaultOperationOutput::message(
                        message,
                    ))));
            }
            Ok(_) => {
                commands.entity(entity).insert((
                    ConnectCommitMessage("Connect vmux Vault".to_string()),
                    ConnectDecodeRemote,
                ));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_decode_remote(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectKeyLoan, &VaultWake),
        (
            With<ConnectDecodeRemote>,
            Without<SnapshotLoadTask>,
            Without<ConnectCanceling>,
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
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut SnapshotLoadTask,
        ),
        With<ConnectDecodeRemote>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(SnapshotLoadTask, ConnectDecodeRemote)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(files) => {
                commands
                    .entity(entity)
                    .insert((ConnectRemoteFiles(files), ConnectCollectBefore));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_collect_before(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectCollectBefore>,
            Without<CollectFilesTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        let task = wake.spawn(move || VaultSync::collect(&root));
        commands.entity(entity).insert(CollectFilesTask(task));
    }
}

fn drain_collect_before(
    mut ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &ConnectRemoteFiles,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut CollectFilesTask,
        ),
        With<ConnectCollectBefore>,
    >,
    mut reconcile: ReconcileLauncher,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, storage, remote, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(CollectFilesTask, ConnectCollectBefore)>();
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
                        baseline: BTreeMap::new(),
                        local,
                        remote: remote.0.clone(),
                    },
                );
                commands
                    .entity(entity)
                    .insert(ConnectAwaitReconcile(session));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn adopt_reconcile(
    ops: Query<(
        Entity,
        &ConnectAwaitReconcile,
        &VaultWake,
        Has<OriginMutation>,
        Has<ConnectCanceling>,
    )>,
    sessions: Query<&ReconcileDone>,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wait, wake, has_origin, canceling) in &ops {
        let Ok(done) = sessions.get(wait.0) else {
            continue;
        };
        commands.entity(wait.0).insert(ReconcileRetire);
        commands.entity(entity).remove::<ConnectAwaitReconcile>();
        if canceling {
            wake.notify();
            continue;
        }
        match &done.0 {
            Ok(_) => {
                commands.entity(entity).insert(ConnectCollectAfter);
            }
            Err(error) => failures.fail(entity, has_origin, error.clone()),
        }
        wake.notify();
    }
}

fn submit_collect_after(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectCollectAfter>,
            Without<CollectFilesTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        let task = wake.spawn(move || VaultSync::collect(&root));
        commands.entity(entity).insert(CollectFilesTask(task));
    }
}

fn drain_collect_after(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut CollectFilesTask,
        ),
        With<ConnectCollectAfter>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(CollectFilesTask, ConnectCollectAfter)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(files) => {
                commands
                    .entity(entity)
                    .insert((ConnectLocalFiles(files), ConnectWriteSnapshot));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_init_check(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectInitCheck>,
            Without<InitCheckTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let task = wake.spawn(move || {
            let vault = VaultRepositoryPath::at(&repository);
            match vault.manifest() {
                Ok(manifest) => Ok(Some(manifest)),
                Err(_) => {
                    vault.validate_empty()?;
                    Ok(None)
                }
            }
        });
        commands.entity(entity).insert(InitCheckTask(task));
    }
}

fn drain_init_check(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut InitCheckTask,
        ),
        With<ConnectInitCheck>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(InitCheckTask, ConnectInitCheck)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(Some(manifest)) => {
                commands
                    .entity(entity)
                    .insert((ConnectVaultId(manifest.vault_id), ConnectInitAcquireKey));
            }
            Ok(None) => match Hex::random(16) {
                Ok(vault_id) => {
                    commands
                        .entity(entity)
                        .insert((ConnectVaultId(vault_id), ConnectInitCreateKey));
                }
                Err(error) => failures.fail(entity, has_origin, error),
            },
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_init_key(
    ops: Query<
        (
            Entity,
            &KeyPort,
            &ConnectVaultId,
            Has<ConnectInitCreateKey>,
            &VaultWake,
        ),
        (
            Or<(With<ConnectInitAcquireKey>, With<ConnectInitCreateKey>)>,
            Without<ConnectKeyRequest>,
            Without<RecoveryCheckTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, port, vault_id, creating, wake) in &ops {
        let request = if creating {
            commands
                .spawn((
                    Operation::bundle("vault.key.create").unwrap().running(),
                    OperationRetained,
                    KeyRequester(port.origin),
                    VaultId(vault_id.0.clone()),
                    CreateKey,
                    EcsKeyRequest,
                ))
                .id()
        } else {
            commands
                .spawn((
                    Operation::bundle("vault.key.load").unwrap().running(),
                    OperationRetained,
                    KeyRequester(port.origin),
                    VaultId(vault_id.0.clone()),
                    LoadKey,
                    EcsKeyRequest,
                ))
                .id()
        };
        commands.entity(entity).insert(ConnectKeyRequest(request));
        wake.notify();
    }
}

fn drain_init_key(
    ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &ConnectKeyRequest,
            Has<ConnectInitCreateKey>,
            Has<OriginMutation>,
            &VaultWake,
            Has<ConnectCanceling>,
        ),
        Or<(With<ConnectInitAcquireKey>, With<ConnectInitCreateKey>)>,
    >,
    outcomes: Query<&KeyOutcome>,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, storage, request, creating, has_origin, wake, canceling) in &ops {
        let Ok(outcome) = outcomes.get(request.0) else {
            continue;
        };
        commands.entity(request.0).despawn();
        commands.entity(entity).remove::<ConnectKeyRequest>();
        if canceling {
            commands
                .entity(entity)
                .remove::<(ConnectInitAcquireKey, ConnectInitCreateKey)>();
            wake.notify();
            continue;
        }
        match &outcome.result {
            Ok(key) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectInitAcquireKey, ConnectInitCreateKey)>()
                    .insert(ConnectKeyLoan {
                        key: Zeroizing::new(key.to_vec()),
                        owner: outcome.owner,
                        generation: outcome.generation,
                    });
                if creating {
                    commands.entity(entity).insert(ConnectCollectAfter);
                } else {
                    commands.entity(entity).insert(ConnectInitLoadPrevious);
                }
            }
            Err(error) if creating => {
                commands.entity(entity).remove::<ConnectInitCreateKey>();
                failures.fail(entity, has_origin, error.clone());
            }
            Err(error) => {
                let repository = storage.0.repository().to_path_buf();
                let task = wake.spawn(move || {
                    RecoveryEnvelope::read(&repository).is_ok_and(|envelope| envelope.is_some())
                });
                commands
                    .entity(entity)
                    .insert((ConnectKeyError(error.clone()), RecoveryCheckTask(task)));
            }
        }
        wake.notify();
    }
}

fn drain_init_recovery_check(
    mut ops: Query<
        (
            Entity,
            &ConnectKeyError,
            Has<OriginMutation>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut RecoveryCheckTask,
        ),
        With<ConnectInitAcquireKey>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, error, has_origin, wake, canceling, mut task) in &mut ops {
        let Some(has_recovery) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(RecoveryCheckTask, ConnectKeyError, ConnectInitAcquireKey)>();
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
        failures.fail(entity, has_origin, message);
        wake.notify();
    }
}

fn submit_init_load_previous(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectKeyLoan, &VaultWake),
        (
            With<ConnectInitLoadPrevious>,
            Without<SnapshotLoadTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, loan, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let key = Zeroizing::new(loan.key.to_vec());
        let task = wake.spawn(move || {
            let files = VaultRepositoryPath::at(&repository)
                .load_encrypted_snapshot(&key)
                .map(|(_, files)| files)
                .unwrap_or_default();
            Ok(files)
        });
        commands.entity(entity).insert(SnapshotLoadTask(task));
    }
}

fn drain_init_load_previous(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut SnapshotLoadTask,
        ),
        With<ConnectInitLoadPrevious>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(SnapshotLoadTask, ConnectInitLoadPrevious)>();
        if canceling {
            wake.notify();
            continue;
        }
        if let Ok(files) = result
            && !files.is_empty()
        {
            commands.entity(entity).insert(ConnectRemoteFiles(files));
        }
        commands.entity(entity).insert(ConnectCollectAfter);
        wake.notify();
    }
}

fn submit_write_snapshot(
    ops: Query<
        (
            Entity,
            &AcceptedStorage,
            &ConnectVaultId,
            &ConnectKeyLoan,
            &ConnectLocalFiles,
            Option<&ConnectRemoteFiles>,
            &VaultWake,
        ),
        (
            With<ConnectWriteSnapshot>,
            Without<SnapshotWriteTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, vault_id, loan, files, previous, wake) in &ops {
        let repository = storage.0.repository().to_path_buf();
        let vault_id = vault_id.0.clone();
        let key = Zeroizing::new(loan.key.to_vec());
        let files = files.0.clone();
        let previous = previous.map(|p| p.0.clone());
        let task = wake.spawn(move || {
            VaultRepositoryPath::at(&repository).write_encrypted_snapshot(
                &vault_id,
                &key,
                &files,
                previous.as_ref(),
            )
        });
        commands.entity(entity).insert(SnapshotWriteTask(task));
    }
}

fn drain_write_snapshot(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut SnapshotWriteTask,
        ),
        With<ConnectWriteSnapshot>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(SnapshotWriteTask, ConnectWriteSnapshot, ConnectLocalFiles)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                commands.entity(entity).insert(ConnectGitAdd);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_git_add(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectGitAdd>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectGitAdd>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectGitAdd)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectGitStatus);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_git_status(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectGitStatus>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
        (
            Entity,
            Has<ConnectAfterInitGhCreate>,
            Has<ConnectAfterInitPush>,
            Has<ConnectInitOnly>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectGitStatus>,
    >,
    mut commands: Commands,
) {
    for (entity, gh_create, init_push, init_only, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectGitStatus)>();
        if canceling {
            wake.notify();
            continue;
        }
        let status = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default();
        if !status.is_empty() {
            commands.entity(entity).insert(ConnectGitCommit);
        } else if init_only {
            commands
                .entity(entity)
                .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan)>()
                .insert(VaultOperationOutcome(Ok(VaultOperationOutput::message(
                    String::new(),
                ))));
        } else if gh_create {
            commands.entity(entity).insert(ConnectCheckCreateOrigin);
        } else if init_push {
            commands.entity(entity).insert(ConnectReadCurrentBranch);
        } else {
            commands.entity(entity).insert(ConnectPush);
        }
        wake.notify();
    }
}

fn submit_git_commit(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectCommitMessage, &VaultWake),
        (
            With<ConnectGitCommit>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, message, wake) in &ops {
        let task = GitProcessTask::spawn(
            wake,
            storage.0.repository().to_path_buf(),
            vec![
                "-c".into(),
                "commit.gpgSign=false".into(),
                "commit".into(),
                "-m".into(),
                message.0.clone(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_git_commit(
    mut ops: Query<
        (
            Entity,
            Has<ConnectAfterInitGhCreate>,
            Has<ConnectAfterInitPush>,
            Has<ConnectInitOnly>,
            Has<OriginMutation>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectGitCommit>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, gh_create, init_push, init_only, has_origin, wake, canceling, mut task) in &mut ops
    {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectGitCommit)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) if init_only => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan)>()
                    .insert(VaultOperationOutcome(Ok(VaultOperationOutput::message(
                        String::new(),
                    ))));
            }
            Ok(_) if gh_create => {
                commands.entity(entity).insert(ConnectCheckCreateOrigin);
            }
            Ok(_) if init_push => {
                commands.entity(entity).insert(ConnectReadCurrentBranch);
            }
            Ok(_) => {
                commands.entity(entity).insert(ConnectPush);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_check_create_origin(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectCheckCreateOrigin>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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

fn drain_check_create_origin(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectCheckCreateOrigin>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectCheckCreateOrigin)>();
        if canceling {
            wake.notify();
            continue;
        }
        let origin = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default();
        if origin.is_empty() {
            commands.entity(entity).insert(ConnectGhRepoCreate);
        } else {
            commands
                .entity(entity)
                .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan)>()
                .insert(VaultOperationOutcome(Err(
                    "Vault already has an origin remote".to_string(),
                )));
        }
        wake.notify();
    }
}

fn submit_gh_repo_create(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectCreateMode, &VaultWake),
        (
            With<ConnectGhRepoCreate>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, mode, wake) in &ops {
        let root_arg = storage.0.repository().to_string_lossy().into_owned();
        let visibility = match mode.visibility {
            RepositoryVisibility::Private => "--private",
            RepositoryVisibility::Public => "--public",
        };
        let task = GitProcessTask::spawn_gh(
            wake,
            Some(storage.0.repository().to_path_buf()),
            vec![
                "repo".into(),
                "create".into(),
                mode.repository.clone(),
                visibility.into(),
                "--source".into(),
                root_arg,
                "--remote".into(),
                "origin".into(),
                "--push".into(),
            ],
        );
        commands.entity(entity).insert(task);
    }
}

fn drain_gh_repo_create(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectGhRepoCreate>,
    >,
    mut commands: Commands,
) {
    for (entity, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectGhRepoCreate)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectWriteLocalState);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan)>()
                    .insert(VaultOperationOutcome(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_read_current_branch(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectReadCurrentBranch>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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

fn drain_read_current_branch(
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectReadCurrentBranch>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectReadCurrentBranch)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(branch) if !branch.is_empty() => {
                commands
                    .entity(entity)
                    .insert((ConnectLocalBranch(branch), ConnectPush));
            }
            Ok(_) => failures.fail(
                entity,
                has_origin,
                "Vault has no current branch".to_string(),
            ),
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_push(
    ops: Query<
        (Entity, &AcceptedStorage, &ConnectLocalBranch, &VaultWake),
        (
            With<ConnectPush>,
            Without<GitProcessTask>,
            Without<ConnectCanceling>,
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
    mut ops: Query<
        (
            Entity,
            &VaultWake,
            Has<OriginMutation>,
            Has<ConnectCanceling>,
            &mut GitProcessTask,
        ),
        With<ConnectPush>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, wake, has_origin, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectPush)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result.and_then(ProcessOutcome::text) {
            Ok(_) => {
                commands.entity(entity).insert(ConnectWriteLocalState);
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_local_state(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectWriteLocalState>,
            Without<LocalStateWriteTask>,
            Without<ConnectCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, storage, wake) in &ops {
        let root = storage.0.root().to_path_buf();
        let repository = storage.0.repository().to_path_buf();
        let task = wake.spawn(move || VaultLocalState::write(&root, &repository));
        commands.entity(entity).insert(LocalStateWriteTask(task));
    }
}

fn drain_local_state(
    mut ops: Query<
        (
            Entity,
            Option<&ConnectResultMessage>,
            Option<&ConnectRemoteUrl>,
            Has<OriginMutation>,
            &VaultWake,
            Has<ConnectCanceling>,
            &mut LocalStateWriteTask,
        ),
        With<ConnectWriteLocalState>,
    >,
    mut failures: FailureRouter,
    mut commands: Commands,
) {
    for (entity, result_message, url, has_origin, wake, canceling, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(LocalStateWriteTask, ConnectWriteLocalState)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                let message = result_message
                    .map(|m| m.0.clone())
                    .or_else(|| url.map(|u| u.0.clone()))
                    .unwrap_or_default();
                commands
                    .entity(entity)
                    .remove::<(ConnectActive, RepoClaim, ConnectKeyLoan, OriginMutation)>()
                    .insert(VaultOperationOutcome(Ok(VaultOperationOutput::message(
                        message,
                    ))));
            }
            Err(error) => failures.fail(entity, has_origin, error),
        }
        wake.notify();
    }
}

fn submit_restore_origin_check(
    ops: Query<
        (Entity, &AcceptedStorage, &VaultWake),
        (
            With<ConnectRestoreOriginCheck>,
            Without<GitProcessTask>,
            Without<FsUnitTask>,
            Without<InitCheckTask>,
            Without<ConnectKeyRequest>,
            Without<RecoveryCheckTask>,
            Without<SnapshotLoadTask>,
            Without<CollectFilesTask>,
            Without<ConnectAwaitReconcile>,
            Without<SnapshotWriteTask>,
            Without<LocalStateWriteTask>,
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

fn drain_restore_origin_check(
    mut ops: Query<
        (
            Entity,
            &OriginMutation,
            &ConnectPendingFailure,
            &VaultWake,
            &mut GitProcessTask,
        ),
        With<ConnectRestoreOriginCheck>,
    >,
    mut commands: Commands,
) {
    for (entity, mutation, failure, wake, mut task) in &mut ops {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(GitProcessTask, ConnectRestoreOriginCheck)>();
        let current = result
            .ok()
            .and_then(|outcome| outcome.text().ok())
            .unwrap_or_default();
        if current == mutation.installed {
            commands.entity(entity).insert(ConnectRestoreOriginApply);
        } else {
            let error = failure.0.clone();
            commands
                .entity(entity)
                .remove::<(
                    ConnectActive,
                    ConnectCanceling,
                    RepoClaim,
                    ConnectKeyLoan,
                    OriginMutation,
                    ConnectPendingFailure,
                )>()
                .insert(VaultOperationOutcome(Err(error)));
        }
        wake.notify();
    }
}

fn submit_restore_origin_apply(
    ops: Query<
        (Entity, &AcceptedStorage, &OriginMutation, &VaultWake),
        (With<ConnectRestoreOriginApply>, Without<GitProcessTask>),
    >,
    mut commands: Commands,
) {
    for (entity, storage, mutation, wake) in &ops {
        let args = match &mutation.previous {
            Some(previous) => vec![
                "remote".into(),
                "set-url".into(),
                "origin".into(),
                previous.clone(),
            ],
            None => vec!["remote".into(), "remove".into(), "origin".into()],
        };
        let task = GitProcessTask::spawn(wake, storage.0.repository().to_path_buf(), args);
        commands.entity(entity).insert(task);
    }
}

fn drain_restore_origin_apply(
    mut ops: Query<
        (
            Entity,
            &ConnectPendingFailure,
            &VaultWake,
            &mut GitProcessTask,
        ),
        With<ConnectRestoreOriginApply>,
    >,
    mut commands: Commands,
) {
    for (entity, failure, wake, mut task) in &mut ops {
        if future::block_on(future::poll_once(&mut task.0)).is_none() {
            continue;
        }
        let error = failure.0.clone();
        commands
            .entity(entity)
            .remove::<(
                GitProcessTask,
                ConnectRestoreOriginApply,
                ConnectActive,
                ConnectCanceling,
                RepoClaim,
                ConnectKeyLoan,
                OriginMutation,
                ConnectPendingFailure,
            )>()
            .insert(VaultOperationOutcome(Err(error)));
        wake.notify();
    }
}

fn finish_canceled(
    ops: Query<
        (Entity, Has<OriginMutation>, &VaultWake),
        (
            With<ConnectCanceling>,
            Without<FolderDialogTask>,
            Without<GitProcessTask>,
            Without<FsUnitTask>,
            Without<InitCheckTask>,
            Without<ConnectKeyRequest>,
            Without<RecoveryCheckTask>,
            Without<SnapshotLoadTask>,
            Without<CollectFilesTask>,
            Without<ConnectAwaitReconcile>,
            Without<SnapshotWriteTask>,
            Without<LocalStateWriteTask>,
            Without<ConnectRestoreOriginCheck>,
            Without<ConnectRestoreOriginApply>,
            Without<VaultOperationOutcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, has_origin, wake) in &ops {
        commands
            .entity(entity)
            .remove::<(
                ConnectPendingClaim,
                ConnectFolderCheckBare,
                ConnectFolderInitBare,
                ConnectEnsureRepo,
                ConnectResolveUrl,
                ConnectReadPreviousOrigin,
                ConnectInstallOrigin,
                ConnectFetch,
                ConnectSetHead,
                ConnectResolveRemoteHead,
                ConnectVerifyRemoteRef,
                ConnectListRemoteRefs,
            )>()
            .remove::<(
                ConnectValidateLog,
                ConnectReadRemoteManifest,
                ConnectLoadRemoteKey,
                ConnectCheckRecipients,
                ConnectCheckoutBranch,
                ConnectSetUpstream,
                ConnectDecodeRemote,
                ConnectCollectBefore,
                ConnectCollectAfter,
                ConnectInitCheck,
                ConnectInitAcquireKey,
                ConnectInitCreateKey,
            )>()
            .remove::<(
                ConnectInitLoadPrevious,
                ConnectWriteSnapshot,
                ConnectGitAdd,
                ConnectGitStatus,
                ConnectGitCommit,
                ConnectCheckCreateOrigin,
                ConnectGhRepoCreate,
                ConnectReadCurrentBranch,
                ConnectPush,
                ConnectWriteLocalState,
            )>();
        if has_origin {
            commands.entity(entity).insert((
                ConnectPendingFailure("Vault connection canceled".to_string()),
                ConnectRestoreOriginCheck,
            ));
        } else {
            commands
                .entity(entity)
                .remove::<(ConnectActive, ConnectCanceling, RepoClaim, ConnectKeyLoan)>()
                .insert(VaultOperationOutcome(Err(
                    "Vault connection canceled".to_string()
                )));
        }
        wake.notify();
    }
}
