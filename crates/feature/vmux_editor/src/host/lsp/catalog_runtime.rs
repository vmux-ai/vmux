use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde_json::Value;
use vmux_ecs::profile::{ActiveProfile, ProfileDirectories, ProfilePaths};
use vmux_ecs::{
    JobPlugin, JobRequest, JobResult, Operation, OperationRetained, OperationState, OperationTarget,
};
use vmux_tool::io::{Authorization, Get, ReadFile, WriteFile};
use vmux_tool::observation::ObservationSet;

use super::catalog::{CatalogReady, Package};
use super::catalog_io::{DecodeArchive, RegistryBytes};
use super::package_path::Sha256Digest;

pub(super) struct CatalogRuntimePlugin;

impl Plugin for CatalogRuntimePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::io::ObservationIoPlugin>() {
            app.add_plugins(vmux_tool::io::ObservationIoPlugin);
        }
        app.add_plugins(JobPlugin::<DecodeArchive>::default())
            .add_message::<RefreshRequest>()
            .add_systems(Startup, spawn)
            .add_systems(PostUpdate, (wake, restart.after(vmux_ecs::JobSet::Cleanup)))
            .add_systems(
                Update,
                (
                    request, retire, start, cached, release, archive, decoded, written, publish,
                )
                    .chain()
                    .after(ObservationSet)
                    .before(vmux_ecs::JobSet::Submit),
            );
    }
}

#[derive(Message)]
pub(super) struct RefreshRequest;

#[derive(Component)]
pub(super) struct Catalog {
    pub(super) revision: u64,
    pub(super) loading: bool,
    pub(super) ready: bool,
    requested: bool,
    refresh: bool,
}

impl Catalog {
    pub(super) fn new() -> Self {
        Self {
            revision: 0,
            loading: false,
            ready: false,
            requested: true,
            refresh: false,
        }
    }
}

#[derive(Component)]
pub(super) struct Source {
    pub(super) cache: PathBuf,
    pub(super) profile: Option<Entity>,
    authorization: Option<Authorization>,
}

impl Source {
    pub(super) fn new(
        cache: PathBuf,
        profile: Option<Entity>,
        authorization: Option<Authorization>,
    ) -> Self {
        Self {
            cache,
            profile,
            authorization,
        }
    }
}

#[derive(Component)]
struct Restart;
#[derive(Component)]
struct Reading;
#[derive(Component)]
struct Release;
#[derive(Component)]
struct Archive {
    sha256: Sha256Digest,
}
#[derive(Component)]
struct Decoding;
#[derive(Component)]
struct Writing;
#[derive(Component)]
struct Received(Option<Vec<u8>>);

fn spawn(existing: Query<Entity, With<Catalog>>, mut commands: Commands) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("LSP catalog"),
        Catalog::new(),
        Operation::bundle("lsp.catalog").unwrap(),
        OperationRetained,
    ));
}

fn request(mut requests: MessageReader<RefreshRequest>, mut catalogs: Query<&mut Catalog>) {
    if requests.read().next().is_none() {
        return;
    }
    for mut catalog in &mut catalogs {
        if !catalog.loading {
            catalog.requested = true;
            catalog.refresh = true;
        }
    }
}

fn restart(requests: Query<Entity, With<Restart>>, mut commands: Commands) {
    for entity in &requests {
        commands.entity(entity).remove::<Restart>();
    }
}

fn wake(changes: Query<Entity, Changed<Catalog>>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if !changes.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

fn retire(
    mut catalogs: Query<(Entity, &mut Catalog, &Source, &mut OperationState)>,
    profiles: Query<(Entity, &ProfileDirectories), With<ActiveProfile>>,
    native: Query<&OperationTarget>,
    mut commands: Commands,
) {
    let profile = profiles.single().ok();
    let cache = profile
        .map(|(_, paths)| paths.0.lsp())
        .unwrap_or_else(|| ProfilePaths::current().lsp())
        .join("registries/registry.json");
    for (entity, mut catalog, source, mut state) in &mut catalogs {
        if source.cache == cache && source.profile == profile.map(|(entity, _)| entity) {
            continue;
        }
        *state = OperationState::Canceled;
        if native.iter().any(|target| target.entity() == entity) {
            continue;
        }
        catalog.loading = false;
        catalog.ready = false;
        catalog.requested = true;
        commands
            .entity(entity)
            .remove::<(
                Source,
                Reading,
                Release,
                Archive,
                Decoding,
                Writing,
                Received,
            )>()
            .insert(Restart);
    }
}

type PendingCatalogs<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Catalog),
    (
        Without<Restart>,
        Without<Reading>,
        Without<Release>,
        Without<Archive>,
        Without<Decoding>,
        Without<Writing>,
        Without<Received>,
    ),
>;

