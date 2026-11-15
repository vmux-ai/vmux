use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use bevy::winit::EventLoopProxyWrapper;
use bevy_app::{App, AppExit, Last, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder};
use vmux_ecs::service::{Restarting, ShutdownSet};
use vmux_ecs::wake::Wake;
use vmux_ecs::{Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget};
use vmux_vm::Owner;
use vmux_vm::{DockerClient, DockerOperation, EngineClient, Settings, Status};

use crate::event::ContainerOperation;

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
        app.add_observer(request)
            .init_resource::<Lifetime>()
            .add_systems(Last, shutdown.before(ShutdownSet))
            .add_systems(Update, poll.in_set(ControlSet::Complete));
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

#[derive(Clone)]
pub(crate) enum Command {
    Start(Ownership),
    Stop,
    Status,
    Logs,
    Configure,
    Containers {
        ids: Vec<String>,
        operation: ContainerOperation,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum Ownership {
    Independent,
    Application,
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Start(_) => "container.start",
            Self::Stop => "container.stop",
            Self::Status => "container.status",
            Self::Logs => "container.logs",
            Self::Configure => "container.configure",
            Self::Containers { .. } => "container.control",
        }
    }

    fn mutates(&self) -> bool {
        !matches!(self, Self::Status | Self::Logs)
    }

    fn execute(self, settings: Settings, helper: &Path, owner: &Owner) -> io::Result<Reply> {
        let mut client = EngineClient::new(settings.clone());
        match self {
            Self::Start(Ownership::Independent) => Ok(Reply::Started(client.start(helper)?)),
            Self::Start(Ownership::Application) => {
                Ok(Reply::Started(client.start_owned(helper, owner)?))
            }
            Self::Stop => {
                client.stop()?;
                Ok(Reply::Stopped(client.status()))
            }
            Self::Status => Ok(Reply::Status(client.status())),
            Self::Configure => Ok(Reply::Configured(client.configure()?)),
            Self::Containers { ids, operation } => {
                if operation == ContainerOperation::Start && !client.status().docker_ready {
                    client.start_owned(helper, owner)?;
                }
                let docker = DockerClient::new(&settings.socket());
                let operation = match operation {
                    ContainerOperation::Start => DockerOperation::Start,
                    ContainerOperation::Stop => DockerOperation::Stop,
                    ContainerOperation::Restart => DockerOperation::Restart,
                    ContainerOperation::Remove => DockerOperation::Remove,
                };
                if matches!(operation, DockerOperation::Start) {
                    docker.start_many_owned(&ids, owner)?;
                    return Ok(Reply::Changed);
                }
                let mut errors = Vec::new();
                for id in ids {
                    if let Err(error) = docker.change(&id, operation) {
                        errors.push(format!("{id}: {error}"));
                    }
                }
                if !errors.is_empty() {
                    return Err(io::Error::other(errors.join("\n")));
                }
                Ok(Reply::Changed)
            }
            Self::Logs => {
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
                Ok(Reply::Logs(logs))
            }
        }
    }
}

#[derive(EntityEvent)]
pub(crate) struct Request {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) reply: Entity,
    pub(crate) command: Command,
    pub(crate) settings: Settings,
    pub(crate) requester: Entity,
}

#[derive(EntityEvent)]
pub(crate) struct Finished {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) result: std::result::Result<Reply, String>,
    pub(crate) mutates: bool,
    pub(crate) requester: Entity,
}

pub(crate) enum Reply {
    Started(Status),
    Stopped(Status),
    Status(Status),
    Logs(String),
    Configured(Settings),
    Changed,
}

#[derive(Component)]
pub(crate) struct Busy {
    command: Command,
    operation: Entity,
}

impl Busy {
    pub(crate) fn starting(&self) -> bool {
        matches!(
            self.command,
            Command::Start(_)
                | Command::Containers {
                    operation: ContainerOperation::Start,
                    ..
                }
        )
    }
    pub(crate) fn stopping(&self) -> bool {
        matches!(self.command, Command::Stop)
    }
}

#[derive(Component)]
struct ReplyTarget(Entity);

#[derive(Component)]
struct RequestedBy(Entity);

#[derive(Component)]
struct RequestKind(Command);

#[derive(Component)]
struct ControlTask(Task<io::Result<Reply>>);

#[derive(Component)]
struct RequestOwner(Owner);

