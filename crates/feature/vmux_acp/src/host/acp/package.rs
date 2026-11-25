use std::path::PathBuf;

use bevy::prelude::*;
use sha2::{Digest, Sha256};
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget};
use vmux_editor::lsp::archive::ArchiveKind;
use vmux_editor::lsp::download::RemoteArtifact;
use vmux_editor::lsp::package_path::{PackageName, PackagePath, Sha256Digest};
use vmux_editor::lsp::store::{PackageStore, Receipt};
use vmux_job::{JobPlugin, JobRequest, JobResult};
use vmux_tool::io::{Get, InspectLink, ReadFile, WriteFile};

use super::InstallState;
use super::installer_driver::ResolvedAgent;
use super::package_io::{
    CloseStaging, CreateStaging, Download, Exists, Extract, MakeExecutable, Staging,
    StagingTransfer, Unlink, WriteReceipt,
};
use super::package_io::{Remove as RemovePackage, Transferred};
use super::registry::{RegistryAgent, Runtime};
use vmux_editor::lsp::activation::{
    self, Completed as ActivationComplete, Request as Activation, RestoreFailed,
};
#[cfg(test)]
use vmux_editor::lsp::activation_io::{Activate, Backup, Restore};

pub(super) struct PackagePreparationPlugin;

impl Plugin for PackagePreparationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::io::ObservationIoPlugin>() {
            app.add_plugins(vmux_tool::io::ObservationIoPlugin);
        }
        if !app.is_plugin_added::<activation::PackageActivationPlugin>() {
            app.add_plugins(activation::PackageActivationPlugin);
        }
        if !app.is_plugin_added::<JobPlugin<CreateStaging>>() {
            app.add_plugins(JobPlugin::<CreateStaging>::default());
        }
        if !app.is_plugin_added::<JobPlugin<CloseStaging>>() {
            app.add_plugins(JobPlugin::<CloseStaging>::default());
        }
        if !app.is_plugin_added::<JobPlugin<RemovePackage>>() {
            app.add_plugins(JobPlugin::<RemovePackage>::default());
        }
        if !app.is_plugin_added::<JobPlugin<Unlink>>() {
            app.add_plugins(JobPlugin::<Unlink>::default());
        }
        app.add_message::<InspectRequest>()
            .add_plugins((
                JobPlugin::<Exists>::default(),
                JobPlugin::<Download>::default(),
                JobPlugin::<Extract>::default(),
                JobPlugin::<MakeExecutable>::default(),
                JobPlugin::<WriteReceipt>::default(),
            ))
            .add_observer(wake::<Request>)
            .add_observer(wake::<RemoveRequest>)
            .add_observer(retire)
            .add_systems(
                Update,
                (
                    (
                        select,
                        (binary, node, uv),
                        scopes,
                        retry_staging,
                        lease,
                        remove,
                        observe,
                        (files, receipts),
                        removal_links,
                        unlink,
                        unlinked,
                        delete_directory,
                        removed,
                        inspect,
                        checksum,
                    )
                        .chain(),
                    (
                        staging, downloaded, extracted, executable, receipt, receipted,
                    )
                        .chain(),
                    (failed_activation, retry_restore).chain(),
                    (
                        activated,
                        orphan_activation,
                        configured,
                        published,
                        progress,
                        cancel_children,
                    )
                        .chain(),
                )
                    .chain()
                    .in_set(PreparationSet)
                    .before(activation::ActivationSet)
                    .before(vmux_job::JobSet::Submit),
            )
            .add_systems(
                PostUpdate,
                (adopt, retire_staging, close_staging, closed, release)
                    .chain()
                    .after(vmux_job::JobSet::Cleanup),
            );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PreparationSet;

#[derive(Message)]
pub(super) struct InspectRequest {
    pub(super) agents: PathBuf,
    pub(super) id: String,
    pub(super) scopes: Vec<PathBuf>,
}

#[derive(Component, Clone)]
pub(super) struct Request {
    pub(super) agent: RegistryAgent,
    pub(super) version: Option<String>,
    pub(super) store: PackageStore,
}

#[derive(Component, Clone)]
pub(super) struct RemoveRequest {
    pub(super) store: PackageStore,
    pub(super) name: PackageName,
}

#[derive(Component)]
pub(super) struct Removed(pub(super) Result<(), String>);

#[derive(Component)]
struct Scopes(Vec<PathBuf>);

#[derive(Component)]
struct Removing;

#[derive(Component)]
struct RemovalRequested;

#[derive(Component)]
struct LinksPlanned;

#[derive(Component)]
struct BinLink {
    path: PathBuf,
    expected: PathBuf,
}

#[derive(Component)]
struct Unlinking;

#[derive(Component)]
pub(super) struct Prepared(pub(super) Result<ResolvedAgent, String>);

#[derive(Component)]
pub(super) struct Released;

#[derive(Component)]
pub(super) struct ConfigurationPending;

#[derive(Component)]
struct OwnerRetired;

#[derive(Component)]
struct StagingFor(Entity);

#[derive(Component)]
struct RetiringStaging;

#[derive(Component)]
struct ClosingStaging;

#[derive(Component)]
struct CleanupFailure;

#[derive(Component)]
struct RetriedCleanup;

#[derive(Component)]
struct Binary;
#[derive(Component)]
struct Node;
#[derive(Component)]
struct Uv;

#[derive(Component)]
struct Plan {
    name: PackageName,
    url: String,
    checksum_url: Option<String>,
    digest: Option<Sha256Digest>,
    filename: String,
    kind: ArchiveKind,
    command: String,
    expected: Vec<PackagePath>,
    receipt: Receipt,
    launch: ResolvedAgent,
}

impl Plan {
    fn artifact(&self, digest: Sha256Digest) -> RemoteArtifact {
        RemoteArtifact::new(self.url.clone(), digest)
    }
}

#[derive(Component)]
struct AgentReceipt(Receipt);

#[derive(Component)]
struct NpmConfig(PathBuf);

#[derive(Component, Clone)]
pub(super) struct Lease(pub(super) Vec<PathBuf>);

#[derive(Component)]
struct Inspecting;
#[derive(Component)]
struct ReceiptProbe;
#[derive(Component)]
struct FileProbe;
#[derive(Component)]
struct Observed;
#[derive(Component)]
struct ProbeFailure(String);
#[derive(Component)]
struct Inspected;
#[derive(Component)]
struct FileReady(bool);
#[derive(Component)]
struct Checksumming;
#[derive(Component)]
struct StagingRequested;
#[derive(Component)]
struct Downloading;
#[derive(Component)]
struct Extracting;
#[derive(Component)]
struct Executing;
#[derive(Component)]
struct Executables;
#[derive(Component)]
struct ArtifactReceipt;
#[derive(Component)]
struct ArtifactWritten;
#[derive(Component)]
struct Activating(Entity);

#[derive(Component)]
struct RetriedRestore;

#[derive(Component)]
struct Ready;
#[derive(Component)]
struct Configuring;
#[derive(Component)]
struct Publishing;
#[derive(Component)]
struct AgentWriting;

#[derive(Component)]
struct Transfer(crossbeam_channel::Receiver<Transferred>);

fn wake<R: Component>(_: On<Add, R>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if let Some(wake) = wake {
        wake.0.wake_by_ref();
    }
}

fn retire(
    trigger: On<Remove, OperationTarget>,
    requests: Query<(), Or<(With<Request>, With<RemoveRequest>)>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if requests.contains(entity)
        && let Ok(mut entity) = commands.get_entity(entity)
    {
        entity.try_insert((OwnerRetired, OperationState::Canceled));
    }
}

