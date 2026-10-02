use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use parking_lot::Mutex;
use vmux_ecs::profile::vault::{GeneratedRecoveryKey, VaultRecovery};

use crate::state::{
    VaultAuthorization, VaultChooseCloudFolderRequest, VaultCompletion, VaultConnectCloudRequest,
    VaultConnectFolderRequest, VaultConnectGithubRequest, VaultConnectRequest,
    VaultCreateCloudFolderRequest, VaultCreateRecoveryKeyRequest, VaultCreateRequest,
    VaultGenerateRecoveryKeyRequest, VaultOperationKind, VaultSyncRequest,
    VaultUnlockRecoveryKeyRequest,
};

use super::{
    OperationPlugin, OperationSet, PendingVaultOperation, ReadyVaultOperation, RuntimeSet,
    VaultOperationContext, VaultOperationOutput, VaultOperationRequest, VaultOperationSequence,
    VaultOperationTarget, VaultOperationTask, VaultRecoveryState, VaultRegistry, VaultScanTask,
    VaultSubscriber,
};

impl Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            VaultCreateRequest,
            VaultConnectRequest,
            VaultSyncRequest,
            VaultConnectGithubRequest,
            VaultConnectFolderRequest,
            VaultGenerateRecoveryKeyRequest,
            VaultCreateRecoveryKeyRequest,
            VaultUnlockRecoveryKeyRequest,
            VaultConnectCloudRequest,
            VaultCreateCloudFolderRequest,
            VaultChooseCloudFolderRequest,
        )>::default())
            .add_observer(create)
            .add_observer(connect)
            .add_observer(sync)
            .add_observer(connect_github)
            .add_observer(connect_folder)
            .add_observer(generate_recovery_key)
            .add_observer(create_recovery_key)
            .add_observer(unlock_recovery_key)
            .add_observer(connect_cloud)
            .add_observer(create_cloud_folder)
            .add_observer(choose_cloud_folder)
            .add_systems(
                Update,
                (start, launch, drain)
                    .chain()
                    .after(RuntimeSet)
                    .in_set(OperationSet),
            );
    }
}

impl VaultOperationTarget {
    fn webview(self) -> Option<Entity> {
        match self {
            Self::Webview(entity) => Some(entity),
            Self::Automatic => None,
        }
    }
}

type VaultOperationFuture =
    Pin<Box<dyn Future<Output = Result<VaultOperationOutput, String>> + Send>>;
type VaultProgress = Box<dyn Fn(VaultAuthorization) + Send>;
type VaultCancellation = Box<dyn Fn() -> bool + Send>;

impl VaultOperationOutput {
    fn message(message: String) -> Self {
        Self {
            message,
            pending_upload: false,
            generated_recovery_key: None,
        }
    }
}

