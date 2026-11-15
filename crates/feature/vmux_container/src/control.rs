use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use bevy::winit::EventLoopProxyWrapper;
use bevy_app::{App, AppExit, Last, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder};
use vmux_ecs::service::{Restarting, ShutdownSet};
use vmux_ecs::wake::Wake;
use vmux_ecs::{Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget};
use vmux_vm::{DockerClient, EngineClient, Owner, Settings, Status};

pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        IoTaskPool::get_or_init(|| {
            TaskPoolBuilder::new()
                .num_threads(2)
                .thread_name("Container I/O".into())
                .build()
        });
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.init_resource::<Lifetime>()
            .add_observer(start)
            .add_observer(stop)
            .add_observer(inspect)
            .add_observer(logs)
            .add_observer(configure)
            .add_observer(start_containers)
            .add_observer(stop_containers)
            .add_observer(restart_containers)
            .add_observer(remove_containers)
            .add_systems(Last, shutdown.before(ShutdownSet))
            .add_systems(
                Update,
                (
                    poll::<Started>,
                    poll::<Stopped>,
                    poll::<StatusRead>,
                    poll::<LogText>,
                    poll::<Configured>,
                    poll::<Changed>,
                )
                    .in_set(ControlSet::Complete),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ControlSet {
    Complete,
}

#[derive(Resource)]
struct Lifetime {
    owner: Owner,
}

impl Default for Lifetime {
    fn default() -> Self {
        Self {
            owner: Owner::new(vmux_profile::ProfilePaths::current().profile()),
        }
    }
}

fn shutdown(
    mut exits: MessageReader<AppExit>,
    restarting: Option<Res<Restarting>>,
    lifetime: Res<Lifetime>,
    configured: Query<&Settings, With<crate::host::EngineView>>,
) {
    if exits.read().count() == 0 || restarting.is_some() {
        return;
    }
    lifetime.owner.cancel();
    for settings in &configured {
        if let Err(error) = EngineClient::new(settings.clone()).stop_owned(&lifetime.owner) {
            bevy::log::error!(%error, "application-owned VM shutdown failed");
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Ownership {
    Independent,
    Application,
}

#[derive(Component, Clone)]
pub(crate) struct Target {
    pub(crate) reply: Entity,
    pub(crate) settings: Settings,
    pub(crate) requester: Entity,
}

#[derive(EntityEvent)]
pub(crate) struct Start {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
    pub(crate) ownership: Ownership,
}

#[derive(EntityEvent)]
pub(crate) struct Stop {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
}

#[derive(EntityEvent)]
pub(crate) struct Inspect {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
}

#[derive(EntityEvent)]
pub(crate) struct ReadLogs {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
}

#[derive(EntityEvent)]
pub(crate) struct Configure {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
}

#[derive(EntityEvent)]
pub(crate) struct StartContainers {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
    pub(crate) ids: Vec<String>,
}

#[derive(EntityEvent)]
pub(crate) struct StopContainers {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
    pub(crate) ids: Vec<String>,
}

#[derive(EntityEvent)]
pub(crate) struct RestartContainers {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
    pub(crate) ids: Vec<String>,
}

#[derive(EntityEvent)]
pub(crate) struct RemoveContainers {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) target: Target,
    pub(crate) ids: Vec<String>,
}

impl Start {
    fn execute(&self, helper: &Path, owner: &Owner) -> io::Result<Started> {
        let mut client = EngineClient::new(self.target.settings.clone());
        let status = match self.ownership {
            Ownership::Independent => client.start(helper)?,
            Ownership::Application => client.start_owned(helper, owner)?,
        };
        Ok(Started(status))
    }
}

#[derive(EntityEvent)]
pub(crate) struct Finished<R: Send + Sync + 'static> {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) result: Result<R, String>,
    pub(crate) mutates: bool,
    pub(crate) requester: Entity,
}

#[derive(Component, Clone)]
pub(crate) struct Started(pub(crate) Status);
#[derive(Component, Clone)]
pub(crate) struct Stopped(pub(crate) Status);
#[derive(Component, Clone)]
pub(crate) struct StatusRead(pub(crate) Status);
#[derive(Component, Clone)]
pub(crate) struct LogText(pub(crate) String);
#[derive(Component, Clone)]
pub(crate) struct Configured(pub(crate) Settings);
#[derive(Component, Clone)]
pub(crate) struct Changed;

#[derive(Component)]
pub(crate) struct Busy {
    starting: bool,
    stopping: bool,
    operation: Entity,
}

impl Busy {
    pub(crate) fn starting(&self) -> bool {
        self.starting
    }
    pub(crate) fn stopping(&self) -> bool {
        self.stopping
    }
}

#[derive(Component, Clone, Copy)]
struct Mutation {
    starting: bool,
    stopping: bool,
}

#[derive(Component)]
struct ControlTask<R: Send + Sync + 'static>(Task<io::Result<R>>);

#[derive(Component)]
struct RequestOwner(Owner);

#[derive(SystemParam)]
struct Admission<'w, 's> {
    busy: Query<'w, 's, &'static Busy>,
    owners: Query<'w, 's, &'static RequestOwner>,
    proxy: Option<Res<'w, EventLoopProxyWrapper>>,
    lifetime: Res<'w, Lifetime>,
    commands: Commands<'w, 's>,
}

impl Admission<'_, '_> {
    fn accept<R: Send + Sync + 'static>(
        &mut self,
        entity: Entity,
        target: &Target,
        name: &'static str,
        mutation: Option<Mutation>,
    ) -> Option<(Entity, PathBuf, Owner, Wake)> {
        if let Some(mutation) = mutation
            && let Ok(busy) = self.busy.get(entity)
        {
            if mutation.stopping && busy.starting() {
                if let Ok(owner) = self.owners.get(busy.operation) {
                    owner.0.cancel();
                }
            } else {
                self.commands.trigger(Finished::<R> {
                    entity: target.reply,
                    result: Err("An engine operation is already running".into()),
                    mutates: true,
                    requester: target.requester,
                });
                return None;
            }
        }
        let helper = match std::env::current_exe() {
            Ok(path) => path.with_file_name("vmux_vm"),
            Err(error) => {
                self.commands.trigger(Finished::<R> {
                    entity: target.reply,
                    result: Err(error.to_string()),
                    mutates: mutation.is_some(),
                    requester: target.requester,
                });
                return None;
            }
        };
        let operation = match Operation::bundle(name) {
            Ok(operation) => operation,
            Err(error) => {
                self.commands.trigger(Finished::<R> {
                    entity: target.reply,
                    result: Err(error),
                    mutates: mutation.is_some(),
                    requester: target.requester,
                });
                return None;
            }
        };
        let owner = self.lifetime.owner.operation();
        let mut operation = self.commands.spawn((
            Name::new(name),
            operation,
            OperationTarget::new(entity),
            target.clone(),
            RequestOwner(owner.clone()),
        ));
        operation.insert(OperationState::Running);
        if let Some(mutation) = mutation {
            operation.insert(mutation);
        }
        let operation = operation.id();
        if let Some(mutation) = mutation {
            self.commands.entity(entity).insert(Busy {
                starting: mutation.starting,
                stopping: mutation.stopping,
                operation,
            });
        }
        Some((
            operation,
            helper,
            owner,
            Wake::beside(self.proxy.as_deref()),
        ))
    }
}

fn start(trigger: On<Start>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, helper, owner, wake)) = admission.accept::<Started>(
        request.entity,
        &request.target,
        "container.start",
        Some(Mutation {
            starting: true,
            stopping: false,
        }),
    ) else {
        return;
    };
    let request = Start {
        entity: request.entity,
        target: request.target.clone(),
        ownership: request.ownership,
    };
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let result = request.execute(&helper, &owner);
            drop(wake);
            result
        })));
}

