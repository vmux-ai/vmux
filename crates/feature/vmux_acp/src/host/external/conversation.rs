use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use vmux_api::session::Message;

use super::{BoundedLines, PROVIDER_LINE_LIMIT, PROVIDER_READ_LIMIT};

pub(super) struct ReadResult {
    pub messages: Vec<Message>,
    pub cursor: u64,
    pub tail: bool,
    pub complete: bool,
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
