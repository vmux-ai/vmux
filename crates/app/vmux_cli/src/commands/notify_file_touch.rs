use std::io::{self, Read};

use bevy_ecs::prelude::*;
use clap::Args;
use vmux_service::client::ServiceConnection;
use vmux_service::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentRequestId, ClientMessage, FileTouchKind, ProcessId,
    ServiceMessage,
};

#[derive(Args, Clone, Component, Debug)]
pub struct NotifyFileTouchRequest {
    #[arg(long)]
    anchor: Option<String>,
}

impl NotifyFileTouchRequest {
    pub(crate) async fn send(self) -> io::Result<()> {
        let anchor = match self.anchor {
            Some(raw) => raw.parse::<ProcessId>().ok(),
            None => std::env::var("VMUX_ANCHOR")
                .ok()
                .and_then(|value| value.parse::<ProcessId>().ok()),
        };
        let Some(anchor) = anchor else {
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
                command: AgentCommand::FileTouched {
                    anchor,
                    path: touch.path,
                    line: touch.line,
                    col: None,
                    end_col: None,
                    kind: touch.kind,
                },
            })
            .await
            .is_err()
        {
            return Ok(());
        }

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

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_read_with_offset() {
        let v = serde_json::json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "/a/b.rs", "offset": 120 }
        });
        assert_eq!(
            FileTouch::try_from(&v),
            Ok(FileTouch {
                path: "/a/b.rs".to_string(),
                line: Some(120),
                kind: FileTouchKind::Read,
            })
        );
    }

    #[test]
    fn claude_edit_no_offset() {
        let v = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/a/b.rs", "old_string": "x", "new_string": "y" }
        });
        assert_eq!(
            FileTouch::try_from(&v),
            Ok(FileTouch {
                path: "/a/b.rs".to_string(),
                line: None,
                kind: FileTouchKind::Edit,
            })
        );
    }

    #[test]
    fn codex_apply_patch_is_edit() {
        let v = serde_json::json!({
            "tool_name": "apply_patch",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(FileTouch::try_from(&v).unwrap().kind, FileTouchKind::Edit);
    }

    #[test]
    fn vibe_lowercase_read() {
        let v = serde_json::json!({
            "tool_name": "read",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(FileTouch::try_from(&v).unwrap().kind, FileTouchKind::Read);
    }

    #[test]
    fn relative_path_skipped() {
        let v = serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": "b.rs" } });
        assert_eq!(FileTouch::try_from(&v), Err(()));
    }

    #[test]
    fn non_file_tool_skipped() {
        let v = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
        assert_eq!(FileTouch::try_from(&v), Err(()));
    }

    #[test]
    fn missing_tool_input_skipped() {
        let v = serde_json::json!({ "tool_name": "Read" });
        assert_eq!(FileTouch::try_from(&v), Err(()));
    }
}