fn stop(trigger: On<Stop>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) = admission.accept::<Stopped>(
        request.entity,
        &request.target,
        "container.stop",
        Some(Mutation {
            starting: false,
            stopping: true,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let client = EngineClient::new(settings);
            let result = client.stop().map(|()| Stopped(client.status()));
            drop(wake);
            result
        })));
}

fn inspect(trigger: On<Inspect>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) =
        admission.accept::<StatusRead>(request.entity, &request.target, "container.status", None)
    else {
        return;
    };
    let settings = request.target.settings.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let result = Ok(StatusRead(EngineClient::new(settings).status()));
            drop(wake);
            result
        })));
}

fn logs(trigger: On<ReadLogs>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) =
        admission.accept::<LogText>(request.entity, &request.target, "container.logs", None)
    else {
        return;
    };
    let settings = request.target.settings.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let result = ReadLogs::read(&settings).map(LogText);
            drop(wake);
            result
        })));
}

fn configure(trigger: On<Configure>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) = admission.accept::<Configured>(
        request.entity,
        &request.target,
        "container.configure",
        Some(Mutation {
            starting: false,
            stopping: false,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let result = EngineClient::new(settings).configure().map(Configured);
            drop(wake);
            result
        })));
}

impl ReadLogs {
    fn read(settings: &Settings) -> io::Result<String> {
        let mut logs = String::new();
        for path in [settings.log(), settings.directory.join("guest.log")] {
            let mut file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            file.seek(SeekFrom::Start(
                file.metadata()?.len().saturating_sub(65536),
            ))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            logs.push_str(&String::from_utf8_lossy(&bytes));
        }
        Ok(logs)
    }
}

