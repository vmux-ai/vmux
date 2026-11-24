use std::marker::PhantomData;

use bevy::prelude::*;
use vmux_ecs::{
    JobRequest, JobResult, Operation, OperationRetained, OperationState, OperationTarget,
};

use crate::io::Execute;
use crate::state::{
    ToolAdoptRequest, ToolForgetRequest, ToolInstallRequest, ToolOperationKey, ToolOperationKind,
    ToolProvider, ToolUninstallRequest, ToolUpdateRequest,
};
use crate::storage::{ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult};
use crate::{
    ToolInventoryChanged, ToolOperationFailed, ToolOperationFinished, ToolOperationRequest,
    ToolOperationRouteSet, ToolOperationSucceeded, ToolProviderId, ToolProviderTarget, ToolStore,
    ToolStoreOperation, ToolStoreTarget, ToolTaskSet, ToolsManifest,
};

pub(crate) struct PackageChangePlugin;

impl Plugin for PackageChangePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                route::<ToolInstallRequest>,
                route::<ToolUpdateRequest>,
                route::<ToolUninstallRequest>,
                route::<ToolAdoptRequest>,
                route::<ToolForgetRequest>,
            )
                .in_set(ToolOperationRouteSet),
        )
        .add_systems(
            Update,
            (
                (
                    retire::<ToolInstallRequest>,
                    retire::<ToolUpdateRequest>,
                    retire::<ToolUninstallRequest>,
                    retire::<ToolAdoptRequest>,
                    retire::<ToolForgetRequest>,
                )
                    .chain(),
                (begin, read, execute, received, reconcile, written).chain(),
                (
                    finish::<ToolInstallRequest>,
                    finish::<ToolUpdateRequest>,
                    finish::<ToolUninstallRequest>,
                    finish::<ToolAdoptRequest>,
                    finish::<ToolForgetRequest>,
                )
                    .chain(),
            )
                .chain()
                .in_set(ToolTaskSet::Start),
        );
    }
}

#[derive(Component)]
struct Change {
    requester: Entity,
    release_requester: bool,
    store: Entity,
    provider: Entity,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    id: ToolProviderId,
    item: String,
    enabled: bool,
    message: String,
}

#[derive(Component)]
struct Accepted<R: Send + Sync + 'static>(PhantomData<R>);

#[derive(Component)]
struct Process(Execute);

#[derive(Component)]
struct Started;

#[derive(Component)]
struct Waiting {
    work: Entity,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ToolPackageDrain<'w, 's> {
    operations: Query<'w, 's, &'static Change>,
}

impl ToolPackageDrain<'_, '_> {
    pub fn is_drained(&self, entity: Entity) -> bool {
        for operation in &self.operations {
            if operation.requester == entity {
                return false;
            }
        }
        true
    }
}

#[derive(Component)]
struct Manifest(ToolsManifest);

#[derive(Component)]
struct Executing;

#[derive(Component)]
struct Ready;

#[derive(Component)]
struct Outcome(Result<(), String>);

#[derive(Component)]
pub(crate) struct Published;

#[derive(bevy::ecs::system::SystemParam)]
struct Notifications<'w> {
    changed: MessageWriter<'w, ToolInventoryChanged>,
    wake: Option<Res<'w, vmux_ecs::JobWake>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Sources<'w, 's> {
    stores: Query<'w, 's, &'static ToolStore>,
    providers: Query<'w, 's, &'static ToolProviderId, With<crate::observation::ManifestProvider>>,
}

impl Sources<'_, '_> {
    fn provider(&self, key: &ToolOperationKey, target: Entity) -> Option<&ToolProviderId> {
        let id = self.providers.get(target).ok()?;
        if key.provider != id.0 {
            return None;
        }
        if id.0.is("mcp") && key.kind != ToolOperationKind::Forget {
            return None;
        }
        let native = matches!(
            key.kind,
            ToolOperationKind::Install | ToolOperationKind::Update | ToolOperationKind::Uninstall
        );
        if native && !matches!(id.0.id(), "npm" | "homebrew-formula" | "homebrew-cask") {
            return None;
        }
        Some(id)
    }

