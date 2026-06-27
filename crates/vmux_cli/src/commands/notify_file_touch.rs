use std::io::{self, Read};

use vmux_service::client::ServiceConnection;
use vmux_service::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentRequestId, ClientMessage, FileTouchKind, ProcessId,
    ServiceMessage,
};

/// Parse a tool-hook JSON payload (Claude PostToolUse / Vibe after_tool / Codex
/// PostToolUse) into a file touch. `None` if it is not a file read/edit or
/// carries no absolute path.
pub fn parse_touch(v: &serde_json::Value) -> Option<(String, Option<u32>, FileTouchKind)> {
    let tool = v.get("tool_name").and_then(|t| t.as_str()).unwrap_or("");
    let input = v.get("tool_input")?;
    let path = input.get("file_path").and_then(|p| p.as_str())?;
    if !path.starts_with('/') {
        return None;
    }
    let kind = match tool {
        "Read" | "read" => FileTouchKind::Read,
        "Edit" | "Write" | "MultiEdit" | "apply_patch" | "edit" | "write" => FileTouchKind::Edit,
        _ => return None,
    };
    let line = input
        .get("offset")
        .and_then(|o| o.as_u64())
        .map(|o| o as u32);
    Some((path.to_string(), line, kind))
}

pub async fn run(anchor: Option<String>) -> io::Result<()> {
    let anchor = match anchor {
        Some(raw) => raw.parse::<ProcessId>().ok(),
        None => std::env::var("VMUX_ANCHOR")
            .ok()
            .and_then(|s| s.parse::<ProcessId>().ok()),
    };
    let Some(anchor) = anchor else {
        return Ok(());
    };

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&buf) else {
        return Ok(());
    };
    let Some((path, line, kind)) = parse_touch(&value) else {
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
                path,
                line,
                kind,
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
            parse_touch(&v),
            Some(("/a/b.rs".to_string(), Some(120), FileTouchKind::Read))
        );
    }

    #[test]
    fn claude_edit_no_offset() {
        let v = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/a/b.rs", "old_string": "x", "new_string": "y" }
        });
        assert_eq!(
            parse_touch(&v),
            Some(("/a/b.rs".to_string(), None, FileTouchKind::Edit))
        );
    }

    #[test]
    fn codex_apply_patch_is_edit() {
        let v = serde_json::json!({
            "tool_name": "apply_patch",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(parse_touch(&v).unwrap().2, FileTouchKind::Edit);
    }

    #[test]
    fn vibe_lowercase_read() {
        let v = serde_json::json!({
            "tool_name": "read",
            "tool_input": { "file_path": "/a/b.rs" }
        });
        assert_eq!(parse_touch(&v).unwrap().2, FileTouchKind::Read);
    }

    #[test]
    fn relative_path_skipped() {
        let v = serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": "b.rs" } });
        assert_eq!(parse_touch(&v), None);
    }

    #[test]
    fn non_file_tool_skipped() {
        let v = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
        assert_eq!(parse_touch(&v), None);
    }

    #[test]
    fn missing_tool_input_skipped() {
        let v = serde_json::json!({ "tool_name": "Read" });
        assert_eq!(parse_touch(&v), None);
    }
}
