use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};
use vmux_service::protocol::{
    AgentCommand, AgentQuery, AgentQueryResult, AgentRequestId, ClientMessage, ServiceMessage,
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
        crate::tools::DispatchTarget::Command(command) => run_agent_command(command).await,
        crate::tools::DispatchTarget::Query(query) => run_agent_query(query).await,
    }
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

async fn run_agent_command(command: AgentCommand) -> Result<Value, String> {
    let request_id = vmux_service::protocol::AgentRequestId::new();
    let connection = vmux_service::client::ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    connection
        .send(&ClientMessage::AgentCommand {
            request_id,
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
                use vmux_service::protocol::AgentCommandResult;
                return match result {
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
                };
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
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

fn query_result_to_mcp_response(result: vmux_service::protocol::AgentQueryResult) -> Value {
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
        AgentQueryResult::Error(message) => {
            json!({
                "isError": true,
                "content": [{"type": "text", "text": message}]
            })
        }
    }
}

fn tool_error(message: &str) -> Value {
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
    fn newline_framing_reads_single_json_message() {
        let mut lines = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".as_slice();
        let request = read_json_line(&mut lines).unwrap().unwrap();

        assert_eq!(request["method"], "tools/list");
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