fn adopt(
    transfers: Query<(Entity, &StagingTransfer, Has<JobRequest<CreateStaging>>)>,
    captured: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, transfer, pending) in &transfers {
        if let Some(staging) = transfer.take() {
            commands.spawn((
                Name::new("ACP staging custody"),
                Operation::bundle("acp.package.staging").unwrap().running(),
                OperationRetained,
                staging.clone(),
                StagingFor(entity),
            ));
            commands
                .entity(entity)
                .insert(staging)
                .remove::<StagingTransfer>();
        } else if !pending && !captured.iter().any(|target| target.entity() == entity) {
            commands.entity(entity).remove::<StagingTransfer>();
        } else {
            continue;
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn select(
    requests: Query<
        (Entity, &Request, &OperationState),
        (
            Without<Plan>,
            Without<Binary>,
            Without<Node>,
            Without<Uv>,
            Without<Prepared>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.insert(InstallState::preparing());
        match request.agent.preferred_runtime() {
            Runtime::None => {
                entity.insert(Binary);
            }
            Runtime::Node => {
                entity.insert(Node);
            }
            Runtime::Uv => {
                entity.insert(Uv);
            }
        }
    }
}

fn binary(
    requests: Query<(Entity, &Request), (With<Binary>, Without<Plan>, Without<Prepared>)>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let Some(target) = request.agent.binary_for_host() else {
            commands.entity(entity).insert((
                OperationState::Failed,
                Prepared(Err(format!(
                    "no binary distribution for this platform: {}",
                    request.agent.id
                ))),
            ));
            continue;
        };
        let Some(digest) = &target.sha256 else {
            commands.entity(entity).insert((
                OperationState::Failed,
                Prepared(Err(format!(
                    "ACP registry has no SHA-256 digest for {}",
                    request.agent.id
                ))),
            ));
            continue;
        };
        let name = PackageName::parse(&request.agent.id);
        let filename = target.archive_filename().to_string();
        let validated = name.and_then(|name| {
            PackageName::parse(&filename)?;
            let command = target.command_path(&request.store.package_dir(&name), &filename)?;
            let relative = command
                .strip_prefix(request.store.package_dir(&name))
                .map_err(|error| error.to_string())?;
            let expected = PackagePath::parse(&relative.to_string_lossy())?;
            Ok((name, command, expected))
        });
        let (name, command, expected) = match validated {
            Ok(value) => value,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error))));
                continue;
            }
        };
        let mut identity = Sha256::new();
        for value in [
            request.agent.id.as_str(),
            target.archive.as_str(),
            digest.as_str(),
        ] {
            identity.update((value.len() as u64).to_le_bytes());
            identity.update(value.as_bytes());
        }
        commands.entity(entity).insert(Plan {
            name: name.clone(),
            url: target.archive.clone(),
            checksum_url: None,
            digest: Some(digest.clone()),
            kind: ArchiveKind::for_file(&filename),
            filename,
            command: target.command_basename().to_string(),
            expected: vec![expected],
            receipt: Receipt {
                name,
                version: request.agent.version.clone(),
                source_id: format!(
                    "acp:{}:binary-v3:{:x}",
                    request.agent.id,
                    identity.finalize()
                ),
                bin: Default::default(),
            },
            launch: ResolvedAgent {
                command: command.to_string_lossy().into_owned(),
                args: target.args.clone(),
                env: target
                    .env
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect(),
                path_prepend: None,
                package_added: false,
            },
        });
    }
}

fn node(
    requests: Query<(Entity, &Request), (With<Node>, Without<Plan>, Without<Prepared>)>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "darwin-arm64",
            ("macos", "x86_64") => "darwin-x64",
            ("linux", "aarch64") => "linux-arm64",
            ("linux", "x86_64") => "linux-x64",
            _ => {
                commands.entity(entity).insert((
                    OperationState::Failed,
                    Prepared(Err("managed Node not supported on this platform".into())),
                ));
                continue;
            }
        };
        let Some(distribution) = &request.agent.distribution.npx else {
            continue;
        };
        let dirname = format!("node-v22.11.0-{target}");
        let filename = format!("{dirname}.tar.gz");
        let url = format!("https://nodejs.org/dist/v22.11.0/{filename}");
        let name = PackageName::parse("node").unwrap();
        let directory = request.store.package_dir(&name).join(&dirname);
        let npmrc = request.store.path().join("npmrc");
        let agent = PackageName::parse(&request.agent.id);
        let agent = match agent {
            Ok(agent) => agent,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error))));
                continue;
            }
        };
        commands.entity(entity).insert((
            Plan {
                name: name.clone(),
                url: url.clone(),
                checksum_url: Some("https://nodejs.org/dist/v22.11.0/SHASUMS256.txt".into()),
                digest: None,
                filename,
                kind: ArchiveKind::TarGz,
                command: dirname.clone(),
                expected: vec![
                    PackagePath::parse(&format!("{dirname}/bin/node")).unwrap(),
                    PackagePath::parse(&format!("{dirname}/lib/node_modules/npm/bin/npx-cli.js"))
                        .unwrap(),
                ],
                receipt: Receipt {
                    name,
                    version: Some("22.11.0".into()),
                    source_id: url,
                    bin: Default::default(),
                },
                launch: ResolvedAgent::npx(
                    distribution,
                    request.version.as_deref(),
                    &directory,
                    &npmrc,
                ),
            },
            AgentReceipt(Receipt {
                name: agent,
                version: request
                    .version
                    .clone()
                    .or_else(|| request.agent.version.clone()),
                source_id: format!("acp:{}", request.agent.id),
                bin: Default::default(),
            }),
            NpmConfig(npmrc),
        ));
    }
}

impl ResolvedAgent {
    pub(super) fn npx(
        distribution: &super::registry::PackageDist,
        version: Option<&str>,
        directory: &std::path::Path,
        npmrc: &std::path::Path,
    ) -> Self {
        let mut args = vec![
            directory
                .join("lib/node_modules/npm/bin/npx-cli.js")
                .to_string_lossy()
                .into_owned(),
            format!("--userconfig={}", npmrc.display()),
            "--registry=https://registry.npmjs.org/".into(),
            "--ignore-scripts=false".into(),
            "-y".into(),
            distribution.spec(version),
        ];
        args.extend(distribution.args.iter().cloned());
        let mut env: Vec<_> = distribution
            .env
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        env.extend([
            (
                "NPM_CONFIG_USERCONFIG".into(),
                npmrc.to_string_lossy().into_owned(),
            ),
            (
                "NPM_CONFIG_REGISTRY".into(),
                "https://registry.npmjs.org/".into(),
            ),
            ("NPM_CONFIG_IGNORE_SCRIPTS".into(), "false".into()),
        ]);
        Self {
            command: directory.join("bin/node").to_string_lossy().into_owned(),
            args,
            env,
            path_prepend: Some(directory.join("bin").to_string_lossy().into_owned()),
            package_added: false,
        }
    }
}

fn uv(
    requests: Query<(Entity, &Request), (With<Uv>, Without<Plan>, Without<Prepared>)>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            _ => {
                commands.entity(entity).insert((
                    OperationState::Failed,
                    Prepared(Err("managed uv not supported on this platform".into())),
                ));
                continue;
            }
        };
        let Some(distribution) = &request.agent.distribution.uvx else {
            continue;
        };
        let dirname = format!("uv-{target}");
        let filename = format!("{dirname}.tar.gz");
        let url = format!("https://github.com/astral-sh/uv/releases/download/0.5.11/{filename}");
        let name = PackageName::parse("uv").unwrap();
        let directory = request.store.package_dir(&name).join(&dirname);
        let agent = match PackageName::parse(&request.agent.id) {
            Ok(agent) => agent,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error))));
                continue;
            }
        };
        let mut args = vec![distribution.spec(request.version.as_deref())];
        args.extend(distribution.args.iter().cloned());
        commands.entity(entity).insert((
            Plan {
                name: name.clone(),
                url: url.clone(),
                checksum_url: Some(format!("{url}.sha256")),
                digest: None,
                filename,
                kind: ArchiveKind::TarGz,
                command: dirname.clone(),
                expected: vec![
                    PackagePath::parse(&format!("{dirname}/uvx")).unwrap(),
                    PackagePath::parse(&format!("{dirname}/uv")).unwrap(),
                ],
                receipt: Receipt {
                    name,
                    version: Some("0.5.11".into()),
                    source_id: url,
                    bin: Default::default(),
                },
                launch: ResolvedAgent {
                    command: directory.join("uvx").to_string_lossy().into_owned(),
                    args,
                    env: distribution
                        .env
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect(),
                    path_prepend: Some(directory.to_string_lossy().into_owned()),
                    package_added: false,
                },
            },
            AgentReceipt(Receipt {
                name: agent,
                version: request
                    .version
                    .clone()
                    .or_else(|| request.agent.version.clone()),
                source_id: format!("acp:{}", request.agent.id),
                bin: Default::default(),
            }),
        ));
    }
}

fn scopes(
    installs: Query<(Entity, &Request, &Plan), Without<Scopes>>,
    removals: Query<(Entity, &RemoveRequest), Without<Scopes>>,
    mut commands: Commands,
) {
    for (entity, request, plan) in &installs {
        let agent = match PackageName::parse(&request.agent.id) {
            Ok(name) => name,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error))));
                continue;
            }
        };
        let mut paths = vec![request.store.package_dir(&plan.name)];
        let agent = request.store.package_dir(&agent);
        if !paths.contains(&agent) {
            paths.push(agent);
        }
        commands.entity(entity).insert(Scopes(paths));
    }
    for (entity, request) in &removals {
        commands.entity(entity).insert(Scopes(vec![
            request.store.bin_dir(),
            request.store.package_dir(&request.name),
        ]));
    }
}

