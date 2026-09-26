use std::future::Future;
use std::io;
use std::num::NonZero;
use std::time::Duration;

use bevy_app::{App, AppExit, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::futures_lite::future;
use clap::{Parser, Subcommand};

pub mod mcp;
pub mod notify;
pub mod notify_file_touch;
pub mod notify_turn_end;
pub mod open;
pub mod remote;
pub mod service;
pub mod tool;

#[derive(Debug, Parser)]
#[command(name = "vmux", version, about = "Vmux command-line interface")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Mcp(mcp::McpArgs),
    Notify(notify::NotifyRequest),
    NotifyFileTouch(notify_file_touch::NotifyFileTouchRequest),
    NotifyTurnEnd(notify_turn_end::NotifyTurnEndRequest),
    Tools(tool::ToolArgs),
    Service(service::ServiceArgs),
    Remote(remote::RemoteArgs),
}

#[derive(Component)]
pub(crate) struct CliTask(tokio::task::JoinHandle<Result<u8, String>>);

impl CliTask {
    pub(crate) fn spawn(task: impl Future<Output = Result<u8, String>> + Send + 'static) -> Self {
        Self(tokio::spawn(task))
    }
}

#[derive(Component)]
pub(crate) struct CliResult(pub(crate) Result<u8, String>);

impl CliResult {
    pub(crate) fn from_io(result: io::Result<i32>) -> Self {
        Self(
            result
                .map(|code| u8::try_from(code).unwrap_or(1))
                .map_err(|error| error.to_string()),
        )
    }

    pub(crate) fn from_unit(result: io::Result<()>) -> Self {
        Self(result.map(|()| 0).map_err(|error| error.to_string()))
    }
}

struct CliPlugin {
    command: Option<Command>,
}

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                execute_open,
                start_notification,
                start_file_touch,
                start_turn_end,
                execute_tool,
                execute_service,
                execute_remote,
                poll_cli_tasks,
                finish_cli,
            )
                .chain(),
        );

        let mut request = app.world_mut().spawn_empty();
        match self.command.as_ref() {
            None => {
                request.insert(open::OpenRequest);
            }
            Some(Command::Notify(command)) => {
                request.insert(command.clone());
            }
            Some(Command::NotifyFileTouch(command)) => {
                request.insert(command.clone());
            }
            Some(Command::NotifyTurnEnd(command)) => {
                request.insert(command.clone());
            }
            Some(Command::Tools(command)) => {
                request.insert(command.clone());
            }
            Some(Command::Service(command)) => {
                request.insert(command.clone());
            }
            Some(Command::Remote(command)) => {
                request.insert(command.clone());
            }
            Some(Command::Mcp(_)) => unreachable!(),
        }
    }
}

pub async fn run(cli: Cli) -> AppExit {
    let command = match cli.command {
        Some(Command::Mcp(args)) => {
            return match mcp::run(args).await {
                Ok(()) => AppExit::Success,
                Err(error) => {
                    eprintln!("vmux mcp: {error}");
                    AppExit::error()
                }
            };
        }
        command => command,
    };

    let mut app = App::new();
    app.add_plugins(CliPlugin { command })
        .set_runner(one_shot_runner);
    app.run()
}

fn execute_open(
    requests: Query<(Entity, &open::OpenRequest), Added<open::OpenRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(CliResult::from_unit(request.execute()));
    }
}

fn start_notification(
    requests: Query<(Entity, &notify::NotifyRequest), Added<notify::NotifyRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let request = request.clone();
        commands.entity(entity).insert(CliTask::spawn(async move {
            request
                .send()
                .await
                .map(|()| 0)
                .map_err(|error| error.to_string())
        }));
    }
}

fn start_file_touch(
    requests: Query<
        (Entity, &notify_file_touch::NotifyFileTouchRequest),
        Added<notify_file_touch::NotifyFileTouchRequest>,
    >,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let request = request.clone();
        commands.entity(entity).insert(CliTask::spawn(async move {
            request
                .send()
                .await
                .map(|()| 0)
                .map_err(|error| error.to_string())
        }));
    }
}

fn start_turn_end(
    requests: Query<
        (Entity, &notify_turn_end::NotifyTurnEndRequest),
        Added<notify_turn_end::NotifyTurnEndRequest>,
    >,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let request = request.clone();
        commands.entity(entity).insert(CliTask::spawn(async move {
            request
                .send()
                .await
                .map(|()| 0)
                .map_err(|error| error.to_string())
        }));
    }
}

fn execute_tool(
    requests: Query<(Entity, &tool::ToolArgs), Added<tool::ToolArgs>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(CliResult::from_unit(request.execute()));
    }
}

fn execute_service(
    requests: Query<(Entity, &service::ServiceArgs), Added<service::ServiceArgs>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(CliResult::from_io(request.execute()));
    }
}

fn execute_remote(
    requests: Query<(Entity, &remote::RemoteArgs), Added<remote::RemoteArgs>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(CliResult::from_io(request.execute()));
    }
}

fn poll_cli_tasks(mut tasks: Query<(Entity, &mut CliTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        if !task.0.is_finished() {
            continue;
        }
        let result = match future::block_on(&mut task.0) {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        commands
            .entity(entity)
            .remove::<CliTask>()
            .insert(CliResult(result));
    }
}

fn finish_cli(results: Query<&CliResult, Added<CliResult>>, mut exits: MessageWriter<AppExit>) {
    for result in &results {
        match &result.0 {
            Ok(0) => {
                exits.write(AppExit::Success);
            }
            Ok(code) => {
                let code = NonZero::new(*code).unwrap_or(NonZero::<u8>::MIN);
                exits.write(AppExit::Error(code));
            }
            Err(error) => {
                eprintln!("vmux: {error}");
                exits.write(AppExit::error());
            }
        }
    }
}

fn one_shot_runner(mut app: App) -> AppExit {
    loop {
        app.update();
        if let Some(exit) = app.should_exit() {
            return exit;
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}