fn create_vault(
    request: VaultCreateRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let visibility = if request.private {
            vmux_ecs::profile::vault::RepositoryVisibility::Private
        } else {
            vmux_ecs::profile::vault::RepositoryVisibility::Public
        };
        let message = vmux_ecs::profile::vault::create_remote(&request.repository, visibility)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault(
    request: VaultConnectRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_ecs::profile::vault::connect_remote(&request.repository)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn sync_vault(
    _request: VaultSyncRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_ecs::profile::vault::sync()?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_github(
    _request: VaultConnectGithubRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    progress: VaultProgress,
    canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = vmux_ecs::profile::vault::connect_github_with_progress(
            |code| {
                progress(VaultAuthorization {
                    code,
                    url: "https://github.com/login/device".to_string(),
                });
            },
            canceled,
        )?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_folder(
    _request: VaultConnectFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let initial_dir = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Library/CloudStorage"))
            .filter(|path| path.is_dir())
            .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(initial_dir) = initial_dir {
            dialog = dialog.set_directory(initial_dir);
        }
        let Some(folder) = dialog.pick_folder().await else {
            return Err(String::new());
        };
        let message = vmux_ecs::profile::vault::connect_folder(folder.path())?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn generate_vault_recovery_key(
    _request: VaultGenerateRecoveryKeyRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let key = GeneratedRecoveryKey::generate()?;
        Ok(VaultOperationOutput {
            message: key.display().to_string(),
            pending_upload: false,
            generated_recovery_key: Some(key),
        })
    })
}

fn create_vault_recovery_key(
    _request: VaultCreateRecoveryKeyRequest,
    recovery: VaultRecovery,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let key = generated_recovery_key
            .ok_or_else(|| "No Recovery Key has been generated for this Vault".to_string())?;
        let result = recovery.create(key)?;
        Ok(VaultOperationOutput {
            message: String::new(),
            pending_upload: result.pending_upload,
            generated_recovery_key: None,
        })
    })
}

fn unlock_vault_recovery_key(
    request: VaultUnlockRecoveryKeyRequest,
    recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = recovery.unlock(&request.recovery_key)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn connect_vault_cloud(
    request: VaultConnectCloudRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let message = connect_cloud_storage(&request.provider).await?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn create_vault_cloud_folder(
    request: VaultCreateCloudFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let folder = Path::new(&request.root).join(&request.folder_name);
        let message = vmux_ecs::profile::vault::connect_folder(&folder)?;
        Ok(VaultOperationOutput::message(message))
    })
}

fn choose_vault_cloud_folder(
    request: VaultChooseCloudFolderRequest,
    _recovery: VaultRecovery,
    _generated_recovery_key: Option<GeneratedRecoveryKey>,
    _progress: VaultProgress,
    _canceled: VaultCancellation,
) -> VaultOperationFuture {
    Box::pin(async move {
        let mut dialog = rfd::AsyncFileDialog::new();
        let root = Path::new(&request.root);
        if root.is_dir() {
            dialog = dialog.set_directory(root);
        }
        let Some(folder) = dialog.pick_folder().await else {
            return Err(String::new());
        };
        let remote = if folder
            .path()
            .extension()
            .is_some_and(|extension| extension == "git")
        {
            folder.path().to_path_buf()
        } else {
            folder.path().join("vmux-vault.git")
        };
        if !remote.exists() {
            return Err("selected folder does not contain a Vault".to_string());
        }
        let message = vmux_ecs::profile::vault::connect_folder(folder.path())?;
        Ok(VaultOperationOutput::message(message))
    })
}

#[derive(SystemParam)]
struct VaultOperationQueue<'w, 's> {
    sequences: Query<'w, 's, &'static mut VaultOperationSequence, With<VaultRegistry>>,
    pending: Query<'w, 's, Entity, With<PendingVaultOperation>>,
    pending_github: Query<
        'w,
        's,
        Entity,
        (
            With<PendingVaultOperation>,
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
        ),
    >,
    active_github: Query<
        'w,
        's,
        (Entity, Option<&'static VaultOperationTask>),
        (
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
            Without<PendingVaultOperation>,
        ),
    >,
    subscribers: Query<'w, 's, &'static mut VaultSubscriber>,
    commands: Commands<'w, 's>,
}

impl VaultOperationQueue<'_, '_> {
    fn push<R: Send + Sync + 'static>(
        &mut self,
        target: Entity,
        request: R,
        kind: VaultOperationKind,
    ) {
        let Ok(mut sequence) = self.sequences.single_mut() else {
            return;
        };
        let operation_id = sequence.next();
        if let Ok(mut subscriber) = self.subscribers.get_mut(target) {
            subscriber.begin_operation(operation_id, kind);
        } else {
            self.commands
                .entity(target)
                .insert(VaultSubscriber::pending(operation_id, kind));
        }
        let mut connecting = false;
        for (entity, task) in &self.active_github {
            connecting = true;
            if let Some(task) = task {
                task.canceled.store(true, Ordering::Relaxed);
            } else {
                self.commands.entity(entity).despawn();
            }
        }
        if connecting {
            for entity in &self.pending {
                self.commands.entity(entity).despawn();
            }
        } else if kind == VaultOperationKind::ConnectGithub {
            for entity in &self.pending_github {
                self.commands.entity(entity).despawn();
            }
        }
        self.commands.spawn((
            PendingVaultOperation,
            VaultOperationContext {
                operation_id,
                target: VaultOperationTarget::Webview(target),
                kind,
            },
            VaultOperationRequest::new(request),
        ));
    }
}

fn create(trigger: On<UiInput<VaultCreateRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::Create,
    );
}