fn retry_staging(
    staging: Query<(Entity, &StagingFor), With<CleanupFailure>>,
    owners: Query<&Lease>,
    pending: Query<(Entity, &Scopes, &OperationState), (Without<Lease>, Without<RetriedCleanup>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let mut retried = std::collections::BTreeSet::new();
    for (entity, owner) in &staging {
        let Ok(lease) = owners.get(owner.0) else {
            continue;
        };
        for (requester, scopes, state) in &pending {
            if !matches!(state, OperationState::Pending | OperationState::Running)
                || retried.contains(&requester)
                || !scopes.0.iter().any(|scope| lease.0.contains(scope))
            {
                continue;
            }
            commands
                .entity(entity)
                .remove::<(CleanupFailure, vmux_ecs::OperationError)>()
                .insert(OperationState::Running);
            commands.entity(requester).insert(RetriedCleanup);
            retried.insert(requester);
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
            break;
        }
    }
}

fn lease(
    requests: Query<(Entity, &Scopes, &OperationState), (Without<Lease>, Without<Prepared>)>,
    leases: Query<&Lease>,
    mut commands: Commands,
) {
    let mut occupied = Vec::new();
    for lease in &leases {
        occupied.extend(lease.0.iter().cloned());
    }
    for (entity, scope, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || scope.0.iter().any(|path| occupied.contains(path))
        {
            continue;
        }
        occupied.extend(scope.0.iter().cloned());
        commands
            .entity(entity)
            .insert((Lease(scope.0.clone()), OperationState::Running));
    }
}

fn remove(
    requests: Query<
        (Entity, &RemoveRequest, &OperationState),
        (With<Lease>, Without<RemovalRequested>, Without<Removed>),
    >,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).insert(RemovalRequested);
        commands.spawn((
            ReceiptProbe,
            ChildOf(entity),
            OperationRetained,
            OperationTarget::new(entity),
            Operation::bundle("acp.package.removal.receipt")
                .unwrap()
                .running(),
            JobRequest(ReadFile {
                path: request
                    .store
                    .package_dir(&request.name)
                    .join("vmux-receipt.json"),
                limit: 1024 * 1024,
            }),
        ));
    }
}

fn removal_links(
    requests: Query<
        (Entity, &RemoveRequest, &OperationState),
        (
            With<RemovalRequested>,
            Without<LinksPlanned>,
            Without<Removed>,
        ),
    >,
    receipts: Query<
        (
            &ChildOf,
            Has<Observed>,
            Option<&Receipt>,
            Option<&ProbeFailure>,
        ),
        With<ReceiptProbe>,
    >,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let Some((_, observed, receipt, failure)) = receipts
            .iter()
            .find(|(parent, _, _, _)| parent.parent() == entity)
        else {
            continue;
        };
        if !observed {
            continue;
        }
        if let Some(failure) = failure {
            commands
                .entity(entity)
                .insert((OperationState::Failed, Removed(Err(failure.0.clone()))));
            continue;
        }
        commands.entity(entity).insert(LinksPlanned);
        if let Some(receipt) = receipt {
            for (name, binary) in &receipt.bin {
                let path = request.store.bin_dir().join(name.as_str());
                let expected = request
                    .store
                    .package_dir(&request.name)
                    .join(binary.as_path());
                commands.spawn((
                    Operation::bundle("acp.package.bin.inspect")
                        .unwrap()
                        .running(),
                    OperationRetained,
                    ChildOf(entity),
                    OperationTarget::new(entity),
                    BinLink {
                        path: path.clone(),
                        expected: expected.clone(),
                    },
                    JobRequest(InspectLink {
                        source: expected,
                        target: path,
                    }),
                ));
            }
        }
    }
}

fn unlink(
    links: Query<(Entity, &BinLink, &JobResult<InspectLink>, &OperationState), Without<Observed>>,
    mut commands: Commands,
) {
    for (entity, link, result, state) in &links {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).remove::<JobResult<InspectLink>>();
        let facts = match &result.0 {
            Ok(Ok(facts)) => facts,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert((
                    Observed,
                    ProbeFailure(error.clone()),
                    OperationState::Failed,
                ));
                continue;
            }
        };
        if facts.target_type.is_some_and(|kind| kind.is_symlink())
            && facts.target.as_ref() == Some(&facts.source)
        {
            commands.entity(entity).insert((
                Unlinking,
                JobRequest(Unlink {
                    path: link.path.clone(),
                    expected: link.expected.clone(),
                }),
            ));
        } else {
            commands
                .entity(entity)
                .insert((Observed, OperationState::Succeeded));
        }
    }
}

fn unlinked(
    links: Query<(Entity, &JobResult<Unlink>, &OperationState), With<Unlinking>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &links {
        if *state != OperationState::Running {
            continue;
        }
        if let Ok(Err(error)) | Err(error) = &result.0 {
            commands.entity(entity).insert(ProbeFailure(error.clone()));
        }
        commands
            .entity(entity)
            .remove::<(Unlinking, JobResult<Unlink>)>()
            .insert((Observed, OperationState::Succeeded));
    }
}

fn delete_directory(
    requests: Query<
        (Entity, &RemoveRequest, &OperationState),
        (With<LinksPlanned>, Without<Removing>, Without<Removed>),
    >,
    links: Query<(&ChildOf, Has<Observed>, Option<&ProbeFailure>), With<BinLink>>,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut failure = None;
        for (parent, observed, error) in &links {
            if parent.parent() == entity {
                pending |= !observed;
                if let Some(error) = error {
                    failure = Some(error.0.clone());
                }
            }
        }
        if pending {
            continue;
        }
        if let Some(error) = failure {
            commands
                .entity(entity)
                .insert((OperationState::Failed, Removed(Err(error))));
            continue;
        }
        commands.entity(entity).insert((
            Removing,
            JobRequest(RemovePackage {
                store: request.store.clone(),
                name: request.name.clone(),
            }),
        ));
    }
}

fn cancel_children(
    parents: Query<&OperationState, Or<(With<Request>, With<RemoveRequest>)>>,
    mut children: Query<
        (&ChildOf, &mut OperationState),
        (
            With<Operation>,
            Or<(
                With<ReceiptProbe>,
                With<FileProbe>,
                With<BinLink>,
                With<Executing>,
            )>,
            Without<Request>,
            Without<RemoveRequest>,
        ),
    >,
) {
    for (owner, mut state) in &mut children {
        if parents
            .get(owner.parent())
            .is_ok_and(|parent| matches!(parent, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        if matches!(*state, OperationState::Pending | OperationState::Running) {
            *state = OperationState::Canceled;
        }
    }
}

fn removed(
    requests: Query<(Entity, &JobResult<RemovePackage>, &OperationState), With<Removing>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).remove::<JobResult<RemovePackage>>();
        match &result.0 {
            Ok(Ok(())) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Succeeded, Removed(Ok(()))));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Removed(Err(error.clone()))));
            }
        }
    }
}

fn observe(
    requests: Query<
        (Entity, &Request, &Plan, &OperationState),
        (With<Lease>, Without<Inspecting>, Without<Prepared>),
    >,
    mut commands: Commands,
) {
    for (entity, request, plan, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).insert(Inspecting);
        let agent = match PackageName::parse(&request.agent.id) {
            Ok(agent) => agent,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error))));
                continue;
            }
        };
        commands.spawn((
            ReceiptProbe,
            ChildOf(entity),
            OperationRetained,
            OperationTarget::new(entity),
            Operation::bundle("acp.package.receipt.read")
                .unwrap()
                .running(),
            JobRequest(ReadFile {
                path: request.store.package_dir(&agent).join("vmux-receipt.json"),
                limit: 1024 * 1024,
            }),
        ));
        for expected in &plan.expected {
            commands.spawn((
                FileProbe,
                ChildOf(entity),
                OperationRetained,
                OperationTarget::new(entity),
                Operation::bundle("acp.package.file.observe")
                    .unwrap()
                    .running(),
                JobRequest(Exists {
                    path: request
                        .store
                        .package_dir(&plan.name)
                        .join(expected.as_path()),
                }),
            ));
        }
    }
}

fn files(
    requests: Query<(Entity, &JobResult<Exists>, &OperationState), With<FileProbe>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let ready = match &result.0 {
            Ok(Ok(facts)) => facts.0,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(ProbeFailure(error.clone()));
                false
            }
        };
        commands
            .entity(entity)
            .remove::<JobResult<Exists>>()
            .insert((Observed, FileReady(ready), OperationState::Succeeded));
    }
}

fn receipts(
    requests: Query<(Entity, &ChildOf, &JobResult<ReadFile>, &OperationState), With<ReceiptProbe>>,
    parents: Query<&Request>,
    removals: Query<&RemoveRequest>,
    mut commands: Commands,
) {
    for (entity, parent, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let expected = match parents.get(parent.parent()) {
            Ok(request) => Some(request.agent.id.as_str()),
            Err(_) => removals
                .get(parent.parent())
                .ok()
                .map(|request| request.name.as_str()),
        };
        if let Ok(Err(error)) | Err(error) = &result.0 {
            commands.entity(entity).insert(ProbeFailure(error.clone()));
        }
        if let Ok(Ok(bytes)) = &result.0
            && let Some(bytes) = &bytes.0
            && let Ok(receipt) = serde_json::from_slice::<Receipt>(bytes)
            && Some(receipt.name.as_str()) == expected
        {
            commands.entity(entity).insert(receipt);
        }
        commands
            .entity(entity)
            .remove::<JobResult<ReadFile>>()
            .insert((Observed, OperationState::Succeeded));
    }
}

type InspectionRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static mut Plan,
        &'static OperationState,
        Has<Binary>,
    ),
    (With<Inspecting>, Without<Inspected>, Without<Prepared>),
>;