fn start(
    mut catalogs: PendingCatalogs,
    profiles: Query<(Entity, &ProfileDirectories), With<ActiveProfile>>,
    mut commands: Commands,
) {
    let profile = profiles.single().ok();
    let cache = profile
        .map(|(_, paths)| paths.0.lsp())
        .unwrap_or_else(|| ProfilePaths::current().lsp())
        .join("registries/registry.json");
    for (entity, mut catalog) in &mut catalogs {
        if !catalog.requested || catalog.loading {
            continue;
        }
        catalog.loading = true;
        catalog.requested = false;
        let authorization =
            Authorization::from_environment("GITHUB_TOKEN", "https://api.github.com")
                .expect("GitHub API origin is valid");
        commands.entity(entity).insert((
            OperationState::Running,
            Source::new(
                cache.clone(),
                profile.map(|(entity, _)| entity),
                authorization.clone(),
            ),
        ));
        if catalog.refresh {
            commands.entity(entity).insert((
                Release,
                JobRequest(Get {
                    url: "https://api.github.com/repos/mason-org/mason-registry/releases/latest"
                        .into(),
                    limit: 4 * 1024 * 1024,
                    authorization: authorization.clone(),
                }),
            ));
        } else {
            commands.entity(entity).insert((
                Reading,
                JobRequest(ReadFile {
                    path: cache.clone(),
                    limit: 32 * 1024 * 1024,
                }),
            ));
        }
    }
}

fn cached(
    requests: Query<(Entity, &Source, &OperationState, &JobResult<ReadFile>), With<Reading>>,
    mut commands: Commands,
) {
    for (entity, source, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Reading, JobResult<ReadFile>)>();
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_ref(),
            _ => None,
        };
        if let Some(bytes) = bytes
            && std::str::from_utf8(bytes)
                .ok()
                .and_then(|text| Package::parse_catalog(text).ok())
                .is_some()
        {
            commands
                .entity(entity)
                .insert(Received(Some(bytes.clone())));
        } else {
            commands.entity(entity).insert((
                Release,
                JobRequest(Get {
                    url: "https://api.github.com/repos/mason-org/mason-registry/releases/latest"
                        .into(),
                    limit: 4 * 1024 * 1024,
                    authorization: source.authorization.clone(),
                }),
            ));
        }
    }
}

fn release(
    requests: Query<(Entity, &OperationState, &JobResult<Get>), With<Release>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Release, JobResult<Get>)>();
        let document = match &result.0 {
            Ok(Ok(bytes)) => match &bytes.0 {
                Some(bytes) => serde_json::from_slice::<Value>(bytes).ok(),
                None => None,
            },
            _ => None,
        };
        let mut parsed = None;
        if let Some(assets) = document
            .as_ref()
            .and_then(|document| document.get("assets"))
            .and_then(Value::as_array)
        {
            for asset in assets {
                if asset.get("name").and_then(Value::as_str) != Some("registry.json.zip") {
                    continue;
                }
                let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else {
                    break;
                };
                let Some(size) = asset.get("size").and_then(Value::as_u64) else {
                    break;
                };
                let Some(digest) = asset.get("digest").and_then(Value::as_str) else {
                    break;
                };
                if size > 32 * 1024 * 1024 {
                    break;
                }
                let Ok(download) = url::Url::parse(url) else {
                    break;
                };
                if download.scheme() != "https" {
                    break;
                }
                let Ok(sha256) = Sha256Digest::parse(digest) else {
                    break;
                };
                parsed = Some((url.to_string(), sha256));
                break;
            }
        }
        if let Some((url, sha256)) = parsed {
            commands.entity(entity).insert((
                Archive { sha256 },
                JobRequest(Get {
                    url,
                    limit: 32 * 1024 * 1024,
                    authorization: None,
                }),
            ));
        } else {
            commands.entity(entity).insert(Received(None));
        }
    }
}

fn archive(
    requests: Query<(Entity, &Archive, &OperationState, &JobResult<Get>), Without<Release>>,
    mut commands: Commands,
) {
    for (entity, archive, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Archive, JobResult<Get>)>();
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_ref(),
            _ => None,
        };
        if let Some(bytes) = bytes {
            commands.entity(entity).insert((
                Decoding,
                JobRequest(DecodeArchive {
                    bytes: bytes.clone(),
                    sha256: archive.sha256.clone(),
                }),
            ));
        } else {
            commands.entity(entity).insert(Received(None));
        }
    }
}

fn decoded(
    requests: Query<(Entity, &Source, &OperationState, &JobResult<DecodeArchive>), With<Decoding>>,
    mut commands: Commands,
) {
    for (entity, source, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Decoding, JobResult<DecodeArchive>)>();
        let bytes = match &result.0 {
            Ok(Ok(RegistryBytes(bytes))) => Some(bytes),
            _ => None,
        };
        if let Some(bytes) = bytes
            && std::str::from_utf8(bytes)
                .ok()
                .and_then(|text| Package::parse_catalog(text).ok())
                .is_some()
        {
            commands.entity(entity).insert((
                Received(Some(bytes.clone())),
                Writing,
                JobRequest(WriteFile {
                    path: source.cache.clone(),
                    bytes: bytes.clone(),
                }),
            ));
        } else {
            commands.entity(entity).insert(Received(None));
        }
    }
}

