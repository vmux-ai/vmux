use std::collections::BTreeSet;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::UiStateWrite;
use vmux_ecs::page::PageReady;

use crate::state::{
    VaultChooseCloudFolderRequest, VaultConnectCloudRequest, VaultConnectGithubRequest,
    VaultConnectRequest, VaultConnectionProvider, VaultCreateCloudFolderRequest,
    VaultCreateRecoveryKeyRequest, VaultCreateRequest, VaultDestination,
    VaultDestinationSelectRequest, VaultNotice, VaultOperation, VaultOperationKind,
    VaultOwnerChoice, VaultOwnerKind, VaultOwnerSelectRequest, VaultPrivacyRequest,
    VaultProviderSelectRequest, VaultRecoveryConfirmationRequest, VaultRecoveryInputRequest,
    VaultRefreshRequest, VaultRepository, VaultRepositoryChoice, VaultRepositoryNameRequest,
    VaultRepositorySelectRequest, VaultSyncStatus, VaultUiState, VaultUnlockRecoveryKeyRequest,
    VaultWorkflowConnectRequest, VaultWorkflowCreateRequest, VaultWorkflowState,
};

use super::{
    OperationSet, VaultPlugin, VaultRegistry, VaultSubscriber, VaultWorkflow, WorkflowPlugin,
};

impl Plugin for WorkflowPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            VaultProviderSelectRequest,
            VaultDestinationSelectRequest,
            VaultOwnerSelectRequest,
            VaultRepositoryNameRequest,
            VaultRepositorySelectRequest,
            VaultPrivacyRequest,
            VaultWorkflowCreateRequest,
            VaultWorkflowConnectRequest,
            VaultRecoveryConfirmationRequest,
            VaultRecoveryInputRequest,
            VaultRefreshRequest,
        )>::default())
            .add_observer(page_ready)
            .add_observer(refresh)
            .add_observer(select_provider)
            .add_observer(select_destination)
            .add_observer(select_owner)
            .add_observer(set_repository_name)
            .add_observer(select_repository)
            .add_observer(set_privacy)
            .add_observer(create)
            .add_observer(connect)
            .add_observer(confirm_recovery)
            .add_observer(set_recovery_input)
            .add_systems(Update, emit_state.after(OperationSet));
    }
}

impl VaultWorkflow {
    fn for_url(url: &str) -> Self {
        let provider = url
            .split_once("?provider=")
            .and_then(|(_, query)| query.split(['&', '#']).next())
            .and_then(|provider| match provider {
                "github" => Some(VaultConnectionProvider::Github),
                "google_drive" | "cloud_folder" => Some(VaultConnectionProvider::GoogleDrive),
                "dropbox" => Some(VaultConnectionProvider::Dropbox),
                "onedrive" => Some(VaultConnectionProvider::OneDrive),
                _ => None,
            });
        Self {
            state: VaultWorkflowState {
                provider,
                ..Default::default()
            },
        }
    }

