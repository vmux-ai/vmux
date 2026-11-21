use std::collections::BTreeSet;
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::profile::{CurrentProfile, ProfilePaths};
use vmux_ecs::{EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState};
use vmux_tool::ToolProviderId;
use vmux_tool::io::{FindExecutable, ReadDirectory, ReadFile};
use vmux_tool::observation::{
    Accepted, Finished, Observation, ObservationSet, ObserveProviderRequest, ObservedPackage,
    ObservedTool,
};
use vmux_tool::state::ToolStatus;

use super::catalog::Package;
use super::catalog_runtime::{Catalog, RefreshRequest};
use super::package_path::PackageName;
use super::purl::Purl;
use super::store::Receipt;

pub(super) struct LspInventoryPlugin;

impl Plugin for LspInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(observe).add_systems(
            Update,
            (
                directories,
                receipts,
                candidates,
                executables,
                publish_receipts,
                publish,
            )
                .chain()
                .in_set(ObservationSet),
        );
    }
}

#[derive(Component)]
struct Listing;
#[derive(Component)]
struct Reading;
#[derive(Component)]
struct ReadDone;
#[derive(Component)]
struct Expected(PackageName);
#[derive(Component)]
struct WaitingCatalog(u64);
#[derive(Component)]
struct Paths(PathBuf);
#[derive(Component)]
struct Candidates;
#[derive(Component)]
struct Candidate;
#[derive(Component)]
struct Lookup;
#[derive(Component)]
struct Toolchain;
#[derive(Component)]
struct ExecutableFound(bool);

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<&ToolProviderId>,
    observations: Query<&OperationState, With<Observation>>,
    catalogs: Query<&Catalog>,
    profile: CurrentProfile,
    mut refresh: MessageWriter<RefreshRequest>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !providers
        .get(request.entity)
        .is_ok_and(|provider| provider.0.id() == "lsp")
        || observations.get(request.observation).ok() != Some(&OperationState::Running)
    {
        return;
    }
    let Ok(catalog) = catalogs.single() else {
        commands.entity(request.observation).insert((
            Accepted,
            Finished {
                error: Some("LSP catalog is unavailable".into()),
            },
        ));
        return;
    };
    let path = profile
        .paths()
        .map(|paths| paths.lsp())
        .unwrap_or_else(|| ProfilePaths::current().lsp());
    let mut observation = commands.entity(request.observation);
    observation.insert((
        Accepted,
        Listing,
        Paths(path.clone()),
        JobRequest(ReadDirectory {
            path: path.join("packages"),
        }),
    ));
    if request.refresh_catalogs {
        observation.insert(WaitingCatalog(catalog.revision));
        refresh.write(RefreshRequest);
    }
}

fn directories(
    requests: Query<(Entity, &OperationState, &JobResult<ReadDirectory>), With<Listing>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Listing, JobResult<ReadDirectory>)>()
            .insert(Reading);
        let entries = match &result.0 {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Finished {
                    error: Some(error.clone()),
                });
                continue;
            }
        };
        for entry in &entries.entries {
            if !entry.file_type.is_dir() {
                continue;
            }
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            let Ok(name) = PackageName::parse(name) else {
                continue;
            };
            commands.spawn((
                Name::new("LSP receipt"),
                Expected(name),
                ChildOf(entity),
                EntityTarget::<Observation>::new(entity),
                OperationRetained,
                Operation::bundle("lsp.receipt").unwrap().running(),
                JobRequest(ReadFile {
                    path: entry.path.join("vmux-receipt.json"),
                    limit: 1024 * 1024,
                }),
            ));
        }
    }
}

fn receipts(
    requests: Query<(Entity, &Expected, &OperationState, &JobResult<ReadFile>), Without<ReadDone>>,
    mut commands: Commands,
) {
    for (entity, expected, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<JobResult<ReadFile>>()
            .insert((ReadDone, OperationState::Succeeded));
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_ref(),
            _ => None,
        };
        if let Some(bytes) = bytes
            && let Ok(receipt) = serde_json::from_slice::<Receipt>(bytes)
            && receipt.name == expected.0
        {
            commands.entity(entity).insert(receipt);
        }
    }
}

type PendingCandidates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationState,
        &'static Paths,
        Option<&'static WaitingCatalog>,
    ),
    (With<Reading>, Without<Candidates>, Without<Finished>),
>;

