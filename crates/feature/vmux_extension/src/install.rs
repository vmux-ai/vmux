use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::Waker;

use base64::Engine;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, TaskPool};
use crossbeam_channel::{Receiver, Sender};
use vmux_api::extension::{ExtInstallPhase, ExtInstallProgress};
use vmux_ecs::{
    Job, JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation, OperationCompleted,
    OperationError, OperationState, OperationTarget,
};

#[cfg(test)]
use crate::crx::ChromeExtensionId;
use crate::crx::CrxArchive;
use crate::filesystem::{MaterializeCompleted, MaterializeRequest};
use crate::host::Binding;
use crate::webstore::ChromeWebStore;
use crate::{
    Catalog, ExtensionSet, InstallOwner, InstallRequest, Mutation, Request,
    catalog::ExtensionCatalog, download, driver, store,
};

const DEFAULT_PRODVERSION: &str = "120.0.0.0";

pub(crate) struct InstallPlugin;

impl Plugin for InstallPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<DownloadJob>::default(),
            JobPlugin::<UnpackJob>::default(),
        ))
        .add_message::<ExtensionInstallRequest>()
        .add_message::<ExtensionInstallCompleted>()
        .add_systems(
            Update,
            (start, download, unpack, materialize, finish)
                .chain()
                .after(ExtensionSet::Step)
                .after(JobSet::Collect),
        );
    }
}

#[derive(Message, Component, Clone, Debug)]
pub struct ExtensionInstallRequest {
    pub source: String,
    pub requester: Option<Entity>,
}

#[derive(Message, Clone, Debug)]
pub struct ExtensionInstallCompleted {
    pub requester: Entity,
    pub id: String,
    pub success: bool,
}

#[derive(Component)]
struct ExtensionInstallOperation;

#[derive(Component)]
struct Resolved(ResolvedInstall);

#[derive(Component)]
struct DownloadProgressChannel(Receiver<DownloadProgress>);

#[derive(Clone, Copy)]
struct DownloadProgress {
    received: u64,
    total: Option<u64>,
}

#[derive(Component)]
struct Materializing;

#[derive(Component, Clone)]
struct Staging(Arc<StagingDirectory>);

struct StagingDirectory(PathBuf);

impl Staging {
    fn new(path: PathBuf) -> Self {
        IoTaskPool::get_or_init(TaskPool::new);
        Self(Arc::new(StagingDirectory(path)))
    }

    fn path(&self) -> &Path {
        &self.0.0
    }

    fn lifetime(&self) -> Arc<dyn Send + Sync> {
        self.0.clone()
    }

    #[cfg(test)]
    fn await_removal(path: &Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while path.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "staging cleanup did not complete"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let path = self.0.clone();
        IoTaskPool::get()
            .spawn(async move {
                if let Err(error) = std::fs::remove_dir_all(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    warn!(path = %path.display(), %error, "failed to remove extension staging");
                }
            })
            .detach();
    }
}

#[derive(Clone)]
struct ResolvedInstall {
    id: String,
    profile: String,
    staging: Staging,
    crx_path: PathBuf,
    download_url: String,
}

#[derive(Clone)]
struct DownloadedInstall {
    id: String,
    profile: String,
    staging: Staging,
    crx_path: PathBuf,
}

struct PackageInstaller {
    id: String,
    profile: String,
    staging: Staging,
}

struct InstallOutput {
    entry: store::ExtEntry,
    source: PathBuf,
    staging: Staging,
}

#[derive(Clone)]
struct DownloadJob {
    install: ResolvedInstall,
    progress: Sender<DownloadProgress>,
    wake: Option<Waker>,
}

impl Job for DownloadJob {
    type Output = Result<DownloadedInstall, String>;

    fn run(self) -> Self::Output {
        self.install.download(|received, total| {
            if self
                .progress
                .try_send(DownloadProgress { received, total })
                .is_ok()
                && let Some(wake) = &self.wake
            {
                wake.wake_by_ref();
            }
        })
    }

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }
}

#[derive(Clone)]
struct UnpackJob(DownloadedInstall);

impl Job for UnpackJob {
    type Output = Result<InstallOutput, String>;

    fn run(self) -> Self::Output {
        let bytes = std::fs::read(&self.0.crx_path).map_err(|error| error.to_string())?;
        PackageInstaller {
            id: self.0.id,
            profile: self.0.profile,
            staging: self.0.staging,
        }
        .unpack(bytes)
    }
}