fn written(
    requests: Query<(Entity, &OperationState, &JobResult<WriteFile>), With<Writing>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        if !matches!(result.0, Ok(Ok(()))) {
            bevy::log::warn!("LSP registry cache write failed");
        }
        commands
            .entity(entity)
            .remove::<(Writing, JobResult<WriteFile>)>();
    }
}

type PublishingCatalogs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Catalog,
        &'static Received,
        &'static OperationState,
    ),
    (Without<Writing>, Without<Reading>),
>;

fn publish(
    mut catalogs: PublishingCatalogs,
    packages: Query<(Entity, &Package, &ChildOf)>,
    mut commands: Commands,
) {
    for (entity, mut catalog, received, state) in &mut catalogs {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(bytes) = &received.0 {
            let Ok(loaded) = Package::parse_catalog(std::str::from_utf8(bytes).unwrap()) else {
                continue;
            };
            let mut existing = HashMap::new();
            for (package, value, parent) in &packages {
                if parent.parent() == entity {
                    existing.insert(value.name.clone(), package);
                }
            }
            for package in loaded {
                let name = Name::new(package.name.as_str().to_string());
                if let Some(entity) = existing.remove(&package.name) {
                    commands.entity(entity).insert((name, package));
                } else {
                    commands.spawn((name, package, ChildOf(entity)));
                }
            }
            for (_, old) in existing {
                commands.entity(old).despawn();
            }
        }
        catalog.revision = catalog.revision.wrapping_add(1);
        catalog.ready = true;
        catalog.loading = false;
        catalog.refresh = false;
        commands
            .entity(entity)
            .remove::<Received>()
            .insert((CatalogReady, OperationState::Succeeded));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Add, App, Commands, Entity, Name, On, Plugin, With};
    use std::time::{Duration, Instant};
    use vmux_ecs::profile::Profile;

    struct OfflineFixturePlugin;

    impl Plugin for OfflineFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(refuse_network);
        }
    }

    fn refuse_network(trigger: On<Add, JobRequest<Get>>, mut commands: Commands) {
        commands
            .entity(trigger.event_target())
            .remove::<JobRequest<Get>>()
            .insert(JobResult::<Get>(
                Err("fixture transport unavailable".into()),
            ));
    }

    struct Fixture {
        app: App,
        _directory: tempfile::TempDir,
        cache: PathBuf,
        bytes: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let paths = ProfilePaths::at(Profile::named("test"), directory.path().join("data"));
            let cache = paths.lsp().join("registries/registry.json");
            let bytes = br#"[{"name":"server","description":"Cached server","source":{"id":"pkg:npm/server@1.2.3"}}]"#.to_vec();
            std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
            std::fs::write(&cache, &bytes).unwrap();
            let mut app = App::new();
            app.add_plugins((CatalogRuntimePlugin, OfflineFixturePlugin));
            app.world_mut().spawn((
                Name::new("Test"),
                ActiveProfile(Profile::named("test")),
                ProfileDirectories(paths),
            ));
            Self {
                app,
                _directory: directory,
                cache,
                bytes,
            }
        }

        fn ready(&mut self) -> Entity {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                if let Some((entity, _)) = world
                    .query::<(Entity, &Catalog)>()
                    .iter(world)
                    .find(|(_, catalog)| catalog.ready && !catalog.loading)
                {
                    return entity;
                }
                assert!(Instant::now() < deadline, "cached catalog did not publish");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn an_offline_cache_publishes_canonical_package_entities() {
        let mut fixture = Fixture::new();
        let catalog = fixture.ready();
        let world = fixture.app.world_mut();
        let packages = world
            .query::<(&Package, &ChildOf)>()
            .iter(world)
            .collect::<Vec<_>>();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].0.name.as_str(), "server");
        assert_eq!(packages[0].0.source_id, "pkg:npm/server@1.2.3");
        assert_eq!(packages[0].1.parent(), catalog);
        assert_eq!(std::fs::read(&fixture.cache).unwrap(), fixture.bytes);
    }

    #[test]
    fn a_failed_refresh_preserves_the_cache_and_published_package_identity() {
        let mut fixture = Fixture::new();
        let catalog = fixture.ready();
        let world = fixture.app.world_mut();
        let before = world
            .query_filtered::<Entity, With<Package>>()
            .iter(world)
            .collect::<Vec<_>>();
        let revision = world.get::<Catalog>(catalog).unwrap().revision;
        fixture.app.world_mut().write_message(RefreshRequest);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            let value = fixture.app.world().get::<Catalog>(catalog).unwrap();
            if !value.loading && value.revision > revision {
                break;
            }
            assert!(Instant::now() < deadline, "failed refresh did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        let world = fixture.app.world_mut();
        let after = world
            .query_filtered::<Entity, With<Package>>()
            .iter(world)
            .collect::<Vec<_>>();
        assert_eq!(after, before);
        assert_eq!(std::fs::read(&fixture.cache).unwrap(), fixture.bytes);
        assert!(world.get::<Catalog>(catalog).unwrap().ready);
    }
}