fn connect(trigger: On<UiInput<VaultConnectRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::Connect,
    );
}

fn sync(trigger: On<UiInput<VaultSyncRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload,
        VaultOperationKind::Sync,
    );
}

fn connect_github(trigger: On<UiInput<VaultConnectGithubRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload,
        VaultOperationKind::ConnectGithub,
    );
}

fn connect_folder(trigger: On<UiInput<VaultConnectFolderRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload,
        VaultOperationKind::ConnectFolder,
    );
}

fn generate_recovery_key(
    trigger: On<UiInput<VaultGenerateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload,
        VaultOperationKind::GenerateRecoveryKey,
    );
}

fn create_recovery_key(
    trigger: On<UiInput<VaultCreateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload,
        VaultOperationKind::CreateRecoveryKey,
    );
}

fn unlock_recovery_key(
    trigger: On<UiInput<VaultUnlockRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::UnlockRecoveryKey,
    );
}

fn connect_cloud(trigger: On<UiInput<VaultConnectCloudRequest>>, mut queue: VaultOperationQueue) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ConnectCloud,
    );
}

fn create_cloud_folder(
    trigger: On<UiInput<VaultCreateCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::CreateCloudFolder,
    );
}

fn choose_cloud_folder(
    trigger: On<UiInput<VaultChooseCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    queue.push(
        trigger.event().webview,
        trigger.event().payload.clone(),
        VaultOperationKind::ChooseCloudFolder,
    );
}

fn start(
    pending: Query<(Entity, &VaultOperationContext), With<PendingVaultOperation>>,
    tasks: Query<(), With<VaultOperationTask>>,
    ready: Query<(), With<ReadyVaultOperation>>,
    scans: Query<(), With<VaultScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !ready.is_empty() || !scans.is_empty() {
        return;
    }
    let mut next = None;
    for (entity, operation) in &pending {
        match next {
            Some((_, operation_id)) if operation_id <= operation.operation_id => {}
            _ => next = Some((entity, operation.operation_id)),
        }
    }
    let Some((entity, _)) = next else {
        return;
    };
    commands
        .entity(entity)
        .remove::<PendingVaultOperation>()
        .insert(ReadyVaultOperation);
}

type ReadyVaultOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static VaultOperationContext,
        Option<&'static VaultOperationRequest<VaultCreateRequest>>,
        Option<&'static VaultOperationRequest<VaultConnectRequest>>,
        Option<&'static VaultOperationRequest<VaultSyncRequest>>,
        Option<&'static VaultOperationRequest<VaultConnectGithubRequest>>,
        Option<&'static VaultOperationRequest<VaultConnectFolderRequest>>,
        Option<&'static VaultOperationRequest<VaultGenerateRecoveryKeyRequest>>,
        Option<&'static VaultOperationRequest<VaultCreateRecoveryKeyRequest>>,
        Option<&'static VaultOperationRequest<VaultUnlockRecoveryKeyRequest>>,
        Option<&'static VaultOperationRequest<VaultConnectCloudRequest>>,
        Option<&'static VaultOperationRequest<VaultCreateCloudFolderRequest>>,
        Option<&'static VaultOperationRequest<VaultChooseCloudFolderRequest>>,
    ),
    Added<ReadyVaultOperation>,
>;