    fn current(
        &self,
        change: &Change,
        captured: &ToolStore,
        store: Option<&ToolStoreTarget>,
        provider: Option<&ToolProviderTarget>,
    ) -> bool {
        store.is_some_and(|store| store.entity() == change.store)
            && provider.is_some_and(|provider| provider.entity() == change.provider)
            && self.stores.get(change.store).ok() == Some(captured)
            && self.providers.get(change.provider).ok() == Some(&change.id)
    }
}

fn route<R: Clone + Send + Sync + 'static>(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<R>,
            &ToolStoreTarget,
            &ToolProviderTarget,
            &OperationState,
            Has<OperationRetained>,
        ),
        (
            Added<ToolStoreTarget>,
            Without<Waiting>,
            Without<ToolOperationFinished>,
        ),
    >,
    sources: Sources,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (entity, request, store, provider, state, retained) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let key = ToolOperationKey::from(request.0.clone());
        let Some(id) = sources.provider(&key, provider.entity()) else {
            continue;
        };
        let native = matches!(
            key.kind,
            ToolOperationKind::Install | ToolOperationKind::Update | ToolOperationKind::Uninstall
        );
        let Ok(captured) = sources.stores.get(store.entity()) else {
            continue;
        };
        let item = key.item_id.trim();
        if item.is_empty() || !id.0.is("mcp") && (item.starts_with('-') || item.contains('\0')) {
            commands.entity(entity).insert((
                ToolStoreOperation,
                ToolOperationFailed("package name is required".into()),
                ToolOperationFinished,
            ));
            continue;
        }
        if id.0.id() == "homebrew-cask"
            && item == "vmux"
            && key.kind == ToolOperationKind::Uninstall
        {
            commands.entity(entity).insert((
                ToolStoreOperation,
                ToolOperationFailed("the Vmux cask cannot be removed".into()),
                ToolOperationFinished,
            ));
            continue;
        }
        let enabled = matches!(
            key.kind,
            ToolOperationKind::Install | ToolOperationKind::Update | ToolOperationKind::Adopt
        );
        let message = match key.kind {
            ToolOperationKind::Install => format!("{item} installed"),
            ToolOperationKind::Update => format!("{item} updated"),
            ToolOperationKind::Uninstall => format!("{item} removed"),
            ToolOperationKind::Adopt => format!("{item} is now managed"),
            ToolOperationKind::Forget => format!("{item} removed from tools.toml"),
            _ => continue,
        };
        let work = commands
            .spawn((
                Name::new(format!("Change package {item}")),
                Operation::bundle("tool.package.change").unwrap(),
                OperationRetained,
                OperationTarget::new(store.entity()),
                captured.clone(),
                Accepted::<R>(PhantomData),
                Change {
                    requester: entity,
                    release_requester: !retained,
                    store: store.entity(),
                    provider: provider.entity(),
                    id: id.clone(),
                    item: item.into(),
                    enabled,
                    message,
                },
            ))
            .id();
        if native {
            commands
                .entity(work)
                .insert(Process::new(&id.0, key.kind, item));
        }
        let mut requester = commands.entity(entity);
        requester
            .insert((ToolStoreOperation, Waiting { work }))
            .remove::<crate::ExternalToolOperation>();
        if !retained {
            requester.insert(OperationRetained);
        }
    }
}

impl Process {
    fn new(provider: &ToolProvider, kind: ToolOperationKind, item: &str) -> Self {
        let mut arguments = Vec::new();
        let program = if provider.id() == "npm" {
            "npm"
        } else {
            "brew"
        };
        let verb = match kind {
            ToolOperationKind::Install => "install",
            ToolOperationKind::Update if program == "brew" => "upgrade",
            ToolOperationKind::Update => "update",
            ToolOperationKind::Uninstall => "uninstall",
            _ => unreachable!(),
        };
        arguments.push(verb.into());
        if program == "npm" {
            arguments.push("--global".into());
        }
        if provider.id() == "homebrew-cask" {
            arguments.push("--cask".into());
        }
        arguments.push("--".into());
        arguments.push(item.into());
        Self(Execute {
            program: program.into(),
            arguments,
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(1200),
        })
    }
}

