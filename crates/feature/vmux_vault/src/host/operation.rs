use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use crate::storage::{GeneratedRecoveryKey, VaultRecovery, VaultStorage};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use parking_lot::Mutex;

use crate::state::{
    VaultAuthorization, VaultChooseCloudFolderRequest, VaultCompletion, VaultConnectCloudRequest,
    VaultConnectFolderRequest, VaultConnectGithubRequest, VaultConnectRequest,
    VaultCreateCloudFolderRequest, VaultCreateRecoveryKeyRequest, VaultCreateRequest,
    VaultGenerateRecoveryKeyRequest, VaultOperationKind, VaultSyncRequest,
    VaultUnlockRecoveryKeyRequest,
};

use super::key::{
    AcceptedStorage, KeyAdmission, KeyCanceled, KeyNamespace, KeyPermissions, KeyPort,
    NamespaceGeneration,
};
use vmux_ecs::{Operation, OperationRetained, OperationTarget};

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
            .add_observer(on_create)
            .add_observer(on_connect)
            .add_observer(on_sync)
            .add_observer(on_connect_github)
            .add_observer(on_connect_folder)
            .add_observer(on_generate_recovery_key)
            .add_observer(on_create_recovery_key)
            .add_observer(on_unlock_recovery_key)
            .add_observer(on_connect_cloud)
            .add_observer(on_create_cloud_folder)
            .add_observer(on_choose_cloud_folder)
            .add_systems(
                Update,
                (
                    cancel_superseded,
                    start,
                    (
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
                    )
                        .chain(),
                    drain,
                )
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

impl VaultOperationOutput {
    fn message(message: String) -> Self {
        Self {
            message,
            pending_upload: false,
            generated_recovery_key: None,
        }
    }
}

#[derive(SystemParam)]
struct VaultOperationQueue<'w, 's> {
    sequences: Query<'w, 's, &'static mut VaultOperationSequence, With<VaultRegistry>>,
    subscribers: Query<'w, 's, &'static mut VaultSubscriber>,
    commands: Commands<'w, 's>,
}

impl VaultOperationQueue<'_, '_> {
    fn next(&mut self) -> Option<u64> {
        Some(self.sequences.single_mut().ok()?.next())
    }

    fn begin(&mut self, target: Entity, operation_id: u64, kind: VaultOperationKind) {
        if let Ok(mut subscriber) = self.subscribers.get_mut(target) {
            subscriber.begin_operation(operation_id, kind);
        } else {
            self.commands
                .entity(target)
                .insert(VaultSubscriber::pending(operation_id, kind));
        }
    }
}

fn cancel_superseded(
    incoming: Query<
        (
            Entity,
            &VaultOperationContext,
            Has<VaultOperationRequest<VaultConnectGithubRequest>>,
        ),
        Added<PendingVaultOperation>,
    >,
    pending: Query<
        (
            Entity,
            Has<VaultOperationRequest<VaultConnectGithubRequest>>,
        ),
        With<PendingVaultOperation>,
    >,
    active: Query<
        (Entity, Option<&VaultOperationTask>),
        (
            With<VaultOperationRequest<VaultConnectGithubRequest>>,
            Without<PendingVaultOperation>,
        ),
    >,
    mut commands: Commands,
) {
    let mut latest = None;
    for (entity, context, github) in &incoming {
        if latest.is_none_or(|(_, id, _)| context.operation_id > id) {
            latest = Some((entity, context.operation_id, github));
        }
    }
    let Some((latest, _, github)) = latest else {
        return;
    };
    let connecting = !active.is_empty();
    for (entity, task) in &active {
        if let Some(task) = task {
            task.canceled.store(true, Ordering::Release);
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (entity, queued_github) in &pending {
        if entity != latest && (connecting || (github && queued_github)) {
            commands.entity(entity).despawn();
        }
    }
}

fn on_create(trigger: On<UiInput<VaultCreateRequest>>, mut queue: VaultOperationQueue) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::Create);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::Create,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
}

fn on_connect(trigger: On<UiInput<VaultConnectRequest>>, mut queue: VaultOperationQueue) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::Connect);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::Connect,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
}