fn inspect(
    mut requests: InspectionRequests,
    probes: Query<(
        &ChildOf,
        Has<Observed>,
        Option<&Receipt>,
        Option<&FileReady>,
        Option<&ProbeFailure>,
    )>,
    mut commands: Commands,
) {
    for (entity, request, mut plan, state, binary) in &mut requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut ready = true;
        let mut agent_receipt = None;
        let mut error = None;
        for (parent, observed, receipt, file, failure) in &probes {
            if parent.parent() != entity {
                continue;
            }
            pending |= !observed;
            if let Some(file) = file {
                ready &= file.0;
            }
            if let Some(receipt) = receipt {
                agent_receipt = Some(receipt);
            }
            if let Some(failure) = failure {
                error = Some(failure.0.clone());
            }
        }
        if pending {
            continue;
        }
        commands.entity(entity).insert(Inspected);
        if let Some(error) = error {
            commands
                .entity(entity)
                .insert((OperationState::Failed, Prepared(Err(error))));
            continue;
        }
        plan.launch.package_added = agent_receipt.is_none() || !binary && !ready;
        let installed = ready
            && (!binary
                || agent_receipt.is_some_and(|receipt| {
                    receipt.version == request.agent.version
                        && receipt.source_id == plan.receipt.source_id
                }));
        if installed {
            commands.entity(entity).insert(Ready);
            continue;
        }
        if let Some(url) = &plan.checksum_url {
            commands.entity(entity).insert((
                Checksumming,
                JobRequest(Get {
                    url: url.clone(),
                    limit: 1024 * 1024,
                    authorization: None,
                }),
            ));
        } else if plan.digest.is_some() {
            let transfer = StagingTransfer::default();
            commands.entity(entity).insert((
                transfer.clone(),
                StagingRequested,
                JobRequest(CreateStaging {
                    root: request.store.staging_dir(),
                    transfer,
                }),
            ));
        } else {
            commands.entity(entity).insert((
                OperationState::Failed,
                Prepared(Err(format!(
                    "ACP registry has no SHA-256 digest for {}",
                    request.agent.id
                ))),
            ));
        }
    }
}

fn checksum(
    mut requests: Query<
        (
            Entity,
            &Request,
            &mut Plan,
            &JobResult<Get>,
            &OperationState,
        ),
        With<Checksumming>,
    >,
    mut commands: Commands,
) {
    for (entity, request, mut plan, result, state) in &mut requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Checksumming, JobResult<Get>)>();
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_deref(),
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
                continue;
            }
        };
        let text = bytes.and_then(|bytes| std::str::from_utf8(bytes).ok());
        let mut digest = None;
        if let Some(text) = text {
            for line in text.lines() {
                let mut fields = line.split_whitespace();
                let Some(value) = fields.next() else {
                    continue;
                };
                let Some(filename) = fields.next() else {
                    continue;
                };
                if filename.trim_start_matches('*') == plan.filename {
                    digest = Sha256Digest::parse(value).ok();
                    break;
                }
            }
        }
        let Some(digest) = digest else {
            commands.entity(entity).insert((
                OperationState::Failed,
                Prepared(Err(format!("checksum not found for {}", plan.filename))),
            ));
            continue;
        };
        plan.digest = Some(digest);
        let transfer = StagingTransfer::default();
        commands.entity(entity).insert((
            transfer.clone(),
            StagingRequested,
            JobRequest(CreateStaging {
                root: request.store.staging_dir(),
                transfer,
            }),
        ));
    }
}

fn staging(
    requests: Query<
        (
            Entity,
            &Plan,
            Option<&Staging>,
            &JobResult<CreateStaging>,
            &OperationState,
        ),
        With<StagingRequested>,
    >,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, plan, staging, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(())) => {}
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(StagingRequested, JobResult<CreateStaging>)>()
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
                continue;
            }
        }
        let Some(staging) = staging else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(StagingRequested, JobResult<CreateStaging>)>();
        let Some(digest) = &plan.digest else {
            commands.entity(entity).insert((
                OperationState::Failed,
                Prepared(Err("ACP package checksum is unavailable".into())),
            ));
            continue;
        };
        let (sender, receiver) = crossbeam_channel::bounded(64);
        commands.entity(entity).insert((
            staging.clone(),
            Transfer(receiver),
            Downloading,
            JobRequest(Download {
                artifact: plan.artifact(digest.clone()),
                staging: staging.clone(),
                filename: plan.filename.clone(),
                progress: sender,
                wake: wake.as_ref().map(|wake| wake.0.clone()),
            }),
        ));
    }
}

fn downloaded(
    requests: Query<
        (
            Entity,
            &Plan,
            &Staging,
            &JobResult<Download>,
            &OperationState,
        ),
        With<Downloading>,
    >,
    mut commands: Commands,
) {
    for (entity, plan, staging, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Downloading, JobResult<Download>)>();
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert((
                    Extracting,
                    JobRequest(Extract {
                        staging: staging.clone(),
                        filename: plan.filename.clone(),
                        kind: plan.kind,
                        command: plan.command.clone(),
                    }),
                ));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
            }
        }
    }
}

fn extracted(
    requests: Query<
        (
            Entity,
            &Plan,
            &Staging,
            &JobResult<Extract>,
            &OperationState,
        ),
        With<Extracting>,
    >,
    mut commands: Commands,
) {
    for (entity, plan, staging, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Extracting, JobResult<Extract>)>();
        match &result.0 {
            Ok(Ok(())) => {}
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
                continue;
            }
        }
        for expected in &plan.expected {
            commands.spawn((
                Executing,
                ChildOf(entity),
                OperationRetained,
                OperationTarget::new(entity),
                Operation::bundle("acp.package.executable")
                    .unwrap()
                    .running(),
                JobRequest(MakeExecutable {
                    staging: staging.clone(),
                    relative: expected.as_path().to_path_buf(),
                }),
            ));
        }
        commands.entity(entity).insert(Executables);
    }
}

fn executable(
    tasks: Query<(Entity, &JobResult<MakeExecutable>, &OperationState), With<Executing>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &tasks {
        if *state != OperationState::Running {
            continue;
        }
        if let Ok(Err(error)) | Err(error) = &result.0 {
            commands.entity(entity).insert(ProbeFailure(error.clone()));
        }
        commands
            .entity(entity)
            .remove::<JobResult<MakeExecutable>>()
            .insert((Observed, OperationState::Succeeded));
    }
}

fn receipt(
    requests: Query<
        (Entity, &Plan, &Staging, &OperationState),
        (
            With<Executables>,
            Without<ArtifactWritten>,
            Without<ArtifactReceipt>,
            Without<Prepared>,
        ),
    >,
    probes: Query<(&ChildOf, Has<Observed>, Option<&ProbeFailure>), With<Executing>>,
    mut commands: Commands,
) {
    for (entity, plan, staging, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut failure = None;
        for (parent, observed, error) in &probes {
            if parent.parent() != entity {
                continue;
            }
            pending |= !observed;
            if let Some(error) = error {
                failure = Some(error.0.clone());
            }
        }
        if pending {
            continue;
        }
        if let Some(error) = failure {
            commands
                .entity(entity)
                .insert((OperationState::Failed, Prepared(Err(error))));
            continue;
        }
        commands.entity(entity).insert((
            ArtifactReceipt,
            JobRequest(WriteReceipt {
                directory: staging.path().join("package"),
                receipt: plan.receipt.clone(),
                staging: Some(staging.clone()),
            }),
        ));
    }
}

fn receipted(
    requests: Query<
        (
            Entity,
            &Request,
            &Plan,
            &Staging,
            &JobResult<WriteReceipt>,
            &OperationState,
        ),
        With<ArtifactReceipt>,
    >,
    mut commands: Commands,
) {
    for (entity, request, plan, staging, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(ArtifactReceipt, JobResult<WriteReceipt>)>()
            .insert(ArtifactWritten);
        match &result.0 {
            Ok(Ok(())) => {
                let activation = commands
                    .spawn((
                        Name::new("ACP package activation"),
                        Operation::bundle("acp.package.activation")
                            .unwrap()
                            .running(),
                        OperationRetained,
                        Activation {
                            owner: entity,
                            store: request.store.clone(),
                            name: plan.name.clone(),
                            backup: staging
                                .path()
                                .join(format!("previous-{}", uuid::Uuid::new_v4())),
                        },
                        staging.clone(),
                    ))
                    .id();
                commands.entity(entity).insert(Activating(activation));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
            }
        }
    }
}

fn failed_activation(
    requests: Query<(Entity, &Activating, &OperationState)>,
    failed: Query<&vmux_ecs::OperationError, With<RestoreFailed>>,
    mut commands: Commands,
) {
    for (entity, activation, state) in &requests {
        let Ok(error) = failed.get(activation.0) else {
            continue;
        };
        commands
            .entity(entity)
            .insert(Prepared(Err(error.message().into())));
        if *state != OperationState::Canceled {
            commands.entity(entity).insert(OperationState::Failed);
        }
    }
}

