use std::path::{Path, PathBuf};

use base64::Engine;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::Receiver;
use vmux_api::extension::{ExtInstallPhase, ExtInstallProgress, ExtensionsEvent};

#[cfg(test)]
use crate::crx::ChromeExtensionId;
use crate::crx::CrxArchive;
use crate::webstore::ChromeWebStore;
use crate::{catalog::ExtensionCatalog, download, manifest, store};

const DEFAULT_PRODVERSION: &str = "120.0.0.0";

pub(crate) struct InstallPlugin;

impl Plugin for InstallPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExtensionInstallRequest>()
            .add_message::<ExtensionInstallCompleted>()
            .add_systems(Update, (start, download, unpack, finish).chain());
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
struct ExtensionDownloadTask {
    progress: Receiver<DownloadProgress>,
    task: Task<Result<DownloadedInstall, String>>,
}

#[derive(Clone, Copy)]
struct DownloadProgress {
    received: u64,
    total: Option<u64>,
}

#[derive(Component)]
struct ExtensionUnpackTask(Task<Result<InstallOutput, String>>);

#[derive(Component)]
struct InstallSucceeded(InstallOutput);

#[derive(Component)]
struct InstallFailed(String);

struct ResolvedInstall {
    id: String,
    store: store::ExtensionStore,
    staging: PathBuf,
    crx_path: PathBuf,
    download_url: String,
}

struct DownloadedInstall {
    id: String,
    store: store::ExtensionStore,
    staging: PathBuf,
    crx_path: PathBuf,
}

struct PackageInstaller {
    id: String,
    store: store::ExtensionStore,
    staging: PathBuf,
}

struct InstallOutput {
    entry: store::ExtEntry,
    snapshot: ExtensionsEvent,
}

impl TryFrom<&str> for ResolvedInstall {
    type Error = String;

    fn try_from(source: &str) -> Result<Self, Self::Error> {
        let id = ChromeWebStore::extension_id(source)
            .ok_or("not a Chrome Web Store URL or extension id")?;
        let store = store::ExtensionStore::current();
        let staging = store.path().join("staging").join(&id);
        let crx_path = staging.join("download.crx");
        let download_url = ChromeWebStore::crx_url(&id, DEFAULT_PRODVERSION);
        Ok(ResolvedInstall {
            id,
            store,
            staging,
            crx_path,
            download_url,
        })
    }
}

impl ResolvedInstall {
    fn download(
        self,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<DownloadedInstall, String> {
        let _ = std::fs::remove_dir_all(&self.staging);
        std::fs::create_dir_all(&self.staging).map_err(|error| error.to_string())?;
        download::Downloader::fetch(&self.download_url, &self.crx_path, |received, total| {
            progress(received, total);
        })?;
        Ok(DownloadedInstall {
            id: self.id,
            store: self.store,
            staging: self.staging,
            crx_path: self.crx_path,
        })
    }
}

impl DownloadedInstall {
    fn unpack(self) -> Result<InstallOutput, String> {
        let bytes = std::fs::read(&self.crx_path).map_err(|error| error.to_string())?;
        PackageInstaller {
            id: self.id,
            store: self.store,
            staging: self.staging,
        }
        .unpack(bytes)
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
        std::fs::create_dir_all(&self.staging).map_err(|error| error.to_string())?;
        let unpack_dir = self.staging.join("unpacked");
        let _ = std::fs::remove_dir_all(&unpack_dir);
        archive.unpack(&unpack_dir)?;

        let manifest_json = std::fs::read_to_string(unpack_dir.join("manifest.json"))
            .map_err(|error| error.to_string())?;
        let manifest = manifest::ExtensionManifest::parse(&manifest_json)?;
        let name = manifest.resolve_name(&unpack_dir);
        let icon = manifest
            .icon
            .as_ref()
            .and_then(|relative| self.icon_data_url(&unpack_dir, relative));

        let final_dir = self.store.source_dir(&self.id, &manifest.version);
        let final_parent = final_dir.parent().ok_or("source directory has no parent")?;
        std::fs::create_dir_all(final_parent).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&unpack_dir, &final_dir).map_err(|error| error.to_string())?;
        let source_hash = self.store.source_hash(&final_dir)?;
        let _ = std::fs::remove_dir_all(&self.staging);

