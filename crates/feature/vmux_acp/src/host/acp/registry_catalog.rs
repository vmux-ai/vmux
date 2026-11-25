use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::profile::{ActiveProfile, ProfileDirectories, ProfilePaths};
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget};
use vmux_job::{JobRequest, JobResult};
use vmux_tool::io::{Get, ReadFile, WriteFile};

use super::registry::{Registry, RegistryAgent};
use crate::policy::{AcpRegistryPolicy, PolicyLoaded};

pub(in crate::host) struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::io::ObservationIoPlugin>() {
            app.add_plugins(vmux_tool::io::ObservationIoPlugin);
        }
        app.add_message::<RefreshRequest>()
            .add_systems(PostUpdate, wake)
            .add_systems(PostUpdate, restart.after(vmux_job::JobSet::Cleanup))
            .add_systems(Startup, spawn.after(PolicyLoaded))
            .add_systems(
                Update,
                (request, retire, start, downloaded, cached, written, publish)
                    .chain()
                    .after(vmux_tool::observation::ObservationSet)
                    .before(vmux_job::JobSet::Submit),
            );
    }
}

#[derive(Message)]
pub(crate) struct RefreshRequest;

#[derive(Component)]
pub(crate) struct Catalog {
    pub(crate) revision: u64,
    pub(crate) ready: bool,
    pub(crate) loading: bool,
    requested: bool,
}

impl Catalog {
    pub(in crate::host::acp) fn new() -> Self {
        Self {
            revision: 0,
            ready: false,
            loading: false,
            requested: true,
        }
    }
}

#[derive(Component)]
pub(in crate::host::acp) struct Source {
    pub(in crate::host::acp) url: String,
    pub(in crate::host::acp) cache: PathBuf,
    pub(in crate::host::acp) profile: Option<Entity>,
}

#[derive(Component)]
struct Downloading;
#[derive(Component)]
struct Reading;
#[derive(Component)]
struct Writing;
#[derive(Component)]
struct Received(Option<Vec<u8>>);

#[derive(Component)]
struct Restart;

fn restart(requests: Query<Entity, With<Restart>>, mut commands: Commands) {
    for entity in &requests {
        commands.entity(entity).remove::<Restart>();
    }
}

fn wake(catalogs: Query<Entity, Changed<Catalog>>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if !catalogs.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

fn spawn(existing: Query<Entity, With<Catalog>>, mut commands: Commands) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("ACP registry"),
        Catalog::new(),
        Operation::bundle("acp.registry").unwrap(),
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
        }
    }
}

fn retire(
    mut catalogs: Query<(Entity, &mut Catalog, &Source, &mut OperationState)>,
    profiles: Query<(Entity, &ProfileDirectories), With<ActiveProfile>>,
    policies: Query<&AcpRegistryPolicy>,
    native: Query<&OperationTarget>,
    mut commands: Commands,
) {
    let profile = profiles.single().ok();
    let cache = profile
        .map(|(_, paths)| paths.0.agents())
        .unwrap_or_else(|| ProfilePaths::current().agents())
        .join("registry.json");
    let url = policies.single().ok().map(|policy| policy.url.as_str());
    for (entity, mut catalog, source, mut state) in &mut catalogs {
        if source.cache == cache
            && source.profile == profile.map(|(entity, _)| entity)
            && url == Some(source.url.as_str())
        {
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
            .remove::<(Source, Downloading, Reading, Writing, Received)>()
            .insert(Restart);
    }
}

fn start(
    mut catalogs: Query<
        (Entity, &mut Catalog),
        (
            Without<Restart>,
            Without<Downloading>,
            Without<Reading>,
            Without<Writing>,
            Without<Received>,
        ),
    >,
    policies: Query<&AcpRegistryPolicy>,
    profiles: Query<(Entity, &ProfileDirectories), With<ActiveProfile>>,
    mut commands: Commands,
) {
    let Ok(policy) = policies.single() else {
        return;
    };
    let profile = profiles.single().ok();
    let cache = profile
        .map(|(_, paths)| paths.0.agents())
        .unwrap_or_else(|| ProfilePaths::current().agents())
        .join("registry.json");
    for (entity, mut catalog) in &mut catalogs {
        if !catalog.requested || catalog.loading {
            continue;
        }
        catalog.loading = true;
        catalog.requested = false;
        commands.entity(entity).insert((
            OperationState::Running,
            Downloading,
            Source {
                url: policy.url.clone(),
                cache: cache.clone(),
                profile: profile.map(|(entity, _)| entity),
            },
            JobRequest(Get {
                url: policy.url.clone(),
                limit: 8 * 1024 * 1024,
                authorization: None,
            }),
        ));
    }
}

fn downloaded(
    requests: Query<(Entity, &Source, &OperationState, &JobResult<Get>), With<Downloading>>,
    mut commands: Commands,
) {
    for (entity, source, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Downloading, JobResult<Get>)>();
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_ref(),
            _ => None,
        };
        if let Some(bytes) = bytes
            && std::str::from_utf8(bytes)
                .ok()
                .and_then(|text| text.parse::<Registry>().ok())
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
            commands.entity(entity).insert((
                Reading,
                JobRequest(ReadFile {
                    path: source.cache.clone(),
                    limit: 8 * 1024 * 1024,
                }),
            ));
        }
    }
}

