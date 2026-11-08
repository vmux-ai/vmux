use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use vmux_api::session::{AssistantBlock, Message, ToolKind};

use super::{BoundedLines, PROVIDER_LINE_LIMIT, PROVIDER_READ_LIMIT};

pub(super) struct ReadResult {
    pub path: Option<PathBuf>,
    pub messages: Vec<Message>,
    pub cursor: u64,
    pub tail: bool,
    pub complete: bool,
}

pub(super) struct Record {
    pub id: String,
    pub message: Message,
}

pub(super) struct ChatDecoder;

impl ChatDecoder {
    pub(super) fn record(value: &serde_json::Value) -> Vec<Message> {
        if value.get("isMeta").and_then(serde_json::Value::as_bool) == Some(true)
            || value
                .get("isSidechain")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            || value.get("injected").and_then(serde_json::Value::as_bool) == Some(true)
                && value.get("manual_shell").is_none()
        {
            return Vec::new();
        }
        let message = value
            .get("message")
            .filter(|message| message.is_object())
            .unwrap_or(value);
        let role = message
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let content = message
            .get("userDisplayContent")
            .filter(|content| !content.is_null())
            .or_else(|| message.get("content"));
        let mut text = Vec::new();
        let mut blocks = Vec::new();
        let mut results = Vec::new();
        if let Some(reasoning) = message
            .get("reasoning_content")
            .and_then(serde_json::Value::as_str)
            && !reasoning.is_empty()
        {
            blocks.push(AssistantBlock::Thinking(reasoning.into()));
        }
        if let Some(content) = content {
            if let Some(value) = content.as_str() {
                text.push(value.to_string());
            }
            if let Some(values) = content.as_array() {
                for block in values {
                    match block
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                    {
                        "text" | "input_text" | "output_text" => {
                            if let Some(value) =
                                block.get("text").and_then(serde_json::Value::as_str)
                            {
                                text.push(value.to_string());
                            }
                        }
                        "thinking" => {
                            if let Some(value) =
                                block.get("thinking").and_then(serde_json::Value::as_str)
                            {
                                blocks.push(AssistantBlock::Thinking(value.into()));
                            }
                        }
                        "tool_use" => {
                            blocks.push(AssistantBlock::ToolUse {
                                call_id: block
                                    .get("id")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                                name: block
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                                args: block
                                    .get("input")
                                    .map(serde_json::Value::to_string)
                                    .unwrap_or_else(|| "{}".into()),
                                kind: None,
                                parent_call_id: None,
                            });
                        }
                        "tool_result" => results.push(Message::ToolResult {
                            call_id: block
                                .get("tool_use_id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .into(),
                            content: block
                                .get("content")
                                .map(|value| JsonText::new(value).text())
                                .unwrap_or_default(),
                            is_error: block
                                .get("is_error")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        }),
                        _ => {}
                    }
                }
            }
        }
        if let Some(calls) = message
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
        {
            for call in calls {
                let function = call.get("function").unwrap_or(call);
                let arguments = function
                    .get("arguments")
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| value.to_string())
                    })
                    .unwrap_or_else(|| "{}".into());
                blocks.push(AssistantBlock::ToolUse {
                    call_id: call
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    name: function
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    args: arguments,
                    kind: None,
                    parent_call_id: None,
                });
            }
        }
        let text = text.join("\n");
        let mut messages = Vec::new();
        match role {
            "user" if !text.is_empty() || results.is_empty() => messages.push(Message::user(text)),
            "assistant" => {
                if !text.is_empty() {
                    blocks.insert(0, AssistantBlock::Text(text));
                }
                if !blocks.is_empty() {
                    messages.push(Message::Assistant { blocks });
                }
            }
            "tool" => messages.push(Message::ToolResult {
                call_id: message
                    .get("tool_call_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                content: content
                    .map(|value| JsonText::new(value).text())
                    .unwrap_or_default(),
                is_error: message
                    .get("is_error")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            }),
            _ => {}
        }
        messages.extend(results);
        messages
    }

    pub(super) fn history(value: &serde_json::Value) -> Vec<Record> {
        let Some(entries) = value.as_array() else {
            return Vec::new();
        };
        let mut records = Vec::new();
        for entry in entries {
            let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let kind = entry
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            match kind {
                "message" => {
                    for (index, message) in Self::record(entry).into_iter().enumerate() {
                        records.push(Record {
                            id: format!("{id}/message/{index}"),
                            message,
                        });
                    }
                }
                "reasoning" => {
                    let text = entry
                        .get("text")
                        .or_else(|| entry.get("summary"))
                        .map(|value| JsonText::new(value).text())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        records.push(Record {
                            id: id.into(),
                            message: Message::Assistant {
                                blocks: vec![AssistantBlock::Thinking(text)],
                            },
                        });
                    }
                }
                "effect" => {
                    let empty = serde_json::Value::Null;
                    let detail = entry.get("detail").unwrap_or(&empty);
                    let state = entry.get("state").unwrap_or(&empty);
                    let name = detail
                        .get("toolName")
                        .or_else(|| entry.get("title"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("tool");
                    let tool_kind = match detail
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                    {
                        "shell" => Some(ToolKind::Execute),
                        "file_read" => Some(ToolKind::Read),
                        "file_search" => Some(ToolKind::Search),
                        "file_edit" | "file_write" => Some(ToolKind::Edit),
                        _ => None,
                    };
                    records.push(Record {
                        id: format!("{id}/use"),
                        message: Message::Assistant {
                            blocks: vec![AssistantBlock::ToolUse {
                                call_id: id.into(),
                                name: name.into(),
                                args: detail
                                    .get("input")
                                    .map(serde_json::Value::to_string)
                                    .unwrap_or_else(|| "{}".into()),
                                kind: tool_kind,
                                parent_call_id: None,
                            }],
                        },
                    });
                    let status = state
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if matches!(status, "completed" | "failed" | "cancelled" | "skipped") {
                        let output = state
                            .get("outputText")
                            .filter(|value| !value.is_null())
                            .or_else(|| state.get("output"))
                            .or_else(|| state.get("error"));
                        records.push(Record {
                            id: format!("{id}/result"),
                            message: Message::ToolResult {
                                call_id: id.into(),
                                content: output
                                    .map(|value| JsonText::new(value).text())
                                    .unwrap_or_default(),
                                is_error: matches!(status, "failed" | "cancelled"),
                            },
                        });
                    }
                }
                _ => {}
            }
        }
        records
    }
}