fn launch(
    operations: ReadyVaultOperations,
    mut recoveries: Query<&mut VaultRecoveryState, With<VaultRegistry>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(mut recovery) = recoveries.single_mut() else {
        return;
    };
    for (
        entity,
        context,
        create,
        connect,
        sync,
        connect_github,
        connect_folder,
        generate_recovery_key,
        create_recovery_key,
        unlock_recovery_key,
        connect_cloud,
        create_cloud_folder,
        choose_cloud_folder,
    ) in &operations
    {
        let service = recovery.service();
        let generated_recovery_key = if context.kind == VaultOperationKind::CreateRecoveryKey {
            recovery.take_pending_key()
        } else {
            None
        };
        let completion_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let progress_wake = completion_wake.clone();
        let (progress_sender, progress_receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let task_canceled = canceled.clone();
        let progress = Box::new(move |authorization| {
            if progress_sender.send(authorization).is_ok()
                && let Some(wake) = &progress_wake
            {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        });
        let cancellation = Box::new(move || task_canceled.load(Ordering::Relaxed));
        let operation = match context.kind {
            VaultOperationKind::Create => {
                let Some(request) = create else {
                    commands.entity(entity).despawn();
                    continue;
                };
                create_vault(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::Connect => {
                let Some(request) = connect else {
                    commands.entity(entity).despawn();
                    continue;
                };
                connect_vault(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::Sync => {
                let Some(request) = sync else {
                    commands.entity(entity).despawn();
                    continue;
                };
                sync_vault(
                    request.0,
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::ConnectGithub => {
                let Some(request) = connect_github else {
                    commands.entity(entity).despawn();
                    continue;
                };
                connect_vault_github(
                    request.0,
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::ConnectFolder => {
                let Some(request) = connect_folder else {
                    commands.entity(entity).despawn();
                    continue;
                };
                connect_vault_folder(
                    request.0,
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::GenerateRecoveryKey => {
                let Some(request) = generate_recovery_key else {
                    commands.entity(entity).despawn();
                    continue;
                };
                generate_vault_recovery_key(
                    request.0,
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::CreateRecoveryKey => {
                let Some(request) = create_recovery_key else {
                    commands.entity(entity).despawn();
                    continue;
                };
                create_vault_recovery_key(
                    request.0,
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::UnlockRecoveryKey => {
                let Some(request) = unlock_recovery_key else {
                    commands.entity(entity).despawn();
                    continue;
                };
                unlock_vault_recovery_key(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::ConnectCloud => {
                let Some(request) = connect_cloud else {
                    commands.entity(entity).despawn();
                    continue;
                };
                connect_vault_cloud(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::CreateCloudFolder => {
                let Some(request) = create_cloud_folder else {
                    commands.entity(entity).despawn();
                    continue;
                };
                create_vault_cloud_folder(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
            VaultOperationKind::ChooseCloudFolder => {
                let Some(request) = choose_cloud_folder else {
                    commands.entity(entity).despawn();
                    continue;
                };
                choose_vault_cloud_folder(
                    request.0.clone(),
                    service,
                    generated_recovery_key,
                    progress,
                    cancellation,
                )
            }
        };
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert(VaultOperationTask {
                task,
                progress: Mutex::new(progress_receiver),
                canceled,
            });
    }
}

fn drain(
    mut operations: Query<(Entity, &VaultOperationContext, &mut VaultOperationTask)>,
    mut registry: Query<(&mut VaultRegistry, &mut VaultRecoveryState)>,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut commands: Commands,
) {
    let Ok((mut state, mut recovery)) = registry.single_mut() else {
        return;
    };
    for (entity, context, mut task) in &mut operations {
        let target = context.target.webview();
        while let Ok(progress) = task.progress.get_mut().try_recv() {
            if let Some(target) = target {
                stack_requests.write(vmux_layout::stack::OpenRequest {
                    url: Some(progress.url.clone()),
                });
                if let Ok(mut subscriber) = subscribers.get_mut(target) {
                    subscriber.authorize(context.operation_id, progress);
                }
            }
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.canceled.load(Ordering::Relaxed) {
            continue;
        }
        let (success, message, pending_upload, generated_recovery_key) = match result {
            Ok(output) => (
                true,
                output.message,
                output.pending_upload,
                output.generated_recovery_key,
            ),
            Err(message) => (false, message, false, None),
        };
        if let Some(key) = generated_recovery_key {
            recovery.retain(key);
        }
        let completion = VaultCompletion {
            success,
            message,
            pending_upload,
        };
        if context.kind == VaultOperationKind::Sync {
            state.snapshot.sync_failed = !success;
            state.revision = state.revision.wrapping_add(1);
            if context.target == VaultOperationTarget::Automatic && !success {
                continue;
            }
        }
        if let Some(target) = target
            && let Ok(mut subscriber) = subscribers.get_mut(target)
        {
            subscriber.complete_operation(context.operation_id, context.kind, completion);
        }
        state.dirty = true;
        state.loaded = false;
        state.load_repositories |= context.kind == VaultOperationKind::ConnectGithub;
        state.generation = state.generation.wrapping_add(1);
    }
}

async fn connect_cloud_storage(provider: &str) -> Result<String, String> {
    let roots = cloud_storage_roots(provider);
    match roots.as_slice() {
        [] => Err(format!("{provider} is not connected on this device")),
        [root] => Ok(root.to_string_lossy().into_owned()),
        roots => {
            let initial = roots
                .first()
                .and_then(|root| root.parent())
                .unwrap_or_else(|| roots[0].as_path());
            let Some(folder) = rfd::AsyncFileDialog::new()
                .set_directory(initial)
                .pick_folder()
                .await
            else {
                return Err(String::new());
            };
            let selected = folder
                .path()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if roots.iter().any(|root| {
                root.canonicalize()
                    .is_ok_and(|root| selected == root || selected.starts_with(root))
            }) {
                Ok(selected.to_string_lossy().into_owned())
            } else {
                Err(format!("selected folder is not in {provider}"))
            }
        }
    }
}

fn cloud_storage_roots(provider: &str) -> Vec<std::path::PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let prefixes: &[&str] = match provider {
        "Google Drive" => &["GoogleDrive"],
        "Dropbox" => &["Dropbox"],
        "OneDrive" => &["OneDrive"],
        _ => return Vec::new(),
    };
    let mut roots = std::fs::read_dir(home.join("Library/CloudStorage"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
        .map(|entry| {
            let root = entry.path();
            if provider == "Google Drive" && root.join("My Drive").is_dir() {
                root.join("My Drive")
            } else {
                root
            }
        })
        .collect::<Vec<_>>();
    for fallback in prefixes.iter().map(|prefix| home.join(prefix)) {
        if fallback.is_dir() {
            roots.push(fallback);
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_is_consumed_once() {
        let mut recovery = VaultRecoveryState::default();
        recovery.retain(GeneratedRecoveryKey::generate().unwrap());

        assert!(recovery.take_pending_key().is_some());
        assert!(recovery.take_pending_key().is_none());
    }

    #[test]
    fn vault_operation_state_preserves_recovery_workflow() {
        let mut subscriber = VaultSubscriber::pending(3, VaultOperationKind::GenerateRecoveryKey);
        subscriber.complete_operation(
            3,
            VaultOperationKind::GenerateRecoveryKey,
            VaultCompletion {
                success: true,
                message: "recovery-key".to_string(),
                pending_upload: false,
            },
        );
        assert_eq!(subscriber.state.generated_recovery_key, "recovery-key");

        subscriber.begin_operation(4, VaultOperationKind::CreateRecoveryKey);
        subscriber.complete_operation(
            4,
            VaultOperationKind::CreateRecoveryKey,
            VaultCompletion {
                success: true,
                message: String::new(),
                pending_upload: true,
            },
        );

        assert!(subscriber.state.generated_recovery_key.is_empty());
        assert!(subscriber.state.recovery_upload_pending);
    }
}