fn cached(
    requests: Query<(Entity, &OperationState, &JobResult<ReadFile>), With<Reading>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes
                .0
                .as_ref()
                .filter(|bytes| {
                    std::str::from_utf8(bytes)
                        .ok()
                        .and_then(|text| text.parse::<Registry>().ok())
                        .is_some()
                })
                .cloned(),
            _ => None,
        };
        commands
            .entity(entity)
            .remove::<(Reading, JobResult<ReadFile>)>()
            .insert(Received(bytes));
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
        if let Err(error) = &result.0 {
            bevy::log::warn!("ACP registry cache write failed: {error}");
        } else if let Ok(Err(error)) = &result.0 {
            bevy::log::warn!("ACP registry cache write failed: {error}");
        }
        commands
            .entity(entity)
            .remove::<(Writing, JobResult<WriteFile>)>();
    }
}

fn publish(
    mut catalogs: Query<
        (Entity, &mut Catalog, &Received, &OperationState),
        (Without<Reading>, Without<Writing>),
    >,
    current: Query<(Entity, &RegistryAgent, Option<&ChildOf>)>,
    mut commands: Commands,
) {
    for (entity, mut catalog, received, state) in &mut catalogs {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(bytes) = &received.0 {
            let Ok(registry) = std::str::from_utf8(bytes).unwrap().parse::<Registry>() else {
                continue;
            };
            let mut existing = HashMap::new();
            for (agent, definition, parent) in &current {
                if parent.is_none_or(|parent| parent.parent() == entity) {
                    existing.insert(definition.id.clone(), agent);
                }
            }
            for agent in registry.agents {
                let name = Name::new(format!("ACP agent {}", agent.id));
                if let Some(existing) = existing.remove(&agent.id) {
                    commands
                        .entity(existing)
                        .insert((name, agent, ChildOf(entity)));
                } else {
                    commands.spawn((name, agent, ChildOf(entity)));
                }
            }
            for (_, old) in existing {
                commands.entity(old).despawn();
            }
        }
        catalog.revision = catalog.revision.wrapping_add(1);
        catalog.loading = false;
        catalog.ready = true;
        commands
            .entity(entity)
            .remove::<Received>()
            .insert(OperationState::Succeeded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::acp::{
        AcpInstallJob, AcpInstallOutcome, AcpInstallRequest, AcpInstallResult, AcpInstallWaiter,
        AcpLaunchStarted, InstallState,
    };
    use bevy::prelude::{App, Entity, Name};
    use vmux_ecs::profile::{ActiveProfile, Profile};
    use vmux_ecs::service::ServiceRequest;
    use vmux_ecs::{Cwd, ProcessAnchor, ProcessId};
    use vmux_session::{AgentId, RunState, Session, SessionId};

    struct InstallFixture {
        app: App,
        profile: Entity,
        catalog: Entity,
        job: Entity,
        session: Entity,
    }

    impl InstallFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(crate::host::acp::install::InstallPlugin)
                .add_message::<ServiceRequest>()
                .add_message::<vmux_ecs::agent::SwapStackSession>();
            let paths = ProfilePaths::current();
            let agents = paths.agents();
            let profile = app
                .world_mut()
                .spawn((
                    Name::new("Test"),
                    ActiveProfile(Profile::named("test")),
                    ProfileDirectories(paths),
                ))
                .id();
            let catalog = app
                .world_mut()
                .spawn((
                    Catalog {
                        revision: 1,
                        ready: true,
                        loading: false,
                        requested: false,
                    },
                    Source {
                        url: "https://example.invalid/registry".into(),
                        cache: agents.join("registry.json"),
                        profile: Some(profile),
                    },
                ))
                .id();
            app.world_mut().spawn(AcpRegistryPolicy {
                url: "https://example.invalid/registry".into(),
            });
            let key = AcpInstallRequest {
                profile: Some(profile),
                agents,
                catalog: Some(catalog),
                catalog_revision: 1,
                selected_agent: None,
                agent_id: "agent".into(),
                fallback: None,
                shell: String::new(),
            }
            .key();
            let job = app
                .world_mut()
                .spawn((
                    key,
                    InstallState::preparing(),
                    AcpInstallJob {
                        package: None,
                        task: None,
                        outcome: Some(AcpInstallOutcome {
                            launch: Err("selected outcome".into()),
                        }),
                    },
                ))
                .id();
            let session = app
                .world_mut()
                .spawn((
                    Session,
                    SessionId("session".into()),
                    AgentId("agent".into()),
                    Cwd(PathBuf::from("/workspace")),
                    ProcessAnchor(ProcessId::new()),
                    RunState::Idle,
                    AcpLaunchStarted,
                    AcpInstallWaiter {
                        job,
                        sid: "session".into(),
                        agent_id: "agent".into(),
                    },
                ))
                .id();
            Self {
                app,
                profile,
                catalog,
                job,
                session,
            }
        }

        fn replace_profile(&mut self) {
            let value = self
                .app
                .world()
                .get::<ActiveProfile>(self.profile)
                .unwrap()
                .clone();
            let directories = self
                .app
                .world()
                .get::<ProfileDirectories>(self.profile)
                .unwrap()
                .clone();
            self.app.world_mut().despawn(self.profile);
            self.app
                .world_mut()
                .spawn((Name::new("Replacement"), value, directories));
        }
    }

    #[test]
    fn install_delivery_requires_the_original_profile_and_catalog() {
        let mut current = InstallFixture::new();
        current.app.update();
        assert_eq!(
            current.app.world().get::<RunState>(current.session),
            Some(&RunState::Errored)
        );
        assert_eq!(
            current
                .app
                .world()
                .get::<vmux_session::RunError>(current.session)
                .map(|error| error.0.as_str()),
            Some("selected outcome")
        );
        for profile_replaced in [true, false] {
            let mut fixture = InstallFixture::new();
            if profile_replaced {
                fixture.replace_profile();
            } else {
                fixture
                    .app
                    .world_mut()
                    .get_mut::<Catalog>(fixture.catalog)
                    .unwrap()
                    .revision += 1;
            }
            fixture.app.update();
            assert!(matches!(
                fixture.app.world().get::<RunState>(fixture.session),
                Some(RunState::Idle)
            ));
            assert!(
                fixture
                    .app
                    .world()
                    .get::<AcpInstallWaiter>(fixture.session)
                    .is_none()
            );
            assert!(fixture.app.world().get_entity(fixture.job).is_err());
        }
    }

    #[test]
    fn install_resolution_prefers_registry_and_uses_the_accepted_fallback_after_failure() {
        for registry_available in [true, false] {
            let mut fixture = InstallFixture::new();
            {
                let mut key = fixture
                    .app
                    .world_mut()
                    .get_mut::<crate::host::acp::AcpInstallKey>(fixture.job)
                    .unwrap();
                key.fallback_command = "/configured/agent".into();
                key.fallback_args = vec!["--fallback".into()];
                key.fallback_env = vec![("MODE".into(), "fallback".into())];
            }
            fixture
                .app
                .world_mut()
                .get_mut::<AcpInstallJob>(fixture.job)
                .unwrap()
                .outcome = None;
            let agent = if registry_available {
                Ok(crate::host::acp::installer_driver::ResolvedAgent {
                    command: "/registry/agent".into(),
                    args: vec!["--registered".into()],
                    env: vec![("MODE".into(), "registry".into())],
                    path_prepend: Some("/managed/bin".into()),
                    package_added: true,
                })
            } else {
                Err("registry install refused".into())
            };
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.job)
                .insert(AcpInstallResult {
                    agent,
                    login_env: vec![("PATH".into(), "/login/bin".into())],
                });
            fixture.app.update();
            let launch = fixture
                .app
                .world()
                .get::<AcpInstallJob>(fixture.job)
                .unwrap()
                .outcome
                .as_ref()
                .unwrap()
                .launch
                .as_ref()
                .unwrap();
            let (command, argument, mode, path) = if registry_available {
                (
                    "/registry/agent",
                    "--registered",
                    "registry",
                    "/managed/bin:/login/bin",
                )
            } else {
                ("/configured/agent", "--fallback", "fallback", "/login/bin")
            };
            assert_eq!(launch.command, command);
            assert_eq!(launch.args, [argument]);
            assert!(launch.env.contains(&("MODE".into(), mode.into())));
            assert!(launch.env.contains(&("PATH".into(), path.into())));
            assert!(
                fixture
                    .app
                    .world()
                    .get::<AcpInstallResult>(fixture.job)
                    .is_none()
            );
        }
    }

    #[test]
    fn install_resolution_refuses_registry_failure_without_a_fallback_and_obsolete_results() {
        let mut failure = InstallFixture::new();
        failure
            .app
            .world_mut()
            .get_mut::<AcpInstallJob>(failure.job)
            .unwrap()
            .outcome = None;
        failure
            .app
            .world_mut()
            .entity_mut(failure.job)
            .insert(AcpInstallResult {
                agent: Err("registry install refused".into()),
                login_env: Vec::new(),
            });
        failure.app.update();
        assert_eq!(
            failure.app.world().get::<RunState>(failure.session),
            Some(&RunState::Errored)
        );
        assert_eq!(
            failure
                .app
                .world()
                .get::<vmux_session::RunError>(failure.session)
                .map(|error| error.0.as_str()),
            Some("registry install refused")
        );
        for replace_profile in [true, false] {
            let mut fixture = InstallFixture::new();
            fixture
                .app
                .world_mut()
                .get_mut::<AcpInstallJob>(fixture.job)
                .unwrap()
                .outcome = None;
            fixture
                .app
                .world_mut()
                .entity_mut(fixture.job)
                .insert(AcpInstallResult {
                    agent: Ok(crate::host::acp::installer_driver::ResolvedAgent {
                        command: "/registry/agent".into(),
                        args: Vec::new(),
                        env: Vec::new(),
                        path_prepend: None,
                        package_added: true,
                    }),
                    login_env: Vec::new(),
                });
            if replace_profile {
                fixture.replace_profile();
            } else {
                fixture
                    .app
                    .world_mut()
                    .get_mut::<Catalog>(fixture.catalog)
                    .unwrap()
                    .revision += 1;
            }
            fixture.app.update();
            assert!(matches!(
                fixture.app.world().get::<RunState>(fixture.session),
                Some(RunState::Idle)
            ));
            assert!(fixture.app.world().get_entity(fixture.job).is_err());
            assert_eq!(
                fixture
                    .app
                    .world()
                    .resource::<Messages<crate::host::acp::AcpPackageChanged>>()
                    .len(),
                0
            );
        }
    }

    #[test]
    fn retired_install_keeps_native_ownership_until_completion() {
        let mut fixture = InstallFixture::new();
        let (release, wait) = std::sync::mpsc::channel();
        let (started, entered) = std::sync::mpsc::channel();
        let task = fixture
            .app
            .world()
            .resource::<vmux_job::BlockingRuntime>()
            .spawn_process(None, move || {
                started.send(()).unwrap();
                wait.recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                AcpInstallResult {
                    agent: Err("late outcome".into()),
                    login_env: Vec::new(),
                }
            })
            .unwrap();
        entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        {
            let mut job = fixture
                .app
                .world_mut()
                .get_mut::<AcpInstallJob>(fixture.job)
                .unwrap();
            job.task = Some(task);
            job.outcome = None;
        }
        fixture.replace_profile();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<AcpInstallJob>(fixture.job)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<AcpInstallWaiter>(fixture.session)
                .is_none()
        );
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<AcpInstallJob>(fixture.job)
            .is_some()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "retired native install did not drain"
            );
            fixture.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(matches!(
            fixture.app.world().get::<RunState>(fixture.session),
            Some(RunState::Idle)
        ));
    }
}
