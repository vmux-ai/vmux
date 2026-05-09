use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use vmux_service::protocol::{AgentCommand, ClientMessage, ServiceMessage};

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

pub async fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    while let Some(message) = read_json_line(&mut reader)? {
        if let Some(response) = handle_message(message).await {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

async fn handle_message(message: Value) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(initialize_result(&params)),
        "tools/list" => Ok(json!({ "tools": crate::tools::tool_definitions() })),
        "tools/call" => tool_call_result(&params).await,
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

async fn tool_call_result(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match crate::tools::dispatch_from_tool_call(name, arguments)? {
        crate::tools::DispatchTarget::Command(command) => run_agent_command(command).await,
        crate::tools::DispatchTarget::Query(query) => run_agent_query(query).await,
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
    let payload = match result {
        AgentQueryResult::State(snapshot) => json!({
            "spaces": snapshot.spaces.iter().map(space_info_to_json).collect::<Vec<_>>(),
            "focused": focused_info_to_json(&snapshot.focused),
        }),
        AgentQueryResult::Tabs(tabs) => json!({
            "tabs": tabs.iter().map(tab_info_to_json).collect::<Vec<_>>(),
        }),
        AgentQueryResult::Spaces(spaces) => json!({
            "spaces": spaces.iter().map(space_info_to_json).collect::<Vec<_>>(),
        }),
        AgentQueryResult::Terminals(terminals) => json!({
            "terminals": terminals.iter().map(terminal_info_to_json).collect::<Vec<_>>(),
        }),
        AgentQueryResult::Focused(focused) => focused_info_to_json(&focused),
        AgentQueryResult::Error(message) => {
            return json!({
                "isError": true,
                "content": [{"type": "text", "text": message}]
            });
        }
    };
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(&payload).unwrap_or_default()
            }
        ]
    })
}

fn tab_info_to_json(t: &vmux_service::protocol::TabInfo) -> Value {
    json!({"id": t.id, "title": t.title, "url": t.url, "kind": t.kind})
}

fn terminal_info_to_json(t: &vmux_service::protocol::TerminalInfo) -> Value {
    json!({"id": t.id, "cwd": t.cwd, "pid": t.pid})
}

fn pane_info_to_json(p: &vmux_service::protocol::PaneInfo) -> Value {
    json!({
        "id": p.id,
        "tabs": p.tabs.iter().map(tab_info_to_json).collect::<Vec<_>>(),
    })
}

fn space_info_to_json(s: &vmux_service::protocol::SpaceInfo) -> Value {
    json!({
        "id": s.id,
        "name": s.name,
        "active": s.active,
        "panes": s.panes.iter().map(pane_info_to_json).collect::<Vec<_>>(),
    })
}

fn focused_info_to_json(f: &vmux_service::protocol::FocusedInfo) -> Value {
    json!({"space": f.space, "pane": f.pane, "tab": f.tab})
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
}
