#![allow(clippy::type_complexity)]

use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolProvider, ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::Commands;
#[cfg(host)]
pub use cli::ToolCliPlugin;
#[cfg(not(target_os = "ios"))]
pub use connection::McpConnectionPlugin;
pub use dotfiles::*;
pub use homebrew::*;
pub use manifest::*;
pub use mcp::*;
#[cfg(host)]
pub use mcp_config::{
    McpAuthSpec, McpConfigFile, McpConfigScope, McpLayerInput, McpResolvedServer,
    McpServerOverride, McpServerPatch, McpServerSpec, McpTrustStore, McpWorkspaceRoot,
};
#[cfg(host)]
pub use mcp_oauth::{McpOauthFlow, McpOauthStatus};
#[cfg(host)]
pub use mcp_proxy::McpProxy;
#[cfg(host)]
pub use mcp_registry::{
    McpApproveTrust, McpDeleteEntry, McpMutationResult, McpRegistryPlugin, McpResolveRequest,
    McpResolveResult, McpSaveEntry,
};
#[cfg(host)]
pub use mcp_sync::{McpDoctor, McpSync, McpSyncAgent};
pub use npm::*;
#[cfg(host)]
pub use package::ToolPackageDrain;
pub use provider::*;
#[cfg(host)]
pub use query::*;
pub use registry::*;
use std::result::Result;
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget, TaskPlugin,
    TaskResult, TaskSet, TaskToken,
};
use vmux_feature::manifest::FeaturePlugin;
pub use vmux_macro::input;

#[doc(hidden)]
pub mod __private {
    pub use inventory;
}

extern crate self as vmux_tool;

pub const EXTENSIONS_URL: &str = "vmux://tools/extensions";

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod apply;
#[cfg(host)]
mod catalog;
#[cfg(host)]
mod cli;
#[cfg(all(host, not(target_os = "ios")))]
mod command_bar;
#[cfg(not(target_os = "ios"))]
mod connection;
#[cfg(host)]
mod dotfile_inventory;
#[cfg(host)]
mod dotfile_ops;
#[cfg(host)]
mod dotfile_staging;
mod dotfiles;
#[cfg(ui)]
mod extension;
mod homebrew;
#[cfg(host)]
mod homebrew_inventory;
#[cfg(all(host, ui))]
mod host;
#[cfg(host)]
pub mod io;
mod manifest;
mod mcp;
#[cfg(host)]
pub mod mcp_config;
#[cfg(host)]
mod mcp_inventory;
#[cfg(host)]
pub mod mcp_oauth;
#[cfg(host)]
pub mod mcp_proxy;
#[cfg(host)]
pub mod mcp_registry;
#[cfg(host)]
pub mod mcp_sync;
mod npm;
#[cfg(host)]
mod npm_inventory;
#[cfg(host)]
pub mod observation;
#[cfg(all(host, ui))]
mod open;
#[cfg(host)]
mod package;
#[cfg(all(host, ui))]
mod presentation;
mod process;
mod provider;
#[cfg(host)]
mod query;
mod registry;
#[cfg(ui)]
mod route;
pub mod state;
mod storage;
#[cfg(host)]
pub(crate) mod tool_inventory;
#[cfg(ui)]
mod ui;

#[vmux_page::page]
pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<Feature>::default());
        #[cfg(ui)]
        app.add_plugins(ui::ToolsPage::plugin());

        #[cfg(all(host, ui))]
        app.add_plugins(
            Self::manifest()
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::subtree(
                    Self::url(),
                    ui::ToolsPage::page().title,
                )),
        );

        app.add_plugins((
            ToolRuntimePlugin,
            npm::NpmToolPlugin,
            homebrew::HomebrewToolPlugin,
            mcp::McpToolPlugin,
            dotfiles::DotfileToolPlugin,
        ));

        #[cfg(all(host, not(target_os = "ios")))]
        app.add_plugins(command_bar::CommandBarPlugin);

        #[cfg(all(host, ui))]
        app.add_plugins(host::ToolHostPlugin);
    }
}

