use std::sync::mpsc;
use std::time::Duration;

use crate::storage::VaultStorage;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, futures_lite::future};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::state::{VaultOperationKind, VaultRepository, VaultSnapshot, VaultSyncRequest};

use super::{
    PendingVaultOperation, ReadyVaultOperation, RuntimePlugin, RuntimeSet, VaultAutoSync,
    VaultOperationContext, VaultOperationRequest, VaultOperationSequence, VaultOperationTarget,
    VaultOperationTask, VaultRecoveryState, VaultRegistry, VaultScanTask,
};

const VAULT_AUTO_SYNC_DELAY: Duration = Duration::from_secs(2);
const VAULT_REMOTE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

impl Plugin for RuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            (drain_watch, start_scan, drain_scan, queue_auto_sync)
                .chain()
                .in_set(RuntimeSet),
        );

        if let Some(watch) = VaultWatch::new(app) {
            app.insert_non_send(watch);
        }
    }
}

struct VaultWatch {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    debounce_tx: mpsc::Sender<()>,
    ready_rx: mpsc::Receiver<()>,
    remote_rx: mpsc::Receiver<()>,
}

impl VaultWatch {
    fn new(app: &App) -> Option<Self> {
        let vault_root = VaultStorage::current().root().to_path_buf();
        let _ = std::fs::create_dir_all(&vault_root);
        let (watch_tx, watch_rx) = mpsc::channel();
        let watch_wake = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        let debounce_wake = watch_wake.clone();
        let remote_wake = watch_wake.clone();
        let mut watcher = match notify::recommended_watcher(move |result| {
            if watch_tx.send(result).is_ok()
                && let Some(wake) = &watch_wake
            {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                warn!("Vault watcher init failed: {error}");
                return None;
            }
        };
        if let Err(error) = watcher.watch(&vault_root, RecursiveMode::Recursive) {
            warn!("Vault watcher init failed: {error}");
            return None;
        }
        let (debounce_tx, debounce_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (remote_tx, remote_rx) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("vmux-vault-auto-sync".to_string())
            .spawn(move || {
                loop {
                    if debounce_rx.recv().is_err() {
                        return;
                    }
                    loop {
                        match debounce_rx.recv_timeout(VAULT_AUTO_SYNC_DELAY) {
                            Ok(()) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if ready_tx.send(()).is_ok()
                                    && let Some(wake) = &debounce_wake
                                {
                                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                                }
                                break;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            })
        {
            warn!("Vault auto-sync worker init failed: {error}");
        }
        if let Err(error) = std::thread::Builder::new()
            .name("vmux-vault-remote-sync".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(VAULT_REMOTE_SYNC_INTERVAL);
                    if remote_tx.send(()).is_err() {
                        return;
                    }
                    if let Some(wake) = &remote_wake {
                        let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                    }
                }
            })
        {
            warn!("Vault remote-sync worker init failed: {error}");
        }
        Some(Self {
            _watcher: watcher,
            rx: watch_rx,
            debounce_tx,
            ready_rx,
            remote_rx,
        })
    }

    fn requests_sync(result: &notify::Result<notify::Event>) -> bool {
        result.as_ref().is_ok_and(|event| {
            !matches!(event.kind, notify::EventKind::Access(_))
                && event
                    .paths
                    .iter()
                    .any(|path| VaultStorage::current().manages(path))
        })
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Vault"),
        VaultRegistry::default(),
        VaultOperationSequence::default(),
        VaultAutoSync::default(),
        VaultRecoveryState::default(),
    ));
}

fn start_scan(
    mut registries: Query<&mut VaultRegistry>,
    scans: Query<(), With<VaultScanTask>>,
    tasks: Query<(), With<VaultOperationTask>>,
    ready: Query<(), With<ReadyVaultOperation>>,
    pending: Query<(), With<PendingVaultOperation>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registries.single_mut() else {
        return;
    };
    if !state.dirty
        || !scans.is_empty()
        || !tasks.is_empty()
        || !ready.is_empty()
        || !pending.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let load_repositories = state.load_repositories;
    let previous = state.snapshot.clone();
    state.dirty = false;
    state.load_repositories = false;
    let task =
        IoTaskPool::get().spawn(async move { VaultSnapshot::scan(load_repositories, previous) });
    commands.spawn(VaultScanTask { generation, task });
}

fn drain_scan(
    mut scans: Query<(Entity, &mut VaultScanTask)>,
    mut registries: Query<(&mut VaultRegistry, &mut VaultAutoSync)>,
    mut commands: Commands,
) {
    let Ok((mut state, mut auto_sync)) = registries.single_mut() else {
        return;
    };
    for (entity, mut scan) in &mut scans {
        let Some(snapshot) = future::block_on(future::poll_once(&mut scan.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if scan.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot = snapshot;
        state.loaded = true;
        state.revision = state.revision.wrapping_add(1);
        if !state.snapshot.github_owner.is_empty()
            && (!state.snapshot.initialized || state.snapshot.remote.is_empty())
            && !state.snapshot.repositories_loaded
        {
            state.dirty = true;
            state.load_repositories = true;
            state.generation = state.generation.wrapping_add(1);
        }
        if !auto_sync.initial_scan_complete {
            auto_sync.requested = state.snapshot.initialized
                && state.snapshot.unlocked
                && !state.snapshot.remote.is_empty()
                && (state.snapshot.dirty > 0
                    || state.snapshot.ahead > 0
                    || state.snapshot.behind > 0);
            auto_sync.initial_scan_complete = true;
        }
    }
}

fn drain_watch(
    watcher: Option<NonSendMut<VaultWatch>>,
    mut registry: Query<(&mut VaultRegistry, &mut VaultAutoSync)>,
) {
    let Some(watcher) = watcher else {
        return;
    };
    let Ok((mut state, mut auto_sync)) = registry.single_mut() else {
        return;
    };
    if watcher.remote_rx.try_iter().next().is_some() {
        auto_sync.requested = true;
        auto_sync.remote_check = true;
    }
    let mut changed = false;
    for result in watcher.rx.try_iter() {
        changed |= VaultWatch::requests_sync(&result);
    }
    if !changed {
        if watcher.ready_rx.try_iter().next().is_some() {
            auto_sync.requested = true;
        }
        return;
    }
    state.dirty = true;
    state.generation = state.generation.wrapping_add(1);
    auto_sync.requested = false;
    auto_sync.remote_check = false;
    let _ = watcher.ready_rx.try_iter().count();
    let _ = watcher.debounce_tx.send(());
}

fn queue_auto_sync(
    mut registry: Query<(
        &VaultRegistry,
        &mut VaultAutoSync,
        &mut VaultOperationSequence,
    )>,
    scans: Query<(), With<VaultScanTask>>,
    tasks: Query<
        (),
        (
            With<VaultOperationTask>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    ready: Query<
        (),
        (
            With<ReadyVaultOperation>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    pending: Query<
        (),
        (
            With<PendingVaultOperation>,
            With<VaultOperationRequest<VaultSyncRequest>>,
        ),
    >,
    mut commands: Commands,
) {
    let Ok((state, mut auto_sync, mut sequence)) = registry.single_mut() else {
        return;
    };
    if !auto_sync.requested || state.dirty || !state.loaded || !scans.is_empty() {
        return;
    }
    let vault = &state.snapshot;
    let sync_needed =
        auto_sync.remote_check || vault.dirty > 0 || vault.ahead > 0 || vault.behind > 0;
    if !vault.initialized || !vault.unlocked || vault.remote.is_empty() || !sync_needed {
        auto_sync.requested = false;
        auto_sync.remote_check = false;
        return;
    }
    if !tasks.is_empty() || !ready.is_empty() || !pending.is_empty() {
        auto_sync.requested = false;
        auto_sync.remote_check = false;
        return;
    }
    commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id: sequence.next(),
            target: VaultOperationTarget::Automatic,
            kind: VaultOperationKind::Sync,
        },
        VaultOperationRequest::new(VaultSyncRequest),
    ));
    auto_sync.requested = false;
    auto_sync.remote_check = false;
}

impl VaultSnapshot {
    fn scan(load_repositories: bool, previous: Self) -> Self {
        let status = if load_repositories {
            crate::storage::VaultStatus::current_with_repositories()
        } else {
            crate::storage::VaultStatus::current()
        };
        let mut snapshot = Self {
            root: status.root.to_string_lossy().into_owned(),
            initialized: status.initialized,
            encrypted: status.encrypted,
            unlocked: status.unlocked,
            vault_id: status.vault_id,
            recovery_enabled: status.recovery_enabled,
            remote: status.remote,
            branch: status.branch,
            dirty: status.dirty,
            ahead: status.ahead,
            behind: status.behind,
            sync_failed: previous.sync_failed,
            github_owner: status.github_owner,
            github_owners: status.github_owners,
            repositories: status
                .repositories
                .into_iter()
                .map(|repository| VaultRepository {
                    name: repository.name,
                    url: repository.url,
                    private: repository.private,
                    empty: repository.empty,
                })
                .collect(),
            repositories_loaded: load_repositories,
            error: status.error,
        };
        if !load_repositories && (!snapshot.initialized || snapshot.remote.is_empty()) {
            snapshot.github_owner = previous.github_owner;
            snapshot.github_owners = previous.github_owners;
            snapshot.repositories = previous.repositories;
            snapshot.repositories_loaded = previous.repositories_loaded;
            snapshot.error = previous.error;
        }
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct VaultAutoSyncScenario;

    impl VaultAutoSyncScenario {
        fn pending_targets(vault: VaultSnapshot, remote_check: bool) -> Vec<VaultOperationTarget> {
            let mut app = App::new();
            app.add_systems(Update, queue_auto_sync);
            let registry = app
                .world_mut()
                .spawn((
                    VaultRegistry::default(),
                    VaultAutoSync::default(),
                    VaultOperationSequence::default(),
                ))
                .id();
            {
                let mut state = app.world_mut().get_mut::<VaultRegistry>(registry).unwrap();
                state.dirty = false;
                state.loaded = true;
                state.snapshot = vault;
            }
            {
                let mut auto_sync = app.world_mut().get_mut::<VaultAutoSync>(registry).unwrap();
                auto_sync.requested = true;
                auto_sync.remote_check = remote_check;
            }

            app.update();

            let world = app.world_mut();
            let mut query =
                world.query_filtered::<&VaultOperationContext, With<PendingVaultOperation>>();
            query
                .iter(world)
                .map(|operation| operation.target)
                .collect()
        }
    }

    #[test]
    fn vault_backup_watcher_ignores_runtime_and_access_events() {
        let root = VaultStorage::current().root().to_path_buf();
        let knowledge =
            notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
                .add_path(root.join("knowledge/note.md"));
        let runtime = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("workspace/repo/file.rs"));
        let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Any))
            .add_path(root.join("tools/tools.toml"));

        assert!(VaultWatch::requests_sync(&Ok(knowledge)));
        assert!(!VaultWatch::requests_sync(&Ok(runtime)));
        assert!(!VaultWatch::requests_sync(&Ok(access)));
    }

    #[test]
    fn automatic_backup_creates_only_needed_pending_operations() {
        let connected = VaultSnapshot {
            initialized: true,
            unlocked: true,
            remote: "https://example.com/vault.git".to_string(),
            dirty: 1,
            ..Default::default()
        };
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(connected.clone(), false),
            [VaultOperationTarget::Automatic]
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    unlocked: false,
                    ..connected.clone()
                },
                false
            ),
            []
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ahead: 1,
                    ..connected.clone()
                },
                false
            ),
            [VaultOperationTarget::Automatic]
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ..connected.clone()
                },
                false
            ),
            []
        );
        assert_eq!(
            VaultAutoSyncScenario::pending_targets(
                VaultSnapshot {
                    dirty: 0,
                    ..connected
                },
                true,
            ),
            [VaultOperationTarget::Automatic]
        );
    }
}
