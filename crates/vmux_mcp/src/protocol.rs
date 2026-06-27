use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};
use vmux_service::protocol::{
    AgentCommand, AgentQuery, AgentQueryResult, AgentRequestId, ClientMessage, FileTouchKind,
    ServiceMessage,
};

/// How long `run` waits for a command to finish before returning a partial
/// result. Kept under vibe's 60s default MCP tool timeout.
const RUN_BLOCK_TIMEOUT: Duration = Duration::from_secs(50);
/// Interval between terminal reads while waiting for the completion marker.
const RUN_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Grace period for the terminal to be created before a "process not found"
/// read is treated as a real error.
const RUN_CREATE_GRACE: Duration = Duration::from_secs(3);

pub fn read_json_line(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        return Ok(None);
    }
    let value = serde_json::from_str(line.trim_end())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(value))
}

pub async fn run_stdio(anchor: Option<vmux_service::protocol::ProcessId>) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    while let Some(message) = read_json_line(&mut reader)? {
        if let Some(response) = handle_message(message, anchor).await {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

async fn handle_message(
    message: Value,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(initialize_result(&params)),
        "tools/list" => Ok(json!({ "tools": crate::tools::tool_definitions() })),
        "tools/call" => tool_call_result(&params, anchor).await,
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {method}")
                }
            }));
        }
    };

    match result {
        Ok(result) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        })),
        Err(message) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": tool_error(&message)
        })),
    }
}

fn initialize_result(params: &Value) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2025-11-25");
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "vmux",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

async fn tool_call_result(
    params: &Value,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if name == "read_file" {
        return read_file_result(&arguments, anchor).await;
    }

    if name == "grep" {
        return grep_result(&arguments, anchor).await;
    }

    match crate::tools::dispatch_with_anchor(name, arguments, anchor)? {
        crate::tools::DispatchTarget::Command(AgentCommand::Run {
            anchor,
            command,
            direction,
            focus,
            beside,
            mode,
            terminal,
            ..
        }) => {
            let run = AgentCommand::Run {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker: None,
            };
            run_blocking(run).await
        }
        crate::tools::DispatchTarget::Command(command) => run_agent_command(command, anchor).await,
        crate::tools::DispatchTarget::Query(query) => run_agent_query(query).await,
    }
}

