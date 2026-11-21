use super::activation::{self, Request as Activation};
use super::catalog::Package;
use super::download::{PACKAGE_MAX_BYTES, RemoteArtifact};
use super::install_io::{
    Download, Extract, Link, MakeExecutable, Prepare, Transferred, WriteReceipt,
};
use super::package_path::{PackageName, PackagePath, Sha256Digest};
use super::purl::Purl;
use super::store::{PackageStore, Receipt};
use super::target::{Asset, PlatformTarget};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use vmux_ecs::event::{InstallPhase, LspInstallProgress};
use vmux_ecs::{
    JobPlugin, JobRequest, JobResult, Operation, OperationRetained, OperationState, OperationTarget,
};
use vmux_tool::io::{
    Authorization, CloseTemporary, CreateTemporary, Execute, Get, TemporaryDirectory,
    TemporaryTransfer,
};

pub(super) struct InstallPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct InstallSet;

impl Plugin for InstallPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::io::ObservationIoPlugin>() {
            app.add_plugins(vmux_tool::io::ObservationIoPlugin);
        }
        if !app.is_plugin_added::<activation::PackageActivationPlugin>() {
            app.add_plugins(activation::PackageActivationPlugin);
        }
        if !app.is_plugin_added::<JobPlugin<CreateTemporary>>() {
            app.add_plugins(JobPlugin::<CreateTemporary>::default());
        }
        if !app.is_plugin_added::<JobPlugin<CloseTemporary>>() {
            app.add_plugins(JobPlugin::<CloseTemporary>::default());
        }
        app.add_plugins((
            JobPlugin::<Prepare>::default(),
            JobPlugin::<Download>::default(),
            JobPlugin::<Extract>::default(),
            JobPlugin::<MakeExecutable>::default(),
            JobPlugin::<WriteReceipt>::default(),
            JobPlugin::<Link>::default(),
        ))
        .add_message::<Retry>()
        .add_systems(
            Update,
            (
                (
                    retire,
                    retry_pending,
                    begin,
                    created,
                    prepared,
                    metadata,
                    release,
                )
                    .chain(),
                (executed, toolchain, downloaded, extracted).chain(),
                (
                    executable, receipt, receipted, activated, linked, progress, retry,
                )
                    .chain(),
            )
                .chain()
                .in_set(InstallSet)
                .before(activation::ActivationSet)
                .before(vmux_ecs::JobSet::Submit),
        )
        .add_systems(
            PostUpdate,
            (drain, closed, wake)
                .chain()
                .after(vmux_ecs::JobSet::Cleanup),
        );
    }
}

#[derive(Component)]
pub(super) struct Request {
    pub package: Entity,
    pub store: PackageStore,
}

#[derive(Component)]
pub(super) struct Completed(pub Result<Receipt, String>);

#[derive(Component)]
pub(super) struct Released;

#[derive(Message)]
pub(super) struct Retry {
    pub entity: Entity,
}

#[derive(Component)]
pub(super) struct Progress(pub LspInstallProgress);

#[derive(Component)]
struct Source {
    name: PackageName,
    id: String,
    purl: Purl,
    assets: Vec<Asset>,
    bin: BTreeMap<PackageName, String>,
}

#[derive(Component, Clone)]
pub(super) struct Lease(pub(super) PathBuf);

#[derive(Component)]
pub(super) struct ConfigurationPending;

#[derive(Component)]
struct SelectedAsset {
    asset: Asset,
    binary: PackagePath,
}

#[derive(Component)]
struct Creating;

#[derive(Component)]
struct Preparing;

#[derive(Component)]
struct Resolving;

#[derive(Component, serde::Deserialize)]
struct ReleaseMetadata {
    assets: Vec<ReleaseAsset>,
}

#[derive(serde::Deserialize)]
struct ReleaseAsset {
    name: String,
    size: Option<u64>,
    digest: Option<String>,
    browser_download_url: Option<String>,
}

impl ReleaseAsset {
    fn artifact(&self) -> Result<RemoteArtifact, String> {
        let size = self.size.ok_or("GitHub release asset size missing")?;
        if size > PACKAGE_MAX_BYTES {
            return Err("GitHub release asset exceeds size limit".into());
        }
        let digest = self
            .digest
            .as_deref()
            .ok_or("GitHub release asset digest missing")?;
        let url = self
            .browser_download_url
            .as_deref()
            .ok_or("GitHub release asset URL missing")?;
        Ok(RemoteArtifact::new(url, Sha256Digest::parse(digest)?))
    }
}

#[derive(Component)]
struct Downloading(crossbeam_channel::Receiver<Transferred>);

#[derive(Component)]
struct Extracting;

#[derive(Component)]
struct Toolchain;

#[derive(Component)]
struct Executing;

#[derive(Component)]
struct Venv;

#[derive(Component)]
struct Files;

#[derive(Component)]
struct ExecutableFor(Entity);

#[derive(Component)]
struct Writing;

#[derive(Component)]
struct Activating(Entity);

#[derive(Component)]
struct Linking;

#[derive(Component)]
struct LinkFor(Entity);

#[derive(Component)]
struct Outcome(Result<Receipt, String>);

#[derive(Component)]
struct CleanupFor(Entity);

#[derive(Component)]
struct CleanupFailed;

#[derive(Component)]
struct RetriedCleanup;

impl Source {
    fn progress(
        &self,
        phase: InstallPhase,
        pct: Option<u8>,
        message: impl Into<String>,
    ) -> Progress {
        Progress(LspInstallProgress {
            name: self.name.as_str().into(),
            phase,
            pct,
            message: message.into(),
        })
    }