fn retry_restore(
    activations: Query<(Entity, &Activation), With<RestoreFailed>>,
    pending: Query<(Entity, &Scopes, &OperationState), (Without<Lease>, Without<RetriedRestore>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut requests: MessageWriter<activation::Retry>,
    mut commands: Commands,
) {
    let mut retried = std::collections::BTreeSet::new();
    for (entity, activation) in &activations {
        let target = activation.store.package_dir(&activation.name);
        for (requester, scopes, state) in &pending {
            if !matches!(state, OperationState::Pending | OperationState::Running)
                || retried.contains(&requester)
                || !scopes.0.contains(&target)
            {
                continue;
            }
            requests.write(activation::Retry { entity });
            commands.entity(requester).insert(RetriedRestore);
            retried.insert(requester);
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
            break;
        }
    }
}

fn activated(
    requests: Query<(Entity, &Activating, &OperationState)>,
    activations: Query<&ActivationComplete>,
    mut commands: Commands,
) {
    for (entity, activation, state) in &requests {
        let Ok(result) = activations.get(activation.0) else {
            continue;
        };
        commands.entity(activation.0).despawn();
        commands.entity(entity).remove::<Activating>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(Ready);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
            }
        }
    }
}

fn orphan_activation(
    activations: Query<(Entity, &Activation), With<ActivationComplete>>,
    owners: Query<&Activating>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, activation) in &activations {
        if !owners
            .get(activation.owner)
            .is_ok_and(|owner| owner.0 == entity)
        {
            commands.entity(entity).despawn();
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

fn configured(
    requests: Query<
        (Entity, Option<&NpmConfig>, &OperationState),
        (
            With<Ready>,
            Without<Configuring>,
            Without<Publishing>,
            Without<Prepared>,
        ),
    >,
    completions: Query<(Entity, &JobResult<WriteFile>, &OperationState), With<Configuring>>,
    mut commands: Commands,
) {
    for (entity, config, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(config) = config {
            commands.entity(entity).insert((
                Configuring,
                JobRequest(WriteFile {
                    path: config.0.clone(),
                    bytes: b"registry=https://registry.npmjs.org/\nignore-scripts=false\n".to_vec(),
                }),
            ));
        } else {
            commands.entity(entity).insert(Publishing);
        }
    }
    for (entity, result, state) in &completions {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).remove::<JobResult<WriteFile>>();
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert(Publishing);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
            }
        }
    }
}

fn published(
    requests: Query<
        (
            Entity,
            &Request,
            &Plan,
            Option<&AgentReceipt>,
            &OperationState,
        ),
        (With<Publishing>, Without<AgentWriting>, Without<Prepared>),
    >,
    completed: Query<
        (Entity, &Plan, &JobResult<WriteReceipt>, &OperationState),
        (With<AgentWriting>, Without<ArtifactReceipt>),
    >,
    mut commands: Commands,
) {
    for (entity, request, plan, receipt, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(receipt) = receipt {
            commands.entity(entity).insert((
                AgentWriting,
                JobRequest(WriteReceipt {
                    directory: request.store.package_dir(&receipt.0.name),
                    receipt: receipt.0.clone(),
                    staging: None,
                }),
            ));
        } else {
            commands
                .entity(entity)
                .insert((OperationState::Succeeded, Prepared(Ok(plan.launch.clone()))));
        }
    }
    for (entity, plan, result, state) in &completed {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).remove::<JobResult<WriteReceipt>>();
        match &result.0 {
            Ok(Ok(())) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Succeeded, Prepared(Ok(plan.launch.clone()))));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Failed, Prepared(Err(error.clone()))));
            }
        }
    }
}

fn retire_staging(
    staging: Query<(Entity, &StagingFor), Without<RetiringStaging>>,
    owners: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, owner) in &staging {
        if owners
            .get(owner.0)
            .is_ok_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        if let Ok(mut owner) = commands.get_entity(owner.0) {
            owner.remove::<Staging>();
        }
        commands.entity(entity).insert(RetiringStaging);
    }
}

