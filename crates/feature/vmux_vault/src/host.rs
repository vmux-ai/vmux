use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::Task;
use parking_lot::Mutex;
use vmux_ecs::host::{UiState, UiStatePlugin};
use vmux_ecs::profile::vault::{GeneratedRecoveryKey, VaultRecovery};

use crate::state::{
    VaultAuthorization, VaultCompletion, VaultOperation, VaultOperationKind, VaultOperationState,
    VaultSnapshot, VaultUiState, VaultWorkflowState,
};

mod operation;
mod runtime;
mod workflow;

struct OperationPlugin;
struct RuntimePlugin;
struct WorkflowPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OperationSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RuntimeSet;

#[vmux_native::page]
pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::VaultPage::plugin()).add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(vmux_ecs::host::page::NativelyHosted::page(
                    Self::URL,
                    crate::ui::VaultPage::NATIVE.title,
                )),
        );

        app.add_plugins((
            crate::agent::VaultAgentPlugin,
            UiStatePlugin::<VaultUiState>::default(),
            RuntimePlugin,
            OperationPlugin,
            WorkflowPlugin,
        ));
    }
}

#[derive(Component)]
struct VaultRegistry {
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
    task: Task<VaultSnapshot>,
}

#[derive(Component, Default)]
struct VaultOperationSequence(u64);

impl VaultOperationSequence {
    fn next(&mut self) -> u64 {
        let sequence = self.0;
        self.0 = self.0.wrapping_add(1);
        sequence
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VaultOperationTarget {
    Webview(Entity),
    Automatic,
}

#[derive(Component)]
struct VaultOperationContext {
    operation_id: u64,
    target: VaultOperationTarget,
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