    fn release_url(&self) -> Result<String, String> {
        let owner = self
            .purl
            .namespace
            .as_deref()
            .ok_or("github purl missing owner")?;
        let tag = self
            .purl
            .version
            .as_deref()
            .ok_or("github purl missing version")?;
        let mut endpoint =
            url::Url::parse("https://api.github.com").map_err(|error| error.to_string())?;
        endpoint
            .path_segments_mut()
            .map_err(|_| "invalid GitHub API base URL")?
            .extend(["repos", owner, &self.purl.name, "releases", "tags", tag]);
        Ok(endpoint.to_string())
    }
}

fn retire(
    requests: Query<(Entity, &Request, Option<&Source>, &OperationState), Without<Released>>,
    packages: Query<&Package>,
    children: Query<(Entity, &OperationTarget)>,
    mut commands: Commands,
) {
    for (entity, request, source, state) in &requests {
        let current = packages.get(request.package).ok();
        let valid = current.is_some_and(|package| {
            source.is_none_or(|source| {
                package.name == source.name
                    && package.source_id == source.id
                    && package.assets == source.assets
                    && package.bin == source.bin
            })
        });
        if !valid && matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
        if valid && matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        for (child, parent) in &children {
            if parent.entity() == entity {
                commands.entity(child).try_insert(OperationState::Canceled);
            }
        }
    }
}

type RetryPendingRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static OperationState),
    (Without<Lease>, Without<RetriedCleanup>, Without<Released>),
>;

fn retry_pending(
    requests: RetryPendingRequests<'_, '_>,
    leases: Query<(&Lease, Option<&Activating>)>,
    failed: Query<(), With<activation::RestoreFailed>>,
    cleanup: Query<(Entity, &CleanupFor, &TemporaryDirectory), With<CleanupFailed>>,
    mut retries: MessageWriter<activation::Retry>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let mut retried = false;
        for (lease, activation) in &leases {
            if lease.0 != request.store.path() {
                continue;
            }
            if let Some(activation) = activation
                && failed.contains(activation.0)
            {
                retries.write(activation::Retry {
                    entity: activation.0,
                });
                retried = true;
            }
        }
        for (child, owner, staging) in &cleanup {
            if !leases
                .get(owner.0)
                .is_ok_and(|(lease, _)| lease.0 == request.store.path())
            {
                continue;
            }
            commands
                .entity(child)
                .remove::<CleanupFailed>()
                .insert(JobRequest(CloseTemporary {
                    staging: staging.clone(),
                }));
            retried = true;
        }
        if retried {
            commands.entity(entity).insert(RetriedCleanup);
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

type BeginRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static OperationState),
    (Without<Lease>, Without<Outcome>, Without<Released>),
>;

fn begin(
    requests: BeginRequests<'_, '_>,
    packages: Query<&Package>,
    leases: Query<&Lease>,
    mut commands: Commands,
) {
    let mut busy = HashSet::new();
    for lease in &leases {
        busy.insert(lease.0.clone());
    }
    for (entity, request, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if !busy.insert(request.store.path().to_path_buf()) {
            continue;
        }
        let Ok(package) = packages.get(request.package) else {
            continue;
        };
        let Some(purl) = Purl::parse(&package.source_id) else {
            commands
                .entity(entity)
                .insert((Outcome(Err("bad purl".into())), OperationState::Failed));
            continue;
        };
        let source = Source {
            name: package.name.clone(),
            id: package.source_id.clone(),
            purl,
            assets: package.assets.clone(),
            bin: package.bin.clone(),
        };
        let mut owner = commands.entity(entity);
        owner.insert((
            Lease(request.store.path().to_path_buf()),
            source.progress(InstallPhase::Resolving, None, "selecting source"),
        ));
        if source.purl.kind == "github" {
            let Some(asset) = PlatformTarget::current().select(&source.assets).cloned() else {
                owner.insert((
                    Outcome(Err("no prebuilt asset for this platform".into())),
                    OperationState::Failed,
                ));
                continue;
            };
            let binary = asset
                .bin
                .clone()
                .unwrap_or_else(|| PackagePath::from(&package.name));
            owner.insert(SelectedAsset { asset, binary });
        } else if source.purl.toolchain().is_some() {
            owner.insert(Toolchain);
        } else {
            owner.insert((
                Outcome(Err(format!(
                    "install source '{}' not yet supported",
                    source.purl.kind
                ))),
                OperationState::Failed,
            ));
            continue;
        }
        let transfer = TemporaryTransfer::default();
        owner.insert((
            source,
            Creating,
            OperationState::Running,
            transfer.clone(),
            JobRequest(CreateTemporary {
                root: request.store.staging_dir(),
                transfer,
            }),
        ));
    }
}

fn created(
    requests: Query<
        (
            Entity,
            &TemporaryTransfer,
            &JobResult<CreateTemporary>,
            &OperationState,
        ),
        With<Creating>,
    >,
    mut commands: Commands,
) {
    for (entity, transfer, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Creating, JobResult<CreateTemporary>)>();
        if let Some(staging) = transfer.take() {
            if *state == OperationState::Running {
                commands.entity(entity).insert((
                    Preparing,
                    JobRequest(Prepare {
                        staging: staging.clone(),
                    }),
                ));
            }
            commands.entity(entity).insert(staging);
        }
        commands.entity(entity).remove::<TemporaryTransfer>();
        if *state != OperationState::Running {
            continue;
        }
        if let Ok(Err(error)) | Err(error) = &result.0 {
            commands
                .entity(entity)
                .insert((Outcome(Err(error.clone())), OperationState::Failed));
        }
    }
}

type PreparedRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static JobResult<Prepare>,
        &'static OperationState,
        Has<Toolchain>,
    ),
    With<Preparing>,
>;

