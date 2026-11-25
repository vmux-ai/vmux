use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use crate::storage::{GeneratedRecoveryKey, VaultRecovery};
use bevy::prelude::*;
use bevy::tasks::Task;
use parking_lot::Mutex;
use vmux_ecs::{UiState, UiStatePlugin};

use crate::state::{
    VaultAuthorization, VaultCompletion, VaultOperation, VaultOperationKind, VaultOperationState,
    VaultSnapshot, VaultUiState, VaultWorkflowState,
};

mod connect;
pub(crate) mod key;
mod key_io;
mod operation;
mod reconcile;
mod runtime;
mod sync;
mod vault_io;
mod workflow;

struct OperationPlugin;
struct RuntimePlugin;
struct WorkflowPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OperationSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeSet;

#[vmux_page::page]
pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::VaultPage::plugin()).add_plugins(
            Self::manifest()
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::page(
                    Self::url(),
                    crate::ui::VaultPage::page().title,
                )),
        );

        app.add_plugins((
            crate::agent::VaultAgentPlugin,
            UiStatePlugin::<VaultUiState>::default(),
            key::KeyAuthorityPlugin,
            sync::SyncPlugin,
            connect::ConnectionPlugin,
            RuntimePlugin,
            OperationPlugin,
            WorkflowPlugin,
        ));
    }
}

#[derive(Component)]
pub(crate) struct VaultRegistry {
    dirty: bool,
    loaded: bool,
    load_repositories: bool,
    generation: u64,
    revision: u64,
    snapshot: VaultSnapshot,
}

impl Default for VaultRegistry {
    fn default() -> Self {
        Self {
            dirty: true,
            loaded: false,
            load_repositories: false,
            generation: 1,
            revision: 0,
            snapshot: VaultSnapshot::default(),
        }
    }
}

#[derive(Component)]
struct VaultScanTask {
    generation: u64,
    task: Task<(VaultSnapshot, crate::storage::VaultStatus)>,
}

#[derive(Clone, Copy)]
struct VaultOperationTag;

#[derive(Component, Default)]
#[require(UiState<VaultUiState>, VaultWorkflow)]
struct VaultSubscriber {
    snapshot_revision: u64,
    revision: u64,
    emitted_revision: u64,
    state: VaultUiState,
}

#[derive(Component, Default)]
struct VaultWorkflow {
    state: VaultWorkflowState,
}

impl VaultWorkflow {
    fn owner_choice(
        value: String,
        kind: crate::state::VaultOwnerKind,
    ) -> crate::state::VaultOwnerChoice {
        crate::state::VaultOwnerChoice { value, kind }
    }
}

impl VaultSubscriber {
    fn pending(operation_id: u64, kind: VaultOperationKind) -> Self {
        let mut subscriber = Self::default();
        subscriber.begin_operation(operation_id, kind);
        subscriber
    }

    fn begin_operation(&mut self, operation_id: u64, kind: VaultOperationKind) {
        match kind {
            VaultOperationKind::GenerateRecoveryKey => {
                self.state.generated_recovery_key.clear();
            }
            VaultOperationKind::ConnectCloud => {
                self.state.cloud_root.clear();
            }
            _ => {}
        }
        self.state.operation = Some(VaultOperation::pending(operation_id, kind));
        self.touch();
    }

    fn authorize(&mut self, operation_id: u64, authorization: VaultAuthorization) {
        let Some(operation) = self.state.operation.as_mut() else {
            return;
        };
        if operation.operation_id != operation_id {
            return;
        }
        operation.state = VaultOperationState::Authorizing(authorization);
        self.touch();
    }

    fn complete_operation(
        &mut self,
        operation_id: u64,
        kind: VaultOperationKind,
        completion: VaultCompletion,
    ) {
        if completion.success {
            match kind {
                VaultOperationKind::Sync => {
                    self.state.recovery_upload_pending = false;
                }
                VaultOperationKind::GenerateRecoveryKey => {
                    self.state
                        .generated_recovery_key
                        .clone_from(&completion.message);
                }
                VaultOperationKind::CreateRecoveryKey => {
                    self.state.generated_recovery_key.clear();
                    self.state.recovery_upload_pending = completion.pending_upload;
                }
                VaultOperationKind::ConnectCloud => {
                    self.state.cloud_root.clone_from(&completion.message);
                }
                _ => {}
            }
        }
        let Some(operation) = self.state.operation.as_mut() else {
            return;
        };
        if operation.operation_id != operation_id {
            return;
        }
        operation.state = VaultOperationState::Completed(completion);
        self.touch();
    }

    fn synchronize(&mut self, revision: u64, snapshot: &VaultSnapshot) -> bool {
        if self.snapshot_revision == revision {
            return false;
        }
        self.snapshot_revision = revision;
        self.state.vault = snapshot.clone();
        self.touch();
        true
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Component)]
struct VaultOperationContext {
    operation_id: u64,
    webview: Option<Entity>,
    kind: VaultOperationKind,
}

#[derive(Component, Default)]
struct PendingVaultOperation;

#[derive(Component, Default)]
struct ReadyVaultOperation;

#[derive(Component, Clone)]
struct VaultOperationRequest<R: Send + Sync + 'static>(R);

impl<R: Send + Sync + 'static> VaultOperationRequest<R> {
    fn new(request: R) -> Self {
        Self(request)
    }
}

#[derive(Component)]
struct VaultOperationTask {
    task: Task<Result<VaultOperationOutput, String>>,
    progress: Mutex<mpsc::Receiver<VaultAuthorization>>,
    canceled: Arc<AtomicBool>,
}

struct VaultOperationOutput {
    message: String,
    pending_upload: bool,
    generated_recovery_key: Option<GeneratedRecoveryKey>,
}

#[derive(Component, Default)]
struct VaultAutoSync {
    requested: bool,
    initial_scan_complete: bool,
    remote_check: bool,
}

#[derive(Component)]
struct VaultRecoveryState {
    service: VaultRecovery,
    pending_key: Option<GeneratedRecoveryKey>,
}

impl Default for VaultRecoveryState {
    fn default() -> Self {
        Self {
            service: VaultRecovery::current(),
            pending_key: None,
        }
    }
}

impl VaultRecoveryState {
    fn service(&self) -> VaultRecovery {
        self.service.clone()
    }

    fn take_pending_key(&mut self) -> Option<GeneratedRecoveryKey> {
        self.pending_key.take()
    }

    fn retain(&mut self, key: GeneratedRecoveryKey) {
        self.pending_key = Some(key);
    }
}

#[derive(Component)]
pub(crate) struct VaultStatusProjection(pub(crate) vmux_api::vault::VaultStatusSnapshot);

impl VaultRegistry {
    pub(crate) fn publish(&mut self, snapshot: VaultSnapshot) {
        self.snapshot = snapshot;
        self.dirty = false;
        self.loaded = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
        self.loaded = false;
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn current(&self) -> bool {
        self.loaded && !self.dirty
    }
}
