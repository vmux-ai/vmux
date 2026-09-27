use std::future::Future;
use std::io::{self, Read};

use bevy::app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentCommandResult, AgentFileTouched, AgentNotify,
    AgentRequestId, AgentTurnEnded, ClientMessage, FileTouchKind, ProcessId, ServiceMessage,
};
use vmux_core::cli::{CliInvocation, CliManifestPlugin, CliResult};
use vmux_service::client::ServiceConnection;

pub struct AgentCliPlugin;

impl Plugin for AgentCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CliManifestPlugin::new(include_str!("cli.ron")))
            .add_systems(
                Update,
                (
                    route_agent_cli,
                    start_notify,
                    start_file_touch,
                    start_turn_end,
                    poll_agent_cli,
                )
                    .chain(),
            );
    }
}

#[derive(Component, Clone)]
struct NotifyCliRequest {
    title: Option<String>,
    body: Option<String>,
    anchor: Option<String>,
}

impl NotifyCliRequest {
    async fn send(self) -> io::Result<()> {
        let anchor = CliAnchor::strict(self.anchor)?;
        let connection = match ServiceConnection::connect().await {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("vmux notify: cannot connect to vmux service: {error}");
                return Ok(());
            }
        };
        let request_id = AgentRequestId::new();
        if let Err(error) = connection
            .send(&ClientMessage::AgentCommand {
                request_id,
                anchor,
                command: AgentCommand::Notify(AgentNotify {
                    title: self.title,
                    body: self.body,
                }),
            })
            .await
        {
            eprintln!("vmux notify: failed to send: {error}");
            return Ok(());
        }
        let _ = tokio::time::timeout(AGENT_COMMAND_TIMEOUT, async {
            while let Ok(Some(message)) = connection.recv().await {
                if let ServiceMessage::AgentCommandResult {
                    request_id: received,
                    result,
                } = message
                    && received == request_id
                {
                    if let AgentCommandResult::Error(message) = result {
                        eprintln!("vmux notify: {message}");
                    }
                    break;
                }
            }
        })
        .await;
        Ok(())
    }
}

#[derive(Component, Clone)]
struct FileTouchCliRequest {
    anchor: Option<String>,
}

impl FileTouchCliRequest {
    async fn send(self) -> io::Result<()> {
        let Some(anchor) = CliAnchor::optional(self.anchor) else {
            return Ok(());
        };
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&input) else {
            return Ok(());
        };
        let Ok(touch) = FileTouch::try_from(&value) else {
            return Ok(());
        };
        let Ok(connection) = ServiceConnection::connect().await else {
            return Ok(());
        };
        let request_id = AgentRequestId::new();
        if connection
            .send(&ClientMessage::AgentCommand {
                request_id,
                anchor: Some(anchor),
                command: AgentCommand::FileTouched(AgentFileTouched {
                    anchor,
                    path: touch.path,
                    line: touch.line,
                    col: None,
                    end_col: None,
                    kind: touch.kind,
                }),
            })
            .await
            .is_err()
        {
            return Ok(());
        }
        AgentCommandResponse::wait(&connection, request_id).await;
        Ok(())
    }
}

#[derive(Component, Clone)]
struct TurnEndCliRequest {
    anchor: Option<String>,
}

impl TurnEndCliRequest {
    async fn send(self) -> io::Result<()> {
        let Some(anchor) = CliAnchor::optional(self.anchor) else {
            return Ok(());
        };
        let mut input = String::new();
        let _ = io::stdin().read_to_string(&mut input);
        let Ok(connection) = ServiceConnection::connect().await else {
            return Ok(());
        };
        let request_id = AgentRequestId::new();
        if connection
            .send(&ClientMessage::AgentCommand {
                request_id,
                anchor: Some(anchor),
                command: AgentCommand::TurnEnded(AgentTurnEnded { anchor }),
            })
            .await
            .is_err()
        {
            return Ok(());
        }
        AgentCommandResponse::wait(&connection, request_id).await;
        Ok(())
    }
}

#[derive(Component)]
struct AgentCliTask(tokio::task::JoinHandle<Result<u8, String>>);

impl AgentCliTask {
    fn spawn(task: impl Future<Output = io::Result<()>> + Send + 'static) -> Self {
        Self(tokio::spawn(async move {
            task.await.map(|()| 0).map_err(|error| error.to_string())
        }))
    }
}

struct CliAnchor;