fn prepared(requests: PreparedRequests<'_, '_>, mut commands: Commands) {
    for (entity, source, result, state, toolchain) in &requests {
        commands
            .entity(entity)
            .remove::<(Preparing, JobResult<Prepare>)>();
        if *state != OperationState::Running {
            continue;
        }
        if let Ok(Err(error)) | Err(error) = &result.0 {
            commands
                .entity(entity)
                .insert((Outcome(Err(error.clone())), OperationState::Failed));
            continue;
        }
        if toolchain {
            continue;
        }
        let url = match source.release_url() {
            Ok(url) => url,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error)), OperationState::Failed));
                continue;
            }
        };
        let authorization =
            Authorization::from_environment("GITHUB_TOKEN", "https://api.github.com")
                .expect("GitHub API origin is valid");
        commands.entity(entity).insert((
            Resolving,
            JobRequest(Get {
                url,
                limit: 8 * 1024 * 1024,
                authorization,
            }),
        ));
    }
}

fn metadata(
    requests: Query<(Entity, &JobResult<Get>, &OperationState), With<Resolving>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        commands.entity(entity).remove::<JobResult<Get>>();
        if *state != OperationState::Running {
            continue;
        }
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => &bytes.0,
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error.clone())), OperationState::Failed));
                continue;
            }
        };
        let Some(bytes) = bytes else {
            commands.entity(entity).insert((
                Outcome(Err("GitHub release metadata missing".into())),
                OperationState::Failed,
            ));
            continue;
        };
        match serde_json::from_slice::<ReleaseMetadata>(bytes) {
            Ok(metadata) => {
                commands.entity(entity).insert(metadata);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error.to_string())), OperationState::Failed));
            }
        }
    }
}

type ReleaseRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static SelectedAsset,
        &'static TemporaryDirectory,
        &'static ReleaseMetadata,
        &'static OperationState,
    ),
    With<Resolving>,
>;

fn release(
    requests: ReleaseRequests<'_, '_>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, source, selected, staging, metadata, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Resolving, ReleaseMetadata)>();
        if *state != OperationState::Running {
            continue;
        }
        let mut matched = None;
        for asset in &metadata.assets {
            if asset.name == selected.asset.file.as_str() {
                matched = Some(asset);
                break;
            }
        }
        let Some(asset) = matched else {
            commands.entity(entity).insert((
                Outcome(Err("GitHub release asset not found".into())),
                OperationState::Failed,
            ));
            continue;
        };
        let artifact = match asset.artifact() {
            Ok(artifact) => artifact,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error)), OperationState::Failed));
                continue;
            }
        };
        if selected
            .asset
            .sha256
            .as_ref()
            .is_some_and(|expected| expected != &artifact.sha256)
        {
            commands.entity(entity).insert((
                Outcome(Err("catalog and GitHub asset digests disagree".into())),
                OperationState::Failed,
            ));
            continue;
        }
        let (progress, updates) = crossbeam_channel::bounded(16);
        commands.entity(entity).insert((
            source.progress(InstallPhase::Downloading, Some(0), &artifact.url),
            Downloading(updates),
            JobRequest(Download {
                artifact,
                staging: staging.clone(),
                file: selected.asset.file.clone(),
                progress,
                wake: wake.as_ref().map(|wake| wake.0.clone()),
            }),
        ));
    }
}

type ToolchainRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static TemporaryDirectory,
        &'static OperationState,
        Has<Venv>,
    ),
    (
        With<Toolchain>,
        Without<Preparing>,
        Without<Executing>,
        Without<Files>,
        Without<Outcome>,
    ),
>;

fn toolchain(requests: ToolchainRequests<'_, '_>, mut commands: Commands) {
    for (entity, source, staging, state, venv) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let package = staging.path().join("package");
        let version = source.purl.version.as_deref().unwrap_or("latest");
        let name = match &source.purl.namespace {
            Some(namespace) => format!("{namespace}/{}", source.purl.name),
            None => source.purl.name.clone(),
        };
        let mut environment = Vec::new();
        let (program, arguments) = match source.purl.kind.as_str() {
            "npm" => (
                "npm".into(),
                vec![
                    "install".into(),
                    "--prefix".into(),
                    package.to_string_lossy().into_owned(),
                    format!("{name}@{version}"),
                ],
            ),
            "cargo" => {
                let mut arguments = vec![
                    "install".into(),
                    "--root".into(),
                    package.to_string_lossy().into_owned(),
                ];
                if let Some(version) = &source.purl.version {
                    arguments.extend(["--version".into(), version.clone()]);
                }
                arguments.push(source.purl.name.clone());
                ("cargo".into(), arguments)
            }
            "golang" => {
                environment.push((
                    "GOBIN".into(),
                    package.join("bin").to_string_lossy().into_owned(),
                ));
                (
                    "go".into(),
                    vec!["install".into(), format!("{name}@{version}")],
                )
            }
            "pypi" if !venv => (
                "python3".into(),
                vec![
                    "-m".into(),
                    "venv".into(),
                    package.join("venv").to_string_lossy().into_owned(),
                ],
            ),
            "pypi" => {
                let spec = match &source.purl.version {
                    Some(version) => format!("{}=={version}", source.purl.name),
                    None => source.purl.name.clone(),
                };
                (
                    package.join("venv/bin/pip").to_string_lossy().into_owned(),
                    vec!["install".into(), spec],
                )
            }
            _ => unreachable!(),
        };
        commands.entity(entity).insert((
            source.progress(InstallPhase::Downloading, None, &program),
            Executing,
            JobRequest(Execute {
                program,
                arguments,
                environment,
                timeout: std::time::Duration::from_secs(1200),
            }),
        ));
    }
}

type ExecutedRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static JobResult<Execute>,
        &'static OperationState,
        Has<Venv>,
    ),
    With<Executing>,
>;

