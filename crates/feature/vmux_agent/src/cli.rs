use std::io::{self, Read};

use bevy::app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::protocol::{
    AGENT_REQUEST_TIMEOUT, AgentCommandResult, AgentFileTouched, AgentNotify, AgentRequest,
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
                    poll_notify,
                    poll_file_touch,
                    poll_turn_end,
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

#[derive(Component, Clone)]
struct FileTouchCliRequest {
    anchor: Option<String>,
}

#[derive(Component, Clone)]
struct TurnEndCliRequest {
    anchor: Option<String>,
}

#[derive(Component)]
struct NotifyCliTask(tokio::task::JoinHandle<io::Result<()>>);

#[derive(Component)]
struct FileTouchCliTask(tokio::task::JoinHandle<io::Result<()>>);

#[derive(Component)]
struct TurnEndCliTask(tokio::task::JoinHandle<io::Result<()>>);

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
        let anchor = request
            .anchor
            .clone()
            .or_else(|| std::env::var("VMUX_ANCHOR").ok());
        let anchor = match anchor {
            Some(value) => match value.parse::<ProcessId>() {
                Ok(anchor) => Some(anchor),
                Err(_) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(format!("invalid --anchor: {value}"))));
                    continue;
                }
            },
            None => None,
        };
        let title = request.title.clone();
        let body = request.body.clone();
        let task = tokio::spawn(async move {
            let connection = match ServiceConnection::connect().await {
                Ok(connection) => connection,
                Err(error) => {
                    eprintln!("vmux notify: cannot connect to vmux service: {error}");
                    return Ok(());
                }
            };
            let request_id = AgentRequestId::new();
            let Ok(request) = AgentRequest::encode(&AgentNotify { title, body }) else {
                return Ok(());
            };
            if let Err(error) = connection
                .send(&ClientMessage::AgentRequest {
                    request_id,
                    anchor,
                    request,
                })
                .await
            {
                eprintln!("vmux notify: failed to send: {error}");
                return Ok(());
            }
            let _ = tokio::time::timeout(AGENT_REQUEST_TIMEOUT, async {
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
        });
        commands.entity(entity).insert(NotifyCliTask(task));
    }
}

fn start_file_touch(
    requests: Query<(Entity, &FileTouchCliRequest), Added<FileTouchCliRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let Some(anchor) = request
            .anchor
            .clone()
            .or_else(|| std::env::var("VMUX_ANCHOR").ok())
            .and_then(|value| value.parse::<ProcessId>().ok())
        else {
            commands.entity(entity).insert(CliResult::success());
            continue;
        };
        let task = tokio::spawn(async move {
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
            let Ok(request) = AgentRequest::encode(&AgentFileTouched {
                anchor,
                path: touch.path,
                line: touch.line,
                col: None,
                end_col: None,
                kind: touch.kind,
            }) else {
                return Ok(());
            };
            if connection
                .send(&ClientMessage::AgentRequest {
                    request_id,
                    anchor: Some(anchor),
                    request,
                })
                .await
                .is_err()
            {
                return Ok(());
            }
            let _ = tokio::time::timeout(AGENT_REQUEST_TIMEOUT, async {
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
            Ok(())
        });
        commands.entity(entity).insert(FileTouchCliTask(task));
    }
}

fn start_turn_end(
    requests: Query<(Entity, &TurnEndCliRequest), Added<TurnEndCliRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let Some(anchor) = request
            .anchor
            .clone()
            .or_else(|| std::env::var("VMUX_ANCHOR").ok())
            .and_then(|value| value.parse::<ProcessId>().ok())
        else {
            commands.entity(entity).insert(CliResult::success());
            continue;
        };
        let task = tokio::spawn(async move {
            let mut input = String::new();
            let _ = io::stdin().read_to_string(&mut input);
            let Ok(connection) = ServiceConnection::connect().await else {
                return Ok(());
            };
            let request_id = AgentRequestId::new();
            let Ok(request) = AgentRequest::encode(&AgentTurnEnded { anchor }) else {
                return Ok(());
            };
            if connection
                .send(&ClientMessage::AgentRequest {
                    request_id,
                    anchor: Some(anchor),
                    request,
                })
                .await
                .is_err()
            {
                return Ok(());
            }
            let _ = tokio::time::timeout(AGENT_REQUEST_TIMEOUT, async {
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
            Ok(())
        });
        commands.entity(entity).insert(TurnEndCliTask(task));
    }
}

fn poll_notify(mut tasks: Query<(Entity, &mut NotifyCliTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        if !task.0.is_finished() {
            continue;
        }
        let result = match bevy::tasks::futures_lite::future::block_on(&mut task.0) {
            Ok(result) => result.map(|()| 0).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        commands
            .entity(entity)
            .remove::<NotifyCliTask>()
            .insert(CliResult(result));
    }
}

fn poll_file_touch(mut tasks: Query<(Entity, &mut FileTouchCliTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        if !task.0.is_finished() {
            continue;
        }
        let result = match bevy::tasks::futures_lite::future::block_on(&mut task.0) {
            Ok(result) => result.map(|()| 0).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        commands
            .entity(entity)
            .remove::<FileTouchCliTask>()
            .insert(CliResult(result));
    }
}

fn poll_turn_end(mut tasks: Query<(Entity, &mut TurnEndCliTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        if !task.0.is_finished() {
            continue;
        }
        let result = match bevy::tasks::futures_lite::future::block_on(&mut task.0) {
            Ok(result) => result.map(|()| 0).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        commands
            .entity(entity)
            .remove::<TurnEndCliTask>()
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