fn retire<R: Clone + Send + Sync + 'static>(
    mut operations: Query<
        (
            Entity,
            &Change,
            &ToolStore,
            Option<&OperationTarget>,
            &mut OperationState,
        ),
        (With<Accepted<R>>, Without<Waiting>),
    >,
    requests: Query<
        (
            &Waiting,
            &OperationState,
            Option<&ToolOperationRequest<R>>,
            Option<&ToolStoreTarget>,
            Option<&ToolProviderTarget>,
            Has<ToolOperationFinished>,
        ),
        With<Waiting>,
    >,
    sources: Sources,
) where
    ToolOperationKey: From<R>,
{
    for (work, change, captured, target, mut state) in &mut operations {
        if !matches!(*state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let valid = match requests.get(change.requester) {
            Ok((waiting, state, request, store, provider, finished)) => {
                waiting.work == work
                    && !finished
                    && matches!(state, OperationState::Pending | OperationState::Running)
                    && target.is_some_and(|target| target.entity() == change.store)
                    && sources.current(change, captured, store, provider)
                    && request.is_some_and(|request| {
                        let key = ToolOperationKey::from(request.0.clone());
                        key.item_id.trim() == change.item && key.provider == change.id.0
                    })
            }
            Err(_) => false,
        };
        if !valid {
            *state = OperationState::Canceled;
        }
    }
}

fn begin(
    operations: Query<(Entity, &Change, &ToolStore, &OperationState), Without<Started>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, change, store, state) in &operations {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Some(wake) = &wake {
            store.bind_waker(wake.0.clone());
        }
        let Some(lease) = store.lease() else {
            continue;
        };
        commands.entity(entity).insert((
            Started,
            StoreLease { _lease: lease },
            ReadRequest { retain_lease: true },
            OperationState::Running,
        ));
        if let Ok(mut requester) = commands.get_entity(change.requester) {
            requester.insert(OperationState::Running);
        }
    }
}

fn read(
    operations: Query<(Entity, &ReadResult, &OperationState), With<Change>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &operations {
        commands.entity(entity).remove::<ReadResult>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(manifest) => {
                commands.entity(entity).insert(Manifest(manifest.clone()));
            }
            Err(error) => {
                commands.entity(entity).insert(Outcome(Err(error.clone())));
            }
        }
    }
}

fn execute(
    operations: Query<
        (Entity, Option<&Process>, &OperationState),
        (
            With<Change>,
            With<Manifest>,
            Without<Executing>,
            Without<Ready>,
            Without<Outcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, process, state) in &operations {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(process) = process {
            commands
                .entity(entity)
                .insert((Executing, JobRequest(process.0.clone())));
        } else {
            commands.entity(entity).insert(Ready);
        }
    }
}

fn received(
    operations: Query<
        (Entity, &JobResult<Execute>, &OperationState),
        (With<Change>, With<Executing>),
    >,
    mut commands: Commands,
) {
    for (entity, result, state) in &operations {
        commands
            .entity(entity)
            .remove::<(JobResult<Execute>, Executing)>();
        if *state != OperationState::Running {
            continue;
        }
        let output = match &result.0 {
            Ok(Ok(Some(output))) => &output.0,
            Ok(Ok(None)) => {
                commands
                    .entity(entity)
                    .insert(Outcome(Err("package manager is not installed".into())));
                continue;
            }
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Outcome(Err(error.clone())));
                continue;
            }
        };
        if output.status.success() {
            commands.entity(entity).insert(Ready);
        } else {
            let mut error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if error.is_empty() {
                error = format!("package manager exited with {}", output.status);
            }
            commands.entity(entity).insert(Outcome(Err(error)));
        }
    }
}