impl ResolvedInstall {
    fn resolve(
        source: &str,
        profile: String,
        store: store::ExtensionStore,
        operation: Entity,
    ) -> Result<Self, String> {
        let id = ChromeWebStore::extension_id(source)
            .ok_or("not a Chrome Web Store URL or extension id")?;
        let staging = store
            .path()
            .join("staging")
            .join(&id)
            .join(operation.to_bits().to_string());
        let staging = Staging::new(staging);
        let crx_path = staging.path().join("download.crx");
        let download_url = ChromeWebStore::crx_url(&id, DEFAULT_PRODVERSION);
        Ok(Self {
            id,
            profile,
            staging,
            crx_path,
            download_url,
        })
    }

    fn download(self, progress: impl FnMut(u64, Option<u64>)) -> Result<DownloadedInstall, String> {
        std::fs::create_dir_all(self.staging.path()).map_err(|error| error.to_string())?;
        download::Downloader::fetch(&self.download_url, &self.crx_path, progress)?;
        Ok(DownloadedInstall {
            id: self.id,
            profile: self.profile,
            staging: self.staging,
            crx_path: self.crx_path,
        })
    }
}

impl PackageInstaller {
    fn unpack(self, bytes: Vec<u8>) -> Result<InstallOutput, String> {
        let archive = CrxArchive::new(bytes);
        let public_key = archive.public_key_for(&self.id).ok_or_else(|| {
            format!(
                "CRX does not contain the developer key for extension {}",
                self.id
            )
        })?;
        let public_key_b64 = Some(base64::engine::general_purpose::STANDARD.encode(public_key));
        let unpack_dir = self.staging.path().join("unpacked");
        archive.unpack(&unpack_dir)?;
        let manifest_json = std::fs::read_to_string(unpack_dir.join("manifest.json"))
            .map_err(|error| error.to_string())?;
        let manifest = driver::ExtensionManifest::parse(&manifest_json)?;
        let name = manifest.resolve_name(&unpack_dir);
        let icon = manifest
            .icon
            .as_ref()
            .and_then(|relative| self.icon_data_url(&unpack_dir, relative));
        let source_hash = crate::filesystem::FileStore::source_hash(&unpack_dir)?;
        let mut profile_enabled = std::collections::BTreeMap::new();
        profile_enabled.insert(self.profile, false);
        let entry = store::ExtEntry {
            id: self.id.clone(),
            name: if name.trim().is_empty() {
                self.id
            } else {
                name
            },
            version: manifest.version,
            popup: manifest.popup,
            icon,
            enabled: false,
            profile_enabled,
            profile_pinned: std::collections::BTreeMap::new(),
            permissions: manifest.permissions,
            optional_permissions: manifest.optional_permissions,
            host_permissions: manifest.host_permissions,
            optional_host_permissions: manifest.optional_host_permissions,
            approved_grants: std::collections::BTreeMap::new(),
            source_hash,
            public_key_b64,
        };
        let mut snapshot = store::Index::default();
        snapshot.entries.push(entry.clone());
        snapshot.validate()?;
        Ok(InstallOutput {
            entry,
            source: unpack_dir,
            staging: self.staging,
        })
    }