fn executed(requests: ExecutedRequests<'_, '_>, mut commands: Commands) {
    for (entity, source, result, state, venv) in &requests {
        commands
            .entity(entity)
            .remove::<(Executing, JobResult<Execute>)>();
        if *state != OperationState::Running {
            continue;
        }
        let error = match &result.0 {
            Ok(Ok(Some(output))) if output.0.status.success() => None,
            Ok(Ok(Some(output))) => Some(format!(
                "toolchain failed ({}): {}",
                output.0.status,
                String::from_utf8_lossy(&output.0.stderr)
            )),
            Ok(Ok(None)) => Some(format!("requires {}", source.purl.toolchain().unwrap())),
            Ok(Err(error)) | Err(error) => Some(error.clone()),
        };
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((Outcome(Err(error)), OperationState::Failed));
        } else if source.purl.kind == "pypi" && !venv {
            commands.entity(entity).insert(Venv);
        } else {
            commands.entity(entity).remove::<Toolchain>().insert(Files);
        }
    }
}

type DownloadedRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static SelectedAsset,
        &'static TemporaryDirectory,
        &'static JobResult<Download>,
        &'static OperationState,
    ),
    With<Downloading>,
>;

fn downloaded(requests: DownloadedRequests<'_, '_>, mut commands: Commands) {
    for (entity, source, selected, staging, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Downloading, JobResult<Download>)>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert((
                    source.progress(InstallPhase::Extracting, None, "extracting"),
                    Extracting,
                    JobRequest(Extract {
                        staging: staging.clone(),
                        file: selected.asset.file.clone(),
                        binary: selected.binary.clone(),
                    }),
                ));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error.clone())), OperationState::Failed));
            }
        }
    }
}

fn extracted(
    requests: Query<(Entity, &JobResult<Extract>, &OperationState), With<Extracting>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Extracting, JobResult<Extract>)>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert(Files);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error.clone())), OperationState::Failed));
            }
        }
    }
}

type ExecutableRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Source,
        &'static TemporaryDirectory,
        Option<&'static SelectedAsset>,
        &'static OperationState,
    ),
    (With<Files>, Without<Receipt>, Without<Outcome>),
>;

fn executable(requests: ExecutableRequests<'_, '_>, mut commands: Commands) {
    for (entity, source, staging, asset, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut links = BTreeMap::new();
        let mut failure = None;
        if let Some(asset) = asset {
            if source.bin.is_empty() {
                links.insert(source.name.clone(), asset.binary.clone());
            } else {
                for (name, template) in &source.bin {
                    let file = template
                        .replace("{{source.asset.bin}}", asset.binary.as_str())
                        .replace("{{source.asset.file}}", asset.binary.as_str());
                    match PackagePath::parse(&file) {
                        Ok(file) => {
                            links.insert(name.clone(), file);
                        }
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                }
            }
        } else {
            let prefix = match source.purl.kind.as_str() {
                "npm" => "node_modules/.bin/",
                "pypi" => "venv/bin/",
                "cargo" | "golang" => "bin/",
                _ => unreachable!(),
            };
            let names = if source.bin.is_empty() {
                vec![source.name.clone()]
            } else {
                source.bin.keys().cloned().collect()
            };
            for name in names {
                match PackagePath::parse(&format!("{prefix}{}", name.as_str())) {
                    Ok(file) => {
                        links.insert(name, file);
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
        }
        if let Some(error) = failure {
            commands
                .entity(entity)
                .insert((Outcome(Err(error)), OperationState::Failed));
            continue;
        }
        for file in links.values() {
            commands.spawn((
                ExecutableFor(entity),
                Operation::bundle("lsp.package.executable")
                    .unwrap()
                    .running(),
                OperationRetained,
                OperationTarget::new(entity),
                JobRequest(MakeExecutable {
                    staging: staging.clone(),
                    file: file.clone(),
                }),
            ));
        }
        commands.entity(entity).insert((
            source.progress(InstallPhase::Linking, None, "linking"),
            Receipt {
                name: source.name.clone(),
                version: source.purl.version.clone(),
                source_id: source.id.clone(),
                bin: links,
            },
        ));
    }
}

type ReceiptRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Receipt,
        &'static TemporaryDirectory,
        &'static OperationState,
    ),
    (
        With<Files>,
        Without<Writing>,
        Without<Activating>,
        Without<Linking>,
        Without<Outcome>,
    ),
>;

fn receipt(
    requests: ReceiptRequests<'_, '_>,
    probes: Query<(Entity, &ExecutableFor, Option<&JobResult<MakeExecutable>>)>,
    mut commands: Commands,
) {
    for (entity, receipt, staging, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut failure = None;
        for (_, owner, result) in &probes {
            if owner.0 != entity {
                continue;
            }
            match result {
                None => pending = true,
                Some(JobResult(Ok(Ok(())))) => {}
                Some(JobResult(Ok(Err(error)) | Err(error))) => failure = Some(error.clone()),
            }
        }
        if pending {
            continue;
        }
        for (probe, owner, _) in &probes {
            if owner.0 == entity {
                commands.entity(probe).despawn();
            }
        }
        if let Some(error) = failure {
            commands
                .entity(entity)
                .insert((Outcome(Err(error)), OperationState::Failed));
            continue;
        }
        commands.entity(entity).remove::<Files>().insert((
            Writing,
            JobRequest(WriteReceipt {
                staging: staging.clone(),
                receipt: receipt.clone(),
            }),
        ));
    }
}

type ReceiptedRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Source,
        &'static TemporaryDirectory,
        &'static JobResult<WriteReceipt>,
        &'static OperationState,
    ),
    With<Writing>,
>;

fn receipted(requests: ReceiptedRequests<'_, '_>, mut commands: Commands) {
    for (entity, request, source, staging, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Writing, JobResult<WriteReceipt>)>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(())) => {
                let activation = commands
                    .spawn((
                        Operation::bundle("lsp.package.activation")
                            .unwrap()
                            .running(),
                        OperationRetained,
                        Activation {
                            owner: entity,
                            store: request.store.clone(),
                            name: source.name.clone(),
                            backup: staging.path().join("previous"),
                        },
                        staging.clone(),
                    ))
                    .id();
                commands.entity(entity).insert(Activating(activation));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error.clone())), OperationState::Failed));
            }
        }
    }
}

