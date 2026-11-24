use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use sha2::{Digest, Sha256};
use vmux_ecs::profile::CurrentProfile;
use vmux_ecs::{
    JobRequest, JobResult, Operation, OperationRetained, OperationState, OperationTarget,
};
use vmux_editor::lsp::package_path::PackageName;
use vmux_editor::lsp::store::{PackageStore, Receipt};
use vmux_tool::ToolInventoryChanged;
use vmux_tool::io::ReadFile;

use super::AcpPackageChanged;
use super::installer_driver::ResolvedAgent;
use super::package::{InspectRequest, Lease};
use super::package_io::{Exists, FileExists};
use super::registry::{Catalog, CatalogSource, RegistryAgent, Runtime};

pub(super) struct InstalledPlugin;

impl Plugin for InstalledPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AcpPackageChanged>()
            .add_message::<ToolInventoryChanged>()
            .add_message::<InspectRequest>()
            .add_systems(
                Update,
                (
                    invalidate,
                    queue,
                    lease,
                    receipt,
                    (binary, node, uv),
                    files,
                    publish,
                )
                    .chain()
                    .in_set(InstalledSet)
                    .after(super::package::PreparationSet)
                    .before(vmux_ecs::JobSet::Submit),
            )
            .add_systems(PostUpdate, release.after(vmux_ecs::JobSet::Cleanup));
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::host) struct InstalledSet;

#[derive(Component)]
pub(in crate::host) struct InstalledAgent;

#[derive(Component, Clone)]
pub(in crate::host) struct InstalledLaunch {
    pub(super) agent: ResolvedAgent,
    pub(in crate::host) npmrc: Option<PathBuf>,
}

#[derive(Component, Clone, PartialEq, Eq)]
pub(in crate::host) struct Checked {
    profile: Entity,
    path: PathBuf,
    catalog: Entity,
    revision: u64,
}

impl Checked {
    pub(in crate::host) fn new(
        profile: Entity,
        path: PathBuf,
        catalog: Entity,
        revision: u64,
    ) -> Self {
        Self {
            profile,
            path,
            catalog,
            revision,
        }
    }

    pub(in crate::host) fn profile_matches(
        &self,
        entity: Option<Entity>,
        paths: Option<&vmux_ecs::profile::ProfilePaths>,
    ) -> bool {
        entity == Some(self.profile) && paths.is_some_and(|paths| paths.agents() == self.path)
    }

    pub(super) fn scopes(&self, launch: &InstalledLaunch, name: &PackageName) -> Vec<PathBuf> {
        let mut scopes = vec![self.path.join("packages").join(name.as_str())];
        let runtime = if launch.npmrc.is_some() {
            Some("node")
        } else if launch.agent.path_prepend.is_some() {
            Some("uv")
        } else {
            None
        };
        if let Some(runtime) = runtime {
            let path = self.path.join("packages").join(runtime);
            if !scopes.contains(&path) {
                scopes.push(path);
            }
        }
        scopes
    }
}

#[derive(Component)]
struct Probe {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    source: Checked,
    scopes: Vec<PathBuf>,
}

#[derive(Component)]
struct Selected(RegistryAgent);

#[derive(Component)]
struct Reading;
#[derive(Component)]
struct Binary;
#[derive(Component)]
struct Node;
#[derive(Component)]
struct Uv;
#[derive(Component)]
struct File;
#[derive(Component)]
struct Ready(bool);
#[derive(Component)]
struct Finished;

#[derive(SystemParam)]
struct Context<'w, 's> {
    profile: CurrentProfile<'w, 's>,
    catalogs: Query<'w, 's, (Entity, &'static Catalog, &'static CatalogSource)>,
    policy: Query<'w, 's, &'static crate::policy::AcpRegistryPolicy>,
}

impl Context<'_, '_> {
    fn source(&self) -> Option<Checked> {
        let profile = self.profile.entity()?;
        let paths = self.profile.paths()?;
        let (catalog, state, source) = self.catalogs.single().ok()?;
        let policy = self.policy.single().ok()?;
        if !state.ready
            || source.profile != Some(profile)
            || source.cache != paths.agents().join("registry.json")
            || source.url != policy.url
        {
            return None;
        }
        Some(Checked::new(
            profile,
            paths.agents(),
            catalog,
            state.revision,
        ))
    }
}