pub struct ToolRuntimePlugin;

impl Plugin for ToolRuntimePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<storage::StoragePlugin>() {
            app.add_plugins(storage::StoragePlugin);
        }
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        if !app.is_plugin_added::<ToolRegistryPlugin>() {
            app.add_plugins(ToolRegistryPlugin);
        }
        #[cfg(host)]
        app.add_plugins((
            tool_inventory::InventoryPlugin,
            package::PackageChangePlugin,
            apply::ApplyPlugin,
        ));
        #[cfg(host)]
        if !app.is_plugin_added::<McpRegistryPlugin>() {
            app.add_plugins(McpRegistryPlugin);
        }
        #[cfg(host)]
        if !app.is_plugin_added::<io::ObservationIoPlugin>() {
            app.add_plugins(io::ObservationIoPlugin);
        }
        app.add_plugins((
            TaskPlugin::<Result<homebrew::ImportedBrewfile, String>>::default(),
            TaskPlugin::<Result<npm::ImportedNpmManifest, String>>::default(),
        ))
        .configure_sets(
            Update,
            (
                ToolTaskSet::Cancel,
                ToolTaskSet::Start,
                ToolTaskSet::Wake,
                ToolTaskSet::Collect,
                ToolTaskSet::Apply,
            )
                .chain()
                .after(ToolOperationRouteFlush),
        )
        .add_systems(Update, cancel.in_set(ToolTaskSet::Cancel))
        .configure_sets(
            Update,
            ToolOperationCancelSet
                .in_set(ToolTaskSet::Cancel)
                .after(cancel),
        )
        .configure_sets(
            Update,
            (
                TaskSet::of::<Result<homebrew::ImportedBrewfile, String>>(),
                TaskSet::of::<Result<npm::ImportedNpmManifest, String>>(),
            )
                .in_set(ToolTaskSet::Collect),
        )
        .add_observer(accept::<ToolInstallRequest>)
        .add_observer(accept::<ToolUpdateRequest>)
        .add_observer(accept::<ToolUninstallRequest>)
        .add_observer(accept::<ToolForgetRequest>)
        .add_observer(accept::<ToolAdoptRequest>)
        .add_observer(accept::<ToolLinkRequest>)
        .add_observer(accept::<ToolUnlinkRequest>)
        .add_observer(accept::<ToolApplyRequest>)
        .add_observer(accept::<ToolImportRequest>)
        .add_observer(completed)
        .add_observer(store::<homebrew::ImportedBrewfile>)
        .add_observer(store::<npm::ImportedNpmManifest>)
        .add_message::<ToolInventoryChanged>()
        .configure_sets(
            Update,
            (ToolOperationRouteSet, ToolOperationRouteFlush).chain(),
        )
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                route_external_operation::<ToolInstallRequest>,
                route_external_operation::<ToolUpdateRequest>,
                route_external_operation::<ToolUninstallRequest>,
                route_external_operation::<ToolForgetRequest>,
                route_external_operation::<ToolAdoptRequest>,
                route_external_operation::<ToolLinkRequest>,
                route_external_operation::<ToolUnlinkRequest>,
                route_external_operation::<ToolApplyRequest>,
                route_external_operation::<ToolImportRequest>,
            )
                .after(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                finish_operation::<homebrew::ImportedBrewfile>,
                finish_operation::<npm::ImportedNpmManifest>,
            )
                .in_set(ToolTaskSet::Apply),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolOperationRouteSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToolOperationCancelSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolOperationRouteFlush;

#[derive(Component, Clone)]
pub struct ToolOperationRequest<R: Send + Sync + 'static>(pub R);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExternalToolOperation;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolStoreOperation;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolOperationFinished;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolOperationSucceeded(pub String);

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ToolInventoryChanged(pub ToolProvider);

pub type ToolStoreTarget = vmux_ecs::EntityTarget<ToolStore>;
pub type ToolProviderTarget = vmux_ecs::EntityTarget<ToolProviderId>;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolOperationFailed(pub String);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ToolTaskSet {
    Start,
    Cancel,
    Wake,
    Collect,
    Apply,
}