fn on_sync(trigger: On<UiInput<VaultSyncRequest>>, mut queue: VaultOperationQueue) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::Sync);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::Sync,
        },
        VaultOperationRequest::new(trigger.event().payload),
    ));
}

fn on_connect_github(
    trigger: On<UiInput<VaultConnectGithubRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::ConnectGithub);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::ConnectGithub,
        },
        VaultOperationRequest::new(trigger.event().payload),
    ));
}

fn on_connect_folder(
    trigger: On<UiInput<VaultConnectFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::ConnectFolder);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::ConnectFolder,
        },
        VaultOperationRequest::new(trigger.event().payload),
    ));
}

fn on_generate_recovery_key(
    trigger: On<UiInput<VaultGenerateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(
        target,
        operation_id,
        VaultOperationKind::GenerateRecoveryKey,
    );
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::GenerateRecoveryKey,
        },
        VaultOperationRequest::new(trigger.event().payload),
    ));
}

fn on_create_recovery_key(
    trigger: On<UiInput<VaultCreateRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::CreateRecoveryKey);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::CreateRecoveryKey,
        },
        VaultOperationRequest::new(trigger.event().payload),
    ));
}

fn on_unlock_recovery_key(
    trigger: On<UiInput<VaultUnlockRecoveryKeyRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::UnlockRecoveryKey);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::UnlockRecoveryKey,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
}

fn on_connect_cloud(
    trigger: On<UiInput<VaultConnectCloudRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::ConnectCloud);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::ConnectCloud,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
}

fn on_create_cloud_folder(
    trigger: On<UiInput<VaultCreateCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::CreateCloudFolder);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::CreateCloudFolder,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
}

