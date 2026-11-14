use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use bevy::winit::EventLoopProxyWrapper;
use bevy_app::{App, AppExit, Last, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder, futures_lite::future};
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
    Start,
    Stop,
    Status,
    Logs,
    Configure,
    Containers {
        ids: Vec<String>,
        operation: ContainerOperation,
    },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Start => "container.start",
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
            Self::Start => Ok(Reply::Started(client.start_owned(helper, owner)?)),
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
                    docker.start_many(&ids)?;
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
pub(crate) struct Busy(Command);

impl Busy {
    pub(crate) fn starting(&self) -> bool {
        matches!(
            self.0,
            Command::Start
                | Command::Containers {
                    operation: ContainerOperation::Start,
                    ..
                }
        )
    }
    pub(crate) fn stopping(&self) -> bool {
        matches!(self.0, Command::Stop)
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

fn request(
    trigger: On<Request>,
    busy: Query<(), With<Busy>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    lifetime: Res<Lifetime>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if request.command.mutates() && busy.contains(request.entity) {
        commands.trigger(Finished {
            entity: request.reply,
            result: Err("An engine operation is already running".into()),
            mutates: true,
            requester: request.requester,
        });
        return;
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
    if request.command.mutates() {
        commands
            .entity(request.entity)
            .insert(Busy(request.command.clone()));
    }
    let kind = request.command.clone();
    let work = kind.clone();
    let settings = request.settings.clone();
    let owner = lifetime.owner.clone();
    let wake = Wake::beside(proxy.as_deref());
    commands
        .spawn((
            Name::new(kind.name()),
            operation,
            OperationTarget::new(request.entity),
            ReplyTarget(request.reply),
            RequestedBy(request.requester),
            RequestKind(kind),
            ControlTask(IoTaskPool::get().spawn(async move {
                let result = work.execute(settings, &helper, &owner);
                drop(wake);
                result
            })),
        ))
        .insert(OperationState::Running);
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

fn poll(mut operations: RunningOperations, mut commands: Commands) {
    for (entity, target, reply, kind, requester, mut task) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ControlTask>();
        if kind.0.mutates() {
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