fn close_staging(
    staging: Query<
        (Entity, &Staging),
        (
            With<RetiringStaging>,
            Without<ClosingStaging>,
            Without<CleanupFailure>,
        ),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, staging) in &staging {
        if staging.exclusive() {
            commands.entity(entity).insert((
                ClosingStaging,
                JobRequest(CloseStaging {
                    staging: staging.clone(),
                }),
            ));
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

fn closed(
    staging: Query<(Entity, &StagingFor, &JobResult<CloseStaging>), With<ClosingStaging>>,
    owners: Query<&OperationState>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &staging {
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).despawn();
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .remove::<(ClosingStaging, JobResult<CloseStaging>)>()
                    .insert((
                        CleanupFailure,
                        OperationState::Failed,
                        vmux_ecs::OperationError::new(format!(
                            "ACP staging cleanup failed: {error}"
                        ))
                        .unwrap(),
                    ));
                if let Ok(state) = owners.get(owner.0)
                    && let Ok(mut parent) = commands.get_entity(owner.0)
                {
                    parent.insert(Prepared(Err(format!(
                        "ACP staging cleanup failed: {error}"
                    ))));
                    if *state != OperationState::Canceled {
                        parent.insert(OperationState::Failed);
                    }
                }
            }
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn release(
    requests: Query<
        (
            Entity,
            &OperationState,
            Option<&Children>,
            Has<OwnerRetired>,
            Has<RemoveRequest>,
            Has<ConfigurationPending>,
            Has<StagingTransfer>,
        ),
        (Or<(With<Request>, With<RemoveRequest>)>, Without<Released>),
    >,
    inputs: Query<(Option<&Request>, Option<&RemoveRequest>, Option<&Scopes>)>,
    mut inspections: MessageWriter<InspectRequest>,
    captured: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    staging: Query<&StagingFor>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, state, children, retired, removal, configuration, transferring) in &requests {
        if transferring || staging.iter().any(|owner| owner.0 == entity) {
            continue;
        }
        if !matches!(
            state,
            OperationState::Succeeded | OperationState::Failed | OperationState::Canceled
        ) || captured.iter().any(|target| {
            target.entity() == entity
                || children.is_some_and(|children| children.contains(&target.entity()))
        }) {
            continue;
        }
        if let Ok((request, remove, scopes)) = inputs.get(entity) {
            let input = match (request, remove) {
                (Some(request), _) => {
                    Some((request.store.path().to_path_buf(), request.agent.id.clone()))
                }
                (_, Some(remove)) => Some((
                    remove.store.path().to_path_buf(),
                    remove.name.as_str().to_string(),
                )),
                _ => None,
            };
            if let Some((agents, id)) = input {
                inspections.write(InspectRequest {
                    agents,
                    id,
                    scopes: scopes.map(|scopes| scopes.0.clone()).unwrap_or_default(),
                });
            }
        }
        if retired {
            commands.entity(entity).despawn();
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
            continue;
        }
        let mut operation = commands.entity(entity);
        operation.remove::<(Staging, Transfer)>().insert(Released);
        if !configuration {
            operation.remove::<Lease>();
        }
        if *state == OperationState::Canceled {
            if removal {
                operation.insert(Removed(Err("ACP package removal canceled".into())));
            } else {
                operation.insert(Prepared(Err("ACP package preparation canceled".into())));
            }
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

type ProgressRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut InstallState,
        Option<&'static Transfer>,
        Has<Downloading>,
        Has<Extracting>,
        Has<Executables>,
        Has<Node>,
        Has<Uv>,
        Option<&'static Prepared>,
    ),
    With<Request>,
>;

fn progress(mut requests: ProgressRequests, wake: Option<Res<vmux_ecs::JobWake>>) {
    for (_, mut state, transfer, downloading, extracting, executables, node, uv, prepared) in
        &mut requests
    {
        let next = if prepared.is_some_and(|prepared| prepared.0.is_ok()) {
            InstallState::from_phase(vmux_ecs::event::InstallPhase::Done, Some(100), "ready")
        } else if downloading {
            let mut latest = None;
            if let Some(transfer) = transfer {
                for progress in transfer.0.try_iter() {
                    latest = Some(progress);
                }
            }
            let percent = latest
                .and_then(|progress| {
                    progress
                        .total
                        .filter(|total| *total > 0)
                        .map(|total| ((progress.bytes.saturating_mul(100) / total).min(100)) as u8)
                })
                .or(state.pct);
            let message = if node {
                "downloading Node runtime"
            } else if uv {
                "downloading uv runtime"
            } else {
                "downloading"
            };
            InstallState::from_phase(vmux_ecs::event::InstallPhase::Downloading, percent, message)
        } else if extracting || executables {
            let message = if node {
                "extracting Node runtime"
            } else if uv {
                "extracting uv runtime"
            } else {
                "extracting"
            };
            InstallState::from_phase(vmux_ecs::event::InstallPhase::Extracting, None, message)
        } else {
            continue;
        };
        if *state != next {
            *state = next;
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::registry::{BinaryTarget, Distribution, PackageDist};
    use super::*;
    use bevy::prelude::{
        Add, App, Commands, Entity, Name, On, Plugin, Query, Res, ResMut, Resource, With,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[derive(Clone)]
    struct Held {
        started: std::sync::mpsc::Sender<()>,
        release: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
        staging: Staging,
    }

    impl vmux_job::Job for Held {
        type Output = ();

        fn run(self) -> Self::Output {
            let _staging = self.staging;
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        operation: Entity,
    }

    #[derive(Resource)]
    struct ActivationFault(bool);

    struct WakeChannel(std::sync::mpsc::SyncSender<()>);

    impl std::task::Wake for WakeChannel {
        fn wake(self: std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    struct ActivationFaultPlugin;

    impl Plugin for ActivationFaultPlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(cancel_after_backup)
                .add_observer(refuse_activation);
        }
    }

    fn cancel_after_backup(
        trigger: On<Add, JobResult<Backup>>,
        activations: Query<(&Activation, &JobResult<Backup>)>,
        fault: Res<ActivationFault>,
        mut commands: Commands,
    ) {
        if !fault.0 {
            return;
        }
        let Ok((activation, JobResult(Ok(Ok(moved))))) = activations.get(trigger.event_target())
        else {
            return;
        };
        if moved.0 {
            commands
                .entity(activation.owner)
                .insert(OperationState::Canceled);
        }
    }

    fn refuse_activation(
        trigger: On<Add, JobRequest<Activate>>,
        fault: Res<ActivationFault>,
        mut commands: Commands,
    ) {
        if fault.0 {
            return;
        }
        commands
            .entity(trigger.event_target())
            .remove::<JobRequest<Activate>>()
            .insert(JobResult::<Activate>(Err(
                "fixture activation refused".into()
            )));
    }

    #[derive(Resource)]
    struct RestoreObstruction(bool);

    struct RestoreFaultPlugin;

    impl Plugin for RestoreFaultPlugin {
        fn build(&self, app: &mut App) {
            if !app.is_plugin_added::<vmux_job::JobPlugin<Restore>>() {
                app.add_plugins(vmux_job::JobPlugin::<Restore>::default());
            }
            app.add_observer(obstruct_restore);
        }
    }

    fn obstruct_restore(
        trigger: On<Add, JobRequest<Restore>>,
        requests: Query<&JobRequest<Restore>>,
        mut fault: ResMut<RestoreObstruction>,
    ) {
        if !fault.0 {
            return;
        }
        let request = requests.get(trigger.event_target()).unwrap();
        std::fs::write(&request.0.target, b"external replacement").unwrap();
        fault.0 = false;
    }

    impl Fixture {
        fn binary(url: String) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(PackagePreparationPlugin);
            let agent = RegistryAgent {
                id: "agent".into(), name: "Agent".into(), version: Some("1".into()), description: None, icon: None,
                distribution: Distribution {
                    binary: Some([(RegistryAgent::host_target().unwrap().into(), BinaryTarget {
                        archive: url, cmd: "agent".into(), sha256: Some(Sha256Digest::parse("ed16a0c68a7df1e55597fcb7c884140ce292def6116cbaab1fc05045433494b9").unwrap()), args: vec!["--acp".into()], env: Default::default(),
                    })].into_iter().collect()),
                    npx: None, uvx: None,
                },
            };
            let operation = app
                .world_mut()
                .spawn((
                    Name::new("Test package"),
                    Operation::bundle("test.acp.package").unwrap(),
                    OperationRetained,
                    Request {
                        agent,
                        version: None,
                        store: PackageStore::at(directory.path()),
                    },
                ))
                .id();
            Self {
                app,
                directory,
                operation,
            }
        }

        fn notifications(&mut self) -> std::sync::mpsc::Receiver<()> {
            let (wake, events) = std::sync::mpsc::sync_channel(64);
            self.app
                .insert_resource(vmux_ecs::JobWake(std::task::Waker::from(
                    std::sync::Arc::new(WakeChannel(wake)),
                )));
            events
        }

        fn finish(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while self.app.world().get::<Released>(self.operation).is_none()
                && self.app.world().get_entity(self.operation).is_ok()
            {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "ACP preparation did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    fn unexpected_checksum(trigger: On<Add, JobRequest<Get>>, mut commands: Commands) {
        commands
            .entity(trigger.event_target())
            .remove::<JobRequest<Get>>()
            .insert(JobResult::<Get>(Err("unexpected runtime download".into())));
    }

    struct ChecksumFixturePlugin;

    impl Plugin for ChecksumFixturePlugin {
        fn build(&self, app: &mut App) {
            if !app.is_plugin_added::<vmux_job::JobPlugin<Get>>() {
                app.add_plugins(vmux_job::JobPlugin::<Get>::default());
            }
            app.add_observer(unexpected_checksum);
        }
    }

    #[test]
    fn activation_failure_or_cancellation_restores_the_previous_package() {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
            });
            let mut fixture = Fixture::binary(format!("http://{address}/agent"));
            fixture
                .app
                .insert_resource(ActivationFault(cancel))
                .add_plugins(ActivationFaultPlugin);
            let previous = fixture.directory.path().join("packages/agent");
            std::fs::create_dir_all(&previous).unwrap();
            std::fs::write(previous.join("sentinel"), b"previous package").unwrap();
            fixture.finish();
            server.join().unwrap();
            assert_eq!(
                std::fs::read(previous.join("sentinel")).unwrap(),
                b"previous package"
            );
            assert!(!previous.join("agent").exists());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Prepared>(fixture.operation)
                    .unwrap()
                    .0
                    .is_err()
            );
            assert_eq!(
                fixture.app.world().get::<OperationState>(fixture.operation),
                Some(&if cancel {
                    OperationState::Canceled
                } else {
                    OperationState::Failed
                })
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
            let world = fixture.app.world_mut();
            assert!(
                world
                    .query::<&StagingFor>()
                    .iter(world)
                    .all(|owner| owner.0 != fixture.operation)
            );
        }
    }

    #[test]
    fn failed_activation_drains_and_cleans_up_using_native_wakes_only() {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
            });
            let mut fixture = Fixture::binary(format!("http://{address}/agent"));
            fixture
                .app
                .insert_resource(ActivationFault(cancel))
                .add_plugins(ActivationFaultPlugin);
            let previous = fixture.directory.path().join("packages/agent");
            std::fs::create_dir_all(&previous).unwrap();
            std::fs::write(previous.join("sentinel"), b"previous package").unwrap();
            let events = fixture.notifications();
            fixture.app.update();
            while fixture
                .app
                .world()
                .get::<Released>(fixture.operation)
                .is_none()
            {
                events
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .expect("cleanup must request its next frame without another input or timer");
                while events.try_recv().is_ok() {}
                fixture.app.update();
            }
            server.join().unwrap();
            assert_eq!(
                std::fs::read(previous.join("sentinel")).unwrap(),
                b"previous package"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_none()
            );
        }
    }

    #[test]
    fn refused_restore_retains_the_backup_and_lease_until_a_new_request_retries() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
        });
        let mut fixture = Fixture::binary(format!("http://{address}/agent"));
        fixture
            .app
            .insert_resource(ActivationFault(false))
            .insert_resource(RestoreObstruction(true))
            .add_plugins((ActivationFaultPlugin, RestoreFaultPlugin));
        let target = fixture.directory.path().join("packages/agent");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("sentinel"), b"previous package").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let activation = loop {
            fixture.app.update();
            let world = fixture.app.world_mut();
            if let Some(entity) = world
                .query_filtered::<Entity, With<RestoreFailed>>()
                .iter(world)
                .next()
            {
                break entity;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "restore was not refused"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        server.join().unwrap();
        let backup = fixture
            .app
            .world()
            .get::<Activation>(activation)
            .unwrap()
            .backup
            .clone();
        assert_eq!(std::fs::read(&target).unwrap(), b"external replacement");
        assert_eq!(
            std::fs::read(backup.join("sentinel")).unwrap(),
            b"previous package"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Released>(fixture.operation)
                .is_none()
        );
        std::fs::remove_file(&target).unwrap();
        let request = fixture
            .app
            .world()
            .get::<Request>(fixture.operation)
            .unwrap()
            .clone();
        let next = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.activation.retry").unwrap(),
                OperationRetained,
                request,
            ))
            .id();
        while fixture.app.world().get::<Released>(next).is_none() {
            fixture.app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "new accepted work did not retry restore"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            std::fs::read(target.join("sentinel")).unwrap(),
            b"previous package"
        );
        assert!(!backup.exists());
        assert!(
            fixture
                .app
                .world()
                .get::<Released>(fixture.operation)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_none()
        );
    }

    #[test]
    fn staging_cleanup_waits_for_the_long_lane_and_keeps_the_package_lease() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
        });
        let mut fixture = Fixture::binary(format!("http://{address}/agent"));
        let previous = fixture.directory.path().join("packages/agent");
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(previous.join("old"), b"previous package").unwrap();
        fixture.app.insert_resource(vmux_job::BlockingRuntime::new(
            vmux_job::blocking::BlockingPool::new("cleanup-short", 1, 32),
            vmux_job::blocking::BlockingPool::new("cleanup-long", 1, 8),
            vmux_job::blocking::BlockingPool::new("cleanup-process", 2, 16),
        ));
        let (entered, started) = std::sync::mpsc::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let blocker = fixture
            .app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn_long(None, move || {
                entered.send(()).unwrap();
                wait.recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            })
            .unwrap();
        started
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<Prepared>(fixture.operation)
            .is_none()
        {
            fixture.app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "package did not reach cleanup"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        server.join().unwrap();
        assert!(
            fixture
                .app
                .world()
                .get::<Prepared>(fixture.operation)
                .unwrap()
                .0
                .is_ok()
        );
        let world = fixture.app.world_mut();
        let staging = world
            .query::<(&Staging, &StagingFor)>()
            .iter(world)
            .find(|(_, owner)| owner.0 == fixture.operation)
            .unwrap()
            .0
            .path()
            .to_path_buf();
        assert!(staging.is_dir());
        assert!(
            std::fs::read_dir(&staging).unwrap().any(|entry| entry
                .unwrap()
                .path()
                .join("old")
                .is_file())
        );
        assert!(!previous.join("old").exists());
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Released>(fixture.operation)
                .is_none()
        );
        let probe_path = fixture.directory.path().join("short-probe");
        let output = probe_path.clone();
        let probe = fixture
            .app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn(None, move || std::fs::write(output, b"available").unwrap())
            .unwrap();
        while !probe.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "cleanup blocked short IO"
            );
            std::thread::yield_now();
        }
        assert_eq!(std::fs::read(probe_path).unwrap(), b"available");
        release.send(()).unwrap();
        fixture.finish();
        assert!(!staging.exists());
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_none()
        );
        assert!(blocker.is_finished());
    }

    #[test]
    fn failed_staging_cleanup_keeps_ownership_until_a_new_request_retries_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
        });
        let mut fixture = Fixture::binary(format!("http://{address}/agent"));
        fixture.app.insert_resource(vmux_job::BlockingRuntime::new(
            vmux_job::blocking::BlockingPool::new("failure-short", 1, 32),
            vmux_job::blocking::BlockingPool::new("failure-long", 1, 8),
            vmux_job::blocking::BlockingPool::new("failure-process", 2, 16),
        ));
        let (entered, started) = std::sync::mpsc::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let blocker = fixture
            .app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn_long(None, move || {
                entered.send(()).unwrap();
                wait.recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            })
            .unwrap();
        started
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<Prepared>(fixture.operation)
            .is_none()
        {
            fixture.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        server.join().unwrap();
        let world = fixture.app.world_mut();
        let (custody, staging) = world
            .query::<(Entity, &Staging, &StagingFor)>()
            .iter(world)
            .find(|(_, _, owner)| owner.0 == fixture.operation)
            .map(|(entity, staging, _)| (entity, staging.path().to_path_buf()))
            .unwrap();
        std::fs::remove_dir_all(&staging).unwrap();
        std::fs::write(&staging, b"cleanup obstruction").unwrap();
        release.send(()).unwrap();
        while fixture.app.world().get::<CleanupFailure>(custody).is_none() {
            fixture.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        for _ in 0..3 {
            fixture.app.update();
        }
        assert_eq!(std::fs::read(&staging).unwrap(), b"cleanup obstruction");
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_some()
        );
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
                .get::<Prepared>(fixture.operation)
                .unwrap()
                .0
                .is_err()
        );
        std::fs::remove_file(&staging).unwrap();
        let removal = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.cleanup.retry").unwrap(),
                OperationRetained,
                RemoveRequest {
                    store: PackageStore::at(fixture.directory.path()),
                    name: PackageName::parse("agent").unwrap(),
                },
            ))
            .id();
        while fixture.app.world().get::<Released>(removal).is_none() {
            fixture.app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "accepted request did not retry owned cleanup"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(fixture.app.world().get_entity(custody).is_err());
        assert!(
            fixture
                .app
                .world()
                .get::<Released>(fixture.operation)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.operation)
                .is_none()
        );
        assert!(!fixture.directory.path().join("packages/agent").exists());
        assert!(blocker.is_finished());
    }

    #[test]
    fn cancellation_before_staging_adoption_keeps_the_directory_until_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("staging");
        let mut app = App::new();
        app.add_plugins(PackagePreparationPlugin)
            .insert_resource(vmux_job::BlockingRuntime::new(
                vmux_job::blocking::BlockingPool::new("creation-short", 1, 8),
                vmux_job::blocking::BlockingPool::new("creation-long", 1, 8),
                vmux_job::blocking::BlockingPool::new("creation-process", 1, 8),
            ));
        let (short_release, short_wait) = std::sync::mpsc::channel();
        let (short_started, short_entered) = std::sync::mpsc::channel();
        let short = app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn(None, move || {
                short_started.send(()).unwrap();
                short_wait
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            })
            .unwrap();
        short_entered
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let (long_release, long_wait) = std::sync::mpsc::channel();
        let (long_started, long_entered) = std::sync::mpsc::channel();
        let long = app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn_long(None, move || {
                long_started.send(()).unwrap();
                long_wait
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            })
            .unwrap();
        long_entered
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let transfer = StagingTransfer::default();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("test.staging.creation")
                    .unwrap()
                    .running(),
                OperationRetained,
                transfer.clone(),
                JobRequest(CreateStaging {
                    root: root.clone(),
                    transfer,
                }),
            ))
            .id();
        app.update();
        let (finished, finished_wait) = std::sync::mpsc::channel();
        let barrier = app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn(None, move || finished.send(()).unwrap())
            .unwrap();
        short_release.send(()).unwrap();
        finished_wait
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let staging = std::fs::read_dir(&root)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        app.world_mut()
            .entity_mut(operation)
            .insert(OperationState::Canceled);
        app.update();
        app.update();
        assert!(
            staging.is_dir(),
            "late native staging must remain owned until the long lane drains"
        );
        long_release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while staging.exists() {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "late staging cleanup did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(short.is_finished() && long.is_finished() && barrier.is_finished());
    }

    #[test]
    fn removing_a_package_preserves_foreign_or_replaced_bin_entries() {
        for replacement in ["owned", "relative", "foreign", "file"] {
            let directory = tempfile::tempdir().unwrap();
            let store = PackageStore::at(directory.path());
            let foo = PackageName::parse("foo").unwrap();
            let bar = PackageName::parse("bar").unwrap();
            let link = PackageName::parse("shared").unwrap();
            let binary = PackagePath::parse("server").unwrap();
            std::fs::create_dir_all(store.package_dir(&foo)).unwrap();
            std::fs::create_dir_all(store.package_dir(&bar)).unwrap();
            std::fs::write(store.package_dir(&foo).join("server"), b"foo").unwrap();
            std::fs::write(store.package_dir(&bar).join("server"), b"bar").unwrap();
            store
                .write_receipt(
                    &foo,
                    &Receipt {
                        name: foo.clone(),
                        version: Some("1".into()),
                        source_id: "fixture:foo".into(),
                        bin: [(link.clone(), binary.clone())].into_iter().collect(),
                    },
                )
                .unwrap();
            store.link_bin(&foo, &binary, &link).unwrap();
            let path = store.bin_dir().join(link.as_str());
            match replacement {
                "relative" => {
                    std::fs::remove_file(&path).unwrap();
                    std::os::unix::fs::symlink("../packages/foo/server", &path).unwrap();
                }
                "foreign" => store.link_bin(&bar, &binary, &link).unwrap(),
                "file" => {
                    std::fs::remove_file(&path).unwrap();
                    std::fs::write(&path, b"replacement").unwrap();
                }
                "owned" => {}
                _ => unreachable!(),
            }
            let mut app = App::new();
            app.add_plugins(PackagePreparationPlugin);
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("test.package.remove").unwrap(),
                    OperationRetained,
                    RemoveRequest {
                        store: store.clone(),
                        name: foo.clone(),
                    },
                ))
                .id();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while app.world().get::<Released>(operation).is_none() {
                app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "package removal did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(app.world().get::<Removed>(operation).unwrap().0.is_ok());
            assert!(!store.package_dir(&foo).exists());
            assert_eq!(
                std::fs::read(store.package_dir(&bar).join("server")).unwrap(),
                b"bar"
            );
            if matches!(replacement, "owned" | "relative") {
                assert!(
                    path.symlink_metadata().is_err(),
                    "owned link was left dangling"
                );
            } else {
                let expected = if replacement == "foreign" {
                    &b"bar"[..]
                } else {
                    &b"replacement"[..]
                };
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    expected,
                    "{replacement}: another owner's bin entry was deleted"
                );
            }
        }
    }

    #[test]
    fn cached_binary_reuse_is_bound_to_the_current_archive_and_digest() {
        for change in ["url", "digest", "refused", "missing", "unchanged"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let requests = if matches!(change, "url" | "digest" | "refused") {
                2
            } else {
                1
            };
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let mut received = 0;
                while received < requests && std::time::Instant::now() < deadline {
                    let (mut stream, _) = match listener.accept() {
                        Ok(stream) => stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                        .unwrap();
                    let mut request = [0; 1024];
                    assert!(stream.read(&mut request).unwrap() > 0);
                    let bytes = if received == 1 && change == "digest" {
                        &b"new vmux agent"[..]
                    } else {
                        &b"hello vmux lsp"[..]
                    };
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).as_bytes()).unwrap();
                    stream.write_all(bytes).unwrap();
                    received += 1;
                }
                received
            });
            let mut fixture = Fixture::binary(format!("http://{address}/agent"));
            fixture.finish();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Prepared>(fixture.operation)
                    .unwrap()
                    .0
                    .is_ok()
            );
            let receipt_path = fixture
                .directory
                .path()
                .join("packages/agent/vmux-receipt.json");
            let committed_receipt = std::fs::read(&receipt_path).unwrap();
            let mut request = fixture
                .app
                .world()
                .get::<Request>(fixture.operation)
                .unwrap()
                .clone();
            let target = request
                .agent
                .distribution
                .binary
                .as_mut()
                .unwrap()
                .get_mut(RegistryAgent::host_target().unwrap())
                .unwrap();
            match change {
                "url" => target.archive = format!("http://{address}/replacement"),
                "digest" | "refused" => {
                    target.sha256 = Some(
                        Sha256Digest::parse(
                            "f8ddc3f48626137d094471efc6c1cd6f19fc9492450d4665eb3936941beffdd3",
                        )
                        .unwrap(),
                    )
                }
                "missing" => target.sha256 = None,
                "unchanged" => {}
                _ => unreachable!(),
            }
            fixture.operation = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.binary.reuse").unwrap(),
                    OperationRetained,
                    request,
                ))
                .id();
            fixture.finish();
            let received = server.join().unwrap();
            let result = &fixture
                .app
                .world()
                .get::<Prepared>(fixture.operation)
                .unwrap()
                .0;
            assert_eq!(
                result.is_err(),
                matches!(change, "missing" | "refused"),
                "{change}: {result:?}"
            );
            assert_eq!(
                received, requests,
                "{change}: stale archive reused without observation"
            );
            let expected = if change == "digest" {
                &b"new vmux agent"[..]
            } else {
                &b"hello vmux lsp"[..]
            };
            assert_eq!(
                std::fs::read(fixture.directory.path().join("packages/agent/agent")).unwrap(),
                expected,
                "{change}"
            );
            if matches!(change, "missing" | "refused") {
                assert_eq!(
                    std::fs::read(&receipt_path).unwrap(),
                    committed_receipt,
                    "{change}: failed artifact replacement changed committed receipt"
                );
            }
        }
    }

    #[test]
    fn an_existing_node_runtime_publishes_the_pin_only_after_configuration_succeeds() {
        for refuse_config in [false, true] {
            let mut fixture = Fixture::binary("https://example.invalid/agent".into());
            fixture.app.add_plugins(ChecksumFixturePlugin);
            {
                let mut request = fixture
                    .app
                    .world_mut()
                    .get_mut::<Request>(fixture.operation)
                    .unwrap();
                request.agent.distribution = Distribution {
                    binary: None,
                    npx: Some(PackageDist {
                        package: "@acme/agent@1".into(),
                        args: vec!["--acp".into()],
                        env: Default::default(),
                    }),
                    uvx: None,
                };
                request.version = Some("2".into());
            }
            let target = match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => "darwin-arm64",
                ("macos", "x86_64") => "darwin-x64",
                ("linux", "aarch64") => "linux-arm64",
                ("linux", "x86_64") => "linux-x64",
                _ => panic!("native runtime fixture requires a supported host"),
            };
            let node = fixture
                .directory
                .path()
                .join("packages/node")
                .join(format!("node-v22.11.0-{target}"));
            std::fs::create_dir_all(node.join("bin")).unwrap();
            std::fs::create_dir_all(node.join("lib/node_modules/npm/bin")).unwrap();
            std::fs::write(node.join("bin/node"), b"native-node-fixture").unwrap();
            std::fs::write(
                node.join("lib/node_modules/npm/bin/npx-cli.js"),
                b"native-npx-fixture",
            )
            .unwrap();
            let store = PackageStore::at(fixture.directory.path());
            let name = PackageName::parse("agent").unwrap();
            store
                .write_receipt(
                    &name,
                    &Receipt {
                        name: name.clone(),
                        version: Some("1".into()),
                        source_id: "acp:agent".into(),
                        bin: Default::default(),
                    },
                )
                .unwrap();
            if refuse_config {
                std::fs::create_dir(fixture.directory.path().join("npmrc")).unwrap();
            }
            fixture.finish();
            let current = store.read_receipt(&name).unwrap();
            let result = &fixture
                .app
                .world()
                .get::<Prepared>(fixture.operation)
                .unwrap()
                .0;
            if refuse_config {
                assert!(result.is_err());
                assert_eq!(current.version.as_deref(), Some("1"));
            } else {
                let launch = result.as_ref().unwrap();
                assert_eq!(current.version.as_deref(), Some("2"));
                assert!(
                    launch
                        .args
                        .iter()
                        .any(|argument| argument == "@acme/agent@2")
                );
                assert!(!launch.package_added);
                assert!(fixture.directory.path().join("npmrc").is_file());
            }
            assert!(!fixture.directory.path().join("staging").exists());
            assert!(
                store
                    .read_receipt(&PackageName::parse("other-agent").unwrap())
                    .is_none()
            );
        }
    }

    #[test]
    fn native_binary_preparation_verifies_and_activates_the_receipt_before_delivery() {
        for old_receipt in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let received = stream.read(&mut request).unwrap();
                assert!(received > 0);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nhello vmux lsp").unwrap();
            });
            let mut fixture = Fixture::binary(format!("http://{address}/agent"));
            if old_receipt {
                let store = PackageStore::at(fixture.directory.path());
                let name = PackageName::parse("agent").unwrap();
                std::fs::create_dir_all(store.package_dir(&name)).unwrap();
                std::fs::write(store.package_dir(&name).join("agent"), b"legacy binary").unwrap();
                store
                    .write_receipt(
                        &name,
                        &Receipt {
                            name: name.clone(),
                            version: Some("1".into()),
                            source_id: "acp:agent".into(),
                            bin: Default::default(),
                        },
                    )
                    .unwrap();
            }
            fixture.finish();
            server.join().unwrap();
            let root = fixture.directory.path().join("packages/agent");
            assert_eq!(
                std::fs::read(root.join("agent")).unwrap(),
                b"hello vmux lsp"
            );
            let receipt: Receipt =
                serde_json::from_slice(&std::fs::read(root.join("vmux-receipt.json")).unwrap())
                    .unwrap();
            assert_eq!(receipt.version.as_deref(), Some("1"));
            let prepared = fixture
                .app
                .world()
                .get::<Prepared>(fixture.operation)
                .unwrap()
                .0
                .as_ref()
                .unwrap();
            assert_eq!(PathBuf::from(&prepared.command), root.join("agent"));
            assert_eq!(prepared.package_added, !old_receipt);
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
                    .get::<Staging>(fixture.operation)
                    .is_none()
            );
        }
    }

    #[test]
    fn canceled_download_keeps_its_staging_and_lease_until_native_io_drains() {
        for remove_owner in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (started, entered) = std::sync::mpsc::channel();
            let (release, wait) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let received = stream.read(&mut request).unwrap();
                assert!(received > 0);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                started.send(()).unwrap();
                wait.recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
                let _ = stream.write_all(b"hello vmux lsp");
            });
            let mut fixture = Fixture::binary(format!("http://{address}/agent"));
            let owner = fixture.app.world_mut().spawn_empty().id();
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.operation)
                .insert(OperationTarget::new(owner));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                fixture.app.update();
                if entered.try_recv().is_ok() {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "download did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let staging = fixture
                .app
                .world()
                .get::<Staging>(fixture.operation)
                .unwrap()
                .path()
                .to_path_buf();
            let (native_release, native_wait) = std::sync::mpsc::channel();
            let (native_started, native_entered) = std::sync::mpsc::channel();
            fixture.app.add_plugins(JobPlugin::<Held>::default());
            let lease = fixture
                .app
                .world()
                .get::<Staging>(fixture.operation)
                .unwrap()
                .clone();
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.operation)
                .insert(JobRequest(Held {
                    started: native_started,
                    release: std::sync::Arc::new(std::sync::Mutex::new(native_wait)),
                    staging: lease,
                }));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                fixture.app.update();
                if native_entered.try_recv().is_ok() {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native hold did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let removal = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.acp.remove").unwrap(),
                    OperationRetained,
                    RemoveRequest {
                        store: PackageStore::at(fixture.directory.path()),
                        name: PackageName::parse("agent").unwrap(),
                    },
                ))
                .id();
            fixture.app.update();
            assert!(fixture.app.world().get::<Lease>(removal).is_none());
            if remove_owner {
                fixture.app.world_mut().despawn(owner);
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.operation)
                    .insert(OperationState::Canceled);
            }

            fixture.app.update();
            assert!(staging.exists());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Lease>(fixture.operation)
                    .is_some()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Released>(fixture.operation)
                    .is_none()
            );
            release.send(()).unwrap();
            native_release.send(()).unwrap();
            fixture.finish();
            server.join().unwrap();
            assert!(!staging.exists());
            assert!(!fixture.directory.path().join("packages/agent").exists());
            if remove_owner {
                assert!(fixture.app.world().get_entity(fixture.operation).is_err());
            } else {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<Prepared>(fixture.operation)
                        .unwrap()
                        .0
                        .is_err()
                );
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<Lease>(fixture.operation)
                        .is_none()
                );
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fixture.app.world().get::<Released>(removal).is_none() {
                fixture.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "queued removal did not drain"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(fixture.app.world().get::<Lease>(removal).is_none());
        }
    }
}