fn start_containers(trigger: On<StartContainers>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, helper, owner, wake)) = admission.accept::<Changed>(
        request.entity,
        &request.target,
        "container.control",
        Some(Mutation {
            starting: true,
            stopping: false,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    let ids = request.ids.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let mut client = EngineClient::new(settings.clone());
            let result = if client.status().docker_ready {
                Ok(())
            } else {
                client.start_owned(&helper, &owner).map(|_| ())
            };
            let result = match result {
                Ok(()) => DockerClient::new(&settings.socket())
                    .start_many_owned(&ids, &owner)
                    .map(|()| Changed),
                Err(error) => Err(error),
            };
            drop(wake);
            result
        })));
}

fn stop_containers(trigger: On<StopContainers>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) = admission.accept::<Changed>(
        request.entity,
        &request.target,
        "container.control",
        Some(Mutation {
            starting: false,
            stopping: false,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    let ids = request.ids.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let docker = DockerClient::new(&settings.socket());
            let mut errors = Vec::new();
            for id in ids {
                if let Err(error) = docker.stop(&id) {
                    errors.push(format!("{id}: {error}"));
                }
            }
            drop(wake);
            if errors.is_empty() {
                Ok(Changed)
            } else {
                Err(io::Error::other(errors.join("\n")))
            }
        })));
}

fn restart_containers(trigger: On<RestartContainers>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) = admission.accept::<Changed>(
        request.entity,
        &request.target,
        "container.control",
        Some(Mutation {
            starting: false,
            stopping: false,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    let ids = request.ids.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let docker = DockerClient::new(&settings.socket());
            let mut errors = Vec::new();
            for id in ids {
                if let Err(error) = docker.restart(&id) {
                    errors.push(format!("{id}: {error}"));
                }
            }
            drop(wake);
            if errors.is_empty() {
                Ok(Changed)
            } else {
                Err(io::Error::other(errors.join("\n")))
            }
        })));
}

fn remove_containers(trigger: On<RemoveContainers>, mut admission: Admission) {
    let request = trigger.event();
    let Some((entity, _, _, wake)) = admission.accept::<Changed>(
        request.entity,
        &request.target,
        "container.control",
        Some(Mutation {
            starting: false,
            stopping: false,
        }),
    ) else {
        return;
    };
    let settings = request.target.settings.clone();
    let ids = request.ids.clone();
    admission
        .commands
        .entity(entity)
        .insert(ControlTask(IoTaskPool::get().spawn(async move {
            let docker = DockerClient::new(&settings.socket());
            let mut errors = Vec::new();
            for id in ids {
                if let Err(error) = docker.remove(&id) {
                    errors.push(format!("{id}: {error}"));
                }
            }
            drop(wake);
            if errors.is_empty() {
                Ok(Changed)
            } else {
                Err(io::Error::other(errors.join("\n")))
            }
        })));
}

type RunningOperations<'w, 's, R> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationTarget,
        &'static Target,
        Option<&'static Mutation>,
        &'static mut ControlTask<R>,
    ),
>;