    fn projected(&self, state: &VaultUiState, snapshot_changed: bool) -> VaultWorkflowState {
        let vault = &state.vault;
        let mut projected = self.state.clone();
        if !vault.github_owner.is_empty()
            && (projected.selected_owner.is_empty()
                || !vault.github_owners.contains(&projected.selected_owner))
        {
            projected.selected_owner.clone_from(&vault.github_owner);
        }
        if snapshot_changed
            && vault.repositories_loaded
            && projected.repository_name == "vmux-vault"
        {
            projected.repository_name =
                Self::suggested_repository_name(&projected.selected_owner, &vault.repositories);
        }
        if vault.unlocked {
            projected.recovery_input.clear();
        }
        if state.generated_recovery_key.is_empty() {
            projected.recovery_confirmation.clear();
        }
        projected.recovery_confirmation_complete =
            Self::recovery_key_complete(&projected.recovery_confirmation);
        projected.recovery_confirmation_matches = Self::recovery_keys_match(
            &state.generated_recovery_key,
            &projected.recovery_confirmation,
        );
        projected.recovery_input_complete = Self::recovery_key_complete(&projected.recovery_input);
        projected.owners = vault
            .github_owners
            .iter()
            .map(|owner| VaultOwnerChoice {
                value: owner.clone(),
                kind: if owner == &vault.github_owner {
                    VaultOwnerKind::User
                } else {
                    VaultOwnerKind::Organization
                },
            })
            .collect();
        let owner_prefix = format!("{}/", projected.selected_owner);
        projected.repositories = vault
            .repositories
            .iter()
            .filter(|repository| repository.name.starts_with(&owner_prefix))
            .map(|repository| VaultRepositoryChoice {
                value: repository.url.clone(),
                name: repository.name.clone(),
                empty: repository.empty,
            })
            .collect();
        if !projected
            .repositories
            .iter()
            .any(|repository| repository.value == projected.selected_repository)
        {
            projected.selected_repository.clear();
        }
        projected.connected = vault.initialized && !vault.remote.is_empty();
        projected.pending = state
            .operation
            .as_ref()
            .filter(|operation| operation.is_pending())
            .map(|operation| operation.kind);
        projected.authenticated = projected.provider.is_some_and(|provider| {
            if provider.is_github() {
                !vault.github_owner.is_empty() && vault.repositories_loaded
            } else {
                !state.cloud_root.is_empty()
            }
        });
        projected.connecting = projected.pending.is_some_and(|kind| {
            kind == VaultOperationKind::ConnectGithub || kind == VaultOperationKind::ConnectCloud
        }) || projected.provider.is_some_and(|provider| {
            provider.is_github() && !vault.github_owner.is_empty() && !vault.repositories_loaded
        });
        let pending_changes = vault
            .dirty
            .saturating_add(vault.ahead)
            .saturating_add(vault.behind);
        projected.sync_status = if vault.sync_failed {
            VaultSyncStatus::Failed
        } else if pending_changes > 0 {
            VaultSyncStatus::Changes(pending_changes)
        } else {
            VaultSyncStatus::Clean
        };
        projected.notice = state.operation.as_ref().and_then(Self::notice);
        projected.github_device_code = state
            .operation
            .as_ref()
            .and_then(VaultOperation::authorization)
            .map(|authorization| authorization.code.clone())
            .unwrap_or_default();
        projected
    }

    fn notice(operation: &VaultOperation) -> Option<VaultNotice> {
        let completion = operation.completion()?;
        if completion.success
            && matches!(
                operation.kind,
                VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey
                    | VaultOperationKind::ConnectCloud
                    | VaultOperationKind::ConnectGithub
            )
        {
            return None;
        }
        let message_id =
            if completion.success {
                match operation.kind {
                    VaultOperationKind::Create => "vault-result-created",
                    VaultOperationKind::Connect => "vault-result-connected",
                    VaultOperationKind::Sync => "vault-result-synced",
                    VaultOperationKind::ConnectGithub => "vault-result-github-connected",
                    VaultOperationKind::ConnectFolder => "vault-result-folder-connected",
                    VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey => "vault-result-created",
                    VaultOperationKind::UnlockRecoveryKey => "vault-result-connected",
                    VaultOperationKind::ConnectCloud => "vault-result-connected",
                    VaultOperationKind::CreateCloudFolder
                    | VaultOperationKind::ChooseCloudFolder => "vault-result-folder-connected",
                }
            } else {
                match operation.kind {
                    VaultOperationKind::Sync => "vault-backup-failed",
                    VaultOperationKind::GenerateRecoveryKey
                    | VaultOperationKind::CreateRecoveryKey => "vault-recovery-key-create-failed",
                    VaultOperationKind::UnlockRecoveryKey => "vault-recovery-key-invalid",
                    _ => "",
                }
            };
        if !completion.success && message_id.is_empty() && completion.message.is_empty() {
            return None;
        }
        Some(VaultNotice {
            success: completion.success,
            message: completion.message.clone(),
            message_id: message_id.to_string(),
        })
    }

