use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use bevy::winit::EventLoopProxyWrapper;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder, futures_lite::future};
use vmux_ecs::wake::Wake;
use vmux_ecs::{Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget};
use vmux_vm::{DockerClient, DockerOperation, EngineClient, Settings, Status};

use crate::compose::ComposeProject;
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
            .add_systems(Update, poll.in_set(ControlSet::Complete));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ControlSet {
    Complete,
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
    Project(std::path::PathBuf),
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
            Self::Project(_) => "container.project",
        }
    }

    fn mutates(&self) -> bool {
        !matches!(self, Self::Status | Self::Logs)
    }

    fn execute(self, settings: Settings, helper: &Path) -> io::Result<Reply> {
        let mut client = EngineClient::new(settings.clone());
        match self {
            Self::Start => Ok(Reply::Started(client.start(helper)?)),
            Self::Stop => {
                client.stop()?;
                Ok(Reply::Stopped(client.status()))
            }
            Self::Status => Ok(Reply::Status(client.status())),
            Self::Configure => Ok(Reply::Configured(client.configure()?)),
            Self::Containers { ids, operation } => {
                let docker = DockerClient::new(&settings.socket());
                let operation = match operation {
                    ContainerOperation::Start => DockerOperation::Start,
                    ContainerOperation::Stop => DockerOperation::Stop,
                    ContainerOperation::Restart => DockerOperation::Restart,
                    ContainerOperation::Remove => DockerOperation::Remove,
                };
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
            Self::Project(directory) => Ok(Reply::Started(
                ComposeProject::at(&directory)?.start(settings, helper)?,
            )),
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
}

#[derive(EntityEvent)]
pub(crate) struct Finished {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) result: Result<Reply, String>,
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
    pub(crate) fn project(&self) -> bool {
        matches!(self.0, Command::Project(_))
    }
    pub(crate) fn starting(&self) -> bool {
        matches!(self.0, Command::Start | Command::Project(_))
    }
    pub(crate) fn stopping(&self) -> bool {
        matches!(self.0, Command::Stop)
    }
}

#[derive(Component)]
struct ReplyTarget(Entity);

#[derive(Component)]
struct RequestKind(Command);

#[derive(Component)]
struct ControlTask(Task<io::Result<Reply>>);

fn request(
    trigger: On<Request>,
    busy: Query<(), With<Busy>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if request.command.mutates() && busy.contains(request.entity) {
        commands.trigger(Finished {
            entity: request.reply,
            result: Err("An engine operation is already running".into()),
        });
        return;
    }
    let helper = match std::env::current_exe() {
        Ok(path) => path.with_file_name("vmux_vm"),
        Err(error) => {
            commands.trigger(Finished {
                entity: request.reply,
                result: Err(error.to_string()),
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
    let wake = Wake::beside(proxy.as_deref());
    commands
        .spawn((
            Name::new(kind.name()),
            operation,
            OperationTarget::new(request.entity),
            ReplyTarget(request.reply),
            RequestKind(kind),
            ControlTask(IoTaskPool::get().spawn(async move {
                let result = work.execute(settings, &helper);
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
        &'static mut ControlTask,
    ),
>;

fn poll(mut operations: RunningOperations, mut commands: Commands) {
    for (entity, target, reply, kind, mut task) in &mut operations {
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
        });
    }
}