fn poll<R: Component + Clone>(
    mut operations: RunningOperations<R>,
    busy: Query<&Busy>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let waker = wake
        .as_ref()
        .map_or(std::task::Waker::noop(), |wake| &wake.0);
    let mut context = std::task::Context::from_waker(waker);
    for (entity, operation_target, target, mutation, mut task) in &mut operations {
        let std::task::Poll::Ready(result) =
            std::future::Future::poll(std::pin::Pin::new(&mut task.0), &mut context)
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(ControlTask<R>, RequestOwner)>();
        let current = mutation.is_none()
            || busy
                .get(operation_target.entity())
                .is_ok_and(|busy| busy.operation == entity);
        if mutation.is_some() && current {
            commands.entity(operation_target.entity()).remove::<Busy>();
        }
        let result = result.map_err(|error| error.to_string());
        if let Ok(value) = &result {
            commands.entity(entity).insert(value.clone());
        }
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        commands.trigger(OperationCompleted {
            entity,
            result: completion,
        });
        if !current {
            continue;
        }
        commands.trigger(Finished {
            entity: target.reply,
            result,
            mutates: mutation.is_some(),
            requester: target.requester,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    #[test]
    fn stop_supersedes_start_without_accepting_its_old_completion() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.validate().unwrap();
        let lease = std::fs::File::create(settings.directory.join("start.lock")).unwrap();
        lease.lock().unwrap();
        let mut app = App::new();
        app.add_plugins(ControlPlugin);
        let target = app.world_mut().spawn_empty().id();
        let owner = app.world().resource::<Lifetime>().owner.operation();
        let status = EngineClient::new(settings.clone()).status();
        let old = app
            .world_mut()
            .spawn((
                Operation::bundle("container.start").unwrap(),
                OperationTarget::new(target),
                Target {
                    reply: target,
                    requester: target,
                    settings: settings.clone(),
                },
                Mutation {
                    starting: true,
                    stopping: false,
                },
                RequestOwner(owner.clone()),
                ControlTask(IoTaskPool::get().spawn(async move { Ok(Started(status)) })),
            ))
            .insert(OperationState::Running)
            .id();
        app.world_mut().entity_mut(target).insert(Busy {
            starting: true,
            stopping: false,
            operation: old,
        });
        app.world_mut().trigger(Stop {
            entity: target,
            target: Target {
                reply: target,
                requester: target,
                settings: settings.clone(),
            },
        });
        app.world_mut().flush();
        let replacement = app.world().get::<Busy>(target).unwrap().operation;
        assert_ne!(replacement, old);
        let error = EngineClient::new(settings)
            .start_owned(Path::new("unused"), &owner)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while app.world().get::<ControlTask<Started>>(old).is_some() {
            assert!(std::time::Instant::now() < deadline);
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            app.world().get::<Busy>(target).unwrap().operation,
            replacement
        );
        drop(lease);
        while app.world().get::<Busy>(target).is_some() {
            assert!(std::time::Instant::now() < deadline);
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn application_cancellation_cannot_cancel_an_independent_cli_start() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.cpus = 0;
        let owner = Owner::new(directory.path().join("profile"));
        owner.cancel();
        let application = Start {
            entity: Entity::PLACEHOLDER,
            target: Target {
                reply: Entity::PLACEHOLDER,
                requester: Entity::PLACEHOLDER,
                settings: settings.clone(),
            },
            ownership: Ownership::Application,
        }
        .execute(Path::new("unused helper"), &owner);
        let Err(application) = application else {
            panic!("cancelled application start was accepted");
        };
        assert_eq!(application.kind(), io::ErrorKind::Interrupted);
        let independent = Start {
            entity: Entity::PLACEHOLDER,
            target: Target {
                reply: Entity::PLACEHOLDER,
                requester: Entity::PLACEHOLDER,
                settings,
            },
            ownership: Ownership::Independent,
        }
        .execute(Path::new("unused helper"), &owner);
        let Err(independent) = independent else {
            panic!("invalid resource configuration was accepted");
        };
        assert_eq!(
            independent.kind(),
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                io::ErrorKind::InvalidInput
            } else {
                io::ErrorKind::Unsupported
            }
        );
    }

    #[test]
    fn quitting_a_restarted_gui_stops_configured_owned_engines_without_a_new_start() {
        for restarting in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let profile = temp.path().join("profile");
            std::fs::create_dir(&profile).unwrap();
            let profile = profile.canonicalize().unwrap();
            let mut settings = Settings::new(temp.path().join("engine"), temp.path());
            settings.validate().unwrap();
            let lease = std::fs::File::create(settings.directory.join("engine.lock")).unwrap();
            lease.lock().unwrap();
            std::fs::write(
                settings.directory.join("engine-state.json"),
                serde_json::to_vec(
                    &serde_json::json!({"phase":"ready", "failure":null, "owner":profile}),
                )
                .unwrap(),
            )
            .unwrap();
            let listener =
                std::os::unix::net::UnixListener::bind(settings.control_socket()).unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                if restarting {
                    std::thread::sleep(Duration::from_millis(50));
                    assert!(
                        matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
                    );
                } else {
                    let deadline = std::time::Instant::now() + Duration::from_secs(2);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                assert!(
                                    std::time::Instant::now() < deadline,
                                    "Quit did not stop its configured VM"
                                );
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Err(error) => panic!("accept shutdown: {error}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 5];
                    stream.read_exact(&mut request).unwrap();
                    assert_eq!(&request, b"STOP\n");
                    stream.write_all(b"STOPPING\n").unwrap();
                }
                drop(lease);
            });
            let mut app = App::new();
            app.insert_resource(Lifetime {
                owner: Owner::new(profile),
            })
            .add_plugins(ControlPlugin)
            .add_message::<AppExit>();
            app.world_mut().spawn((crate::host::EngineView, settings));
            if restarting {
                app.insert_resource(Restarting);
            }
            app.world_mut().write_message(AppExit::Success);
            app.world_mut().run_schedule(Last);
            server.join().unwrap();
        }
    }
}