        let profile = vmux_ecs::profile::Profile::current().into_id();
        let mut profile_enabled = std::collections::BTreeMap::new();
        profile_enabled.insert(profile.clone(), false);

        let entry = store::ExtEntry {
            id: self.id.clone(),
            name: if name.trim().is_empty() {
                self.id.clone()
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
        let mut persisted = None;
        self.store.update_index(|index| {
            let mut next = entry.clone();
            if let Some(existing) = index.entries.iter().find(|item| item.id == next.id) {
                next.enabled = existing.enabled;
                next.profile_enabled.clone_from(&existing.profile_enabled);
                next.profile_pinned.clone_from(&existing.profile_pinned);
                next.profile_enabled.entry(profile.clone()).or_insert(false);
                next.approved_grants.clone_from(&existing.approved_grants);
                for grants in next.approved_grants.values_mut() {
                    grants.retain_declared(
                        &next.permissions,
                        &next.optional_permissions,
                        &next.host_permissions,
                        &next.optional_host_permissions,
                    );
                }
                let enabled_profiles = next
                    .profile_enabled
                    .iter()
                    .filter_map(|(profile, enabled)| enabled.then_some(profile.clone()))
                    .collect::<Vec<_>>();
                for enabled_profile in enabled_profiles {
                    if !next
                        .grants_for(&enabled_profile)
                        .covers(&next.permissions, &next.host_permissions)
                    {
                        next.profile_enabled.insert(enabled_profile, false);
                    }
                }
            }
            index.upsert(next.clone());
            persisted = Some(next);
        })?;
        let entry = persisted.ok_or("extension index update produced no entry")?;
        let snapshot = self.store.snapshot(&profile)?;
        Ok(InstallOutput { entry, snapshot })
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
    mut catalog: Single<&mut ExtensionCatalog>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Resolving,
            pct: None,
            message: "resolving".to_string(),
        });
        let mut operation = commands.spawn((
            Name::new("Extension install"),
            ExtensionInstallOperation,
            request.clone(),
        ));
        let resolved = match ResolvedInstall::try_from(request.source.as_str()) {
            Ok(resolved) => resolved,
            Err(error) => {
                operation.insert(InstallFailed(error));
                continue;
            }
        };
        let (progress_sender, progress) = crossbeam_channel::bounded(16);
        let progress_wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let completion_wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
        let task = IoTaskPool::get().spawn(async move {
            let result = resolved.download(|received, total| {
                let update = DownloadProgress { received, total };
                if progress_sender.try_send(update).is_ok()
                    && let Some(wake) = progress_wake.as_ref()
                {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
            });
            drop(completion_wake);
            result
        });
        operation.insert(ExtensionDownloadTask { progress, task });
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Downloading,
            pct: None,
            message: "downloading".to_string(),
        });
    }
}

fn download(
    mut operations: Query<(Entity, &ExtensionInstallRequest, &mut ExtensionDownloadTask)>,
    mut catalog: Single<&mut ExtensionCatalog>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, request, mut download) in &mut operations {
        let mut latest = None;
        for progress in download.progress.try_iter() {
            latest = Some(progress);
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
                message: "downloading".to_string(),
            });
        }
        let Some(result) = future::block_on(future::poll_once(&mut download.task)) else {
            continue;
        };
        let mut operation = commands.entity(entity);
        operation.remove::<ExtensionDownloadTask>();
        let downloaded = match result {
            Ok(downloaded) => downloaded,
            Err(error) => {
                operation.insert(InstallFailed(error));
                continue;
            }
        };
        let completion_wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
        let task = IoTaskPool::get().spawn(async move {
            let result = downloaded.unpack();
            drop(completion_wake);
            result
        });
        operation.insert(ExtensionUnpackTask(task));
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Unpacking,
            pct: None,
            message: "unpacking".to_string(),
        });
    }
}

fn unpack(mut operations: Query<(Entity, &mut ExtensionUnpackTask)>, mut commands: Commands) {
    for (entity, mut unpack) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut unpack.0)) else {
            continue;
        };
        let mut operation = commands.entity(entity);
        operation.remove::<ExtensionUnpackTask>();
        match result {
            Ok(output) => {
                operation.insert(InstallSucceeded(output));
            }
            Err(error) => {
                operation.insert(InstallFailed(error));
            }
        }
    }
}