    fn suggested_repository_name(owner: &str, repositories: &[VaultRepository]) -> String {
        let prefix = format!("{owner}/");
        let names = repositories
            .iter()
            .filter_map(|repository| repository.name.strip_prefix(&prefix))
            .collect::<BTreeSet<_>>();
        if !names.contains("vmux-vault") {
            return "vmux-vault".to_string();
        }
        (2..)
            .map(|suffix| format!("vmux-vault-{suffix}"))
            .find(|name| !names.contains(name.as_str()))
            .unwrap()
    }

    fn normalized_recovery_key(value: &str) -> String {
        value
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && *character != '-')
            .collect()
    }

    fn recovery_key_complete(value: &str) -> bool {
        Self::normalized_recovery_key(value).len() == 68
    }

    fn recovery_keys_match(expected: &str, actual: &str) -> bool {
        Self::recovery_key_complete(actual)
            && Self::normalized_recovery_key(expected) == Self::normalized_recovery_key(actual)
    }
}

fn page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    mut registry: Query<&mut VaultRegistry>,
    mut subscribers: Query<&mut VaultWorkflow, With<VaultSubscriber>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(page) = pages.get(webview) else {
        return;
    };
    if !page.url.starts_with(VaultPlugin::URL) {
        return;
    }
    let requested = VaultWorkflow::for_url(&page.url);
    if let Ok(mut workflow) = subscribers.get_mut(webview) {
        if workflow.state.provider.is_none() {
            workflow.state.provider = requested.state.provider;
        }
    } else {
        commands
            .entity(webview)
            .insert((VaultSubscriber::default(), requested));
    }
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    state.dirty = true;
    state.loaded = false;
    state.generation = state.generation.wrapping_add(1);
    state.revision = state.revision.wrapping_add(1);
}

fn refresh(
    trigger: On<UiInput<VaultRefreshRequest>>,
    mut registry: Query<&mut VaultRegistry>,
    pages: Query<&vmux_ecs::PageMetadata>,
    mut subscribers: Query<&mut VaultWorkflow, With<VaultSubscriber>>,
    mut commands: Commands,
) {
    let Ok(mut state) = registry.single_mut() else {
        return;
    };
    let webview = trigger.event().webview;
    let requested = pages
        .get(webview)
        .map(|page| VaultWorkflow::for_url(&page.url))
        .unwrap_or_default();
    if let Ok(mut workflow) = subscribers.get_mut(webview) {
        if workflow.state.provider.is_none() {
            workflow.state.provider = requested.state.provider;
        }
    } else {
        commands
            .entity(webview)
            .insert((VaultSubscriber::default(), requested));
    }
    state.dirty = true;
    state.loaded = false;
    state.load_repositories |= trigger.event().payload.load_repositories;
    state.generation = state.generation.wrapping_add(1);
    state.revision = state.revision.wrapping_add(1);
}

fn select_provider(
    trigger: On<UiInput<VaultProviderSelectRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let provider = trigger.event().payload.provider;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow.state.provider = Some(provider);
    workflow.state.destination = VaultDestination::Create;
    workflow.state.selected_repository.clear();
    if provider.is_github() {
        if subscriber.state.vault.github_owner.is_empty() {
            commands.trigger(UiInput {
                webview,
                payload: VaultConnectGithubRequest,
            });
        }
        return;
    }
    workflow.state.repository_name = "vmux-vault".to_string();
    commands.trigger(UiInput {
        webview,
        payload: VaultConnectCloudRequest {
            provider: provider.name().to_string(),
        },
    });
}

fn select_destination(
    trigger: On<UiInput<VaultDestinationSelectRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow.state.destination = trigger.event().payload.destination;
}

fn select_owner(
    trigger: On<UiInput<VaultOwnerSelectRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
) {
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(trigger.event().webview) else {
        return;
    };
    let owner = &trigger.event().payload.owner;
    if !subscriber.state.vault.github_owners.contains(owner) {
        return;
    }
    workflow.state.selected_owner.clone_from(owner);
    workflow.state.repository_name =
        VaultWorkflow::suggested_repository_name(owner, &subscriber.state.vault.repositories);
    workflow.state.selected_repository.clear();
}