fn invalidate(
    context: Context,
    definitions: Query<(Entity, Ref<RegistryAgent>, &ChildOf, Option<&Checked>)>,
    probes: Query<(
        Entity,
        &Probe,
        &Selected,
        Option<&OperationTarget>,
        &OperationState,
    )>,
    mut packages: MessageReader<AcpPackageChanged>,
    mut tools: MessageReader<ToolInventoryChanged>,
    mut inspections: MessageReader<InspectRequest>,
    mut commands: Commands,
) {
    let source = context.source();
    let mut changed = std::collections::BTreeSet::new();
    for package in packages.read() {
        changed.insert(package.agent_id.clone());
    }
    let mut refresh_all = false;
    for tool in tools.read() {
        refresh_all |= tool.0.is("acp");
    }
    for inspection in inspections.read() {
        let Some(source) = &source else {
            continue;
        };
        if inspection.agents != source.path {
            continue;
        }
        changed.insert(inspection.id.clone());
        for (_, agent, _, _) in &definitions {
            let runtime = match agent.preferred_runtime() {
                Runtime::None => None,
                Runtime::Node => Some("node"),
                Runtime::Uv => Some("uv"),
            };
            if let Some(runtime) = runtime
                && inspection
                    .scopes
                    .contains(&source.path.join("packages").join(runtime))
            {
                changed.insert(agent.id.clone());
            }
        }
    }
    for (entity, agent, parent, checked) in &definitions {
        if checked.is_some()
            && (refresh_all
                || changed.contains(&agent.id)
                || agent.is_changed()
                || source.as_ref() != checked
                || source
                    .as_ref()
                    .is_none_or(|source| parent.parent() != source.catalog))
        {
            commands
                .entity(entity)
                .remove::<(Checked, InstalledAgent, InstalledLaunch, Receipt)>();
        }
    }
    for (entity, probe, selected, target, state) in &probes {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let valid = target.is_some_and(|target| {
            definitions
                .get(target.entity())
                .is_ok_and(|(_, agent, parent, _)| {
                    !agent.is_changed() && parent.parent() == probe.source.catalog
                })
        });
        if refresh_all
            || changed.contains(&selected.0.id)
            || source.as_ref() != Some(&probe.source)
            || !valid
        {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn queue(
    context: Context,
    definitions: Query<(Entity, &RegistryAgent, &ChildOf), Without<Checked>>,
    probes: Query<&OperationTarget, With<Probe>>,
    mut commands: Commands,
) {
    let Some(source) = context.source() else {
        return;
    };
    let store = PackageStore::at(source.path.clone());
    for (entity, agent, parent) in &definitions {
        if parent.parent() != source.catalog
            || probes.iter().any(|target| target.entity() == entity)
        {
            continue;
        }
        let Ok(name) = PackageName::parse(&agent.id) else {
            commands.entity(entity).insert(source.clone());
            continue;
        };
        let mut scopes = vec![store.package_dir(&name)];
        let runtime = match agent.preferred_runtime() {
            Runtime::None => None,
            Runtime::Node => Some("node"),
            Runtime::Uv => Some("uv"),
        };
        if let Some(runtime) = runtime {
            let path = store.package_dir(&PackageName::parse(runtime).unwrap());
            if !scopes.contains(&path) {
                scopes.push(path);
            }
        }
        commands.spawn((
            Name::new(format!("Observe installed ACP {}", agent.id)),
            Operation::bundle("acp.installed").unwrap(),
            OperationRetained,
            OperationTarget::new(entity),
            Selected(agent.clone()),
            Probe {
                source: source.clone(),
                scopes,
            },
        ));
    }
}

fn lease(
    probes: Query<
        (Entity, &Probe, &Selected, &OperationState),
        (Without<Lease>, Without<Finished>),
    >,
    leases: Query<&Lease>,
    mut commands: Commands,
) {
    let mut occupied = Vec::new();
    for lease in &leases {
        occupied.extend(lease.0.iter().cloned());
    }
    for (entity, probe, selected, state) in &probes {
        let agent = &selected.0;
        if *state != OperationState::Pending
            || probe.scopes.iter().any(|path| occupied.contains(path))
        {
            continue;
        }
        occupied.extend(probe.scopes.iter().cloned());
        let store = PackageStore::at(probe.source.path.clone());
        let name = PackageName::parse(&agent.id).unwrap();
        commands.entity(entity).insert((
            Lease(probe.scopes.clone()),
            OperationState::Running,
            Reading,
            JobRequest(ReadFile {
                path: store.package_dir(&name).join("vmux-receipt.json"),
                limit: 1024 * 1024,
            }),
        ));
    }
}

fn receipt(
    probes: Query<(Entity, &Selected, &OperationState, &JobResult<ReadFile>), With<Reading>>,
    mut commands: Commands,
) {
    for (entity, selected, state, result) in &probes {
        let agent = &selected.0;
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Reading, JobResult<ReadFile>)>();
        let receipt = match &result.0 {
            Ok(Ok(bytes)) => bytes
                .0
                .as_ref()
                .and_then(|bytes| serde_json::from_slice::<Receipt>(bytes).ok()),
            _ => None,
        };
        match receipt {
            Some(receipt) if receipt.name.as_str() == agent.id => {
                let mut entity = commands.entity(entity);
                entity.insert(receipt);
                match agent.preferred_runtime() {
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
            _ => {
                commands.entity(entity).insert(Finished);
            }
        }
    }
}

fn binary(
    probes: Query<
        (Entity, &Probe, &Selected, &Receipt, &OperationState),
        (With<Binary>, Without<InstalledLaunch>, Without<Finished>),
    >,
    mut commands: Commands,
) {
    for (entity, probe, selected, receipt, state) in &probes {
        let agent = &selected.0;
        if *state != OperationState::Running {
            continue;
        }
        let Some(target) = agent.binary_for_host() else {
            commands.entity(entity).insert(Finished);
            continue;
        };
        let Some(digest) = &target.sha256 else {
            commands.entity(entity).insert(Finished);
            continue;
        };
        let mut identity = Sha256::new();
        for value in [agent.id.as_str(), target.archive.as_str(), digest.as_str()] {
            identity.update((value.len() as u64).to_le_bytes());
            identity.update(value.as_bytes());
        }
        let expected = format!("acp:{}:binary-v3:{:x}", agent.id, identity.finalize());
        if receipt.source_id != expected {
            commands.entity(entity).insert(Finished);
            continue;
        }
        let store = PackageStore::at(probe.source.path.clone());
        let path = match target
            .command_path(&store.package_dir(&receipt.name), target.archive_filename())
        {
            Ok(path) => path,
            Err(_) => {
                commands.entity(entity).insert(Finished);
                continue;
            }
        };
        commands.entity(entity).insert(InstalledLaunch {
            agent: ResolvedAgent {
                command: path.to_string_lossy().into_owned(),
                args: target.args.clone(),
                env: target
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                path_prepend: None,
                package_added: false,
            },
            npmrc: None,
        });
        commands.spawn((
            Name::new("Installed ACP executable"),
            File,
            ChildOf(entity),
            Operation::bundle("acp.installed.file").unwrap().running(),
            OperationRetained,
            JobRequest(Exists { path }),
        ));
    }
}

fn node(
    probes: Query<
        (Entity, &Probe, &Selected, &Receipt, &OperationState),
        (With<Node>, Without<InstalledLaunch>, Without<Finished>),
    >,
    mut commands: Commands,
) {
    for (entity, probe, selected, receipt, state) in &probes {
        let agent = &selected.0;
        if *state != OperationState::Running {
            continue;
        }
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "darwin-arm64",
            ("macos", "x86_64") => "darwin-x64",
            ("linux", "aarch64") => "linux-arm64",
            ("linux", "x86_64") => "linux-x64",
            _ => {
                commands.entity(entity).insert(Finished);
                continue;
            }
        };
        let Some(distribution) = &agent.distribution.npx else {
            commands.entity(entity).insert(Finished);
            continue;
        };
        let directory = probe
            .source
            .path
            .join("packages/node")
            .join(format!("node-v22.11.0-{target}"));
        let npmrc = probe.source.path.join("npmrc");
        commands.entity(entity).insert(InstalledLaunch {
            agent: ResolvedAgent::npx(distribution, receipt.version.as_deref(), &directory, &npmrc),
            npmrc: Some(npmrc),
        });
        for path in [
            directory.join("bin/node"),
            directory.join("lib/node_modules/npm/bin/npx-cli.js"),
        ] {
            commands.spawn((
                Name::new("Installed ACP executable"),
                File,
                ChildOf(entity),
                Operation::bundle("acp.installed.file").unwrap().running(),
                OperationRetained,
                JobRequest(Exists { path }),
            ));
        }
    }
}

fn uv(
    probes: Query<
        (Entity, &Probe, &Selected, &Receipt, &OperationState),
        (With<Uv>, Without<InstalledLaunch>, Without<Finished>),
    >,
    mut commands: Commands,
) {
    for (entity, probe, selected, receipt, state) in &probes {
        let agent = &selected.0;
        if *state != OperationState::Running {
            continue;
        }
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            _ => {
                commands.entity(entity).insert(Finished);
                continue;
            }
        };
        let Some(distribution) = &agent.distribution.uvx else {
            commands.entity(entity).insert(Finished);
            continue;
        };
        let directory = probe
            .source
            .path
            .join("packages/uv")
            .join(format!("uv-{target}"));
        let path = directory.join("uvx");
        let mut args = vec![distribution.spec(receipt.version.as_deref())];
        args.extend(distribution.args.iter().cloned());
        commands.entity(entity).insert(InstalledLaunch {
            agent: ResolvedAgent {
                command: path.to_string_lossy().into_owned(),
                args,
                env: distribution
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                path_prepend: Some(directory.to_string_lossy().into_owned()),
                package_added: false,
            },
            npmrc: None,
        });
        commands.spawn((
            Name::new("Installed ACP executable"),
            File,
            ChildOf(entity),
            Operation::bundle("acp.installed.file").unwrap().running(),
            OperationRetained,
            JobRequest(Exists { path }),
        ));
    }
}

fn files(
    files: Query<(Entity, &JobResult<Exists>), (With<File>, Without<Ready>)>,
    mut commands: Commands,
) {
    for (entity, result) in &files {
        let ready = matches!(&result.0, Ok(Ok(FileExists(true))));
        commands
            .entity(entity)
            .insert(Ready(ready))
            .remove::<JobResult<Exists>>();
    }
}

fn publish(
    probes: Query<(
        Entity,
        &Probe,
        Option<&OperationTarget>,
        &OperationState,
        Option<&InstalledLaunch>,
        Option<&Receipt>,
        Has<Finished>,
    )>,
    files: Query<(&ChildOf, Option<&Ready>), With<File>>,
    context: Context,
    definitions: Query<(&RegistryAgent, &ChildOf)>,
    mut commands: Commands,
) {
    let source = context.source();
    for (entity, probe, target, state, launch, receipt, finished) in &probes {
        if *state != OperationState::Running {
            continue;
        }
        let Some(target) = target else {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let mut ready = launch.is_some();
        let mut pending = launch.is_none() && !finished;
        for (parent, result) in &files {
            if parent.parent() != entity {
                continue;
            }
            pending |= result.is_none();
            ready &= result.is_some_and(|ready| ready.0);
        }
        if pending {
            continue;
        }
        if source.as_ref() == Some(&probe.source)
            && definitions
                .get(target.entity())
                .is_ok_and(|(_, parent)| parent.parent() == probe.source.catalog)
        {
            let mut owner = commands.entity(target.entity());
            owner.insert(probe.source.clone());
            if ready && let (Some(launch), Some(receipt)) = (launch, receipt) {
                owner.insert((InstalledAgent, launch.clone(), receipt.clone()));
            }
        }
        commands
            .entity(entity)
            .insert((Finished, OperationState::Succeeded));
    }
}

fn release(
    probes: Query<(Entity, &OperationState), With<Probe>>,
    files: Query<(Entity, &ChildOf, &OperationState), With<File>>,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, state) in &probes {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let mut pending = captures.iter().any(|target| target.entity() == entity);
        for (child, parent, status) in &files {
            if parent.parent() != entity {
                continue;
            }
            if matches!(status, OperationState::Pending | OperationState::Running) {
                commands.entity(child).insert(OperationState::Canceled);
            }
            pending |= captures.iter().any(|target| target.entity() == child);
        }
        if pending {
            continue;
        }
        commands.entity(entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::task::{Wake, Waker};
    use std::time::Duration;

    use vmux_ecs::JobWake;
    use vmux_ecs::profile::{ActiveProfile, Profile, ProfileDirectories, ProfilePaths};

    use super::super::registry::{BinaryTarget, Distribution, PackageDist};
    use super::*;
    use vmux_editor::lsp::package_path::Sha256Digest;

    struct WakeChannel(std::sync::mpsc::Sender<()>);

    impl Wake for WakeChannel {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    struct ShortGate {
        release: std::sync::mpsc::Sender<()>,
        task: vmux_ecs::BlockingTask<()>,
    }

    impl ShortGate {
        fn new(runtime: &vmux_ecs::BlockingRuntime) -> Self {
            let (started, entered) = std::sync::mpsc::channel();
            let (release, wait) = std::sync::mpsc::channel();
            let task = runtime
                .try_spawn(vmux_ecs::blocking::Lane::Short, None, move || {
                    started.send(()).unwrap();
                    let _ = wait.recv_timeout(Duration::from_secs(10));
                })
                .unwrap();
            entered.recv_timeout(Duration::from_secs(3)).unwrap();
            Self { release, task }
        }
    }

    impl Drop for ShortGate {
        fn drop(&mut self) {
            let _ = self.release.send(());
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while !self.task.is_finished() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture native hold failed to drain"
                );
                std::thread::yield_now();
            }
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        agent: Entity,
        catalog: Entity,
        profile: Entity,
        node: PathBuf,
        wakes: std::sync::mpsc::Receiver<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let paths = ProfilePaths::at(Profile::named("test"), directory.path().join("data"));
            let agents = paths.agents();
            let target = match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => "darwin-arm64",
                ("macos", "x86_64") => "darwin-x64",
                ("linux", "aarch64") => "linux-arm64",
                ("linux", "x86_64") => "linux-x64",
                _ => panic!("native fixture requires supported platform"),
            };
            let node = agents
                .join("packages/node")
                .join(format!("node-v22.11.0-{target}"));
            std::fs::create_dir_all(node.join("bin")).unwrap();
            std::fs::create_dir_all(node.join("lib/node_modules/npm/bin")).unwrap();
            std::fs::write(node.join("bin/node"), b"native node fixture").unwrap();
            std::fs::write(
                node.join("lib/node_modules/npm/bin/npx-cli.js"),
                b"native npx fixture",
            )
            .unwrap();
            let store = PackageStore::at(agents.clone());
            let node_name = PackageName::parse("node").unwrap();
            store
                .write_receipt(
                    &node_name,
                    &Receipt {
                        name: node_name.clone(),
                        version: Some("22.11.0".into()),
                        source_id: format!(
                            "https://nodejs.org/dist/v22.11.0/node-v22.11.0-{target}.tar.gz"
                        ),
                        bin: Default::default(),
                    },
                )
                .unwrap();
            let name = PackageName::parse("agent").unwrap();
            store
                .write_receipt(
                    &name,
                    &Receipt {
                        name: name.clone(),
                        version: Some("0.9.0".into()),
                        source_id: "acp:agent".into(),
                        bin: Default::default(),
                    },
                )
                .unwrap();
            std::fs::write(agents.join("npmrc"), b"user configuration sentinel").unwrap();
            let runtime_name = directory.path().file_name().unwrap().to_string_lossy();
            let runtime = vmux_ecs::BlockingRuntime::new(
                vmux_ecs::blocking::BlockingPool::new(&format!("{runtime_name}-short"), 1, 16),
                vmux_ecs::blocking::BlockingPool::new(&format!("{runtime_name}-long"), 1, 16),
                vmux_ecs::blocking::BlockingPool::new(&format!("{runtime_name}-process"), 1, 16),
            );
            let mut app = App::new();
            app.insert_resource(runtime)
                .add_message::<vmux_ecs::agent::SwapStackSession>()
                .add_message::<vmux_ecs::service::ServiceRequest>();
            let (sender, wakes) = std::sync::mpsc::channel();
            app.insert_resource(JobWake(Waker::from(Arc::new(WakeChannel(sender)))))
                .add_plugins((super::super::install::InstallPlugin, InstalledPlugin));
            let profile = app
                .world_mut()
                .spawn((
                    Name::new("Test"),
                    ActiveProfile(Profile::named("test")),
                    ProfileDirectories(paths),
                ))
                .id();
            let mut state = Catalog::new();
            state.ready = true;
            state.revision = 1;
            let catalog = app
                .world_mut()
                .spawn((
                    state,
                    CatalogSource {
                        profile: Some(profile),
                        cache: agents.join("registry.json"),
                        url: "https://example.invalid/registry".into(),
                    },
                ))
                .id();
            app.world_mut().spawn(crate::policy::AcpRegistryPolicy {
                url: "https://example.invalid/registry".into(),
            });
            let agent = app
                .world_mut()
                .spawn((
                    RegistryAgent {
                        id: "agent".into(),
                        name: "Agent".into(),
                        version: Some("2.0.0".into()),
                        description: None,
                        icon: None,
                        distribution: Distribution {
                            npx: Some(PackageDist {
                                package: "@fixture/agent".into(),
                                args: vec!["serve".into()],
                                env: Default::default(),
                            }),
                            ..Default::default()
                        },
                    },
                    ChildOf(catalog),
                ))
                .id();
            Self {
                app,
                directory,
                agent,
                catalog,
                profile,
                node,
                wakes,
            }
        }

        fn checked(&mut self) {
            self.app.update();
            for _ in 0..32 {
                if self.app.world().get::<Checked>(self.agent).is_some() {
                    return;
                }
                self.wakes
                    .recv_timeout(Duration::from_secs(5))
                    .expect("native work must wake readiness consumers");
                while self.wakes.try_recv().is_ok() {}
                self.app.update();
            }
            panic!("readiness failed to finish from native wake");
        }

        fn launch(&mut self) -> (Entity, Entity) {
            let source = self.app.world().get::<Checked>(self.agent).unwrap().clone();
            self.app
                .add_plugins(super::super::installed_launch::InstalledLaunchPlugin);
            let read = self
                .app
                .world_mut()
                .spawn((
                    vmux_ecs::Operation::bundle("fixture.catalog").unwrap(),
                    vmux_ecs::OperationRetained,
                    vmux_session::provider::Read {
                        generation: 1,
                        wire_id: 1,
                    },
                ))
                .id();
            let preparation = self
                .app
                .world_mut()
                .spawn((
                    vmux_ecs::Operation::bundle("fixture.catalog.prepare").unwrap(),
                    vmux_ecs::OperationRetained,
                    OperationTarget::new(read),
                    super::super::installed_launch::Request {
                        agent: self.agent,
                        source,
                        shell: "/bin/sh".into(),
                    },
                ))
                .id();
            (read, preparation)
        }

        fn completed(&mut self, preparation: Entity) {
            self.app.update();
            for _ in 0..32 {
                if self
                    .app
                    .world()
                    .get::<super::super::installed_launch::Completed>(preparation)
                    .is_some()
                {
                    return;
                }
                self.wakes
                    .recv_timeout(Duration::from_secs(5))
                    .expect("native catalog work must wake consumers");
                while self.wakes.try_recv().is_ok() {}
                self.app.update();
            }
            panic!("catalog preparation failed to finish from native wake");
        }

        fn released(&mut self, operation: Entity) {
            self.app.update();
            for _ in 0..64 {
                if self
                    .app
                    .world()
                    .get::<super::super::package::Released>(operation)
                    .is_some()
                {
                    return;
                }
                self.wakes
                    .recv_timeout(Duration::from_secs(5))
                    .expect("native package work must wake consumers");
                while self.wakes.try_recv().is_ok() {}
                self.app.update();
            }
            panic!("package work failed to release from native wake");
        }

        fn retired(&mut self, preparation: Entity) {
            self.app.update();
            for _ in 0..32 {
                if self.app.world().get_entity(preparation).is_err() {
                    return;
                }
                self.wakes
                    .recv_timeout(Duration::from_secs(5))
                    .expect("native cancellation must wake drain consumers");
                while self.wakes.try_recv().is_ok() {}
                self.app.update();
            }
            panic!("canceled preparation did not retire after native drain");
        }

        fn agents(&self) -> PathBuf {
            self.app
                .world()
                .get::<ProfileDirectories>(self.profile)
                .unwrap()
                .0
                .agents()
        }
    }

    #[test]
    fn readiness_preserves_persisted_pin_and_never_writes_configuration() {
        let mut fixture = Fixture::new();
        let receipt = fixture.agents().join("packages/agent/vmux-receipt.json");
        let before = std::fs::read(&receipt).unwrap();
        fixture.checked();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_some()
        );
        let launch = fixture
            .app
            .world()
            .get::<InstalledLaunch>(fixture.agent)
            .unwrap();
        assert!(launch.agent.args.contains(&"@fixture/agent@0.9.0".into()));
        assert!(!launch.agent.args.contains(&"@fixture/agent@2.0.0".into()));
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
        assert_eq!(std::fs::read(receipt).unwrap(), before);
        assert!(!fixture.agents().join("staging").exists());
        assert!(fixture.directory.path().exists());
    }

    #[test]
    fn catalog_replacement_and_missing_runtime_refuse_old_readiness() {
        let mut fixture = Fixture::new();
        fixture.checked();
        std::fs::remove_file(fixture.node.join("lib/node_modules/npm/bin/npx-cli.js")).unwrap();
        fixture
            .app
            .world_mut()
            .get_mut::<Catalog>(fixture.catalog)
            .unwrap()
            .revision += 1;
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_none()
        );
        fixture.checked();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledLaunch>(fixture.agent)
                .is_none()
        );
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }

    #[test]
    fn competing_package_lease_and_profile_replacement_prevent_stale_publication() {
        let mut fixture = Fixture::new();
        let lease = fixture
            .app
            .world_mut()
            .spawn(Lease(vec![fixture.node.parent().unwrap().to_path_buf()]))
            .id();
        fixture.app.update();
        fixture.app.update();
        assert!(fixture.app.world().get::<Checked>(fixture.agent).is_none());
        fixture
            .app
            .world_mut()
            .get_mut::<CatalogSource>(fixture.catalog)
            .unwrap()
            .profile = None;
        fixture.app.update();
        fixture.app.world_mut().entity_mut(lease).despawn();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world_mut()
                .query::<&Probe>()
                .iter(fixture.app.world())
                .next()
                .is_none()
        );
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }
    #[test]
    fn catalog_preparation_commits_native_npmrc_and_preserves_receipt_pin() {
        let mut fixture = Fixture::new();
        fixture.checked();
        let receipt = fixture.agents().join("packages/agent/vmux-receipt.json");
        let before = std::fs::read(&receipt).unwrap();
        let (_, preparation) = fixture.launch();
        fixture.completed(preparation);
        let result = &fixture
            .app
            .world()
            .get::<super::super::installed_launch::Completed>(preparation)
            .unwrap()
            .0;
        let launch = result.as_ref().unwrap();
        assert!(launch.args.contains(&"@fixture/agent@0.9.0".into()));
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"registry=https://registry.npmjs.org/\nignore-scripts=false\n"
        );
        assert_eq!(std::fs::read(receipt).unwrap(), before);
    }

    #[test]
    fn canceled_queued_catalog_write_retains_lease_until_drain_and_blocks_removal() {
        let mut fixture = Fixture::new();
        fixture.checked();
        let (read, preparation) = fixture.launch();
        let gate = ShortGate::new(fixture.app.world().resource::<vmux_ecs::BlockingRuntime>());
        fixture.app.update();
        assert!(fixture.app.world().get::<Lease>(preparation).is_some());
        fixture
            .app
            .world_mut()
            .entity_mut(read)
            .insert(OperationState::Canceled);
        let store = PackageStore::at(fixture.agents());
        let removal = fixture
            .app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("fixture.remove").unwrap(),
                vmux_ecs::OperationRetained,
                super::super::package::RemoveRequest {
                    store,
                    name: PackageName::parse("agent").unwrap(),
                },
            ))
            .id();
        fixture.app.update();
        fixture.app.update();
        assert!(fixture.app.world().get::<Lease>(preparation).is_some());
        assert_eq!(
            fixture.app.world().get::<OperationState>(removal),
            Some(&OperationState::Pending)
        );
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
        drop(gate);
        fixture.retired(preparation);
        fixture.released(removal);
        assert!(
            fixture
                .app
                .world()
                .get::<super::super::package::Removed>(removal)
                .unwrap()
                .0
                .is_ok()
        );
        assert!(!fixture.agents().join("packages/agent").exists());
        fixture.checked();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_none()
        );
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }

    #[test]
    fn native_package_pin_commit_reconciles_readiness_without_session_notification() {
        let mut fixture = Fixture::new();
        fixture.checked();
        let agent = fixture
            .app
            .world()
            .get::<RegistryAgent>(fixture.agent)
            .unwrap()
            .clone();
        let store = PackageStore::at(fixture.agents());
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("fixture.pin").unwrap(),
                vmux_ecs::OperationRetained,
                super::super::package::Request {
                    agent,
                    version: Some("1.2.3".into()),
                    store,
                },
            ))
            .id();
        fixture.released(operation);
        assert!(
            fixture
                .app
                .world()
                .get::<super::super::package::Prepared>(operation)
                .unwrap()
                .0
                .is_ok()
        );
        fixture.checked();
        let launch = fixture
            .app
            .world()
            .get::<InstalledLaunch>(fixture.agent)
            .unwrap();
        assert!(launch.agent.args.contains(&"@fixture/agent@1.2.3".into()));
        assert!(!launch.agent.args.contains(&"@fixture/agent@0.9.0".into()));
    }
    #[test]
    fn cached_uv_readiness_requires_uvx_without_installing_uv() {
        let mut fixture = Fixture::new();
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            _ => panic!("native fixture requires supported platform"),
        };
        let directory = fixture
            .agents()
            .join("packages/uv")
            .join(format!("uv-{target}"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("uvx"), b"uvx fixture").unwrap();
        fixture
            .app
            .world_mut()
            .get_mut::<RegistryAgent>(fixture.agent)
            .unwrap()
            .distribution = Distribution {
            uvx: Some(PackageDist {
                package: "fast-agent".into(),
                args: vec!["serve".into()],
                env: Default::default(),
            }),
            ..Default::default()
        };
        fixture.checked();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_some()
        );
        let launch = fixture
            .app
            .world()
            .get::<InstalledLaunch>(fixture.agent)
            .unwrap();
        assert!(launch.agent.args.contains(&"fast-agent@0.9.0".into()));
        assert!(!directory.join("uv").exists());
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }

    #[test]
    fn cached_binary_source_and_digest_changes_refuse_old_launch_without_changing_bytes() {
        for change_url in [true, false] {
            let mut fixture = Fixture::new();
            let target = RegistryAgent::host_target().unwrap().to_string();
            let mut targets = std::collections::BTreeMap::new();
            targets.insert(
                target.clone(),
                BinaryTarget {
                    archive: "https://example.invalid/agent.bin".into(),
                    cmd: "agent".into(),
                    sha256: Some(
                        Sha256Digest::parse(
                            "6f1af2dfc4d7f16dacf404b1f6c9fd4a65cfffb8edde6dcf957463a0e41fb1ed",
                        )
                        .unwrap(),
                    ),
                    args: vec!["serve".into()],
                    env: Default::default(),
                },
            );
            fixture
                .app
                .world_mut()
                .get_mut::<RegistryAgent>(fixture.agent)
                .unwrap()
                .distribution = Distribution {
                binary: Some(targets),
                ..Default::default()
            };
            let store = PackageStore::at(fixture.agents());
            let name = PackageName::parse("agent").unwrap();
            store.write_receipt(&name, &Receipt { name: name.clone(), version: Some("0.9.0".into()), source_id: "acp:agent:binary-v3:e0efd477c6a9c9c95bcb949d54389083bcf4f65bc6b2d58c4b29eb1ca91c44f7".into(), bin: Default::default() }).unwrap();
            let executable = store.package_dir(&name).join("agent");
            std::fs::write(&executable, b"fixture executable").unwrap();
            fixture.checked();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<InstalledAgent>(fixture.agent)
                    .is_some()
            );
            {
                let mut agent = fixture
                    .app
                    .world_mut()
                    .get_mut::<RegistryAgent>(fixture.agent)
                    .unwrap();
                let binary = agent
                    .distribution
                    .binary
                    .as_mut()
                    .unwrap()
                    .get_mut(&target)
                    .unwrap();
                if change_url {
                    binary.archive = "https://example.invalid/replaced.bin".into();
                } else {
                    binary.sha256 = Some(
                        Sha256Digest::parse(
                            "0000000000000000000000000000000000000000000000000000000000000000",
                        )
                        .unwrap(),
                    );
                }
            }
            fixture.checked();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<InstalledAgent>(fixture.agent)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<InstalledLaunch>(fixture.agent)
                    .is_none()
            );
            assert_eq!(std::fs::read(executable).unwrap(), b"fixture executable");
        }
    }
    #[test]
    fn probe_owner_despawn_preserves_lease_until_native_drain() {
        let mut fixture = Fixture::new();
        let gate = ShortGate::new(fixture.app.world().resource::<vmux_ecs::BlockingRuntime>());
        fixture.app.update();
        let probe = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Probe>>()
            .single(fixture.app.world())
            .unwrap();
        assert!(fixture.app.world().get::<Lease>(probe).is_some());
        fixture.app.world_mut().entity_mut(fixture.agent).despawn();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(probe),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get::<Lease>(probe).is_some());
        drop(gate);
        fixture.retired(probe);
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }

    #[test]
    fn catalog_owner_despawn_cancels_queued_native_write_and_releases_after_drain() {
        let mut fixture = Fixture::new();
        fixture.checked();
        let (read, preparation) = fixture.launch();
        let gate = ShortGate::new(fixture.app.world().resource::<vmux_ecs::BlockingRuntime>());
        fixture.app.update();
        fixture.app.world_mut().entity_mut(read).despawn();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(preparation),
            Some(&OperationState::Canceled)
        );
        assert!(fixture.app.world().get::<Lease>(preparation).is_some());
        drop(gate);
        fixture.retired(preparation);
        assert_eq!(
            std::fs::read(fixture.agents().join("npmrc")).unwrap(),
            b"user configuration sentinel"
        );
    }

    #[test]
    fn same_frame_package_removal_excludes_probe_and_catalog_launch_leases() {
        for launch in [false, true] {
            let mut fixture = Fixture::new();
            let preparation = if launch {
                fixture.checked();
                Some(fixture.launch().1)
            } else {
                None
            };
            let gate = ShortGate::new(fixture.app.world().resource::<vmux_ecs::BlockingRuntime>());
            let store = PackageStore::at(fixture.agents());
            let removal = fixture
                .app
                .world_mut()
                .spawn((
                    vmux_ecs::Operation::bundle("fixture.remove").unwrap(),
                    vmux_ecs::OperationRetained,
                    super::super::package::RemoveRequest {
                        store,
                        name: PackageName::parse("agent").unwrap(),
                    },
                ))
                .id();
            fixture.app.update();
            assert!(fixture.app.world().get::<Lease>(removal).is_some());
            if let Some(preparation) = preparation {
                assert!(fixture.app.world().get::<Lease>(preparation).is_none());
                assert_eq!(
                    fixture.app.world().get::<OperationState>(preparation),
                    Some(&OperationState::Pending)
                );
            } else {
                let probe = fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<Probe>>()
                    .single(fixture.app.world())
                    .unwrap();
                assert!(fixture.app.world().get::<Lease>(probe).is_none());
            }
            drop(gate);
            fixture.released(removal);
            assert!(!fixture.agents().join("packages/agent").exists());
        }
    }

    #[test]
    fn session_installer_consumes_package_result_without_losing_pin_notification() {
        let mut fixture = Fixture::new();
        fixture.checked();
        let agent = fixture
            .app
            .world()
            .get::<RegistryAgent>(fixture.agent)
            .unwrap()
            .clone();
        let agents = fixture.agents();
        fixture.app.world_mut().spawn((
            vmux_session::Session,
            vmux_session::SessionId("fixture-session".into()),
            vmux_session::AgentId("agent".into()),
            vmux_ecs::Cwd(agents.clone()),
            vmux_ecs::ProcessAnchor(vmux_ecs::ProcessId::new()),
            vmux_session::RunState::Idle,
            super::super::AcpInstallRequest {
                profile: Some(fixture.profile),
                agents,
                catalog: Some(fixture.catalog),
                catalog_revision: 1,
                selected_agent: Some(agent),
                agent_id: "agent".into(),
                shell: "/bin/sh".into(),
                fallback: Some(vmux_setting::AcpAgentConfig {
                    id: "agent".into(),
                    name: "Agent".into(),
                    command: "fallback".into(),
                    args: Vec::new(),
                    env: Vec::new(),
                    cwd: None,
                    version: Some("1.2.3".into()),
                }),
            },
        ));
        fixture.app.update();
        for _ in 0..64 {
            if fixture
                .app
                .world()
                .get::<InstalledLaunch>(fixture.agent)
                .is_some_and(|launch| launch.agent.args.contains(&"@fixture/agent@1.2.3".into()))
            {
                break;
            }
            fixture
                .wakes
                .recv_timeout(Duration::from_secs(5))
                .expect("session package pin changes must wake cached readiness");
            while fixture.wakes.try_recv().is_ok() {}
            fixture.app.update();
        }
        let launch = fixture
            .app
            .world()
            .get::<InstalledLaunch>(fixture.agent)
            .unwrap();
        assert!(launch.agent.args.contains(&"@fixture/agent@1.2.3".into()));
        assert!(
            fixture
                .app
                .world_mut()
                .query::<&super::super::package::Request>()
                .iter(fixture.app.world())
                .next()
                .is_none(),
            "production session consumer must retire its released package result"
        );
    }
    #[test]
    fn unrelated_package_notification_preserves_checked_readiness() {
        let mut fixture = Fixture::new();
        fixture.checked();
        fixture.app.world_mut().write_message(AcpPackageChanged {
            agent_id: "unrelated".into(),
        });
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<InstalledAgent>(fixture.agent)
                .is_some()
        );
        assert!(fixture.app.world().get::<Checked>(fixture.agent).is_some());
        assert!(
            fixture
                .app
                .world_mut()
                .query::<&Probe>()
                .iter(fixture.app.world())
                .next()
                .is_none()
        );
    }
}