fn candidates(
    observations: PendingCandidates,
    pending: Query<(&ChildOf, Has<ReadDone>), With<Expected>>,
    receipts: Query<(&Receipt, &ChildOf)>,
    catalogs: Query<&Catalog>,
    packages: Query<(Entity, &Package)>,
    mut commands: Commands,
) {
    let Ok(catalog) = catalogs.single() else {
        return;
    };
    for (entity, state, paths, waiting) in &observations {
        if *state != OperationState::Running
            || !catalog.ready
            || waiting.is_some_and(|waiting| catalog.revision <= waiting.0)
            || pending
                .iter()
                .any(|(parent, done)| parent.parent() == entity && !done)
        {
            continue;
        }
        let mut installed = BTreeSet::new();
        for (receipt, parent) in &receipts {
            if parent.parent() == entity {
                installed.insert(receipt.name.clone());
            }
        }
        for (package_entity, package) in &packages {
            if installed.contains(&package.name) {
                continue;
            }
            let candidate = commands
                .spawn((
                    Name::new(package.name.as_str().to_string()),
                    Candidate,
                    ChildOf(entity),
                    EntityTarget::<Package>::new(package_entity),
                ))
                .id();
            for command in package.bin.keys() {
                commands.spawn((
                    Name::new("LSP executable lookup"),
                    Lookup,
                    ChildOf(candidate),
                    EntityTarget::<Observation>::new(entity),
                    OperationRetained,
                    Operation::bundle("lsp.executable").unwrap().running(),
                    JobRequest(FindExecutable {
                        command: command.as_str().to_string(),
                        explicit: Some(paths.0.join("bin").join(command.as_str())),
                    }),
                ));
            }
            if let Some(source) = Purl::parse(&package.source_id)
                && let Some(command) = source.toolchain()
            {
                commands.spawn((
                    Name::new("LSP toolchain lookup"),
                    Lookup,
                    Toolchain,
                    ChildOf(candidate),
                    EntityTarget::<Observation>::new(entity),
                    OperationRetained,
                    Operation::bundle("lsp.toolchain").unwrap().running(),
                    JobRequest(FindExecutable {
                        command: command.to_string(),
                        explicit: None,
                    }),
                ));
            }
        }
        commands.entity(entity).insert(Candidates);
    }
}

type ExecutableResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationState,
        &'static JobResult<FindExecutable>,
        Has<Toolchain>,
    ),
    With<Lookup>,
>;

fn executables(requests: ExecutableResults, mut commands: Commands) {
    for (entity, state, result, toolchain) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let found = match &result.0 {
            Ok(Ok(facts)) => {
                facts.path.is_some()
                    && (toolchain
                        || !facts
                            .explicit
                            .is_some_and(|kind| kind.is_file() || kind.is_symlink()))
            }
            _ => false,
        };
        commands
            .entity(entity)
            .remove::<JobResult<FindExecutable>>()
            .insert((ExecutableFound(found), OperationState::Succeeded));
    }
}

#[derive(Component)]
struct ReceiptsPublished;

type PendingReceipts<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static OperationState),
    (
        With<Candidates>,
        Without<ReceiptsPublished>,
        Without<Finished>,
    ),
>;

fn publish_receipts(
    observations: PendingReceipts,
    receipts: Query<(&Receipt, &ChildOf)>,
    packages: Query<&Package>,
    mut commands: Commands,
) {
    for (entity, state) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        for (receipt, parent) in &receipts {
            if parent.parent() != entity {
                continue;
            }
            let package = packages.iter().find(|package| package.name == receipt.name);
            let latest = package
                .and_then(|package| Purl::parse(&package.source_id))
                .and_then(|source| source.version);
            let status =
                if receipt.version.is_some() && latest.is_some() && receipt.version != latest {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                };
            let detail = package
                .map(|package| package.description.clone())
                .filter(|description| !description.is_empty())
                .unwrap_or_else(|| "Vmux-managed language tool".into());
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        receipt.name.as_str().to_string(),
                        receipt.name.as_str().to_string(),
                        receipt.version.clone(),
                        None,
                        detail,
                        status,
                        Vec::new(),
                    ),
                    true,
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(ReceiptsPublished);
    }
}

type PublishingObservations<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static OperationState),
    (With<Candidates>, With<ReceiptsPublished>, Without<Finished>),
>;
type PublishingCandidates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        Option<&'static EntityTarget<Package>>,
    ),
    With<Candidate>,
>;
type PublishingLookups<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        Option<&'static ExecutableFound>,
        Has<Toolchain>,
    ),
    With<Lookup>,
>;