fn set_repository_name(
    trigger: On<UiInput<VaultRepositoryNameRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow
        .state
        .repository_name
        .clone_from(&trigger.event().payload.name);
}

fn select_repository(
    trigger: On<UiInput<VaultRepositorySelectRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow
        .state
        .selected_repository
        .clone_from(&trigger.event().payload.repository);
}

fn set_privacy(
    trigger: On<UiInput<VaultPrivacyRequest>>,
    mut workflows: Query<&mut VaultWorkflow>,
) {
    let Ok(mut workflow) = workflows.get_mut(trigger.event().webview) else {
        return;
    };
    workflow.state.private = trigger.event().payload.private;
}

fn create(
    trigger: On<UiInput<VaultWorkflowCreateRequest>>,
    workflows: Query<(&VaultSubscriber, &VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, workflow)) = workflows.get(webview) else {
        return;
    };
    if subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending)
    {
        return;
    }
    let name = workflow.state.repository_name.trim();
    if name.is_empty() {
        return;
    }
    match workflow.state.provider {
        Some(VaultConnectionProvider::Github) if !workflow.state.selected_owner.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultCreateRequest {
                    repository: format!("{}/{}", workflow.state.selected_owner, name),
                    private: workflow.state.private,
                },
            });
        }
        Some(_) if !subscriber.state.cloud_root.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultCreateCloudFolderRequest {
                    root: subscriber.state.cloud_root.clone(),
                    folder_name: name.to_string(),
                },
            });
        }
        _ => {}
    }
}

fn connect(
    trigger: On<UiInput<VaultWorkflowConnectRequest>>,
    workflows: Query<(&VaultSubscriber, &VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, workflow)) = workflows.get(webview) else {
        return;
    };
    if subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending)
    {
        return;
    }
    match workflow.state.provider {
        Some(VaultConnectionProvider::Github) if !workflow.state.selected_repository.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultConnectRequest {
                    repository: workflow.state.selected_repository.clone(),
                },
            });
        }
        Some(_) if !subscriber.state.cloud_root.is_empty() => {
            commands.trigger(UiInput {
                webview,
                payload: VaultChooseCloudFolderRequest {
                    root: subscriber.state.cloud_root.clone(),
                },
            });
        }
        _ => {}
    }
}

fn confirm_recovery(
    trigger: On<UiInput<VaultRecoveryConfirmationRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow
        .state
        .recovery_confirmation
        .clone_from(&trigger.event().payload.value);
    workflow.state.recovery_confirmation_complete =
        VaultWorkflow::recovery_key_complete(&workflow.state.recovery_confirmation);
    workflow.state.recovery_confirmation_matches = VaultWorkflow::recovery_keys_match(
        &subscriber.state.generated_recovery_key,
        &workflow.state.recovery_confirmation,
    );
    let pending = subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending);
    if !pending && workflow.state.recovery_confirmation_matches {
        commands.trigger(UiInput {
            webview,
            payload: VaultCreateRecoveryKeyRequest,
        });
    }
}