impl CliAnchor {
    fn optional(value: Option<String>) -> Option<ProcessId> {
        value
            .or_else(|| std::env::var("VMUX_ANCHOR").ok())
            .and_then(|value| value.parse::<ProcessId>().ok())
    }

    fn strict(value: Option<String>) -> io::Result<Option<ProcessId>> {
        let Some(value) = value.or_else(|| std::env::var("VMUX_ANCHOR").ok()) else {
            return Ok(None);
        };
        value.parse::<ProcessId>().map(Some).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid --anchor: {value}"),
            )
        })
    }
}

struct AgentCommandResponse;

impl AgentCommandResponse {
    async fn wait(connection: &ServiceConnection, request_id: AgentRequestId) {
        let _ = tokio::time::timeout(AGENT_COMMAND_TIMEOUT, async {
            while let Ok(Some(message)) = connection.recv().await {
                if let ServiceMessage::AgentCommandResult {
                    request_id: received,
                    ..
                } = message
                    && received == request_id
                {
                    break;
                }
            }
        })
        .await;
    }
}

#[derive(Debug, PartialEq)]
struct FileTouch {
    path: String,
    line: Option<u32>,
    kind: FileTouchKind,
}

impl TryFrom<&serde_json::Value> for FileTouch {
    type Error = ();

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        let tool = value
            .get("tool_name")
            .and_then(|tool| tool.as_str())
            .unwrap_or("");
        let input = value.get("tool_input").ok_or(())?;
        let path = input
            .get("file_path")
            .and_then(|path| path.as_str())
            .ok_or(())?;
        if !path.starts_with('/') {
            return Err(());
        }
        let kind = match tool {
            "Read" | "read" => FileTouchKind::Read,
            "Edit" | "Write" | "MultiEdit" | "apply_patch" | "edit" | "write" => {
                FileTouchKind::Edit
            }
            _ => return Err(()),
        };
        let line = input
            .get("offset")
            .and_then(|offset| offset.as_u64())
            .map(|offset| offset as u32);
        Ok(Self {
            path: path.to_string(),
            line,
            kind,
        })
    }
}

fn route_agent_cli(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        let mut entity = commands.entity(entity);
        match invocation.command.as_str() {
            "agent.notify" => {
                entity.insert(NotifyCliRequest {
                    title: invocation.value("title").map(str::to_string),
                    body: invocation.value("body").map(str::to_string),
                    anchor: invocation.value("anchor").map(str::to_string),
                });
            }
            "agent.notify_file_touch" => {
                entity.insert(FileTouchCliRequest {
                    anchor: invocation.value("anchor").map(str::to_string),
                });
            }
            "agent.notify_turn_end" => {
                entity.insert(TurnEndCliRequest {
                    anchor: invocation.value("anchor").map(str::to_string),
                });
            }
            _ => {}
        }
    }
}

fn start_notify(
    requests: Query<(Entity, &NotifyCliRequest), Added<NotifyCliRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(AgentCliTask::spawn(request.clone().send()));
    }
}

fn start_file_touch(
    requests: Query<(Entity, &FileTouchCliRequest), Added<FileTouchCliRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(AgentCliTask::spawn(request.clone().send()));
    }
}

fn start_turn_end(
    requests: Query<(Entity, &TurnEndCliRequest), Added<TurnEndCliRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands
            .entity(entity)
            .insert(AgentCliTask::spawn(request.clone().send()));
    }
}

fn poll_agent_cli(mut tasks: Query<(Entity, &mut AgentCliTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        if !task.0.is_finished() {
            continue;
        }
        let result = match bevy::tasks::futures_lite::future::block_on(&mut task.0) {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        commands
            .entity(entity)
            .remove::<AgentCliTask>()
            .insert(CliResult(result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_read_with_offset() {
        let value = serde_json::json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "/a/b.rs", "offset": 120 }
        });
        assert_eq!(
            FileTouch::try_from(&value),
            Ok(FileTouch {
                path: "/a/b.rs".to_string(),
                line: Some(120),
                kind: FileTouchKind::Read,
            })
        );
    }

    #[test]
    fn edit_without_offset() {
        let value = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(
            FileTouch::try_from(&value).unwrap().kind,
            FileTouchKind::Edit
        );
    }

    #[test]
    fn relative_path_is_ignored() {
        let value = serde_json::json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "b.rs" }
        });
        assert_eq!(FileTouch::try_from(&value), Err(()));
    }
}