fn publish(
    observations: PublishingObservations,
    candidates: PublishingCandidates,
    lookups: PublishingLookups,
    packages: Query<&Package>,
    mut commands: Commands,
) {
    for (entity, state) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        if candidates.iter().any(|(candidate, parent, _)| {
            parent.parent() == entity
                && lookups
                    .iter()
                    .any(|(owner, found, _)| owner.parent() == candidate && found.is_none())
        }) {
            continue;
        }
        for (candidate, parent, target) in &candidates {
            if parent.parent() != entity {
                continue;
            }
            let Some(package) = target.and_then(|target| packages.get(target.entity()).ok()) else {
                continue;
            };
            let source = Purl::parse(&package.source_id);
            let mut on_path = false;
            let mut toolchain = false;
            for (owner, found, is_toolchain) in &lookups {
                if owner.parent() == candidate && found.is_some_and(|found| found.0) {
                    if is_toolchain {
                        toolchain = true;
                    } else {
                        on_path = true;
                    }
                }
            }
            let installable = source
                .as_ref()
                .is_some_and(|source| source.kind == "github")
                || toolchain;
            if !on_path && !installable {
                continue;
            }
            let detail = if on_path {
                "Available on PATH".into()
            } else {
                package.description.clone()
            };
            let status = if on_path {
                ToolStatus::Installed
            } else {
                ToolStatus::Available
            };
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        package.name.as_str().to_string(),
                        package.name.as_str().to_string(),
                        source.and_then(|source| source.version),
                        None,
                        detail,
                        status,
                        Vec::new(),
                    ),
                    false,
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(Finished { error: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Add, App, Commands, Entity, Name, On, Plugin, Query, Res, Resource, With};
    use vmux_tool::io::{DirectoryEntries, DirectoryEntry, ExecutableFacts, FileBytes};

    #[derive(Resource)]
    struct NativeInputs {
        directory: tempfile::TempDir,
        npm: bool,
        catalog_version: u8,
    }

    struct NativeInputsPlugin;

    impl Plugin for NativeInputsPlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(files)
                .add_observer(directories)
                .add_observer(paths);
        }
    }

    fn files(
        trigger: On<Add, JobRequest<ReadFile>>,
        requests: Query<(Entity, &JobRequest<ReadFile>)>,
        inputs: Res<NativeInputs>,
        mut commands: Commands,
    ) {
        for (entity, request) in &requests {
            if entity != trigger.event_target() {
                continue;
            }
            let bytes = if request
                .0
                .path
                .file_name()
                .is_some_and(|name| name == "registry.json")
            {
                format!(r#"[
{{"name":"github-tool","source":{{"id":"pkg:github/acme/github-tool@1"}},"bin":{{"github-tool":"bin"}}}},
{{"name":"npm-tool","source":{{"id":"pkg:npm/npm-tool@1"}},"bin":{{"npm-tool":"bin"}}}},
{{"name":"unknown-tool","source":{{"id":"pkg:unsupported/unknown-tool@1"}},"bin":{{"unknown-tool":"bin"}}}},
{{"name":"installed-tool","source":{{"id":"pkg:github/acme/installed-tool@{}"}},"bin":{{"installed-tool":"bin"}}}}
]"#, inputs.catalog_version).into_bytes()
            } else {
                assert!(
                    request
                        .0
                        .path
                        .file_name()
                        .is_some_and(|name| name == "vmux-receipt.json")
                );
                br#"{"name":"installed-tool","version":"1","source_id":"pkg:github/acme/installed-tool@1","bin":{}}"#.to_vec()
            };
            commands
                .entity(entity)
                .remove::<JobRequest<ReadFile>>()
                .insert(JobResult::<ReadFile>(Ok(Ok(FileBytes(Some(bytes))))));
        }
    }

    fn directories(
        trigger: On<Add, JobRequest<ReadDirectory>>,
        requests: Query<Entity, With<JobRequest<ReadDirectory>>>,
        inputs: Res<NativeInputs>,
        mut commands: Commands,
    ) {
        for entity in &requests {
            if entity != trigger.event_target() {
                continue;
            }
            let entry = DirectoryEntry {
                name: "installed-tool".into(),
                path: inputs.directory.path().join("installed-tool"),
                file_type: inputs.directory.path().metadata().unwrap().file_type(),
            };
            commands
                .entity(entity)
                .remove::<JobRequest<ReadDirectory>>()
                .insert(JobResult::<ReadDirectory>(Ok(Ok(DirectoryEntries {
                    exists: true,
                    entries: vec![entry],
                }))));
        }
    }

    fn paths(
        trigger: On<Add, JobRequest<FindExecutable>>,
        requests: Query<(Entity, &JobRequest<FindExecutable>)>,
        inputs: Res<NativeInputs>,
        mut commands: Commands,
    ) {
        for (entity, request) in &requests {
            if entity != trigger.event_target() {
                continue;
            }
            let path = if request.0.command == "npm" && inputs.npm {
                Some(inputs.directory.path().join("npm"))
            } else {
                None
            };
            commands
                .entity(entity)
                .remove::<JobRequest<FindExecutable>>()
                .insert(JobResult::<FindExecutable>(Ok(Ok(ExecutableFacts {
                    path,
                    explicit: None,
                }))));
        }
    }

    struct Fixture {
        app: App,
        provider: Entity,
        profile: Entity,
    }

    impl Fixture {
        fn new(npm: bool) -> Self {
            let mut app = App::new();
            app.add_plugins((
                super::super::catalog::CatalogPlugin,
                LspInventoryPlugin,
                NativeInputsPlugin,
            ))
            .insert_resource(NativeInputs {
                directory: tempfile::tempdir().unwrap(),
                npm,
                catalog_version: 2,
            });
            let paths = ProfilePaths::current();
            let profile = app
                .world_mut()
                .spawn((
                    Name::new("Active fixture"),
                    vmux_ecs::profile::ActiveProfile(vmux_ecs::profile::Profile::current()),
                    vmux_ecs::profile::ProfileDirectories(paths.clone()),
                ))
                .id();
            app.world_mut()
                .spawn(vmux_ecs::profile::ProfileDirectories(paths));
            let provider = app
                .world_mut()
                .spawn(ToolProviderId(vmux_tool::state::ToolProvider::new("lsp")))
                .id();
            Self {
                app,
                provider,
                profile,
            }
        }

        fn observe(&mut self) -> Entity {
            let entity = self
                .app
                .world_mut()
                .spawn((
                    Observation,
                    Operation::bundle("test.lsp.observe").unwrap().running(),
                    OperationRetained,
                ))
                .id();
            self.app.world_mut().trigger(ObserveProviderRequest {
                entity: self.provider,
                observation: entity,
                refresh_catalogs: false,
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while self.app.world().get::<Finished>(entity).is_none() {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "LSP observation did not finish"
                );
            }
            entity
        }

        fn items(
            &mut self,
            observation: Entity,
        ) -> std::collections::BTreeMap<String, (ToolStatus, Option<String>)> {
            let mut items = std::collections::BTreeMap::new();
            for (parent, id, status, version) in self
                .app
                .world_mut()
                .query::<(
                    &ChildOf,
                    &vmux_tool::observation::Id,
                    &vmux_tool::observation::Status,
                    &vmux_tool::observation::Version,
                )>()
                .iter(self.app.world())
            {
                if parent.parent() == observation {
                    items.insert(id.0.clone(), (status.0, version.0.clone()));
                }
            }
            items
        }
    }

    #[test]
    fn canonical_lsp_inventory_joins_receipts_versions_and_native_toolchain_facts() {
        for npm in [false, true] {
            let mut fixture = Fixture::new(npm);
            fixture.app.update();
            let observation = fixture.observe();
            let items = fixture.items(observation);
            assert_eq!(items["github-tool"].0, ToolStatus::Available);
            assert_eq!(items.contains_key("npm-tool"), npm);
            assert!(!items.contains_key("unknown-tool"));
            assert_eq!(
                items["installed-tool"],
                (ToolStatus::Outdated, Some("1".into()))
            );
        }
    }

    #[test]
    fn inactive_profile_directories_cannot_hide_active_catalog_replacement() {
        let mut fixture = Fixture::new(true);
        fixture.app.update();
        let observation = fixture.observe();
        assert_eq!(
            fixture.items(observation)["installed-tool"].0,
            ToolStatus::Outdated
        );
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeInputs>()
            .catalog_version = 1;
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.profile)
            .remove::<vmux_ecs::profile::ActiveProfile>();
        fixture.app.world_mut().spawn((
            Name::new("Replacement fixture"),
            vmux_ecs::profile::ActiveProfile(vmux_ecs::profile::Profile::current()),
            vmux_ecs::profile::ProfileDirectories(ProfilePaths::current()),
        ));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world_mut()
                .query::<&Package>()
                .iter(fixture.app.world())
                .any(|package| {
                    package.name.as_str() == "installed-tool" && package.source_id.ends_with("@1")
                })
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "replacement profile did not reload canonical catalog"
            );
        }
        let observation = fixture.observe();
        assert_eq!(
            fixture.items(observation)["installed-tool"].0,
            ToolStatus::Installed
        );
    }
}