fn cancel(
    mut operations: Query<
        (
            Entity,
            &mut OperationState,
            Option<&TaskToken>,
            Option<&ToolStoreTarget>,
        ),
        With<Operation>,
    >,
    stores: Query<(), With<ToolStore>>,
    mut commands: Commands,
) {
    for (entity, mut state, token, target) in &mut operations {
        let active = matches!(*state, OperationState::Pending | OperationState::Running);
        if active
            && (token.is_some_and(TaskToken::is_canceled)
                || target.is_some_and(|target| !stores.contains(target.entity())))
        {
            *state = OperationState::Canceled;
        }
        if *state == OperationState::Canceled {
            if let Some(token) = token {
                token.cancel();
            }
            commands
                .entity(entity)
                .insert(ToolOperationFinished)
                .remove::<(
                    ToolStoreTarget,
                    ToolProviderTarget,
                    ExternalToolOperation,
                    ToolOperationSucceeded,
                    ToolOperationFailed,
                )>();
        } else if *state == OperationState::Pending && token.is_some_and(TaskToken::is_active) {
            *state = OperationState::Running;
        }
    }
}

fn accept<R: Clone + Send + Sync + 'static>(
    trigger: On<Add, ToolOperationRequest<R>>,
    requests: Query<(
        &ToolOperationRequest<R>,
        Option<&ToolStoreTarget>,
        Option<&OperationState>,
    )>,
    mut commands: Commands,
) where
    state::ToolOperationKey: From<R>,
{
    let entity = trigger.event_target();
    let Ok((request, target, state)) = requests.get(entity) else {
        return;
    };
    if state
        .is_some_and(|state| !matches!(state, OperationState::Pending | OperationState::Running))
    {
        return;
    }
    let operation = state::ToolOperationKey::from(request.0.clone());
    let mut entity = commands.entity(entity);
    entity.insert(Operation::bundle(operation.kind.operation_kind()).unwrap());
    if let Some(target) = target {
        entity.insert(OperationTarget::new(target.entity()));
    }
}

fn completed(
    trigger: On<Add, ToolOperationFinished>,
    outcomes: Query<(Option<&ToolOperationFailed>, Option<&OperationState>)>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((failure, state)) = outcomes.get(entity) else {
        return;
    };
    commands
        .entity(entity)
        .remove::<(ToolStoreTarget, ToolProviderTarget)>();
    if state == Some(&OperationState::Canceled) {
        commands
            .entity(entity)
            .remove::<(ToolOperationSucceeded, ToolOperationFailed)>();
        return;
    }
    let result = match failure {
        Some(failure) => Err(failure.0.clone()),
        None => Ok(()),
    };
    commands.trigger(OperationCompleted { entity, result });
}