impl Decoder for ChatDecoder {
    fn messages(&mut self, line: &str) -> Vec<Message> {
        let Ok(value) = serde_json::from_str(line) else {
            return Vec::new();
        };
        Self::record(&value)
    }
}

pub(super) struct Files;

impl Files {
    pub(super) fn locate(
        root: &Path,
        sid: &str,
        metadata: bool,
    ) -> Result<Option<PathBuf>, String> {
        if !Self::valid_id(sid) {
            return Err("invalid session history identifier".into());
        }
        let directories = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        for entry in directories.take(10_000) {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                continue;
            }
            let directory = entry.path();
            if metadata {
                let meta = directory.join("meta.json");
                if !std::fs::symlink_metadata(&meta).is_ok_and(|meta| meta.file_type().is_file()) {
                    continue;
                }
                let file = File::open(meta).map_err(|error| error.to_string())?;
                let value: serde_json::Value = match serde_json::from_reader(file.take(1024 * 1024))
                {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if value.get("session_id").and_then(serde_json::Value::as_str) != Some(sid) {
                    continue;
                }
            }
            let file = directory.join(if metadata {
                "messages.jsonl".into()
            } else {
                format!("{sid}.jsonl")
            });
            if std::fs::symlink_metadata(&file).is_ok_and(|meta| meta.file_type().is_file()) {
                return Ok(Some(file));
            }
        }
        Ok(None)
    }

    pub(super) fn valid_id(sid: &str) -> bool {
        !sid.is_empty()
            && sid.len() <= 256
            && !sid.contains(['/', '\\'])
            && !matches!(sid, "." | "..")
            && !sid.chars().any(char::is_control)
    }
}

pub(super) struct ProcessReader {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<serde_json::Value, String>>,
}

impl ProcessReader {
    pub(super) fn spawn(program: &Path, args: &[&str]) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        let input = child.stdin.take().ok_or("history reader has no input")?;
        let stdout = child.stdout.take().ok_or("history reader has no output")?;
        let (sender, output) = mpsc::channel();
        let process = Self {
            child,
            input,
            output,
        };
        std::thread::Builder::new()
            .name("vmux-history-reader-output".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    let read = Read::by_ref(&mut reader)
                        .take(PROVIDER_READ_LIMIT as u64)
                        .read_line(&mut line);
                    let result = match read {
                        Ok(0) => break,
                        Ok(_) if line.ends_with('\n') => {
                            serde_json::from_str(&line).map_err(|error| error.to_string())
                        }
                        Ok(_) => Err("history reader response exceeds the limit".into()),
                        Err(error) => Err(error.to_string()),
                    };
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(process)
    }

