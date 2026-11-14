use std::collections::VecDeque;
use std::fs::{File, Metadata};
use std::io::Write;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use std::time::UNIX_EPOCH;

use vmux_api::session::{AssistantBlock, Message, ToolKind};
use vmux_session::history::{Boundary as HistoryBoundary, FileFacts, Received};

use super::{BoundedLines, PROVIDER_LINE_LIMIT, PROVIDER_READ_LIMIT};

pub(super) struct ReadResult {
    pub path: Option<PathBuf>,
    pub history: Received,
    pub watch_paths: Vec<PathBuf>,
    pub cursor: u64,
    pub complete: bool,
}

#[cfg(test)]
impl ReadResult {
    pub(super) fn messages(&self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(self.history.entries.len());
        for (_, message) in &self.history.entries {
            messages.push(message.clone());
        }
        messages
    }
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

    pub(super) fn history(value: &serde_json::Value) -> Received {
        let Some(entries) = value.as_array() else {
            return Received::messages(Vec::new(), false);
        };
        let mut history = Received::messages(Vec::new(), false);
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
                        history
                            .entries
                            .push((Some(format!("{id}/message/{index}")), message));
                    }
                }
                "reasoning" => {
                    let text = entry
                        .get("text")
                        .or_else(|| entry.get("summary"))
                        .map(|value| JsonText::new(value).text())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        history.entries.push((
                            Some(id.into()),
                            Message::Assistant {
                                blocks: vec![AssistantBlock::Thinking(text)],
                            },
                        ));
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
                    history.entries.push((
                        Some(format!("{id}/use")),
                        Message::Assistant {
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
                    ));
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
                        history.entries.push((
                            Some(format!("{id}/result")),
                            Message::ToolResult {
                                call_id: id.into(),
                                content: output
                                    .map(|value| JsonText::new(value).text())
                                    .unwrap_or_default(),
                                is_error: matches!(status, "failed" | "cancelled"),
                            },
                        ));
                    }
                }
                _ => {}
            }
        }
        history
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
        known: Option<&FileFacts>,
        tail_bytes: u64,
        mut decoder: impl Decoder,
    ) -> Result<ReadResult, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let physical_length = metadata.len();
        let identity = Self::identity(path, &metadata)?;
        let length = Self::complete_length(&mut file, physical_length)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let mut prefix = Vec::new();
        Read::by_ref(&mut file)
            .take(4096)
            .read_to_end(&mut prefix)
            .map_err(|error| error.to_string())?;
        let unchanged = known.is_none_or(|known| {
            known.identity == identity
                && physical_length >= known.length
                && prefix.starts_with(&known.prefix)
        });
        let incremental = cursor.filter(|cursor| unchanged && *cursor <= length);
        let tail = incremental.is_none();
        let start = incremental.unwrap_or_else(|| length.saturating_sub(tail_bytes));
        let end = if tail {
            length
        } else {
            start.saturating_add(Self::TAIL_BYTES).min(length)
        };
        let start_aligned = Self::line_boundary(&mut file, start)?;
        let end_aligned = Self::line_boundary(&mut file, end)?;
        file.seek(SeekFrom::Start(start))
            .map_err(|error| error.to_string())?;
        let mut reader = BufReader::new(file.take(end.saturating_sub(start)));
        let skipped = if start > 0 && !start_aligned {
            reader
                .skip_until(b'\n')
                .map_err(|error| error.to_string())? as u64
        } else {
            0
        };
        let base = start.saturating_add(skipped);
        let mut messages = VecDeque::new();
        let mut lines = BoundedLines::new(reader);
        let mut discarded = false;
        let mut partial_start = None;
        while let Some(line) = lines.next() {
            let position = base.saturating_add(lines.line_start() as u64);
            if !end_aligned && base.saturating_add(lines.consumed() as u64) == end {
                partial_start = Some(position);
                break;
            }
            for (subrecord, message) in decoder.messages(&line).into_iter().enumerate() {
                if tail && messages.len() == Self::TAIL_MESSAGES {
                    messages.pop_front();
                    discarded = true;
                }
                messages.push_back((message, (position, subrecord)));
            }
            if !tail && messages.len() >= Self::TAIL_MESSAGES {
                break;
            }
        }
        let cursor = if tail {
            length
        } else {
            partial_start.unwrap_or_else(|| base.saturating_add(lines.consumed() as u64).min(end))
        };
        let mut history = Received::messages(Vec::new(), tail);
        let before = if tail && (discarded || start > 0) {
            messages
                .front()
                .map(|(_, (byte, message))| HistoryBoundary {
                    byte: *byte,
                    message: *message,
                })
                .or_else(|| {
                    (start > 0).then_some(HistoryBoundary {
                        byte: start,
                        message: 0,
                    })
                })
        } else {
            None
        };
        let mut positions = Vec::with_capacity(messages.len());
        for (message, position) in messages {
            history.entries.push((None, message));
            positions.push(position);
        }
        history.file = Some(FileFacts {
            before,
            identity,
            prefix,
            length: physical_length,
            cursor,
            positions,
        });
        Ok(ReadResult {
            path: Some(path.to_path_buf()),
            history,
            watch_paths: Vec::new(),
            cursor,
            complete: cursor == length,
        })
    }

    pub(super) fn before(
        path: &Path,
        boundary: vmux_session::history::Boundary,
        known: Option<&FileFacts>,
        mut decoder: impl Decoder,
    ) -> Result<ReadResult, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let length = metadata.len();
        let identity = Self::identity(path, &metadata)?;
        let mut prefix = Vec::new();
        Read::by_ref(&mut file)
            .take(4096)
            .read_to_end(&mut prefix)
            .map_err(|error| error.to_string())?;
        if boundary.byte > length
            || known.is_some_and(|known| {
                known.identity != identity
                    || length < known.length
                    || !prefix.starts_with(&known.prefix)
            })
        {
            return Err("provider history changed during pagination".into());
        }
        let end = if boundary.message > 0 {
            file.seek(SeekFrom::Start(boundary.byte))
                .map_err(|error| error.to_string())?;
            let mut line = Vec::new();
            let size = BufReader::new(Read::by_ref(&mut file).take(PROVIDER_LINE_LIMIT as u64 + 1))
                .read_until(b'\n', &mut line)
                .map_err(|error| error.to_string())?;
            boundary.byte.saturating_add(size as u64).min(length)
        } else {
            boundary.byte
        };
        let mut position = end;
        let mut scanned = 0u64;
        let mut line = Vec::new();
        let mut oversized = false;
        let mut ignore = boundary.message == 0 && !Self::line_boundary(&mut file, boundary.byte)?;
        let mut records = VecDeque::new();
        let mut block = [0u8; 64 * 1024];
        'scan: while position > 0 {
            let start = position.saturating_sub(block.len() as u64);
            let count = (position - start) as usize;
            file.seek(SeekFrom::Start(start))
                .map_err(|error| error.to_string())?;
            file.read_exact(&mut block[..count])
                .map_err(|error| error.to_string())?;
            for index in (0..count).rev() {
                let at = start + index as u64;
                scanned += 1;
                position = at;
                if block[index] == b'\n' {
                    if !ignore && !oversized && !line.is_empty() {
                        line.reverse();
                        if let Ok(text) = std::str::from_utf8(&line) {
                            let messages = decoder.messages(text);
                            for (message, content) in messages.into_iter().enumerate().rev() {
                                let record = HistoryBoundary {
                                    byte: at + 1,
                                    message,
                                };
                                if record < boundary {
                                    records.push_front((record, content));
                                    if records.len() == Self::TAIL_MESSAGES {
                                        break 'scan;
                                    }
                                }
                            }
                        }
                    }
                    line.clear();
                    oversized = false;
                    ignore = false;
                    if scanned >= Self::TAIL_BYTES {
                        break 'scan;
                    }
                } else if !ignore && !oversized {
                    if line.len() == PROVIDER_LINE_LIMIT {
                        line.clear();
                        oversized = true;
                    } else {
                        line.push(block[index]);
                    }
                }
                if scanned >= Self::TAIL_BYTES && (ignore || oversized) {
                    break 'scan;
                }
            }
        }
        if position == 0
            && !ignore
            && !oversized
            && !line.is_empty()
            && records.len() < Self::TAIL_MESSAGES
        {
            line.reverse();
            if let Ok(text) = std::str::from_utf8(&line) {
                for (message, content) in decoder.messages(text).into_iter().enumerate().rev() {
                    let record = HistoryBoundary { byte: 0, message };
                    if record < boundary {
                        records.push_front((record, content));
                        if records.len() == Self::TAIL_MESSAGES {
                            break;
                        }
                    }
                }
            }
        }
        let before = if records.len() == Self::TAIL_MESSAGES {
            records
                .front()
                .map(|(position, _)| *position)
                .filter(|position| position.byte != 0 || position.message != 0)
        } else if position > 0 {
            Some(records.front().map_or(
                HistoryBoundary {
                    byte: position,
                    message: 0,
                },
                |(position, _)| *position,
            ))
        } else {
            None
        };
        if before.is_some_and(|before| before >= boundary) {
            return Err("provider history pagination did not advance".into());
        }
        let mut history = Received::messages(Vec::new(), true);
        let mut positions = Vec::with_capacity(records.len());
        for (position, message) in records {
            history.entries.push((None, message));
            positions.push((position.byte, position.message));
        }
        let cursor = known.map_or(length, |known| known.cursor);
        history.file = Some(FileFacts {
            identity,
            prefix,
            length,
            cursor,
            positions,
            before,
        });
        Ok(ReadResult {
            path: Some(path.to_path_buf()),
            history,
            watch_paths: Vec::new(),
            cursor,
            complete: true,
        })
    }

    fn line_boundary(file: &mut File, position: u64) -> Result<bool, String> {
        if position == 0 {
            return Ok(true);
        }
        file.seek(SeekFrom::Start(position - 1))
            .map_err(|error| error.to_string())?;
        let mut byte = [0];
        file.read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        Ok(byte[0] == b'\n')
    }

    fn identity(path: &Path, metadata: &Metadata) -> Result<String, String> {
        let created = metadata
            .created()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|time| time.as_nanos());
        #[cfg(unix)]
        {
            let _ = path;
            Ok(format!(
                "unix:{}:{}:{created:?}",
                metadata.dev(),
                metadata.ino()
            ))
        }
        #[cfg(not(unix))]
        {
            let created = created.ok_or("native file creation identity is unavailable")?;
            let path = path.canonicalize().map_err(|error| error.to_string())?;
            Ok(format!("file:{}:{created}", path.display()))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_aligned_with_the_first_prompt_keeps_it_through_ignored_records() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.jsonl");
        let prompt = "{\"role\":\"user\",\"content\":\"First prompt\"}\n";
        let answer = "{\"role\":\"assistant\",\"content\":\"Latest answer\"}\n";
        let head = "{\"isMeta\":true,\"padding\":\"";
        let end = "\"}\n";
        let filler = "x"
            .repeat(FileConversation::TAIL_BYTES as usize - prompt.len() - head.len() - end.len());
        std::fs::write(&path, format!("{{}}\n{prompt}{head}{filler}{end}{answer}")).unwrap();
        let mut read =
            FileConversation::read(&path, None, None, FileConversation::TAIL_BYTES, ChatDecoder)
                .unwrap();
        let mut found = read.messages();
        while let Some(boundary) = read.history.file.as_ref().and_then(|file| file.before) {
            read = FileConversation::before(&path, boundary, None, ChatDecoder).unwrap();
            found.extend(read.messages());
        }
        assert!(found.contains(&Message::user("First prompt")));
    }

    #[test]
    fn an_incremental_chunk_boundary_keeps_a_partial_jsonl_message() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.jsonl");
        std::fs::write(&path, "{\"role\":\"user\",\"content\":\"Original\"}\n").unwrap();
        let original =
            FileConversation::read(&path, None, None, FileConversation::TAIL_BYTES, ChatDecoder)
                .unwrap();
        let expected = "A normal message crossing the bounded read window".repeat(8);
        let record = format!(
            "{{\"role\":\"user\",\"content\":{}}}\n",
            serde_json::to_string(&expected).unwrap()
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&vec![b'x'; FileConversation::TAIL_BYTES as usize - 65])
            .unwrap();
        file.write_all(b"\n").unwrap();
        file.write_all(record.as_bytes()).unwrap();
        file.write_all(b"{\"role\":\"assistant\",\"content\":\"Final answer\"}\n")
            .unwrap();
        drop(file);
        let mut read = original;
        let mut messages = Vec::new();
        for _ in 0..4 {
            let next = FileConversation::read(
                &path,
                Some(read.cursor),
                read.history.file.as_ref(),
                FileConversation::TAIL_BYTES,
                ChatDecoder,
            )
            .unwrap();
            messages.extend(next.messages());
            assert!(next.cursor > read.cursor);
            let complete = next.complete;
            read = next;
            if complete {
                break;
            }
        }
        assert!(read.complete);
        assert_eq!(
            messages,
            [
                Message::user(expected),
                Message::Assistant {
                    blocks: vec![AssistantBlock::Text("Final answer".into())]
                }
            ]
        );
    }

    #[test]
    fn a_large_append_keeps_reading_from_the_accepted_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.jsonl");
        std::fs::write(&path, "{\"role\":\"user\",\"content\":\"Original\"}\n").unwrap();
        let original =
            FileConversation::read(&path, None, None, FileConversation::TAIL_BYTES, ChatDecoder)
                .unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let length = original.cursor + PROVIDER_READ_LIMIT as u64 + 1024;
        file.set_len(length).unwrap();
        file.seek(SeekFrom::Start(original.cursor)).unwrap();
        file.write_all(b"{\"role\":\"user\",\"content\":\"New request\"}\n")
            .unwrap();
        file.seek(SeekFrom::Start(length - 1)).unwrap();
        file.write_all(b"\n").unwrap();
        let read = FileConversation::read(
            &path,
            Some(original.cursor),
            original.history.file.as_ref(),
            FileConversation::TAIL_BYTES,
            ChatDecoder,
        )
        .unwrap();
        assert!(!read.history.tail);
        assert_eq!(read.messages(), [Message::user("New request")]);
        assert!(read.cursor > original.cursor && read.cursor < length);
    }
}