fn request(
    trigger: On<Request>,
    busy: Query<&Busy>,
    owners: Query<&RequestOwner>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    lifetime: Res<Lifetime>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if request.command.mutates()
        && let Ok(busy) = busy.get(request.entity)
    {
        if matches!(request.command, Command::Stop) && busy.starting() {
            if let Ok(owner) = owners.get(busy.operation) {
                owner.0.cancel();
            }
        } else {
            commands.trigger(Finished {
                entity: request.reply,
                result: Err("An engine operation is already running".into()),
                mutates: true,
                requester: request.requester,
            });
            return;
        }
    }
    let helper = match std::env::current_exe() {
        Ok(path) => path.with_file_name("vmux_vm"),
        Err(error) => {
            commands.trigger(Finished {
                entity: request.reply,
                result: Err(error.to_string()),
                mutates: request.command.mutates(),
                requester: request.requester,
            });
            return;
        }
    };
    let operation = match Operation::bundle(request.command.name()) {
        Ok(operation) => operation,
        Err(error) => {
            commands.trigger(Finished {
                entity: request.reply,
                result: Err(error),
                mutates: request.command.mutates(),
                requester: request.requester,
            });
            return;
        }
    };
    let kind = request.command.clone();
    let work = kind.clone();
    let settings = request.settings.clone();
    let owner = lifetime.owner.operation();
    let request_owner = owner.clone();
    let wake = Wake::beside(proxy.as_deref());
    let operation = commands
        .spawn((
            Name::new(kind.name()),
            operation,
            OperationTarget::new(request.entity),
            ReplyTarget(request.reply),
            RequestedBy(request.requester),
            RequestKind(kind),
            RequestOwner(request_owner),
            ControlTask(IoTaskPool::get().spawn(async move {
                let result = work.execute(settings, &helper, &owner);
                drop(wake);
                result
            })),
        ))
        .insert(OperationState::Running)
        .id();
    if request.command.mutates() {
        commands.entity(request.entity).insert(Busy {
            command: request.command.clone(),
            operation,
        });
    }
}

type RunningOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationTarget,
        &'static ReplyTarget,
        &'static RequestKind,
        &'static RequestedBy,
        &'static mut ControlTask,
    ),
>;

fn poll(
    mut operations: RunningOperations,
    busy: Query<&Busy>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let waker = wake
        .as_ref()
        .map_or(std::task::Waker::noop(), |wake| &wake.0);
    let mut context = std::task::Context::from_waker(waker);
    for (entity, target, reply, kind, requester, mut task) in &mut operations {
        let std::task::Poll::Ready(result) =
            std::future::Future::poll(std::pin::Pin::new(&mut task.0), &mut context)
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(ControlTask, RequestOwner)>();
        let current = !kind.0.mutates()
            || busy
                .get(target.entity())
                .is_ok_and(|busy| busy.operation == entity);
        if kind.0.mutates() && current {
            commands.entity(target.entity()).remove::<Busy>();
        }
        let result = result.map_err(|error| error.to_string());
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
            entity: reply.0,
            result,
            mutates: kind.0.mutates(),
            requester: requester.0,
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
                ReplyTarget(target),
                RequestedBy(target),
                RequestKind(Command::Start(Ownership::Application)),
                RequestOwner(owner.clone()),
                ControlTask(IoTaskPool::get().spawn(async move { Ok(Reply::Started(status)) })),
            ))
            .insert(OperationState::Running)
            .id();
        app.world_mut().entity_mut(target).insert(Busy {
            command: Command::Start(Ownership::Application),
            operation: old,
        });
        app.world_mut().trigger(Request {
            entity: target,
            reply: target,
            requester: target,
            command: Command::Stop,
            settings: settings.clone(),
        });
        app.world_mut().flush();
        let replacement = app.world().get::<Busy>(target).unwrap().operation;
        assert_ne!(replacement, old);
        let error = EngineClient::new(settings)
            .start_owned(Path::new("unused"), &owner)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while app.world().get::<ControlTask>(old).is_some() {
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
        let application = Command::Start(Ownership::Application).execute(
            settings.clone(),
            Path::new("unused helper"),
            &owner,
        );
        let Err(application) = application else {
            panic!("cancelled application start was accepted");
        };
        assert_eq!(application.kind(), io::ErrorKind::Interrupted);
        let independent = Command::Start(Ownership::Independent).execute(
            settings,
            Path::new("unused helper"),
            &owner,
        );
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