fn finish(
    succeeded: Query<
        (Entity, &ExtensionInstallRequest, &InstallSucceeded),
        Added<InstallSucceeded>,
    >,
    failed: Query<(Entity, &ExtensionInstallRequest, &InstallFailed), Added<InstallFailed>>,
    mut catalog: Single<&mut ExtensionCatalog>,
    mut completed: MessageWriter<ExtensionInstallCompleted>,
    mut commands: Commands,
) {
    for (entity, request, succeeded) in &succeeded {
        catalog.replace(succeeded.0.snapshot.clone());
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Done,
            pct: Some(100),
            message: "done".to_string(),
        });
        if let Some(requester) = request.requester {
            completed.write(ExtensionInstallCompleted {
                requester,
                id: succeeded.0.entry.id.clone(),
                success: true,
            });
        }
        commands.entity(entity).despawn();
    }
    for (entity, request, failed) in &failed {
        catalog.update_progress(ExtInstallProgress {
            key: request.source.clone(),
            phase: ExtInstallPhase::Failed,
            pct: None,
            message: failed.0.clone(),
        });
        if let Some(requester) = request.requester {
            completed.write(ExtensionInstallCompleted {
                requester,
                id: request.source.clone(),
                success: false,
            });
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

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

    struct FixtureStore {
        store: store::ExtensionStore,
    }

    impl FixtureStore {
        fn at(root: &Path) -> Self {
            Self {
                store: store::ExtensionStore::at(root),
            }
        }

        fn unpack(&self, fixture: &FixtureCrx) -> InstallOutput {
            let staging = self.store.path().join("staging").join(&fixture.id);
            PackageInstaller {
                id: fixture.id.clone(),
                store: self.store.clone(),
                staging,
            }
            .unpack(fixture.bytes.clone())
            .unwrap()
        }
    }

    #[test]
    fn installs_source_under_immutable_package_path() {
        let root = tempfile::tempdir().unwrap();
        let fixture = FixtureCrx::new(
            r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "1.0",
                "background": { "service_worker": "background.js" }
            }"#,
        );
        let fixture_store = FixtureStore::at(root.path());

        let entry = fixture_store.unpack(&fixture).entry;

        let source = fixture_store.store.source_dir(&fixture.id, &entry.version);
        assert!(source.join("manifest.json").exists());
        assert!(!root.path().join(&fixture.id).exists());
        assert_eq!(
            entry.source_hash,
            fixture_store.store.source_hash(&source).unwrap()
        );
        assert_eq!(
            entry.public_key_b64,
            Some(base64::engine::general_purpose::STANDARD.encode(b"PUBKEY"))
        );
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(source.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["background"]["service_worker"], "background.js");
        assert!(entry.installed_for("personal"));
        assert!(!entry.enabled_for("personal"));
        assert_eq!(
            entry.grants_for("personal"),
            store::ExtensionGrants::default()
        );
    }

    #[test]
    fn update_reconciles_grants_and_disables_every_affected_profile() {
        let root = tempfile::tempdir().unwrap();
        let initial = FixtureCrx::new(
            r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "1.0",
                "permissions": ["storage"],
                "optional_permissions": ["history"],
                "background": { "service_worker": "background.js" }
            }"#,
        );
        let fixture_store = FixtureStore::at(root.path());
        fixture_store.unpack(&initial);
        fixture_store
            .store
            .update_index(|index| {
                let entry = index
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id == initial.id)
                    .unwrap();
                for profile in ["personal", "work"] {
                    entry.profile_enabled.insert(profile.into(), true);
                    entry.approved_grants.insert(
                        profile.into(),
                        store::ExtensionGrants {
                            permissions: vec!["storage".into(), "history".into()],
                            host_permissions: Vec::new(),
                        },
                    );
                }
            })
            .unwrap();
        let update = FixtureCrx::new(
            r#"{
                "manifest_version": 3,
                "name": "Fixture",
                "version": "2.0",
                "permissions": ["storage", "bookmarks"],
                "background": { "service_worker": "background.js" }
            }"#,
        );

        let returned = fixture_store.unpack(&update).entry;
        let stored = fixture_store
            .store
            .load_index()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.id == update.id)
            .unwrap();

        assert_eq!(returned, stored);
        for profile in ["personal", "work"] {
            assert!(!stored.enabled_for(profile));
            assert_eq!(
                stored.grants_for(profile).permissions,
                vec!["storage".to_string()]
            );
        }
    }
}