fn finish_operation<T: Component>(
    mut operations: Query<(Entity, &mut TaskResult<Result<T, String>>)>,
    mut commands: Commands,
) {
    for (entity, mut completion) in &mut operations {
        let Some(result) = completion.take() else {
            continue;
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        commands.queue(completion.guard(
            entity,
            StoreCompletion {
                entity,
                result: Some(result),
            },
        ));
    }
}

#[derive(Event)]
struct StoreCompletion<T: Component> {
    entity: Entity,
    result: Option<Result<T, String>>,
}

impl<T: Component> Command for StoreCompletion<T> {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

fn store<T: Component>(
    mut receipt: On<StoreCompletion<T>>,
    operations: Query<(
        Option<&OperationState>,
        Option<&vmux_ecs::EntityTarget<ToolStore>>,
    )>,
    stores: Query<(), With<ToolStore>>,
    mut commands: Commands,
) {
    let receipt = receipt.event_mut();
    let Ok((state, target)) = operations.get(receipt.entity) else {
        return;
    };
    if state
        .is_some_and(|state| !matches!(state, OperationState::Pending | OperationState::Running))
    {
        return;
    }
    if target.is_some_and(|target| !stores.contains(target.entity())) {
        commands
            .entity(receipt.entity)
            .insert(OperationState::Canceled);
        return;
    }
    let Some(result) = receipt.result.take() else {
        return;
    };
    match result {
        Ok(output) => {
            commands.entity(receipt.entity).insert(output);
        }
        Err(error) => {
            commands
                .entity(receipt.entity)
                .insert((ToolOperationFinished, ToolOperationFailed(error)));
        }
    }
}

fn route_external_operation<R: Clone + Send + Sync + 'static>(
    requests: Query<
        (Entity, Option<&OperationState>),
        (
            Added<ToolStoreTarget>,
            With<ToolOperationRequest<R>>,
            Without<ToolStoreOperation>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, state) in &requests {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        commands.entity(entity).insert(ExternalToolOperation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vmux_ecs::AsyncTask;

    #[test]
    fn overridden_data_root_keeps_configuration_and_link_effects_in_the_copy() {
        if let Some(root) = std::env::var_os("VMUX_QA_PATHS_CHILD") {
            let root = PathBuf::from(root);
            let store = ToolStore::current();
            assert!(
                store.root().starts_with(&root),
                "refuse live configuration writes"
            );
            assert!(
                store.home().starts_with(&root),
                "refuse live home mutations"
            );
            ToolsManifest::default()
                .write_to(&store.manifest_path())
                .unwrap();
            let source = store.dotfiles_dir().join("qa/.vmx-qa");
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, b"copied input").unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy_app::TaskPoolPlugin::default(),
                ToolRuntimePlugin,
                dotfiles::DotfileToolPlugin,
            ));
            let owner = app.world_mut().spawn(store.clone()).id();
            let request = app
                .world_mut()
                .spawn((
                    Operation::bundle("test.copied.dotfile.link").unwrap(),
                    ToolStoreTarget::new(owner),
                    LinkDotfilePackage::new("qa"),
                ))
                .id();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.world().get::<ToolOperationFinished>(request).is_none() {
                app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "copied operation did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(app.world().get::<ToolOperationSucceeded>(request).is_some());
            assert!(store.lease().is_some());
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let live = ToolStore::current().manifest_path();
        let before = std::fs::read(&live).ok();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::overridden_data_root_keeps_configuration_and_link_effects_in_the_copy",
                "--nocapture",
            ])
            .env("VMUX_DATA_DIR", directory.path())
            .env("VMUX_QA_PATHS_CHILD", directory.path())
            .env_remove("VMUX_TEST")
            .status()
            .unwrap();
        assert!(status.success());
        let config = directory.path().join("config/tools/tools.toml");
        let manifest = ToolsManifest::read(&config).unwrap();
        assert_eq!(manifest.dotfiles.packages, ["qa"]);
        assert_eq!(
            std::fs::read(directory.path().join("home/.vmx-qa")).unwrap(),
            b"copied input"
        );
        assert_eq!(std::fs::read(&live).ok(), before);
    }

    struct TaskFixture {
        app: App,
        pool: bevy_tasks::TaskPool,
        store: Entity,
        _directory: tempfile::TempDir,
    }

    impl TaskFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy_app::TaskPoolPlugin::default(),
                ToolRuntimePlugin,
                npm::NpmToolPlugin,
            ));
            app.update();
            let store = app
                .world_mut()
                .spawn(ToolStore::new(directory.path(), directory.path()))
                .id();
            Self {
                app,
                pool: bevy_tasks::TaskPoolBuilder::new().num_threads(1).build(),
                store,
                _directory: directory,
            }
        }

        fn failing(&mut self) -> Entity {
            let task = self
                .pool
                .spawn(async { Err::<ImportedNpmManifest, _>("late failure".to_string()) });
            let entity = self
                .app
                .world_mut()
                .spawn((
                    ToolOperationRequest(ToolImportRequest {
                        provider: ToolProvider::new("npm"),
                        value: String::new(),
                    }),
                    ToolStoreTarget::new(self.store),
                    AsyncTask::new(task),
                ))
                .id();
            self.app.world_mut().flush();
            entity
        }

        fn settle(&mut self) {
            for _ in 0..4 {
                self.app.update();
            }
        }

        fn import(&mut self, path: &std::path::Path, canceled: bool) -> Entity {
            let provider = self
                .app
                .world_mut()
                .query::<(Entity, &ToolProviderBinding)>()
                .iter(self.app.world())
                .find_map(|(entity, binding)| (binding.index() == 2).then_some(entity))
                .unwrap();
            let mut operation = self.app.world_mut().spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: ToolProvider::new("npm"),
                    value: path.to_string_lossy().into_owned(),
                }),
                ToolStoreTarget::new(self.store),
                ToolProviderTarget::new(provider),
            ));
            if canceled {
                operation.insert(OperationState::Canceled);
            }
            let entity = operation.id();
            self.app.world_mut().flush();
            entity
        }

        fn writing(
            &mut self,
        ) -> (
            Entity,
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        ) {
            let store = self
                .app
                .world()
                .get::<ToolStore>(self.store)
                .unwrap()
                .clone();
            let lease = store.lease().unwrap();
            let path = store.manifest_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut manifest = ToolsManifest::default();
            manifest.set_package("npm", "first", true);
            let bytes = toml::to_string_pretty(&manifest).unwrap();
            let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
            let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
            let task = self.pool.spawn(async move {
                let _lease = lease;
                started_tx.send(()).map_err(|error| error.to_string())?;
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .map_err(|error| error.to_string())?;
                std::fs::write(path, bytes).map_err(|error| error.to_string())?;
                Err::<ImportedNpmManifest, _>("canceled writer finished".to_string())
            });
            let entity = self
                .app
                .world_mut()
                .spawn((
                    ToolOperationRequest(ToolImportRequest {
                        provider: ToolProvider::new("npm"),
                        value: String::new(),
                    }),
                    ToolStoreTarget::new(self.store),
                    AsyncTask::new(task),
                ))
                .id();
            self.app.world_mut().flush();
            (entity, release_tx, started_rx)
        }

        fn finish(&mut self, entity: Entity) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if self
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(entity)
                    .is_some()
                {
                    return;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn canceled_requests_never_start_adapter_writes() {
        let mut fixture = TaskFixture::new();
        let path = fixture._directory.path().join("package.json");
        std::fs::write(&path, r#"{"dependencies":{"typescript":"^5"}}"#).unwrap();
        let entity = fixture.import(&path, true);
        fixture.settle();
        let store = fixture.app.world().get::<ToolStore>(fixture.store).unwrap();
        assert!(!store.manifest_path().exists());
        assert!(!store.is_busy());
        assert_eq!(
            fixture.app.world().get::<OperationState>(entity),
            Some(&OperationState::Canceled)
        );
    }

    #[test]
    fn canceled_blocking_writers_keep_the_store_lease_until_they_finish() {
        let mut fixture = TaskFixture::new();
        let path = fixture._directory.path().join("package.json");
        std::fs::write(&path, r#"{"dependencies":{"second":"^1"}}"#).unwrap();
        let (first, release, started) = fixture.writing();
        started
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .insert(OperationState::Canceled);
        fixture.settle();
        let fresh_store = ToolStore::new(fixture._directory.path(), fixture._directory.path());
        fixture.store = fixture.app.world_mut().spawn(fresh_store).id();
        let second = fixture.import(&path, false);
        fixture.settle();
        assert!(
            fixture
                .app
                .world()
                .get::<AsyncTask<Result<ImportedNpmManifest, String>>>(second)
                .is_none()
        );
        let store = fixture.app.world().get::<ToolStore>(fixture.store).unwrap();
        assert!(store.is_busy());
        assert!(!store.manifest_path().exists());
        release.send(()).unwrap();
        fixture.finish(second);
        let store = fixture.app.world().get::<ToolStore>(fixture.store).unwrap();
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .packages["npm"],
            ["first", "second"]
        );
        assert!(!store.is_busy());
        assert_eq!(
            fixture.app.world().get::<OperationState>(first),
            Some(&OperationState::Canceled)
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(first)
                .is_none()
        );
    }

    #[test]
    fn helper_imports_complete_without_operation_state() {
        let mut fixture = TaskFixture::new();
        let path = fixture._directory.path().join("package.json");
        std::fs::write(&path, r#"{"dependencies":{"typescript":"^5"}}"#).unwrap();
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                npm::ImportNpmManifest::new(path),
                ToolStoreOperation,
                ToolStoreTarget::new(fixture.store),
            ))
            .id();
        fixture.finish(entity);
        let store = fixture.app.world().get::<ToolStore>(fixture.store).unwrap();
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .packages["npm"],
            ["typescript"]
        );
        assert!(fixture.app.world().get::<OperationState>(entity).is_none());
    }

    #[test]
    fn canceled_tool_work_cannot_publish_a_late_failure_or_hold_the_store_slot() {
        let mut fixture = TaskFixture::new();
        let entity = fixture.failing();
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(OperationState::Canceled);
        fixture.settle();
        assert_eq!(
            fixture.app.world().get::<OperationState>(entity),
            Some(&OperationState::Canceled)
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(entity)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(entity)
                .is_none()
        );
        assert!(fixture.app.world().get::<ToolStoreTarget>(entity).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<AsyncTask<Result<ImportedNpmManifest, String>>>(entity)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<TaskResult<Result<ImportedNpmManifest, String>>>(entity)
                .is_none()
        );
    }

    #[test]
    fn losing_the_target_store_cancels_work_before_its_result_is_adopted() {
        let mut fixture = TaskFixture::new();
        let entity = fixture.failing();
        fixture.app.world_mut().entity_mut(fixture.store).despawn();
        fixture.settle();
        assert_eq!(
            fixture.app.world().get::<OperationState>(entity),
            Some(&OperationState::Canceled)
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(entity)
                .is_none()
        );
        assert!(fixture.app.world().get::<ToolStoreTarget>(entity).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<AsyncTask<Result<ImportedNpmManifest, String>>>(entity)
                .is_none()
        );
    }

    #[test]
    fn manifest_roundtrip_normalizes_packages() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "typescript", true);
        manifest.set_package("npm", "eslint", true);
        manifest.set_package("npm", "typescript", true);
        manifest.set_dotfile_package("shell", true);
        manifest.write_to(&path).unwrap();

        let loaded = ToolsManifest::read(&path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "typescript"]);
        assert_eq!(loaded.dotfiles.packages, ["shell"]);
    }

    #[test]
    fn manifest_omits_empty_sections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "typescript", true);

        manifest.write_to(&path).unwrap();

        let source = std::fs::read_to_string(path).unwrap();
        assert!(source.contains("[packages]"));
        assert!(!source.contains("[mcp"));
        assert!(!source.contains("[dotfiles]"));
    }

    #[test]
    fn unsupported_manifest_versions_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        std::fs::write(&path, "version = 2\n").unwrap();
        assert!(
            ToolsManifest::read(&path)
                .unwrap_err()
                .contains("unsupported tools manifest version: 2")
        );
    }

    #[test]
    fn brewfile_import_separates_formulae_and_casks() {
        let imported = BrewfileImport::parse(
            r#"
tap "homebrew/cask-fonts"
brew "ripgrep"
brew 'openssl@3', link: false
cask "ghostty"
brew "ripgrep"
"#,
        );

        assert_eq!(imported.formulae, ["openssl@3", "ripgrep"]);
        assert_eq!(imported.casks, ["ghostty"]);
    }

    #[test]
    fn npm_import_combines_runtime_development_and_optional_dependencies() {
        let imported = NpmManifest::parse(
            r#"{
                "dependencies": {"typescript": "^5"},
                "devDependencies": {"eslint": "^9"},
                "optionalDependencies": {"prettier": "^3"},
                "peerDependencies": {"react": "^19"}
            }"#,
        )
        .unwrap();

        assert_eq!(imported.packages, ["eslint", "prettier", "typescript"]);
    }

    #[test]
    fn npm_provider_routes_request_and_updates_the_target_store() {
        let temp = tempfile::tempdir().unwrap();
        let package_json = temp.path().join("package.json");
        std::fs::write(&package_json, r#"{"dependencies":{"typescript":"^5"}}"#).unwrap();
        let store = ToolStore::new(temp.path(), temp.path());
        let mut app = App::new();
        app.add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            ToolRuntimePlugin,
            npm::NpmToolPlugin,
        ));
        app.update();
        let provider = {
            let mut providers = app.world_mut().query::<(Entity, &ToolProviderBinding)>();
            providers
                .iter(app.world())
                .find_map(|(entity, binding)| (binding.index() == 2).then_some(entity))
                .unwrap()
        };
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: state::ToolProvider::new("npm"),
                    value: package_json.to_string_lossy().into_owned(),
                }),
                ToolStoreTarget::new(store_entity),
                ToolProviderTarget::new(provider),
            ))
            .id();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            app.update();
            if app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let output = app
            .world()
            .get::<ImportedNpmManifest>(operation)
            .expect("NPM import did not complete before the deadline");
        assert_eq!(output.packages(), 1);
        assert_eq!(
            app.world().get::<ToolOperationSucceeded>(operation),
            Some(&ToolOperationSucceeded(
                "imported 1 NPM package(s)".to_string()
            )),
        );
        assert!(
            app.world()
                .get::<ExternalToolOperation>(operation)
                .is_none()
        );
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .packages["npm"],
            ["typescript"]
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn package_imports_merge_without_removing_existing_desired_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = ToolStore::new(temp.path(), temp.path());
        let manifest_path = store.manifest_path();
        let brewfile = temp.path().join("Brewfile");
        let package_json = temp.path().join("package.json");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "existing", true);
        manifest.write_to(&manifest_path).unwrap();
        std::fs::write(&brewfile, "tap \"homebrew/cask-fonts\"\n# retained\nbrew \"ripgrep\", link: false\ncask \"ghostty\"\n").unwrap();
        std::fs::write(&package_json, r#"{"devDependencies":{"eslint":"1"}}"#).unwrap();

        let mut app = App::new();
        app.add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            ToolRuntimePlugin,
            homebrew::HomebrewToolPlugin,
            npm::NpmToolPlugin,
        ));
        let source = app.world_mut().spawn(store.clone()).id();
        let brew = app
            .world_mut()
            .spawn((
                homebrew::ImportBrewfile::new(brewfile),
                ToolStoreOperation,
                ToolStoreTarget::new(source),
            ))
            .id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.world().get::<ToolOperationFinished>(brew).is_none() {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "Brewfile import did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(app.world().get::<ToolOperationFailed>(brew).is_none());
        assert_eq!(
            std::fs::read_to_string(store.brewfile_path()).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# retained\nbrew \"ripgrep\", link: false\ncask \"ghostty\"\n"
        );

        let npm = app
            .world_mut()
            .spawn((
                npm::ImportNpmManifest::new(package_json),
                ToolStoreOperation,
                ToolStoreTarget::new(source),
            ))
            .id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.world().get::<ToolOperationFinished>(npm).is_none() {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "NPM import did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(app.world().get::<ToolOperationFailed>(npm).is_none());
        let loaded = ToolsManifest::read(&manifest_path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "existing"]);
        assert_eq!(loaded.packages["homebrew-formula"], ["ripgrep"]);
        assert_eq!(loaded.packages["homebrew-cask"], ["ghostty"]);
    }

    #[cfg(host)]
    #[test]
    fn tool_runtime_composes_when_mcp_registry_is_already_registered() {
        let mut app = App::new();
        app.add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            McpRegistryPlugin,
            ToolRuntimePlugin,
        ));
        assert!(app.is_plugin_added::<McpRegistryPlugin>());
        assert!(app.is_plugin_added::<ToolRuntimePlugin>());
    }
}