    fn icon_data_url(&self, directory: &Path, relative: &str) -> Option<String> {
        let bytes = std::fs::read(directory.join(relative)).ok()?;
        let mime = if relative.ends_with(".svg") {
            "image/svg+xml"
        } else {
            "image/png"
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Some(format!("data:{mime};base64,{encoded}"))
    }
}

fn start(
    mut requests: MessageReader<ExtensionInstallRequest>,
    root: Single<Entity, With<Binding>>,
    source: Catalog,
    profile: vmux_ecs::profile::CurrentProfile,
    mut catalog: Single<&mut ExtensionCatalog>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    let Some(profile) = profile.profile() else {
        return;
    };
    let Some(ticket) = source.ticket(*root).filter(|ticket| source.ready(ticket)) else {
        return;
    };
    for request in requests.read() {
        let operation = commands
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                OperationTarget::new(*root),
                Name::new("Extension install"),
                ExtensionInstallOperation,
                request.clone(),
            ))
            .id();
        match ResolvedInstall::resolve(
            &request.source,
            profile.clone().into_id(),
            store::ExtensionStore::at(&ticket.directory),
            operation,
        ) {
            Ok(resolved) => {
                commands.entity(operation).insert((
                    InstallRequest {
                        ticket: ticket.clone(),
                        profile: resolved.profile.clone(),
                        id: resolved.id.clone(),
                    },
                    Resolved(resolved),
                ));
                catalog.update_progress(ExtInstallProgress {
                    key: request.source.clone(),
                    phase: ExtInstallPhase::Resolving,
                    pct: None,
                    message: "resolving".into(),
                });
            }
            Err(error) => {
                commands.trigger(OperationCompleted {
                    entity: operation,
                    result: Err(error),
                });
            }
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn download(
    ready: Query<
        (Entity, &Resolved, &ExtensionInstallRequest, &OperationState),
        With<InstallOwner>,
    >,
    mut catalog: Single<&mut ExtensionCatalog>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, resolved, request, state) in &ready {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let (sender, receiver) = crossbeam_channel::bounded(16);
        commands.entity(entity).remove::<Resolved>().insert((
            DownloadProgressChannel(receiver),
            JobRequest(DownloadJob {
                install: resolved.0.clone(),
                progress: sender,
                wake: wake.as_deref().map(|wake| wake.0.clone()),
            }),
        ));
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Downloading,
            pct: None,
            message: "downloading".into(),
        });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref()
        }
    }
}

fn unpack(
    operations: Query<(
        Entity,
        &ExtensionInstallRequest,
        &DownloadProgressChannel,
        Option<&JobResult<DownloadJob>>,
        &OperationState,
    )>,
    mut catalog: Single<&mut ExtensionCatalog>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, request, progress, result, state) in &operations {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let mut latest = None;
        for progress in progress.0.try_iter() {
            latest = Some(progress)
        }
        if let Some(progress) = latest {
            let pct = progress.total.and_then(|total| {
                if total == 0 {
                    return None;
                }
                Some(((progress.received.saturating_mul(100) / total).min(99)) as u8)
            });
            catalog.update_progress(ExtInstallProgress {
                key: request.source.clone(),
                phase: ExtInstallPhase::Downloading,
                pct,
                message: "downloading".into(),
            });
        }
        let Some(result) = result else { continue };
        commands.entity(entity).remove::<(
            DownloadProgressChannel,
            JobResult<DownloadJob>,
            vmux_ecs::JobOwner<DownloadJob>,
        )>();
        match &result.0 {
            Ok(Ok(downloaded)) => {
                commands
                    .entity(entity)
                    .insert(JobRequest(UnpackJob(downloaded.clone())));
                catalog.update_progress(ExtInstallProgress {
                    key: request.source.clone(),
                    phase: ExtInstallPhase::Unpacking,
                    pct: None,
                    message: "unpacking".into(),
                });
                if let Some(wake) = &wake {
                    wake.0.wake_by_ref()
                }
            }
            Ok(Err(error)) | Err(error) => {
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error.clone()),
                });
            }
        }
    }
}

fn materialize(
    unpacked: Query<(Entity, &InstallRequest, &JobResult<UnpackJob>), With<InstallOwner>>,
    mut completed: MessageReader<MaterializeCompleted>,
    current: Query<&InstallRequest, With<InstallOwner>>,
    mut materialize: MessageWriter<MaterializeRequest>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (entity, install, result) in &unpacked {
        commands
            .entity(entity)
            .remove::<(JobResult<UnpackJob>, vmux_ecs::JobOwner<UnpackJob>)>();
        match &result.0 {
            Ok(Ok(output)) => {
                materialize.write(
                    MaterializeRequest::new(
                        install.ticket.clone(),
                        entity,
                        Box::new(output.entry.clone()),
                        output.source.clone(),
                    )
                    .with_lifetime(output.staging.lifetime()),
                );
                commands
                    .entity(entity)
                    .insert((Materializing, output.staging.clone()));
                if let Some(wake) = &wake {
                    wake.0.wake_by_ref()
                }
            }
            Ok(Err(error)) | Err(error) => {
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error.clone()),
                });
            }
        }
    }
    for completion in completed.read() {
        let Ok(install) = current.get(completion.operation) else {
            continue;
        };
        if completion.ticket != install.ticket {
            continue;
        }
        commands
            .entity(completion.operation)
            .remove::<Materializing>();
        match &completion.result {
            Ok(entry) => {
                commands.entity(completion.operation).insert(Request {
                    ticket: install.ticket.clone(),
                    profile: install.profile.clone(),
                    mutation: Mutation::Install(Box::new(entry.clone())),
                });
                if let Some(wake) = &wake {
                    wake.0.wake_by_ref()
                }
            }
            Err(error) => {
                commands.trigger(OperationCompleted {
                    entity: completion.operation,
                    result: Err(error.clone()),
                });
            }
        }
    }
}