fn activated(
    requests: Query<(Entity, &Request, &Receipt, &Activating, &OperationState)>,
    activations: Query<(
        Option<&activation::Completed>,
        Option<&vmux_ecs::OperationError>,
        Has<activation::RestoreFailed>,
    )>,
    mut commands: Commands,
) {
    for (entity, request, receipt, activation, state) in &requests {
        let Ok((result, error, failed)) = activations.get(activation.0) else {
            continue;
        };
        if failed {
            if let Some(error) = error {
                commands.entity(entity).insert((
                    Outcome(Err(error.message().into())),
                    Completed(Err(error.message().into())),
                    OperationState::Failed,
                ));
            }
            continue;
        }
        let Some(result) = result else { continue };
        commands.entity(activation.0).despawn();
        commands.entity(entity).remove::<Activating>();
        if *state != OperationState::Running {
            continue;
        }
        if let Err(error) = &result.0 {
            commands
                .entity(entity)
                .insert((Outcome(Err(error.clone())), OperationState::Failed));
            continue;
        }
        for (name, file) in &receipt.bin {
            commands.spawn((
                LinkFor(entity),
                Operation::bundle("lsp.package.link").unwrap().running(),
                OperationRetained,
                OperationTarget::new(entity),
                JobRequest(Link {
                    store: request.store.clone(),
                    package: receipt.name.clone(),
                    name: name.clone(),
                    file: file.clone(),
                }),
            ));
        }
        commands.entity(entity).insert(Linking);
    }
}

fn linked(
    requests: Query<(Entity, &Receipt, &OperationState), With<Linking>>,
    links: Query<(Entity, &LinkFor, Option<&JobResult<Link>>)>,
    mut commands: Commands,
) {
    for (entity, receipt, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut failure = None;
        for (_, owner, result) in &links {
            if owner.0 != entity {
                continue;
            }
            match result {
                None => pending = true,
                Some(JobResult(Ok(Ok(())))) => {}
                Some(JobResult(Ok(Err(error)) | Err(error))) => failure = Some(error.clone()),
            }
        }
        if pending {
            continue;
        }
        for (link, owner, _) in &links {
            if owner.0 == entity {
                commands.entity(link).despawn();
            }
        }
        commands.entity(entity).remove::<Linking>();
        match failure {
            Some(error) => {
                commands
                    .entity(entity)
                    .insert((Outcome(Err(error)), OperationState::Failed));
            }
            None => {
                commands
                    .entity(entity)
                    .insert((Outcome(Ok(receipt.clone())), OperationState::Succeeded));
            }
        }
    }
}

fn progress(mut downloads: Query<(&Source, &Downloading, &mut Progress)>) {
    for (source, updates, mut progress) in &mut downloads {
        for update in updates.0.try_iter() {
            let pct = update
                .total
                .filter(|total| *total > 0)
                .map(|total| ((update.bytes.saturating_mul(100) / total).min(100)) as u8);
            *progress = source.progress(InstallPhase::Downloading, pct, "downloading");
        }
    }
}

fn retry(
    mut requests: MessageReader<Retry>,
    activations: Query<&Activating>,
    failed: Query<(), With<activation::RestoreFailed>>,
    cleanup: Query<(Entity, &CleanupFor, &TemporaryDirectory), With<CleanupFailed>>,
    mut restore: MessageWriter<activation::Retry>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if let Ok(activation) = activations.get(request.entity)
            && failed.contains(activation.0)
        {
            restore.write(activation::Retry {
                entity: activation.0,
            });
        }
        for (entity, owner, staging) in &cleanup {
            if owner.0 == request.entity {
                commands
                    .entity(entity)
                    .remove::<CleanupFailed>()
                    .insert(JobRequest(CloseTemporary {
                        staging: staging.clone(),
                    }));
            }
        }
    }
}

type DrainRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationState,
        Option<&'static Outcome>,
        Option<&'static TemporaryDirectory>,
        Option<&'static TemporaryTransfer>,
        Has<ConfigurationPending>,
    ),
    (With<Request>, Without<Released>, Without<Activating>),
>;

fn drain(
    requests: DrainRequests<'_, '_>,
    native: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    probes: Query<(Entity, &ExecutableFor)>,
    links: Query<(Entity, &LinkFor)>,
    cleanup: Query<&CleanupFor>,
    mut commands: Commands,
) {
    for (entity, state, outcome, staging, transfer, configuration) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if native.iter().any(|target| {
            target.entity() == entity
                || probes
                    .get(target.entity())
                    .is_ok_and(|(_, owner)| owner.0 == entity)
                || links
                    .get(target.entity())
                    .is_ok_and(|(_, owner)| owner.0 == entity)
        }) {
            continue;
        }
        if let Some(transfer) = transfer {
            commands
                .entity(entity)
                .remove::<(Creating, TemporaryTransfer)>();
            if let Some(staging) = transfer.take() {
                commands.entity(entity).insert(staging);
                continue;
            }
        }
        for (probe, owner) in &probes {
            if owner.0 == entity {
                commands.entity(probe).despawn();
            }
        }
        for (link, owner) in &links {
            if owner.0 == entity {
                commands.entity(link).despawn();
            }
        }
        if cleanup.iter().any(|owner| owner.0 == entity) {
            continue;
        }
        let result = outcome
            .map(|outcome| outcome.0.clone())
            .unwrap_or_else(|| Err("LSP install canceled".into()));
        if let Some(staging) = staging {
            if !staging.exclusive() {
                continue;
            }
            commands.spawn((
                CleanupFor(entity),
                Operation::bundle("lsp.package.cleanup").unwrap().running(),
                OperationRetained,
                staging.clone(),
                JobRequest(CloseTemporary {
                    staging: staging.clone(),
                }),
            ));
            commands.entity(entity).remove::<TemporaryDirectory>();
        } else {
            let mut owner = commands.entity(entity);
            if !configuration {
                owner.remove::<Lease>();
            }
            owner.insert((Completed(result), Released));
        }
    }
}