fn reconcile(
    mut operations: Query<
        (Entity, &Change, &mut Manifest, &OperationState),
        (
            With<Ready>,
            Without<WriteRequest>,
            Without<WriteResult>,
            Without<Outcome>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, change, mut manifest, state) in &mut operations {
        if *state != OperationState::Running {
            continue;
        }
        let present = if change.id.0.is("mcp") {
            manifest.0.mcp.servers.contains_key(&change.item)
        } else {
            manifest.0.contains(change.id.0.id(), &change.item)
        };
        if present == change.enabled {
            commands.entity(entity).insert(Outcome(Ok(())));
            continue;
        }
        if change.id.0.is("mcp") {
            manifest.0.mcp.servers.remove(&change.item);
        } else {
            manifest
                .0
                .set_package(change.id.0.id(), &change.item, change.enabled);
        }
        commands.entity(entity).insert(WriteRequest {
            manifest: manifest.0.clone(),
            retain_lease: true,
        });
    }
}

fn written(operations: Query<(Entity, &WriteResult), With<Change>>, mut commands: Commands) {
    for (entity, result) in &operations {
        commands
            .entity(entity)
            .remove::<WriteResult>()
            .insert(Outcome(result.0.clone()));
    }
}

fn finish<R: Clone + Send + Sync + 'static>(
    operations: Query<
        (
            Entity,
            &Change,
            &ToolStore,
            &OperationState,
            Option<&Manifest>,
            Option<&Outcome>,
            Has<ReadRequest>,
            Has<WriteRequest>,
            Has<Started>,
        ),
        With<Accepted<R>>,
    >,
    requests: Query<
        (
            &Waiting,
            &OperationState,
            Option<&ToolOperationRequest<R>>,
            Option<&ToolStoreTarget>,
            Option<&ToolProviderTarget>,
        ),
        With<Waiting>,
    >,
    sources: Sources,
    native: Query<&OperationTarget>,
    mut registry: Query<(
        &mut crate::tool_inventory::Registry,
        &ToolStore,
        &mut ToolsManifest,
    )>,
    mut notifications: Notifications,
    mut commands: Commands,
) where
    ToolOperationKey: From<R>,
{
    for (work, change, store, state, manifest, outcome, reading, writing, started) in &operations {
        if reading || writing || native.iter().any(|target| target.entity() == work) {
            continue;
        }
        if matches!(state, OperationState::Pending | OperationState::Running) && outcome.is_none() {
            continue;
        }
        let requester = requests
            .get(change.requester)
            .ok()
            .filter(|(waiting, _, _, _, _)| waiting.work == work);
        let valid = *state == OperationState::Running
            && requester.is_some_and(|(_, state, request, target, provider)| {
                *state == OperationState::Running
                    && sources.current(change, store, target, provider)
                    && request.is_some_and(|request| {
                        let key = ToolOperationKey::from(request.0.clone());
                        key.item_id.trim() == change.item && key.provider == change.id.0
                    })
            });
        let success = valid && outcome.is_some_and(|outcome| outcome.0.is_ok());
        if started {
            for (mut registry, current, mut committed) in &mut registry {
                if current != store {
                    continue;
                }
                if success && let Some(manifest) = manifest {
                    committed.clone_from(&manifest.0);
                }
                registry.dirty = true;
                registry.generation = registry.generation.wrapping_add(1);
            }
        }
        if let Some((_, state, _, _, _)) = requester {
            let mut requester = commands.entity(change.requester);
            requester.remove::<Waiting>();
            if change.release_requester {
                requester.remove::<OperationRetained>();
            }
            if matches!(state, OperationState::Pending | OperationState::Running) {
                let result = if valid {
                    outcome.unwrap().0.clone()
                } else {
                    Err("package operation owner changed".into())
                };
                match result {
                    Ok(()) => {
                        notifications
                            .changed
                            .write(ToolInventoryChanged(change.id.0.clone()));
                        requester
                            .insert((Published, ToolOperationSucceeded(change.message.clone())));
                    }
                    Err(error) => {
                        requester.insert(ToolOperationFailed(error));
                    }
                }
                requester.insert(ToolOperationFinished);
            }
        }
        commands.entity(work).despawn();
        if let Some(wake) = &notifications.wake {
            wake.0.wake_by_ref();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Commands, Entity, Plugin, Query, Res, Resource, Update};
    use std::os::unix::process::ExitStatusExt;

    struct McpFixture {
        app: App,
        directory: tempfile::TempDir,
        store: ToolStore,
        source: Entity,
        provider: Entity,
    }

    impl McpFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            let manifest: ToolsManifest = toml::from_str(
                "version = 1\n[mcp.servers.keep]\ntransport = 'stdio'\ncommand = 'keep'\n[mcp.servers.'-remove']\ntransport = 'stdio'\ncommand = 'remove'\n",
            )
            .unwrap();
            manifest.write_to(&store.manifest_path()).unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                vmux_feature::manifest::FeaturePlugin::<crate::Feature>::default(),
                crate::ToolRuntimePlugin,
                crate::mcp::McpToolPlugin,
            ));
            let source = app
                .world_mut()
                .spawn((
                    crate::tool_inventory::Registry::default(),
                    crate::catalog::Catalog::default(),
                    store.clone(),
                    manifest,
                    crate::tool_inventory::Sequence::default(),
                ))
                .id();
            app.update();
            let provider = app
                .world_mut()
                .query::<(Entity, &ToolProviderId)>()
                .iter(app.world())
                .find(|(_, id)| id.0.is("mcp"))
                .unwrap()
                .0;
            Self {
                app,
                directory,
                store,
                source,
                provider,
            }
        }

        fn forget(&mut self) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("test.mcp.forget").unwrap(),
                    ToolStoreTarget::new(self.source),
                    ToolProviderTarget::new(self.provider),
                    ToolOperationRequest(ToolForgetRequest {
                        provider: ToolProvider::new("mcp"),
                        id: "-remove".into(),
                    }),
                ))
                .id()
        }

        fn finish(&mut self, requester: Entity) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                let pending = self
                    .app
                    .world_mut()
                    .query::<&Change>()
                    .iter(self.app.world())
                    .any(|change| change.requester == requester);
                let finished = self.app.world().get_entity(requester).is_err()
                    || self
                        .app
                        .world()
                        .get::<ToolOperationFinished>(requester)
                        .is_some()
                    || self
                        .app
                        .world()
                        .get::<OperationState>(requester)
                        .is_some_and(|state| {
                            !matches!(state, OperationState::Pending | OperationState::Running)
                        });
                if !pending && finished {
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "MCP configuration did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn mcp_forget_commits_configuration_through_the_registered_store_systems() {
        let mut fixture = McpFixture::new();
        let request = fixture.forget();
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(request)
                .is_some()
        );
        let manifest: ToolsManifest =
            toml::from_str(&std::fs::read_to_string(fixture.store.manifest_path()).unwrap())
                .unwrap();
        assert_eq!(
            manifest.mcp.servers.keys().cloned().collect::<Vec<_>>(),
            ["keep"]
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn mcp_forget_refuses_cancelled_replaced_and_orphaned_configuration_owners() {
        for interruption in ["cancel", "store", "provider", "requester"] {
            let mut fixture = McpFixture::new();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let blocker = fixture.store.lease().unwrap();
            let request = fixture.forget();
            fixture.app.update();
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(request)
                        .insert(OperationState::Canceled);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.source)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "requester" => {
                    fixture.app.world_mut().despawn(request);
                }
                _ => unreachable!(),
            }
            drop(blocker);
            fixture.finish(request);
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before,
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(request)
                    .is_none()
            );
        }
    }

    struct DrainWake(std::sync::mpsc::SyncSender<()>);

    impl std::task::Wake for DrainWake {
        fn wake(self: std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    #[derive(Resource)]
    struct ProbeDirectory(std::path::PathBuf);

    #[derive(Clone)]
    struct Probe {
        root: std::path::PathBuf,
        command: Execute,
    }

    impl vmux_ecs::Job for Probe {
        type Output = <Execute as vmux_ecs::Job>::Output;

        fn run(self) -> Self::Output {
            let invocation =
                serde_json::to_vec(&(self.command.program, self.command.arguments)).unwrap();
            std::fs::write(self.root.join("invocation.json"), invocation).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.root.join("hold").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "package probe was not released"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let failed = self.root.join("fail").exists();
            Ok(Some(crate::process::ProcessOutput(std::process::Output {
                status: std::process::ExitStatus::from_raw(if failed { 256 } else { 0 }),
                stdout: Vec::new(),
                stderr: if failed {
                    b"package manager refused the operation".to_vec()
                } else {
                    Vec::new()
                },
            })))
        }
    }

    struct ProbePlugin;

    impl Plugin for ProbePlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(vmux_ecs::JobPlugin::<Probe>::default())
                .add_systems(
                    Update,
                    capture
                        .after(ToolTaskSet::Start)
                        .before(vmux_ecs::JobSet::Submit),
                )
                .add_systems(Update, receive.before(ToolTaskSet::Start));
        }
    }

    fn capture(
        operations: Query<(Entity, &JobRequest<Execute>)>,
        root: Res<ProbeDirectory>,
        mut commands: Commands,
    ) {
        for (entity, command) in &operations {
            commands
                .entity(entity)
                .remove::<JobRequest<Execute>>()
                .insert(JobRequest(Probe {
                    root: root.0.clone(),
                    command: command.0.clone(),
                }));
        }
    }

    fn receive(mut results: Query<(Entity, &mut JobResult<Probe>)>, mut commands: Commands) {
        for (entity, mut result) in &mut results {
            let result = std::mem::replace(&mut result.0, Err("consumed".into()));
            commands
                .entity(entity)
                .remove::<JobResult<Probe>>()
                .insert(JobResult::<Execute>(result));
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        store: ToolStore,
        source: Entity,
        provider: Entity,
        id: ToolProvider,
    }

    impl Fixture {
        fn new(provider: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            let mut manifest = ToolsManifest::default();
            manifest.set_package(provider, "keep", true);
            manifest.set_package("unrelated", "other", true);
            manifest.write_to(&store.manifest_path()).unwrap();
            std::fs::write(directory.path().join("hold"), "").unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                crate::ToolRuntimePlugin,
                ProbePlugin,
            ))
            .insert_resource(ProbeDirectory(directory.path().into()));
            let source = app
                .world_mut()
                .spawn((
                    crate::tool_inventory::Registry::default(),
                    crate::catalog::Catalog::default(),
                    store.clone(),
                    manifest,
                    crate::tool_inventory::Sequence::default(),
                ))
                .id();
            let id = ToolProvider::new(provider);
            let provider = app
                .world_mut()
                .spawn((
                    ToolProviderId(id.clone()),
                    crate::observation::PackageProvider,
                ))
                .id();
            Self {
                app,
                directory,
                store,
                source,
                provider,
                id,
            }
        }

        fn notifications(&mut self) -> std::sync::mpsc::Receiver<()> {
            let (wake, events) = std::sync::mpsc::sync_channel(16);
            self.app
                .insert_resource(vmux_ecs::JobWake(std::task::Waker::from(
                    std::sync::Arc::new(DrainWake(wake)),
                )));
            events
        }

        fn request<R: Clone + Send + Sync + 'static>(&mut self, request: R) -> Entity
        where
            ToolOperationKey: From<R>,
        {
            self.app
                .world_mut()
                .spawn((
                    ToolOperationRequest(request),
                    ToolStoreTarget::new(self.source),
                    ToolProviderTarget::new(self.provider),
                ))
                .id()
        }

        fn drained(&mut self, requester: Entity) -> bool {
            let world = self.app.world_mut();
            let mut drain = bevy::ecs::system::SystemState::<ToolPackageDrain>::new(world);
            drain.get(world).unwrap().is_drained(requester)
        }

        fn wait_started(&mut self, requester: Entity) -> Entity {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.directory.path().join("invocation.json").exists() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "package process was not accepted"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            self.app.world().get::<Waiting>(requester).unwrap().work
        }

        fn drain(&mut self, work: Entity) {
            std::fs::remove_file(self.directory.path().join("hold")).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.world().get_entity(work).is_ok() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "package work did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn package_mutations_preserve_other_declarations_and_commit_before_success() {
        for provider in ["npm", "homebrew-formula", "homebrew-cask", "acp", "lsp"] {
            for operation in ["install", "update", "uninstall", "adopt", "forget"] {
                if matches!(provider, "acp" | "lsp") && !matches!(operation, "adopt" | "forget") {
                    continue;
                }
                let mut fixture = Fixture::new(provider);
                if matches!(operation, "uninstall" | "forget") {
                    let source = format!(
                        "version = 1\n[packages]\n{provider} = ['alpha', 'keep']\nunrelated = ['other']\n"
                    );
                    std::fs::write(fixture.store.manifest_path(), source).unwrap();
                    let manifest = ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.source)
                        .insert(manifest);
                }
                let requester = match operation {
                    "install" => fixture.request(ToolInstallRequest {
                        provider: fixture.id.clone(),
                        id: "alpha".into(),
                    }),
                    "update" => fixture.request(ToolUpdateRequest {
                        provider: fixture.id.clone(),
                        id: "alpha".into(),
                    }),
                    "uninstall" => fixture.request(ToolUninstallRequest {
                        provider: fixture.id.clone(),
                        id: "alpha".into(),
                    }),
                    "adopt" => fixture.request(ToolAdoptRequest {
                        provider: fixture.id.clone(),
                        id: "alpha".into(),
                        value: String::new(),
                    }),
                    "forget" => fixture.request(ToolForgetRequest {
                        provider: fixture.id.clone(),
                        id: "alpha".into(),
                    }),
                    _ => unreachable!(),
                };
                if matches!(operation, "install" | "update" | "uninstall") {
                    let work = fixture.wait_started(requester);
                    assert!(
                        fixture
                            .app
                            .world()
                            .get::<ToolOperationSucceeded>(requester)
                            .is_none()
                    );
                    assert_eq!(
                        ToolsManifest::read(&fixture.store.manifest_path())
                            .unwrap()
                            .contains(provider, "alpha"),
                        operation == "uninstall"
                    );
                    fixture.drain(work);
                } else {
                    fixture.app.update();
                    let work = fixture.app.world().get::<Waiting>(requester).unwrap().work;
                    fixture.drain(work);
                    assert!(!fixture.directory.path().join("invocation.json").exists());
                }
                let saved = ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
                assert_eq!(
                    saved.contains(provider, "alpha"),
                    !matches!(operation, "uninstall" | "forget"),
                    "{provider}/{operation}"
                );
                assert!(saved.contains(provider, "keep"));
                assert!(saved.contains("unrelated", "other"));
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<ToolOperationSucceeded>(requester)
                        .is_some(),
                    "{provider}/{operation}"
                );
                assert_eq!(
                    fixture.app.world().get::<ToolsManifest>(fixture.source),
                    Some(&saved)
                );
                assert!(fixture.store.lease().is_some());
            }
        }
    }

    #[test]
    fn canceled_queued_configuration_never_changes_the_manifest() {
        for adopt in [true, false] {
            let mut fixture = Fixture::new("acp");
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let lease = fixture.store.lease().unwrap();
            let notices = fixture.notifications();
            let requester = if adopt {
                fixture.request(ToolAdoptRequest {
                    provider: fixture.id.clone(),
                    id: "alpha".into(),
                    value: String::new(),
                })
            } else {
                fixture.request(ToolForgetRequest {
                    provider: fixture.id.clone(),
                    id: "keep".into(),
                })
            };
            fixture.app.update();
            assert!(!fixture.drained(requester));
            while notices.try_recv().is_ok() {}
            fixture
                .app
                .world_mut()
                .entity_mut(requester)
                .insert(OperationState::Canceled);
            fixture.app.update();
            assert!(fixture.drained(requester));
            assert!(
                notices.try_recv().is_ok(),
                "queued package retirement must wake its parent before another input"
            );
            drop(lease);
            for _ in 0..3 {
                fixture.app.update();
            }
            assert!(fixture.drained(requester));
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert!(!fixture.directory.path().join("invocation.json").exists());
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn canceled_and_replaced_package_work_retains_its_lease_until_native_drain() {
        for interruption in [
            "cancel",
            "provider",
            "provider-target",
            "store-target",
            "store",
            "requester",
            "input",
            "waiting",
        ] {
            let mut fixture = Fixture::new("npm");
            let requester = fixture.request(ToolInstallRequest {
                provider: fixture.id.clone(),
                id: "alpha".into(),
            });
            let work = fixture.wait_started(requester);
            assert!(!fixture.drained(requester));
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(requester)
                        .insert(OperationState::Canceled);
                }
                "provider" => {
                    fixture.app.world_mut().despawn(fixture.provider);
                }
                "provider-target" => {
                    let other = fixture
                        .app
                        .world_mut()
                        .spawn((
                            ToolProviderId(fixture.id.clone()),
                            crate::observation::PackageProvider,
                        ))
                        .id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(requester)
                        .insert(ToolProviderTarget::new(other));
                }
                "store-target" => {
                    let other = fixture.app.world_mut().spawn(fixture.store.clone()).id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(requester)
                        .insert(ToolStoreTarget::new(other));
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.source)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.directory.path(),
                        ));
                }
                "requester" => {
                    fixture.app.world_mut().despawn(requester);
                }
                "input" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(requester)
                        .insert(ToolOperationRequest(ToolInstallRequest {
                            provider: fixture.id.clone(),
                            id: "beta".into(),
                        }));
                }
                "waiting" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(requester)
                        .insert(Waiting {
                            work: Entity::PLACEHOLDER,
                        });
                }
                _ => unreachable!(),
            }
            for _ in 0..4 {
                fixture.app.update();
            }
            assert!(
                fixture.app.world().get_entity(work).is_ok(),
                "{interruption}"
            );
            assert!(
                fixture.store.lease().is_none(),
                "{interruption}: lease released before native IO drain"
            );
            assert!(!fixture.drained(requester), "{interruption}");
            fixture.drain(work);
            assert!(fixture.drained(requester), "{interruption}");
            assert!(
                !ToolsManifest::read(&fixture.store.manifest_path())
                    .unwrap()
                    .contains("npm", "alpha")
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(requester)
                    .is_none(),
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
            if interruption == "waiting" {
                assert_eq!(
                    fixture.app.world().get::<Waiting>(requester).unwrap().work,
                    Entity::PLACEHOLDER
                );
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<OperationRetained>(requester)
                        .is_some()
                );
            } else if fixture.app.world().get_entity(requester).is_ok() {
                assert!(fixture.app.world().get::<Waiting>(requester).is_none());
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<OperationRetained>(requester)
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn package_process_and_save_failures_preserve_committed_declarations() {
        for failure in ["process", "save"] {
            let mut fixture = Fixture::new("npm");
            let requester = fixture.request(ToolInstallRequest {
                provider: fixture.id.clone(),
                id: "alpha".into(),
            });
            let work = fixture.wait_started(requester);
            if failure == "process" {
                std::fs::write(fixture.directory.path().join("fail"), "").unwrap();
            } else {
                std::fs::remove_file(fixture.store.manifest_path()).unwrap();
                std::fs::create_dir(fixture.store.manifest_path()).unwrap();
            }
            fixture.drain(work);
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationFailed>(requester)
                    .is_some()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationSucceeded>(requester)
                    .is_none()
            );
            assert!(
                !fixture
                    .app
                    .world()
                    .get::<ToolsManifest>(fixture.source)
                    .unwrap()
                    .contains("npm", "alpha")
            );
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn protected_casks_and_option_like_package_names_never_start_native_mutation() {
        for (provider, name) in [("homebrew-cask", "vmux"), ("npm", "--prefix=/outside")] {
            let mut fixture = Fixture::new(provider);
            let requester = fixture.request(ToolUninstallRequest {
                provider: fixture.id.clone(),
                id: name.into(),
            });
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ToolOperationFailed>(requester)
                    .is_some()
            );
            assert!(!fixture.directory.path().join("invocation.json").exists());
            assert!(fixture.store.lease().is_some());
        }
    }
}