async fn read_file_result(
    arguments: &Value,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<Value, String> {
    let path = arguments
        .get("path")
        .and_then(Value::as_str)
        .ok_or("read_file.path is required")?;
    if !std::path::Path::new(path).is_absolute() {
        return Err("read_file.path must be an absolute path".to_string());
    }
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .map(|n| n as u32);
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize);
    let content = std::fs::read_to_string(path).map_err(|e| format!("read_file: {e}"))?;
    if let Some(anchor) = anchor {
        let _ = run_agent_command(
            AgentCommand::FileTouched {
                anchor,
                path: path.to_string(),
                line: offset,
                col: None,
                end_col: None,
                kind: FileTouchKind::Read,
            },
            Some(anchor),
        )
        .await;
    }
    let text = slice_lines(&content, offset, limit);
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

const GREP_MAX_FILES: usize = 10;
const GREP_MAX_LINES: usize = 200;

async fn grep_result(
    arguments: &Value,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<Value, String> {
    let query = arguments
        .get("query")
        .and_then(Value::as_str)
        .ok_or("grep.query is required")?;
    let search_path = arguments.get("path").and_then(Value::as_str).unwrap_or(".");

    let output = std::process::Command::new("rg")
        .args(["--json", "--", query, search_path])
        .output()
        .map_err(|e| format!("grep: cannot run rg (is ripgrep installed?): {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut order: Vec<String> = Vec::new();
    let mut first_line: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut first_cols: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    let mut lines_out: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }
        let Some(data) = v.get("data") else { continue };
        let path = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if path.is_empty() {
            continue;
        }
        let lineno = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as u32;
        let text = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_end();
        if !first_line.contains_key(path) {
            first_line.insert(path.to_string(), lineno);
            if let Some(sm) = data
                .get("submatches")
                .and_then(|s| s.as_array())
                .and_then(|a| a.first())
            {
                let s = sm.get("start").and_then(Value::as_u64).unwrap_or(0) as u32;
                let e = sm.get("end").and_then(Value::as_u64).unwrap_or(0) as u32;
                first_cols.insert(path.to_string(), (s, e));
            }
            order.push(path.to_string());
        }
        if lines_out.len() < GREP_MAX_LINES {
            lines_out.push(format!("{path}:{lineno}: {text}"));
        }
    }

    if order.is_empty() {
        return Ok(
            json!({ "content": [{"type": "text", "text": format!("no matches for {query:?}")}] }),
        );
    }

    if let Some(anchor) = anchor {
        for file in order.iter().take(GREP_MAX_FILES) {
            let Ok(abs) = std::fs::canonicalize(file) else {
                continue;
            };
            let _ = run_agent_command(
                AgentCommand::FileTouched {
                    anchor,
                    path: abs.to_string_lossy().to_string(),
                    line: first_line.get(file).copied(),
                    col: first_cols.get(file).map(|c| c.0),
                    end_col: first_cols.get(file).map(|c| c.1),
                    kind: FileTouchKind::Read,
                },
                Some(anchor),
            )
            .await;
        }
    }

    let mut text = lines_out.join("\n");
    if order.len() > GREP_MAX_FILES {
        text.push_str(&format!(
            "\n\u{2026} opened first {GREP_MAX_FILES} of {} matching files",
            order.len()
        ));
    }
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

fn slice_lines(content: &str, offset: Option<u32>, limit: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = offset
        .map(|o| o.saturating_sub(1) as usize)
        .unwrap_or(0)
        .min(lines.len());
    let end = match limit {
        Some(l) => start.saturating_add(l).min(lines.len()),
        None => lines.len(),
    };
    lines[start..end].join("\n")
}

fn output_since(baseline: &str, final_text: &str) -> String {
    final_text
        .strip_prefix(baseline)
        .unwrap_or(final_text)
        .trim_matches('\n')
        .trim_end()
        .to_string()
}

fn run_result(pid: &str, exit: Option<i32>, output: &str, timed_out: bool) -> Value {
    let mut text = format!("terminal: {pid}\n");
    match exit {
        Some(code) => text.push_str(&format!("exit: {code}\n")),
        None if timed_out => text.push_str(&format!(
            "note: still running after {}s; call read_terminal({pid}) to read more\n",
            RUN_BLOCK_TIMEOUT.as_secs()
        )),
        None => {}
    }
    text.push_str("output:\n");
    text.push_str(output);
    json!({ "content": [{"type": "text", "text": text}] })
}

/// Send `run`, then block (polling the full terminal buffer) until the command's
/// completion marker appears, returning the output + exit code in one response.
async fn run_blocking(run: AgentCommand) -> Result<Value, String> {
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;

    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor: None,
            command: run,
        })
        .await
        .map_err(|error| format!("cannot send run command: {error}"))?;

    let pid = loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read service response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentCommandResult {
                request_id: received,
                result,
            } if received == request_id => {
                use vmux_service::protocol::AgentCommandResult;
                match result {
                    AgentCommandResult::Text(pid) => break pid,
                    AgentCommandResult::Error(message) => return Err(message),
                    other => return Err(format!("run: unexpected result: {other:?}")),
                }
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    };

    let process_id = pid
        .parse::<vmux_service::protocol::ProcessId>()
        .map_err(|_| format!("run: service returned an invalid terminal id: {pid}"))?;

    let start = Instant::now();
    let baseline_seq = loop {
        match agent_query(&connection, AgentQuery::CommandExit { process_id }).await? {
            AgentQueryResult::CommandExit { seq, .. } => break seq,
            AgentQueryResult::Error(message) => {
                if start.elapsed() > RUN_CREATE_GRACE {
                    return Err(message);
                }
                tokio::time::sleep(RUN_POLL_INTERVAL).await;
            }
            other => return Err(format!("run: unexpected command-exit result: {other:?}")),
        }
    };
    let baseline_text = read_full_text(&connection, process_id).await;

    let deadline = start + RUN_BLOCK_TIMEOUT;
    loop {
        match agent_query(&connection, AgentQuery::CommandExit { process_id }).await? {
            AgentQueryResult::CommandExit { seq, exit } if seq > baseline_seq => {
                let final_text = read_full_text(&connection, process_id).await;
                let output = output_since(&baseline_text, &final_text);
                return Ok(run_result(&pid, exit, &output, false));
            }
            AgentQueryResult::CommandExit { .. } => {}
            AgentQueryResult::Error(message) => return Err(message),
            other => return Err(format!("run: unexpected command-exit result: {other:?}")),
        }
        if Instant::now() >= deadline {
            let final_text = read_full_text(&connection, process_id).await;
            let output = output_since(&baseline_text, &final_text);
            return Ok(run_result(&pid, None, &output, true));
        }
        tokio::time::sleep(RUN_POLL_INTERVAL).await;
    }
}

async fn agent_query(
    connection: &vmux_service::client::ServiceConnection,
    query: AgentQuery,
) -> Result<AgentQueryResult, String> {
    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentQuery { request_id, query })
        .await
        .map_err(|error| format!("cannot send query: {error}"))?;
    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read query response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentQueryResult {
                request_id: received,
                result,
            } if received == request_id => return Ok(result),
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

async fn read_full_text(
    connection: &vmux_service::client::ServiceConnection,
    process_id: vmux_service::protocol::ProcessId,
) -> String {
    match agent_query(connection, AgentQuery::ReadTerminalFull { process_id }).await {
        Ok(AgentQueryResult::Text(text)) => text,
        _ => String::new(),
    }
}

async fn run_agent_command(
    command: AgentCommand,
    anchor: Option<vmux_service::protocol::ProcessId>,
) -> Result<Value, String> {
    let request_id = vmux_service::protocol::AgentRequestId::new();
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor,
            command,
        })
        .await
        .map_err(|error| format!("cannot send agent command: {error}"))?;

    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read service response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentCommandResult {
                request_id: received,
                result,
            } if received == request_id => {
                return command_result_to_mcp_response(result);
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

pub fn command_result_to_mcp_response(
    result: vmux_service::protocol::AgentCommandResult,
) -> Result<Value, String> {
    use vmux_service::protocol::AgentCommandResult;
    match result {
        AgentCommandResult::Ok => Ok(json!({
            "content": [{"type": "text", "text": "ok"}]
        })),
        AgentCommandResult::Text(text) => Ok(json!({
            "content": [{"type": "text", "text": text}]
        })),
        AgentCommandResult::Layout(snapshot) => {
            let text = serde_json::to_string(&snapshot).unwrap_or_default();
            Ok(json!({
                "content": [{"type": "text", "text": text}]
            }))
        }
        AgentCommandResult::Error(message) => Err(message),
    }
}

async fn run_agent_query(query: vmux_service::protocol::AgentQuery) -> Result<Value, String> {
    let request_id = vmux_service::protocol::AgentRequestId::new();
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentQuery { request_id, query })
        .await
        .map_err(|error| format!("cannot send agent query: {error}"))?;

    loop {
        let Some(message) = connection
            .recv()
            .await
            .map_err(|error| format!("cannot read service response: {error}"))?
        else {
            return Err("vmux_service disconnected".to_string());
        };
        match message {
            ServiceMessage::AgentQueryResult {
                request_id: received,
                result,
            } if received == request_id => {
                return Ok(query_result_to_mcp_response(result));
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

pub fn query_result_to_mcp_response(result: vmux_service::protocol::AgentQueryResult) -> Value {
    use vmux_service::protocol::AgentQueryResult;
    match result {
        AgentQueryResult::Layout(snapshot) => {
            let text = serde_json::to_string(&snapshot).unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Text(text) => {
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Settings(json_str) => {
            json!({
                "content": [{"type": "text", "text": json_str}]
            })
        }
        AgentQueryResult::Spaces(json_str) => {
            json!({
                "content": [{"type": "text", "text": json_str}]
            })
        }
        AgentQueryResult::CommandExit { seq, exit } => {
            let exit = exit.map_or_else(|| "null".to_string(), |code| code.to_string());
            json!({
                "content": [{"type": "text", "text": format!("{{\"seq\":{seq},\"exit\":{exit}}}")}]
            })
        }
        AgentQueryResult::Image {
            path,
            png,
            width,
            height,
        } => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(&png);
            json!({
                "content": [
                    {"type": "text", "text": format!("saved {path} ({width}×{height})")},
                    {"type": "image", "data": data, "mimeType": "image/png"}
                ]
            })
        }
        AgentQueryResult::Recording {
            mp4_path,
            gif_path,
            duration_ms,
            bytes,
            auto_stopped,
        } => {
            let secs = duration_ms as f64 / 1000.0;
            let mut text = format!("recorded {secs:.1}s → {mp4_path} ({bytes} bytes)");
            if let Some(g) = gif_path {
                text.push_str(&format!(" + {g}"));
            }
            if auto_stopped {
                text.push_str(" (auto-stopped)");
            }
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
        AgentQueryResult::Error(message) => {
            json!({
                "isError": true,
                "content": [{"type": "text", "text": message}]
            })
        }
    }
}

pub fn tool_error(message: &str) -> Value {
    json!({
        "isError": true,
        "content": [
            {
                "type": "text",
                "text": message
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_lines_offset_and_limit() {
        let content = "a\nb\nc\nd\ne";
        assert_eq!(slice_lines(content, None, None), "a\nb\nc\nd\ne");
        assert_eq!(slice_lines(content, Some(2), Some(2)), "b\nc");
        assert_eq!(slice_lines(content, Some(4), None), "d\ne");
        assert_eq!(slice_lines(content, Some(99), None), "");
        assert_eq!(slice_lines(content, Some(1), Some(100)), "a\nb\nc\nd\ne");
    }

    #[test]
    fn newline_framing_reads_single_json_message() {
        let mut lines = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".as_slice();
        let request = read_json_line(&mut lines).unwrap().unwrap();

        assert_eq!(request["method"], "tools/list");
    }

    #[test]
    fn image_query_result_maps_to_text_and_image_blocks() {
        use vmux_service::protocol::AgentQueryResult;
        let resp = query_result_to_mcp_response(AgentQueryResult::Image {
            path: "/tmp/shot.png".into(),
            png: vec![137, 80, 78, 71],
            width: 800,
            height: 600,
        });
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("/tmp/shot.png")
        );
        assert!(content[0]["text"].as_str().unwrap().contains("800"));
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["mimeType"], "image/png");
        assert_eq!(content[1]["data"], "iVBORw==");
    }

    #[test]
    fn recording_maps_to_text_block() {
        use vmux_service::protocol::AgentQueryResult;
        let v = query_result_to_mcp_response(AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_000_000,
            auto_stopped: true,
        });
        let text = v["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("/tmp/x.mp4"));
        assert!(text.contains("/tmp/x.gif"));
        assert!(text.contains("auto-stopped"));
        assert!(v.get("isError").is_none());
    }

    #[test]
    fn output_since_returns_appended_tail() {
        let baseline = "prompt$ ";
        let final_text = "prompt$ ls\nfile_a\nfile_b\nprompt$ ";
        assert_eq!(
            output_since(baseline, final_text),
            "ls\nfile_a\nfile_b\nprompt$"
        );
    }

    #[test]
    fn output_since_falls_back_to_full_when_prefix_shifted() {
        let baseline = "old prompt$ ";
        let final_text = "different\noutput here";
        assert_eq!(output_since(baseline, final_text), "different\noutput here");
    }

    #[test]
    fn run_result_shapes_text() {
        let done = run_result("pid7", Some(1), "boom", false);
        let text = done["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("terminal: pid7"));
        assert!(text.contains("exit: 1"));
        assert!(text.contains("output:\nboom"));

        let timeout = run_result("pid7", None, "partial", true);
        let text = timeout["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("still running"));
        assert!(text.contains("read_terminal(pid7)"));
    }
}