type ClosedCleanup<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CleanupFor,
        Option<&'static JobResult<CloseTemporary>>,
        Has<CleanupFailed>,
    ),
>;

fn closed(
    cleanup: ClosedCleanup<'_, '_>,
    owners: Query<(Option<&Outcome>, Has<ConfigurationPending>), With<Request>>,
    mut commands: Commands,
) {
    for (entity, owner, result, failed) in &cleanup {
        if failed {
            continue;
        }
        let Some(result) = result else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => {
                let Ok((result, configuration)) = owners.get(owner.0) else {
                    commands.entity(entity).despawn();
                    continue;
                };
                let outcome = result
                    .map(|outcome| outcome.0.clone())
                    .unwrap_or_else(|| Err("LSP install canceled".into()));
                if !configuration {
                    commands.entity(owner.0).try_remove::<Lease>();
                }
                commands
                    .entity(owner.0)
                    .try_insert((Completed(outcome), Released));
                commands.entity(entity).despawn();
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .remove::<JobResult<CloseTemporary>>()
                    .insert(CleanupFailed);
                commands
                    .entity(owner.0)
                    .try_insert(Completed(Err(format!("LSP cleanup failed: {error}"))));
            }
        }
    }
}

type WakeProgress<'w, 's> =
    Query<'w, 's, Entity, Or<(Changed<Progress>, Added<TemporaryDirectory>)>>;

type WakeCleanup<'w, 's> = Query<'w, 's, Entity, Or<(Added<CleanupFor>, Changed<CleanupFailed>)>>;

type WakeComplete<'w, 's> = Query<'w, 's, Entity, Or<(Added<Completed>, Added<Released>)>>;