    pub(super) fn read(&mut self, request: serde_json::Value) -> Result<serde_json::Value, String> {
        serde_json::to_writer(&mut self.input, &request).map_err(|error| error.to_string())?;
        self.input
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
        self.input.flush().map_err(|error| error.to_string())?;
        let value = self
            .output
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| error.to_string())??;
        if let Some(error) = value.get("error").and_then(serde_json::Value::as_str) {
            return Err(error.into());
        }
        Ok(value)
    }
}

impl Drop for ProcessReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) trait Decoder {
    fn messages(&mut self, line: &str) -> Vec<Message>;
}

pub(super) struct FileConversation;

impl FileConversation {
    pub(super) const TAIL_BYTES: u64 = 16 * 1024 * 1024;
    pub(super) const TAIL_MESSAGES: usize = 512;

    pub(super) fn read(
        path: &Path,
        cursor: Option<u64>,
        tail_bytes: u64,
        mut decoder: impl Decoder,
    ) -> Result<ReadResult, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let physical_length = file.metadata().map_err(|error| error.to_string())?.len();
        let length = Self::complete_length(&mut file, physical_length)?;
        let incremental = cursor.filter(|cursor| {
            *cursor <= length && length.saturating_sub(*cursor) <= PROVIDER_READ_LIMIT as u64
        });
        let tail = incremental.is_none();
        let start = incremental.unwrap_or_else(|| length.saturating_sub(tail_bytes));
        file.seek(SeekFrom::Start(start))
            .map_err(|error| error.to_string())?;
        let remaining = length.saturating_sub(start);
        let mut reader = BufReader::new(file.take(remaining));
        if tail && start > 0 {
            reader
                .skip_until(b'\n')
                .map_err(|error| error.to_string())?;
        }
        let mut messages = VecDeque::new();
        let mut lines = BoundedLines::new(reader);
        for line in lines.by_ref() {
            for message in decoder.messages(&line) {
                if tail && messages.len() == Self::TAIL_MESSAGES {
                    messages.pop_front();
                }
                messages.push_back(message);
            }
            if !tail && messages.len() >= Self::TAIL_MESSAGES {
                break;
            }
        }
        let cursor = if tail {
            length
        } else {
            start.saturating_add(lines.consumed() as u64).min(length)
        };
        Ok(ReadResult {
            path: Some(path.to_path_buf()),
            messages: messages.into_iter().collect(),
            cursor,
            tail,
            complete: cursor == length,
        })
    }

    fn complete_length(file: &mut File, length: u64) -> Result<u64, String> {
        if length == 0 {
            return Ok(0);
        }
        file.seek(SeekFrom::Start(length - 1))
            .map_err(|error| error.to_string())?;
        let mut last = [0];
        file.read_exact(&mut last)
            .map_err(|error| error.to_string())?;
        if last[0] == b'\n' {
            return Ok(length);
        }
        let search_start = length.saturating_sub(PROVIDER_LINE_LIMIT as u64 + 1);
        file.seek(SeekFrom::Start(search_start))
            .map_err(|error| error.to_string())?;
        let mut trailing = Vec::new();
        file.take(length - search_start)
            .read_to_end(&mut trailing)
            .map_err(|error| error.to_string())?;
        if let Some(offset) = trailing.iter().rposition(|byte| *byte == b'\n') {
            return Ok(search_start + offset as u64 + 1);
        }
        if search_start == 0 {
            return Ok(0);
        }
        Ok(length)
    }
}

pub(super) struct JsonText<'a>(&'a serde_json::Value);

impl<'a> JsonText<'a> {
    pub(super) fn new(value: &'a serde_json::Value) -> Self {
        Self(value)
    }

    pub(super) fn text(&self) -> String {
        match self.0 {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Array(items) => items
                .iter()
                .map(Self::new)
                .map(|value| value.text())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            serde_json::Value::Object(values) => values
                .get("text")
                .or_else(|| values.get("content"))
                .map(Self::new)
                .map(|value| value.text())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| serde_json::to_string(self.0).unwrap_or_default()),
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) => self.0.to_string(),
            serde_json::Value::Null => String::new(),
        }
    }
}