fn set_recovery_input(
    trigger: On<UiInput<VaultRecoveryInputRequest>>,
    mut subscribers: Query<(&VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((subscriber, mut workflow)) = subscribers.get_mut(webview) else {
        return;
    };
    workflow
        .state
        .recovery_input
        .clone_from(&trigger.event().payload.value);
    workflow.state.recovery_input_complete =
        VaultWorkflow::recovery_key_complete(&workflow.state.recovery_input);
    let pending = subscriber
        .state
        .operation
        .as_ref()
        .is_some_and(VaultOperation::is_pending);
    if !pending && workflow.state.recovery_input_complete {
        commands.trigger(UiInput {
            webview,
            payload: VaultUnlockRecoveryKeyRequest {
                recovery_key: workflow.state.recovery_input.clone(),
            },
        });
    }
}

fn emit_state(
    registry: Query<&VaultRegistry>,
    mut subscribers: Query<(Entity, &mut VaultSubscriber, &mut VaultWorkflow)>,
    mut commands: Commands,
) {
    let Ok(state) = registry.single() else {
        return;
    };
    for (entity, mut subscriber, mut workflow) in &mut subscribers {
        let snapshot_changed = subscriber.synchronize(state.revision, &state.snapshot);
        let projected = workflow.projected(&subscriber.state, snapshot_changed);
        if workflow.state != projected {
            workflow.state = projected;
        }
        if subscriber.state.workflow != workflow.state {
            subscriber.state.workflow.clone_from(&workflow.state);
            subscriber.touch();
        }
        if subscriber.emitted_revision == subscriber.revision {
            continue;
        }
        commands.trigger(UiStateWrite::<VaultUiState>::from_event(
            entity,
            &subscriber.state,
        ));
        subscriber.emitted_revision = subscriber.revision;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{VaultCompletion, VaultOperationState, VaultSnapshot};

    #[test]
    fn page_ready_registers_state_subscriber() {
        let mut app = App::new();
        app.add_observer(page_ready);
        app.world_mut().spawn(VaultRegistry::default());
        let webview = app
            .world_mut()
            .spawn(vmux_ecs::PageMetadata {
                url: "vmux://vault/?provider=dropbox".to_string(),
                ..Default::default()
            })
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: PageReady {},
        });
        app.update();

        assert!(app.world().get::<VaultSubscriber>(webview).is_some());
        assert_eq!(
            app.world()
                .get::<VaultWorkflow>(webview)
                .unwrap()
                .state
                .provider,
            Some(VaultConnectionProvider::Dropbox)
        );
    }

    #[test]
    fn projects_choices_notice_and_recovery_validation() {
        let recovery_key = "a".repeat(68);
        let state = VaultUiState {
            vault: VaultSnapshot {
                github_owner: "jun".to_string(),
                github_owners: vec!["jun".to_string(), "vmux-ai".to_string()],
                repositories: vec![
                    VaultRepository {
                        name: "jun/vmux-vault".to_string(),
                        url: "first".to_string(),
                        private: true,
                        empty: false,
                    },
                    VaultRepository {
                        name: "jun/vmux-vault-2".to_string(),
                        url: "second".to_string(),
                        private: true,
                        empty: true,
                    },
                    VaultRepository {
                        name: "vmux-ai/shared".to_string(),
                        url: "other".to_string(),
                        private: false,
                        empty: false,
                    },
                ],
                repositories_loaded: true,
                dirty: 2,
                ..Default::default()
            },
            operation: Some(VaultOperation {
                operation_id: 8,
                kind: VaultOperationKind::Sync,
                state: VaultOperationState::Completed(VaultCompletion {
                    success: false,
                    message: "network".to_string(),
                    pending_upload: false,
                }),
            }),
            generated_recovery_key: recovery_key.clone(),
            workflow: VaultWorkflowState {
                recovery_confirmation: recovery_key,
                ..Default::default()
            },
            ..Default::default()
        };
        let workflow = VaultWorkflow {
            state: state.workflow.clone(),
        }
        .projected(&state, true);

        assert_eq!(workflow.selected_owner, "jun");
        assert_eq!(workflow.repository_name, "vmux-vault-3");
        assert_eq!(workflow.repositories.len(), 2);
        assert_eq!(workflow.owners[0].kind, VaultOwnerKind::User);
        assert_eq!(workflow.owners[1].kind, VaultOwnerKind::Organization);
        assert_eq!(workflow.sync_status, VaultSyncStatus::Changes(2));
        assert!(workflow.recovery_confirmation_complete);
        assert!(workflow.recovery_confirmation_matches);
        assert_eq!(
            workflow
                .notice
                .as_ref()
                .map(|notice| notice.message_id.as_str()),
            Some("vault-backup-failed")
        );
    }

    #[test]
    fn reads_the_requested_provider_from_the_page_url() {
        assert_eq!(
            VaultWorkflow::for_url("vmux://vault/?provider=dropbox")
                .state
                .provider,
            Some(VaultConnectionProvider::Dropbox)
        );
        assert_eq!(
            VaultWorkflow::for_url("vmux://vault/?provider=cloud_folder")
                .state
                .provider,
            Some(VaultConnectionProvider::GoogleDrive)
        );
    }
}