fn finish(
    operations: Query<
        (
            Entity,
            &ExtensionInstallRequest,
            &OperationState,
            Option<&OperationError>,
        ),
        With<ExtensionInstallOperation>,
    >,
    mut catalog: Single<&mut ExtensionCatalog>,
    mut completed: MessageWriter<ExtensionInstallCompleted>,
    mut commands: Commands,
) {
    for (entity, request, state, error) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let success = *state == OperationState::Succeeded;
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: if success {
                ExtInstallPhase::Done
            } else {
                ExtInstallPhase::Failed
            },
            pct: success.then_some(100),
            message: if success {
                "done".into()
            } else {
                error
                    .map(|error| error.message().to_string())
                    .unwrap_or_else(|| "extension installation canceled".into())
            },
        });
        if let Some(requester) = request.requester {
            completed.write(ExtensionInstallCompleted {
                requester,
                id: ChromeWebStore::extension_id(&request.source)
                    .unwrap_or_else(|| request.source.clone()),
                success,
            });
        }
        commands.entity(entity).remove::<(
            ExtensionInstallOperation,
            ExtensionInstallRequest,
            Resolved,
            DownloadProgressChannel,
            Materializing,
            Staging,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct FixtureCrx {
        id: String,
        bytes: Vec<u8>,
    }

    impl FixtureCrx {
        fn new(manifest: &str) -> Self {
            let public_key = b"PUBKEY";
            let id = String::from(ChromeExtensionId::from_public_key(public_key));
            let mut zip_bytes = Vec::new();
            {
                let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
                zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(manifest.as_bytes()).unwrap();
                zip.start_file("background.js", zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"chrome.runtime.onInstalled.addListener(() => {});")
                    .unwrap();
                zip.finish().unwrap();
            }
            let header = [0x12u8, 0x08, 0x0a, 0x06, b'P', b'U', b'B', b'K', b'E', b'Y'];
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"Cr24");
            bytes.extend_from_slice(&3u32.to_le_bytes());
            bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&header);
            bytes.extend_from_slice(&zip_bytes);
            Self { id, bytes }
        }
    }

    #[test]
    fn crx_unpack_stages_verified_source_without_changing_the_installed_package_or_index() {
        let root = tempfile::tempdir().unwrap();
        let fixture = FixtureCrx::new(
            r#"{"manifest_version":3,"name":"Fixture","version":"1.0","background":{"service_worker":"background.js"}}"#,
        );
        let store = store::ExtensionStore::at(root.path());
        let installed = store.source_dir(&fixture.id, "1.0");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("background.js"), "previous").unwrap();
        let output = PackageInstaller {
            id: fixture.id.clone(),
            profile: "test".into(),
            staging: Staging::new(root.path().join("staging/operation")),
        }
        .unpack(fixture.bytes)
        .unwrap();
        assert!(output.source.join("manifest.json").exists());
        assert_eq!(
            std::fs::read_to_string(installed.join("background.js")).unwrap(),
            "previous"
        );
        let failed = root.path().join("staging/failed");
        std::fs::create_dir_all(&failed).unwrap();
        std::fs::write(failed.join("download.crx"), "invalid archive").unwrap();
        let result = PackageInstaller {
            id: fixture.id,
            profile: "test".into(),
            staging: Staging::new(failed.clone()),
        }
        .unpack(Vec::new());
        assert!(result.is_err());
        Staging::await_removal(&failed);
        assert!(!failed.exists());
        assert!(!root.path().join("index.json").exists());
        assert_eq!(
            output.entry.source_hash,
            store.source_hash(&output.source).unwrap()
        );
        assert_eq!(
            output.entry.public_key_b64,
            Some(base64::engine::general_purpose::STANDARD.encode(b"PUBKEY"))
        );
        assert!(!output.entry.enabled_for("test"));
        let staged = output.staging.path().to_path_buf();
        drop(output);
        Staging::await_removal(&staged);
        assert!(!staged.exists());
        assert_eq!(
            std::fs::read_to_string(installed.join("background.js")).unwrap(),
            "previous"
        );
    }

    #[test]
    fn requester_completion_waits_for_materialization_and_the_core_metadata_commit() {
        let directory = tempfile::tempdir().unwrap();
        let staged = directory.path().join("staging/success");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("download.crx"), "download").unwrap();
        let mut app = App::new();
        app.add_plugins((crate::CorePlugin, InstallPlugin))
            .add_message::<MaterializeRequest>()
            .add_message::<MaterializeCompleted>();
        let root = app
            .world_mut()
            .spawn((
                Binding,
                crate::Root("/extensions".into()),
                crate::Seed(Ok(store::Index::default())),
            ))
            .id();
        app.world_mut().spawn(ExtensionCatalog::default());
        app.update();
        let mut catalog = bevy::ecs::system::SystemState::<Catalog>::new(app.world_mut());
        let ticket = catalog
            .get(app.world())
            .unwrap()
            .ticket(root)
            .unwrap()
            .clone();
        let requester = app.world_mut().spawn_empty().id();
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let entry = store::ExtEntry {
            id: id.into(),
            name: "Example".into(),
            version: "1".into(),
            ..Default::default()
        };
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                ExtensionInstallOperation,
                ExtensionInstallRequest {
                    source: id.into(),
                    requester: Some(requester),
                },
                InstallRequest {
                    ticket: ticket.clone(),
                    profile: "personal".into(),
                    id: id.into(),
                },
                JobResult::<UnpackJob>(Ok(Ok(InstallOutput {
                    entry: entry.clone(),
                    source: staged.join("unpacked"),
                    staging: Staging::new(staged.clone()),
                }))),
            ))
            .id();
        app.update();
        let materialize = app
            .world_mut()
            .resource_mut::<Messages<MaterializeRequest>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(materialize.operation, operation);
        assert!(
            app.world_mut()
                .resource_mut::<Messages<ExtensionInstallCompleted>>()
                .drain()
                .next()
                .is_none()
        );
        app.world_mut().write_message(MaterializeCompleted {
            ticket: ticket.clone(),
            operation,
            result: Ok(entry),
        });
        app.update();
        app.update();
        let write = app
            .world_mut()
            .resource_mut::<Messages<crate::WriteTaskRequest>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(write.operation, operation);
        assert!(
            app.world_mut()
                .resource_mut::<Messages<ExtensionInstallCompleted>>()
                .drain()
                .next()
                .is_none()
        );
        app.world_mut().write_message(crate::WriteCompleted {
            ticket: ticket.clone(),
            operation,
            revision: write.revision,
            result: Ok(()),
        });
        app.update();
        let completed = app
            .world_mut()
            .resource_mut::<Messages<ExtensionInstallCompleted>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(completed.requester, requester);
        assert_eq!(completed.id, id);
        assert!(completed.success);
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Succeeded)
        );
        assert!(
            app.world()
                .get::<ExtensionInstallRequest>(operation)
                .is_none()
        );
        assert!(app.world().get::<InstallOwner>(operation).is_none());
        drop(materialize);
        Staging::await_removal(&staged);
        assert!(!staged.exists());
        let canceled_staging = directory.path().join("staging/canceled");
        std::fs::create_dir_all(&canceled_staging).unwrap();
        std::fs::write(canceled_staging.join("download.crx"), "download").unwrap();
        let canceled = app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                ExtensionInstallOperation,
                ExtensionInstallRequest {
                    source: id.into(),
                    requester: None,
                },
                InstallRequest {
                    ticket,
                    profile: "personal".into(),
                    id: id.into(),
                },
                JobResult::<UnpackJob>(Ok(Ok(InstallOutput {
                    entry: store::ExtEntry {
                        id: id.into(),
                        name: "Example".into(),
                        version: "2".into(),
                        ..Default::default()
                    },
                    source: canceled_staging.join("unpacked"),
                    staging: Staging::new(canceled_staging.clone()),
                }))),
            ))
            .id();
        app.update();
        let worker = app
            .world_mut()
            .resource_mut::<Messages<MaterializeRequest>>()
            .drain()
            .next()
            .unwrap();
        app.world_mut()
            .entity_mut(canceled)
            .insert(OperationState::Canceled);
        app.update();
        assert!(canceled_staging.join("download.crx").exists());
        drop(worker);
        Staging::await_removal(&canceled_staging);
        assert!(!canceled_staging.exists());
    }
}