fn wake(
    progress: WakeProgress<'_, '_>,
    cleanup: WakeCleanup<'_, '_>,
    complete: WakeComplete<'_, '_>,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    if (!progress.is_empty() || !cleanup.is_empty() || !complete.is_empty())
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Add, App, Commands, Entity, On, Plugin, Query, Res, Resource, With};
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    struct WakeChannel(mpsc::SyncSender<()>);

    impl std::task::Wake for WakeChannel {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }
        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    #[derive(Resource)]
    struct MetadataEndpoint(String);

    struct MetadataFixturePlugin;

    impl Plugin for MetadataFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(metadata_endpoint);
        }
    }

    fn metadata_endpoint(
        trigger: On<Add, JobRequest<Get>>,
        requests: Query<&JobRequest<Get>, With<Resolving>>,
        endpoint: Res<MetadataEndpoint>,
        mut commands: Commands,
    ) {
        if let Ok(request) = requests.get(trigger.event_target()) {
            let mut request = request.0.clone();
            request.url = endpoint.0.clone();
            request.authorization = None;
            commands
                .entity(trigger.event_target())
                .insert(JobRequest(request));
        }
    }

    struct NativeServer {
        stop: Arc<std::sync::atomic::AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for NativeServer {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Release);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    impl NativeServer {
        fn new(metadata: Vec<u8>, archive: Vec<u8>, listener: TcpListener) -> Self {
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stopping = stop.clone();
            let worker = std::thread::spawn(move || {
                while !stopping.load(std::sync::atomic::Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 2048];
                    let count = stream.read(&mut request).unwrap();
                    let body = if request[..count].starts_with(b"GET /release ") {
                        &metadata
                    } else {
                        &archive
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                }
            });
            Self {
                stop,
                worker: Some(worker),
            }
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        store: PackageStore,
        operation: Entity,
        events: mpsc::Receiver<()>,
        _server: NativeServer,
    }

    impl Fixture {
        fn new(failure: &str) -> Self {
            let binary = b"#!/bin/sh\nprintf lsp-installed";
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(binary).unwrap();
            let archive = encoder.finish().unwrap();
            let digest = format!("{:x}", Sha256::digest(&archive));
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let mut metadata = serde_json::json!({
                "assets": [{
                    "name": "server.gz",
                    "size": archive.len(),
                    "digest": format!("sha256:{digest}"),
                    "browser_download_url": format!("http://{address}/server.gz"),
                }]
            });
            if failure == "missing-digest" {
                metadata["assets"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("digest");
            }
            if failure == "archive-digest" {
                metadata["assets"][0]["digest"] =
                    serde_json::json!(format!("sha256:{}", "0".repeat(64)));
            }
            let server =
                NativeServer::new(serde_json::to_vec(&metadata).unwrap(), archive, listener);
            let directory = tempfile::tempdir().unwrap();
            let store = PackageStore::at(directory.path());
            let package = Package {
                name: PackageName::parse("server").unwrap(),
                description: String::new(),
                languages: Vec::new(),
                categories: Vec::new(),
                source_id: "pkg:github/acme/server@1.2.3".into(),
                assets: vec![Asset {
                    target: PlatformTarget::current().as_str().into(),
                    file: PackagePath::parse("server.gz").unwrap(),
                    bin: Some(PackagePath::parse("server-bin").unwrap()),
                    sha256: if failure == "catalog-digest" {
                        Some(Sha256Digest::parse(&"0".repeat(64)).unwrap())
                    } else {
                        None
                    },
                }],
                bin: BTreeMap::from([(
                    PackageName::parse("server").unwrap(),
                    "{{source.asset.file}}".into(),
                )]),
            };
            let mut app = App::new();
            let (wake, events) = mpsc::sync_channel(256);
            app.insert_resource(vmux_ecs::JobWake(std::task::Waker::from(Arc::new(
                WakeChannel(wake),
            ))))
            .insert_resource(MetadataEndpoint(format!("http://{address}/release")))
            .add_plugins((InstallPlugin, MetadataFixturePlugin));
            let package = app.world_mut().spawn(package).id();
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("lsp.install.fixture").unwrap(),
                    OperationRetained,
                    Request {
                        package,
                        store: store.clone(),
                    },
                ))
                .id();
            Self {
                app,
                directory,
                store,
                operation,
                events,
                _server: server,
            }
        }

        fn finish(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(10);
            self.app.update();
            while self.app.world().get::<Released>(self.operation).is_none() {
                self.events
                    .recv_timeout(Duration::from_secs(2))
                    .expect("LSP installer must wake without periodic input");
                self.app.update();
                assert!(Instant::now() < deadline, "LSP install did not drain");
            }
        }
    }

    #[derive(Clone)]
    struct Held {
        staging: TemporaryDirectory,
        started: mpsc::SyncSender<()>,
        release: Arc<Mutex<mpsc::Receiver<()>>>,
    }

    impl vmux_ecs::Job for Held {
        type Output = ();
        fn run(self) {
            let _staging = self.staging;
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
    }

    #[derive(Resource)]
    struct HoldInput {
        started: mpsc::SyncSender<()>,
        release: Arc<Mutex<mpsc::Receiver<()>>>,
    }

    struct HoldPlugin;

    impl Plugin for HoldPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(JobPlugin::<Held>::default())
                .add_observer(hold);
        }
    }

    fn hold(
        trigger: On<Add, TemporaryDirectory>,
        owners: Query<&TemporaryDirectory, With<Request>>,
        input: Res<HoldInput>,
        mut commands: Commands,
    ) {
        if let Ok(staging) = owners.get(trigger.event_target()) {
            commands.spawn((
                Operation::bundle("lsp.held.fixture").unwrap().running(),
                OperationRetained,
                OperationTarget::new(trigger.event_target()),
                JobRequest(Held {
                    staging: staging.clone(),
                    started: input.started.clone(),
                    release: input.release.clone(),
                }),
            ));
        }
    }

    #[test]
    fn package_installs_through_native_io_activation_linking_and_cleanup_without_periodic_input() {
        let mut fixture = Fixture::new("");
        fixture.finish();
        assert!(
            fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0
                .is_ok()
        );
        let name = PackageName::parse("server").unwrap();
        let command = fixture.store.bin_path(&name).unwrap();
        let output = std::process::Command::new(command).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"lsp-installed");
        let receipt = fixture.store.read_receipt(&name).unwrap();
        assert_eq!(receipt.version.as_deref(), Some("1.2.3"));
        assert_eq!(receipt.bin[&name].as_str(), "server-bin");
        assert_eq!(
            std::fs::read_dir(fixture.store.staging_dir())
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn missing_or_disagreeing_digests_preserve_the_previous_installation() {
        for failure in ["missing-digest", "catalog-digest", "archive-digest"] {
            let mut fixture = Fixture::new(failure);
            let previous = fixture.directory.path().join("packages/server");
            std::fs::create_dir_all(&previous).unwrap();
            std::fs::write(previous.join("sentinel"), b"previous installation").unwrap();
            fixture.finish();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Completed>(fixture.operation)
                    .unwrap()
                    .0
                    .is_err(),
                "{failure}"
            );
            assert_eq!(
                std::fs::read(previous.join("sentinel")).unwrap(),
                b"previous installation",
                "{failure}"
            );
            assert!(!fixture.store.bin_dir().join("server").exists());
            assert_eq!(
                std::fs::read_dir(fixture.store.staging_dir())
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn cancellation_retains_temporary_and_lease_until_native_holders_drain() {
        for interruption in ["cancel", "bin", "asset"] {
            let mut fixture = Fixture::new("");
            let (started, native_started) = mpsc::sync_channel(1);
            let (release, released) = mpsc::channel();
            fixture
                .app
                .insert_resource(HoldInput {
                    started,
                    release: Arc::new(Mutex::new(released)),
                })
                .add_plugins(HoldPlugin);
            let deadline = Instant::now() + Duration::from_secs(5);
            while fixture
                .app
                .world()
                .get::<TemporaryDirectory>(fixture.operation)
                .is_none()
            {
                fixture.app.update();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            fixture.app.update();
            native_started.recv_timeout(Duration::from_secs(2)).unwrap();
            let path = fixture
                .app
                .world()
                .get::<TemporaryDirectory>(fixture.operation)
                .unwrap()
                .path()
                .to_path_buf();
            if interruption == "cancel" {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .insert(OperationState::Canceled);
            } else {
                let package = fixture
                    .app
                    .world()
                    .get::<Request>(fixture.operation)
                    .unwrap()
                    .package;
                let mut package = fixture.app.world_mut().get_mut::<Package>(package).unwrap();
                if interruption == "bin" {
                    package.bin.insert(
                        PackageName::parse("server").unwrap(),
                        "unexpected-command".into(),
                    );
                } else {
                    package.assets[0].sha256 = Some(Sha256Digest::parse(&"0".repeat(64)).unwrap());
                }
            }
            fixture.app.update();
            assert_eq!(
                fixture.app.world().get::<OperationState>(fixture.operation),
                Some(&OperationState::Canceled),
                "{interruption}"
            );
            assert!(path.exists());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Released>(fixture.operation)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_some()
            );
            release.send(()).unwrap();
            fixture.finish();
            assert!(!path.exists());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Completed>(fixture.operation)
                    .unwrap()
                    .0
                    .is_err()
            );
        }
    }

    #[derive(Resource)]
    struct ToolchainEndpoint {
        program: std::path::PathBuf,
        records: std::path::PathBuf,
    }

    struct ToolchainFixturePlugin;

    impl Plugin for ToolchainFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(toolchain_endpoint);
        }
    }

    fn toolchain_endpoint(
        trigger: On<Add, JobRequest<Execute>>,
        requests: Query<&JobRequest<Execute>, With<Toolchain>>,
        endpoint: Res<ToolchainEndpoint>,
        mut commands: Commands,
    ) {
        let Ok(request) = requests.get(trigger.event_target()) else {
            return;
        };
        let mut request = request.0.clone();
        if matches!(request.program.as_str(), "npm" | "cargo" | "go" | "python3") {
            request.program = endpoint.program.to_string_lossy().into_owned();
        }
        request.environment.push((
            "VMUX_FIXTURE_RECORD".into(),
            endpoint.records.to_string_lossy().into_owned(),
        ));
        commands
            .entity(trigger.event_target())
            .insert(JobRequest(request));
    }

    impl Fixture {
        fn toolchain(kind: &str) -> Self {
            let mut fixture = Self::new("");
            let source = fixture
                .app
                .world()
                .get::<Request>(fixture.operation)
                .unwrap()
                .package;
            fixture
                .app
                .world_mut()
                .get_mut::<Package>(source)
                .unwrap()
                .source_id = format!("pkg:{kind}/server@1.2.3");
            let program = fixture.directory.path().join(kind);
            let records = fixture.directory.path().join("invocations.jsonl");
            std::fs::write(
                &program,
                r##"#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
records = pathlib.Path(os.environ["VMUX_FIXTURE_RECORD"])
with records.open("a") as output:
    output.write(json.dumps({"argv": sys.argv[1:], "gobin": os.environ.get("GOBIN")}) + "\n")
kind = pathlib.Path(sys.argv[0]).name
if kind == "npm":
    root = pathlib.Path(sys.argv[sys.argv.index("--prefix") + 1]) / "node_modules/.bin"
elif kind == "cargo":
    root = pathlib.Path(sys.argv[sys.argv.index("--root") + 1]) / "bin"
elif kind == "golang":
    root = pathlib.Path(os.environ["GOBIN"])
elif kind == "pypi":
    root = pathlib.Path(sys.argv[-1]) / "bin"
    root.mkdir(parents=True)
    shutil.copyfile(sys.argv[0], root / "pip")
    (root / "pip").chmod(0o755)
    sys.exit(0)
elif kind == "pip":
    root = pathlib.Path(sys.argv[0]).parent
    if records.with_suffix(".fail").exists():
        sys.exit(7)
else:
    sys.exit(9)
root.mkdir(parents=True, exist_ok=True)
(root / "server").write_text("#!/bin/sh\nprintf toolchain-installed")
"##,
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            fixture
                .app
                .insert_resource(ToolchainEndpoint { program, records })
                .add_plugins(ToolchainFixturePlugin);
            fixture
        }
    }

    #[test]
    fn native_toolchain_invocations_preserve_arguments_environment_and_pypi_order() {
        for kind in ["npm", "cargo", "golang", "pypi"] {
            let mut fixture = Fixture::toolchain(kind);
            fixture.finish();
            let result = &fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0;
            assert!(result.is_ok(), "{kind}: {result:?}");
            let records =
                std::fs::read_to_string(fixture.directory.path().join("invocations.jsonl"))
                    .unwrap();
            let records: Vec<serde_json::Value> = records
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let args = records[0]["argv"].as_array().unwrap();
            match kind {
                "npm" => {
                    assert_eq!(args[0], "install");
                    assert_eq!(args[1], "--prefix");
                    assert!(args[2].as_str().unwrap().ends_with("/package"));
                    assert_eq!(args[3], "server@1.2.3");
                }
                "cargo" => {
                    assert_eq!(args[0], "install");
                    assert_eq!(args[1], "--root");
                    assert!(args[2].as_str().unwrap().ends_with("/package"));
                    assert_eq!(
                        &args[3..],
                        [
                            serde_json::json!("--version"),
                            serde_json::json!("1.2.3"),
                            serde_json::json!("server")
                        ]
                    );
                }
                "golang" => {
                    assert_eq!(
                        args,
                        &[
                            serde_json::json!("install"),
                            serde_json::json!("server@1.2.3")
                        ]
                    );
                    assert!(
                        records[0]["gobin"]
                            .as_str()
                            .unwrap()
                            .ends_with("/package/bin")
                    );
                }
                "pypi" => {
                    assert_eq!(records.len(), 2);
                    assert_eq!(
                        &args[..2],
                        [serde_json::json!("-m"), serde_json::json!("venv")]
                    );
                    assert_eq!(
                        records[1]["argv"],
                        serde_json::json!(["install", "server==1.2.3"])
                    );
                }
                _ => unreachable!(),
            }
            let binary = fixture
                .store
                .bin_path(&PackageName::parse("server").unwrap())
                .unwrap();
            let output = std::process::Command::new(binary).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"toolchain-installed");
            assert_eq!(
                std::fs::read_dir(fixture.store.staging_dir())
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn a_failed_second_pypi_command_preserves_the_previous_package() {
        let mut fixture = Fixture::toolchain("pypi");
        std::fs::write(fixture.directory.path().join("invocations.fail"), "").unwrap();
        let previous = fixture.directory.path().join("packages/server");
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(previous.join("sentinel"), b"previous installation").unwrap();
        fixture.finish();
        assert!(
            fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0
                .is_err()
        );
        assert_eq!(
            std::fs::read(previous.join("sentinel")).unwrap(),
            b"previous installation"
        );
        assert!(!fixture.store.bin_dir().join("server").exists());
    }
}