fn on_choose_cloud_folder(
    trigger: On<UiInput<VaultChooseCloudFolderRequest>>,
    mut queue: VaultOperationQueue,
) {
    let Some(operation_id) = queue.next() else {
        return;
    };
    let target = trigger.event().webview;
    queue.begin(target, operation_id, VaultOperationKind::ChooseCloudFolder);
    queue.commands.spawn((
        PendingVaultOperation,
        VaultOperationContext {
            operation_id,
            target: VaultOperationTarget::Webview(target),
            kind: VaultOperationKind::ChooseCloudFolder,
        },
        VaultOperationRequest::new(trigger.event().payload.clone()),
    ));
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

fn create(
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
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = create_vault(worker_storage, worker_keys, request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.create").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn create_vault(
    storage: VaultStorage,
    keys: KeyPort,
    request: VaultCreateRequest,
) -> Result<VaultOperationOutput, String> {
    let visibility = if request.private {
        crate::storage::RepositoryVisibility::Private
    } else {
        crate::storage::RepositoryVisibility::Public
    };
    let message = storage.create_remote(&request.repository, visibility, &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn connect(
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
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = connect_vault(worker_storage, worker_keys, request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn connect_vault(
    storage: VaultStorage,
    keys: KeyPort,
    request: VaultConnectRequest,
) -> Result<VaultOperationOutput, String> {
    let message = storage.connect_remote(&request.repository, &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn sync(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultSyncRequest>),
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
    for (entity, _request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = sync_vault(worker_storage, worker_keys);
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.sync").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: false,
                    store: false,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn sync_vault(storage: VaultStorage, keys: KeyPort) -> Result<VaultOperationOutput, String> {
    let message = storage.sync(&keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn connect_github(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectGithubRequest>),
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
    for (entity, _request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let worker_canceled = canceled.clone();
        let progress = move |authorization| {
            if sender.send(authorization).is_ok()
                && let Some(wake) = &wake
            {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        };
        let cancellation = move || worker_canceled.load(Ordering::Acquire);
        let operation = connect_vault_github(worker_storage, progress, cancellation);
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect_github").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: false,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn connect_vault_github<F, C>(
    storage: VaultStorage,
    progress: F,
    canceled: C,
) -> Result<VaultOperationOutput, String>
where
    F: Fn(VaultAuthorization) + Send,
    C: Fn() -> bool + Send,
{
    let message = storage.connect_github_with_progress(
        |code| {
            progress(VaultAuthorization {
                code,
                url: "https://github.com/login/device".to_string(),
            });
        },
        canceled,
    )?;
    Ok(VaultOperationOutput::message(message))
}

fn connect_folder(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectFolderRequest>),
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
    for (entity, _request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = connect_vault_folder(worker_storage, worker_keys);
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect_folder").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn connect_vault_folder(
    storage: VaultStorage,
    keys: KeyPort,
) -> Result<VaultOperationOutput, String> {
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
    let message = storage.connect_folder(folder.path(), &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn generate_recovery_key(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultGenerateRecoveryKeyRequest>,
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
    for (entity, _request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = generate_vault_recovery_key();
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.generate_recovery_key")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: false,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn generate_vault_recovery_key() -> Result<VaultOperationOutput, String> {
    let key = GeneratedRecoveryKey::generate()?;
    Ok(VaultOperationOutput {
        message: key.display().to_string(),
        pending_upload: false,
        generated_recovery_key: Some(key),
    })
}

fn create_recovery_key(
    operations: Query<
        (
            Entity,
            &VaultOperationRequest<VaultCreateRecoveryKeyRequest>,
        ),
        Added<ReadyVaultOperation>,
    >,
    mut namespaces: Query<
        (
            Entity,
            &KeyNamespace,
            &NamespaceGeneration,
            &mut KeyAdmission,
            &mut VaultRecoveryState,
            &AcceptedStorage,
        ),
        With<VaultRegistry>,
    >,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((namespace, access, generation, mut admission, mut recovery, captured)) =
        namespaces.single_mut()
    else {
        return;
    };
    for (entity, _request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation =
            create_vault_recovery_key(worker_keys, recovery.service(), recovery.take_pending_key());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.create_recovery_key")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: false,
                    store: false,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn create_vault_recovery_key(
    keys: KeyPort,
    recovery: VaultRecovery,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
) -> Result<VaultOperationOutput, String> {
    let key = generated_recovery_key
        .ok_or_else(|| "No Recovery Key has been generated for this Vault".to_string())?;
    let result = recovery.create(key, &keys)?;
    Ok(VaultOperationOutput {
        message: String::new(),
        pending_upload: result.pending_upload,
        generated_recovery_key: None,
    })
}

fn unlock_recovery_key(
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
            &NamespaceGeneration,
            &mut KeyAdmission,
            &VaultRecoveryState,
            &AcceptedStorage,
        ),
        With<VaultRegistry>,
    >,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((namespace, access, generation, mut admission, recovery, captured)) =
        namespaces.single_mut()
    else {
        return;
    };
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation =
            unlock_vault_recovery_key(worker_keys, recovery.service(), request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.unlock_recovery_key")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn unlock_vault_recovery_key(
    keys: KeyPort,
    recovery: VaultRecovery,
    request: VaultUnlockRecoveryKeyRequest,
) -> Result<VaultOperationOutput, String> {
    let message = recovery.unlock(&request.recovery_key, &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn connect_cloud(
    operations: Query<
        (Entity, &VaultOperationRequest<VaultConnectCloudRequest>),
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
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = connect_vault_cloud(request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.connect_cloud").unwrap().running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: false,
                    observe: false,
                    create: false,
                    store: false,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn connect_vault_cloud(
    request: VaultConnectCloudRequest,
) -> Result<VaultOperationOutput, String> {
    let message = connect_cloud_storage(&request.provider).await?;
    Ok(VaultOperationOutput::message(message))
}

fn create_cloud_folder(
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
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = create_vault_cloud_folder(worker_storage, worker_keys, request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.create_cloud_folder")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn create_vault_cloud_folder(
    storage: VaultStorage,
    keys: KeyPort,
    request: VaultCreateCloudFolderRequest,
) -> Result<VaultOperationOutput, String> {
    let folder = Path::new(&request.root).join(&request.folder_name);
    let message = storage.connect_folder(&folder, &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn choose_cloud_folder(
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
    for (entity, request) in &operations {
        let storage = captured.0.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let (keys, ingress) = access.port(
            namespace,
            generation.0,
            &mut admission,
            entity,
            wake.clone(),
        );
        let worker_keys = keys.clone();
        let worker_storage = storage.clone();
        let completion_wake = wake.clone();
        let (_sender, receiver) = mpsc::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let operation = choose_vault_cloud_folder(worker_storage, worker_keys, request.0.clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = operation.await;
            if let Some(wake) = completion_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(namespace)
            .remove::<super::VaultStatusProjection>();
        commands
            .entity(entity)
            .remove::<ReadyVaultOperation>()
            .insert((
                Operation::bundle("vault.choose_cloud_folder")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(namespace),
                vmux_ecs::OperationTargetRevision(generation.0),
                AcceptedStorage(storage),
                keys,
                ingress,
                KeyCanceled(canceled.clone()),
                KeyPermissions {
                    load: true,
                    observe: false,
                    create: true,
                    store: true,
                },
                VaultOperationTask {
                    task,
                    progress: Mutex::new(receiver),
                    canceled,
                },
            ));
    }
}

async fn choose_vault_cloud_folder(
    storage: VaultStorage,
    keys: KeyPort,
    request: VaultChooseCloudFolderRequest,
) -> Result<VaultOperationOutput, String> {
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
    let message = storage.connect_folder(folder.path(), &keys)?;
    Ok(VaultOperationOutput::message(message))
}

fn drain(
    mut operations: Query<(
        Entity,
        &VaultOperationContext,
        &vmux_ecs::OperationTarget,
        &vmux_ecs::OperationTargetRevision,
        &mut VaultOperationTask,
        Has<VaultOperationRequest<VaultSyncRequest>>,
        Has<VaultOperationRequest<VaultConnectGithubRequest>>,
    )>,
    mut registry: Query<(
        &mut VaultRegistry,
        &mut VaultRecoveryState,
        &NamespaceGeneration,
    )>,
    mut subscribers: Query<&mut VaultSubscriber>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    mut commands: Commands,
) {
    for (entity, context, target, accepted, mut task, is_sync, is_github) in &mut operations {
        let valid = registry
            .get(target.entity())
            .is_ok_and(|(_, _, g)| g.0 == accepted.0)
            && !task.canceled.load(Ordering::Acquire);
        let webview = context.target.webview();
        while let Ok(progress) = task.progress.get_mut().try_recv() {
            if valid
                && let Some(webview) = webview
                && let Ok(mut subscriber) = subscribers.get_mut(webview)
                && subscriber
                    .state
                    .operation
                    .as_ref()
                    .is_some_and(|o| o.operation_id == context.operation_id)
            {
                stack_requests.write(vmux_layout::stack::OpenRequest {
                    url: Some(progress.url.clone()),
                    policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
                });
                subscriber.authorize(context.operation_id, progress);
            }
        }
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if !valid {
            continue;
        }
        let Ok((mut state, mut recovery, generation)) = registry.get_mut(target.entity()) else {
            continue;
        };
        if generation.0 != accepted.0 {
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
        if is_sync {
            state.snapshot.sync_failed = !success;
            state.revision = state.revision.wrapping_add(1);
        }
        if let Some(webview) = webview
            && let Ok(mut subscriber) = subscribers.get_mut(webview)
        {
            subscriber.complete_operation(context.operation_id, context.kind, completion);
        }
        state.dirty = true;
        state.loaded = false;
        state.load_repositories |= is_github;
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
