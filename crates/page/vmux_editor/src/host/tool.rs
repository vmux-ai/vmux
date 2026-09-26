use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vmux_api::protocol::{
    AgentCommand, AgentQuery, AgentRequestId, ClientMessage, FileTouchKind, ProcessId,
    ServiceMessage,
};
use vmux_core::{JsonArguments, ProcessAnchor};
use vmux_mcp::protocol::McpExecution;
use vmux_service::client::ServiceConnection;
use vmux_tool::{
    AddedTool, ToolDispatchError, ToolDispatchSet, ToolManifestPlugin, ToolRequestSet,
};

pub struct FileToolPlugin;

impl Plugin for FileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::<FileTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(Update, (read_file, grep).in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FileTool {
    ReadFile,
    Grep,
}

#[derive(Component, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadFileArgs {
    path: String,
    offset: Option<std::num::NonZeroU32>,
    limit: Option<usize>,
}

#[derive(Component, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GrepArgs {
    query: String,
    path: Option<String>,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &FileTool), AddedTool<FileTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed = match tool {
            FileTool::ReadFile => arguments.parse::<ReadFileArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
            FileTool::Grep => arguments.parse::<GrepArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn read_file(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &ReadFileArgs), Added<ReadFileArgs>>,
    protocol_requests: Query<(), With<vmux_mcp::protocol::McpRequest>>,
) {
    for (entity, name, anchor, args) in &requests {
        if protocol_requests.contains(entity) {
            commands
                .entity(entity)
                .insert(McpExecution::new(read_file_result(
                    args.path.clone(),
                    args.offset.map(std::num::NonZeroU32::get),
                    args.limit,
                    anchor.map(|anchor| anchor.0),
                )));
        } else {
            commands
                .entity(entity)
                .insert(ToolDispatchError::new(format!(
                    "tool {} requires MCP protocol context",
                    name.as_str()
                )));
        }
    }
}

fn grep(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &GrepArgs), Added<GrepArgs>>,
    protocol_requests: Query<(), With<vmux_mcp::protocol::McpRequest>>,
) {
    for (entity, name, anchor, args) in &requests {
        if protocol_requests.contains(entity) {
            commands
                .entity(entity)
                .insert(McpExecution::new(grep_result(
                    args.query.clone(),
                    args.path.clone(),
                    anchor.map(|anchor| anchor.0),
                )));
        } else {
            commands
                .entity(entity)
                .insert(ToolDispatchError::new(format!(
                    "tool {} requires MCP protocol context",
                    name.as_str()
                )));
        }
    }
}

async fn agent_working_directory(anchor: Option<ProcessId>) -> Result<PathBuf, String> {
    let Some(anchor) = anchor else {
        return std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("cannot resolve current directory: {error}"));
    };
    let connection = ServiceConnection::connect()
        .await
        .map_err(|error| format!("cannot connect to vmux_service: {error}"))?;
    let request_id = AgentRequestId::new();
    connection
        .send(&ClientMessage::AgentQuery {
            request_id,
            query: AgentQuery::WorkingDirectory { anchor },
        })
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
            ServiceMessage::AgentWorkingDirectoryResult {
                request_id: received,
                result,
            } if received == request_id => {
                return PathBuf::from(result?)
                    .canonicalize()
                    .map_err(|error| format!("cannot resolve agent working directory: {error}"));
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}

fn resolve_scoped_existing_path(scope: &Path, requested: &Path) -> Option<PathBuf> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        scope.join(requested)
    };
    let path = path.canonicalize().ok()?;
    path.starts_with(scope).then_some(path)
}

async fn scoped_existing_path(
    anchor: Option<ProcessId>,
    requested: &Path,
    tool: &str,
) -> Result<PathBuf, String> {
    let scope = agent_working_directory(anchor).await?;
    resolve_scoped_existing_path(&scope, requested).ok_or_else(|| {
        format!(
            "{tool}: path is outside the selected project; call select_project and wait for user approval first"
        )
    })
}

async fn read_file_result(
    requested: String,
    offset: Option<u32>,
    limit: Option<usize>,
    anchor: Option<ProcessId>,
) -> Result<Value, String> {
    if !Path::new(&requested).is_absolute() {
        return Err("read_file.path must be an absolute path".to_string());
    }
    let path = scoped_existing_path(anchor, Path::new(&requested), "read_file").await?;
    let metadata = std::fs::metadata(&path).map_err(|error| format!("read_file: {error}"))?;
    if !metadata.is_file() {
        return Err("read_file: not a regular file".to_string());
    }
    let path_text = path.to_string_lossy();
    let text = read_lines_bounded(&path_text, offset, limit)
        .map_err(|error| format!("read_file: {error}"))?;
    if let Some(anchor) = anchor {
        let _ = run_agent_command(
            AgentCommand::FileTouched {
                anchor,
                path: path.to_string_lossy().into_owned(),
                line: offset,
                col: None,
                end_col: None,
                kind: FileTouchKind::Read,
            },
            Some(anchor),
        )
        .await;
    }
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

const GREP_MAX_FILES: usize = 10;
const GREP_MAX_LINES: usize = 200;

async fn grep_result(
    query: String,
    requested: Option<String>,
    anchor: Option<ProcessId>,
) -> Result<Value, String> {
    let requested = requested.as_deref().unwrap_or(".");
    let search_path = scoped_existing_path(anchor, Path::new(requested), "grep").await?;

    use std::io::{BufRead, Read};
    let mut child = std::process::Command::new("rg")
        .args(["--json", "--", &query])
        .arg(&search_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("grep: cannot run rg (is ripgrep installed?): {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("grep: failed to capture rg output")?;
    let mut stderr_pipe = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stderr) = stderr_pipe.as_mut() {
            let _ = stderr.read_to_string(&mut buffer);
        }
        buffer
    });

    let mut order = Vec::new();
    let mut first_line = std::collections::HashMap::new();
    let mut first_cols = std::collections::HashMap::new();
    let mut lines_out = Vec::new();
    let mut search_matches = Vec::new();
    let mut capped = false;
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }
        let Some(data) = value.get("data") else {
            continue;
        };
        let path = data
            .get("path")
            .and_then(|path| path.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if path.is_empty() {
            continue;
        }
        let line = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as u32;
        let raw = data
            .get("lines")
            .and_then(|lines| lines.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let first_match = data
            .get("submatches")
            .and_then(Value::as_array)
            .and_then(|matches| matches.first());
        let cols = first_match.map(|matched| {
            let start = matched.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
            let end = matched.get("end").and_then(Value::as_u64).unwrap_or(0) as usize;
            (byte_to_utf16(raw, start), byte_to_utf16(raw, end))
        });
        if !first_line.contains_key(path) {
            first_line.insert(path.to_string(), line);
            if let Some(cols) = cols {
                first_cols.insert(path.to_string(), cols);
            }
            order.push(path.to_string());
        }
        if lines_out.len() < GREP_MAX_LINES {
            lines_out.push(format!("{path}:{line}: {}", raw.trim_end()));
            let (col, end_col) = cols.unwrap_or((0, 0));
            search_matches.push((
                path.to_string(),
                line,
                col,
                end_col,
                raw.trim_end().to_string(),
            ));
        }
        if lines_out.len() >= GREP_MAX_LINES || order.len() >= GREP_MAX_FILES {
            capped = true;
            break;
        }
    }
    if capped {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|error| format!("grep: {error}"))?;
    let stderr = stderr_handle.join().unwrap_or_default();

    if order.is_empty() {
        if !capped && status.code() != Some(0) && status.code() != Some(1) {
            let error = stderr.trim();
            return Err(if error.is_empty() {
                "grep: rg failed".to_string()
            } else {
                format!("grep: {error}")
            });
        }
        return Ok(
            json!({ "content": [{"type": "text", "text": format!("no matches for {query:?}")}] }),
        );
    }

    if let Some(anchor) = anchor {
        if let Some(file) = order.first()
            && let Ok(path) = std::fs::canonicalize(file)
        {
            let _ = run_agent_command(
                AgentCommand::FileTouched {
                    anchor,
                    path: path.to_string_lossy().into_owned(),
                    line: first_line.get(file).copied(),
                    col: first_cols.get(file).map(|cols| cols.0),
                    end_col: first_cols.get(file).map(|cols| cols.1),
                    kind: FileTouchKind::Read,
                },
                Some(anchor),
            )
            .await;
        }
        let mut canonical_paths = std::collections::HashMap::new();
        let matches = search_matches
            .into_iter()
            .filter_map(|(path, line, col, end_col, preview)| {
                let canonical = canonical_paths
                    .entry(path.clone())
                    .or_insert_with(|| std::fs::canonicalize(&path).ok())
                    .clone()?;
                Some(vmux_api::protocol::FileSearchMatch {
                    path: canonical.to_string_lossy().into_owned(),
                    line,
                    col,
                    end_col,
                    preview,
                })
            })
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            let _ = run_agent_command(
                AgentCommand::FileSearch {
                    anchor,
                    root: search_path.to_string_lossy().into_owned(),
                    query: query.clone(),
                    matches,
                },
                Some(anchor),
            )
            .await;
        }
    }

    let mut text = lines_out.join("\n");
    if capped {
        text.push_str(&format!(
            "\n… results truncated at {GREP_MAX_FILES} files / {GREP_MAX_LINES} lines; refine the query"
        ));
    } else if order.len() > GREP_MAX_FILES {
        text.push_str(&format!(
            "\n… opened first {GREP_MAX_FILES} of {} matching files",
            order.len()
        ));
    }
    Ok(json!({ "content": [{"type": "text", "text": text}] }))
}

const READ_FILE_DEFAULT_LINES: usize = 2000;
const READ_FILE_MAX_LINES: usize = 50_000;

fn read_lines_bounded(
    path: &str,
    offset: Option<u32>,
    limit: Option<usize>,
) -> std::io::Result<String> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let start = offset
        .map(|offset| offset.saturating_sub(1) as usize)
        .unwrap_or(0);
    let take = limit
        .map(|limit| limit.min(READ_FILE_MAX_LINES))
        .unwrap_or(READ_FILE_DEFAULT_LINES);
    let mut output = Vec::new();
    for line in reader.lines().skip(start).take(take) {
        output.push(line?);
    }
    Ok(output.join("\n"))
}

fn byte_to_utf16(line: &str, byte: usize) -> u32 {
    let mut index = byte.min(line.len());
    while index > 0 && !line.is_char_boundary(index) {
        index -= 1;
    }
    line[..index].encode_utf16().count() as u32
}

async fn run_agent_command(command: AgentCommand, anchor: Option<ProcessId>) -> Result<(), String> {
    let request_id = AgentRequestId::new();
    let connection = ServiceConnection::connect()
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
                return match result {
                    vmux_api::protocol::AgentCommandResult::Error(message) => Err(message),
                    _ => Ok(()),
                };
            }
            ServiceMessage::Error { message } => return Err(message),
            _ => {}
        }
    }
}
