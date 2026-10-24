use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use vmux_api::session::{AgentAttachment, AssistantBlock, Message};
use vmux_session::{ArchiveState, ProviderSession};

use super::{
    BoundedLines, PROVIDER_LINE_LIMIT, PROVIDER_READ_LIMIT, PromptText, Reader as ProviderReader,
    Timestamp,
};

pub(super) struct Reader {
    root: PathBuf,
    index: PathBuf,
    sessions: PathBuf,
    previous: HashMap<String, ProviderSession>,
}

pub(super) struct ConversationRead {
    pub(super) messages: Vec<Message>,
    pub(super) cursor: u64,
    pub(super) tail: bool,
}

impl Reader {
    const MAX_SESSION_DEPTH: usize = 8;
    const MAX_SESSION_ENTRIES: usize = 100_000;
    const MAX_SESSION_FILES: usize = 50_000;
    const CONVERSATION_TAIL_BYTES: u64 = 16 * 1024 * 1024;
    const CONVERSATION_TAIL_MESSAGES: usize = 512;

    pub(super) fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            index: root.join("session_index.jsonl"),
            sessions: root.join("sessions"),
            previous: HashMap::new(),
        }
    }

    pub(super) fn with_previous(mut self, sessions: &[ProviderSession]) -> Self {
        for session in sessions {
            self.previous.insert(session.sid.clone(), session.clone());
        }
        self
    }

    pub(super) fn conversation(
        &self,
        sid: &str,
        created_at: i64,
        cursor: Option<u64>,
    ) -> Result<ConversationRead, String> {
        let Some(path) = self.rollout(sid, created_at)? else {
            return Ok(ConversationRead {
                messages: Vec::new(),
                cursor: cursor.unwrap_or_default(),
                tail: cursor.is_none(),
            });
        };
        Self::conversation_file(&path, cursor, Self::CONVERSATION_TAIL_BYTES)
    }

    fn conversation_file(
        path: &Path,
        cursor: Option<u64>,
        tail_bytes: u64,
    ) -> Result<ConversationRead, String> {
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
        for line in BoundedLines::new(reader) {
            let Ok(line) = serde_json::from_str::<CodexConversationLine>(&line) else {
                continue;
            };
            let Some(message) = line.message() else {
                continue;
            };
            if messages.len() == Self::CONVERSATION_TAIL_MESSAGES {
                messages.pop_front();
            }
            messages.push_back(message);
        }
        Ok(ConversationRead {
            messages: messages.into_iter().collect(),
            cursor: length,
            tail,
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

    fn archives(&self) -> Result<Option<HashMap<String, ArchiveState>>, String> {
        let Some(path) = self.state_database()? else {
            return Ok(None);
        };
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_millis(100))
            .map_err(|error| error.to_string())?;
        let mut statement =
            match connection.prepare("SELECT id, archived, archived_at, updated_at FROM threads") {
                Ok(statement) => statement,
                Err(_) => return Ok(None),
            };
        let rows = statement
            .query_map([], |row| {
                let id = row.get::<_, String>(0)?;
                let archived = row.get::<_, bool>(1)?;
                let archived_at = row.get::<_, Option<i64>>(2)?;
                let updated_at = row.get::<_, i64>(3)?;
                Ok((id, archived, archived_at, updated_at))
            })
            .map_err(|error| error.to_string())?;
        let mut archives = HashMap::new();
        for row in rows {
            let (id, archived, archived_at, updated_at) = row.map_err(|error| error.to_string())?;
            let state = if archived {
                ArchiveState::Archived(Self::milliseconds(archived_at.unwrap_or(updated_at)))
            } else {
                ArchiveState::Active(Self::milliseconds(updated_at))
            };
            archives.insert(id, state);
        }
        Ok(Some(archives))
    }

    pub(super) fn set_archive(&self, sid: &str, archived: bool) -> Result<ArchiveState, String> {
        let mut delay = 40;
        for attempt in 0..3 {
            match self.set_archive_once(sid, archived) {
                Ok(state) => return Ok(state),
                Err(error) if attempt < 2 && Self::archive_error_is_transient(&error) => {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    delay *= 2;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    fn set_archive_once(&self, sid: &str, archived: bool) -> Result<ArchiveState, String> {
        let Some(path) = self.state_database()? else {
            return Err("Codex state database is unavailable".to_string());
        };
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_millis(500))
            .map_err(|error| error.to_string())?;
        let now = Utc::now().timestamp();
        let archived_at = archived.then_some(now);
        let changed = connection
            .execute(
                "UPDATE threads SET archived = ?1, archived_at = ?2, updated_at = ?3 WHERE id = ?4",
                rusqlite::params![archived, archived_at, now, sid],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err(format!("Codex session not found: {sid}"));
        }
        let now = Self::milliseconds(now);
        Ok(if archived {
            ArchiveState::Archived(now)
        } else {
            ArchiveState::Active(now)
        })
    }

    fn archive_error_is_transient(error: &str) -> bool {
        let error = error.to_ascii_lowercase();
        error.contains("database is locked") || error.contains("database is busy")
    }

    fn state_database(&self) -> Result<Option<PathBuf>, String> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let mut selected = None::<(u32, PathBuf)>;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if !kind.is_file() || kind.is_symlink() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(version) = name
                .strip_prefix("state_")
                .and_then(|name| name.strip_suffix(".sqlite"))
                .and_then(|version| version.parse::<u32>().ok())
            else {
                continue;
            };
            if selected
                .as_ref()
                .is_none_or(|(current, _)| version > *current)
            {
                selected = Some((version, entry.path()));
            }
        }
        let Some((_, path)) = selected else {
            return Ok(None);
        };
        let root = self
            .root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        if path.parent() != Some(root.as_path()) {
            return Err("Codex state database escaped its root".to_string());
        }
        Ok(Some(path))
    }

    fn milliseconds(value: i64) -> i64 {
        if value == 0 || value.abs() >= 10_000_000_000 {
            value
        } else {
            value.saturating_mul(1_000)
        }
    }

    fn rollout(&self, sid: &str, created_at: i64) -> Result<Option<PathBuf>, String> {
        let Some(created_at) = chrono::DateTime::<Utc>::from_timestamp_millis(created_at) else {
            return Ok(None);
        };
        let suffix = format!("-{sid}.jsonl");
        for offset in [-1, 0, 1] {
            let date = created_at.date_naive() + Duration::days(offset);
            let directory = self
                .sessions
                .join(format!("{:04}", date.year()))
                .join(format!("{:02}", date.month()))
                .join(format!("{:02}", date.day()));
            let metadata = match std::fs::symlink_metadata(&directory) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            if metadata.file_type().is_symlink() {
                return Err("Codex rollout directory is a symlink".to_string());
            }
            let entries = std::fs::read_dir(directory).map_err(|error| error.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                let kind = entry.file_type().map_err(|error| error.to_string())?;
                if !kind.is_file() || kind.is_symlink() {
                    continue;
                }
                let path = entry.path();
                let matches = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(&suffix));
                if !matches {
                    continue;
                }
                let file = File::open(&path).map_err(|error| error.to_string())?;
                let Some(line) = BoundedLines::new(BufReader::new(file)).next() else {
                    continue;
                };
                let Ok(meta) = serde_json::from_str::<CodexMetaLine>(&line) else {
                    continue;
                };
                if meta.payload.session_id == sid {
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    fn indexed(&self) -> Result<HashMap<String, CodexIndexed>, String> {
        let file = match File::open(&self.index) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HashMap::new());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut indexed = HashMap::<String, CodexIndexed>::new();
        for line in BoundedLines::new(BufReader::new(file)) {
            let Ok(entry) = serde_json::from_str::<CodexIndexLine>(&line) else {
                continue;
            };
            if entry.id.trim().is_empty() {
                continue;
            }
            let updated_at = Timestamp::parse(&entry.updated_at);
            let candidate = CodexIndexed {
                title: PromptText::compact(&entry.thread_name),
                updated_at,
            };
            match indexed.get(&entry.id) {
                Some(current) if current.updated_at > candidate.updated_at => {}
                _ => {
                    indexed.insert(entry.id, candidate);
                }
            }
        }
        Ok(indexed)
    }

    fn metadata(
        &self,
        indexed: &HashMap<String, CodexIndexed>,
    ) -> Result<HashMap<String, CodexMeta>, String> {
        let mut metadata = HashMap::new();
        let mut missing = indexed.keys().cloned().collect::<HashSet<_>>();
        let cached_rollouts = self.cached_rollouts(indexed)?;
        for id in indexed.keys() {
            let Some(previous) = self.previous.get(id) else {
                continue;
            };
            let Some(updated_at) = cached_rollouts.get(id) else {
                continue;
            };
            metadata.insert(
                id.clone(),
                CodexMeta {
                    cwd: previous.cwd.clone(),
                    created_at: previous.created_at,
                    updated_at: *updated_at,
                },
            );
            missing.remove(id);
        }
        if missing.is_empty() {
            return Ok(metadata);
        }
        let Ok(root) = self.sessions.canonicalize() else {
            return Ok(metadata);
        };
        let mut directories = vec![(root, 0usize)];
        let mut entries_seen = 0usize;
        let mut files = 0usize;
        'walk: while let Some((directory, depth)) = directories.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                entries_seen += 1;
                if entries_seen > Self::MAX_SESSION_ENTRIES {
                    break 'walk;
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    if depth < Self::MAX_SESSION_DEPTH {
                        directories.push((path, depth + 1));
                    }
                    continue;
                }
                if !kind.is_file()
                    || path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
                {
                    continue;
                }
                files += 1;
                if files > Self::MAX_SESSION_FILES {
                    break 'walk;
                }
                let Some(activity) = RolloutActivity::read(&path) else {
                    continue;
                };
                if !missing.contains(&activity.sid) {
                    continue;
                }
                let Ok(file) = File::open(&path) else {
                    continue;
                };
                let Some(line) = BoundedLines::new(BufReader::new(file)).next() else {
                    continue;
                };
                let Ok(envelope) = serde_json::from_str::<CodexMetaLine>(&line) else {
                    continue;
                };
                if envelope.payload.session_id != activity.sid
                    || envelope.payload.cwd.trim().is_empty()
                {
                    continue;
                }
                metadata.insert(
                    activity.sid.clone(),
                    CodexMeta {
                        cwd: envelope.payload.cwd,
                        created_at: Timestamp::parse(&envelope.payload.timestamp),
                        updated_at: activity.updated_at,
                    },
                );
                missing.remove(&activity.sid);
                if missing.is_empty() {
                    break 'walk;
                }
            }
        }
        Ok(metadata)
    }

    fn cached_rollouts(
        &self,
        indexed: &HashMap<String, CodexIndexed>,
    ) -> Result<HashMap<String, i64>, String> {
        let mut directories = HashMap::<PathBuf, HashSet<String>>::new();
        let mut unresolved = HashSet::new();
        for id in indexed.keys() {
            let Some(previous) = self.previous.get(id) else {
                continue;
            };
            let Some(created_at) =
                chrono::DateTime::<Utc>::from_timestamp_millis(previous.created_at)
            else {
                continue;
            };
            unresolved.insert(id.clone());
            for offset in [-1, 0, 1] {
                let date = created_at.date_naive() + Duration::days(offset);
                let directory = self
                    .sessions
                    .join(format!("{:04}", date.year()))
                    .join(format!("{:02}", date.month()))
                    .join(format!("{:02}", date.day()));
                directories.entry(directory).or_default().insert(id.clone());
            }
        }
        let mut found = HashMap::new();
        let mut entries_seen = 0usize;
        let mut files = 0usize;
        'walk: for (directory, expected) in directories {
            let mut remaining = expected
                .iter()
                .filter(|id| unresolved.contains(*id))
                .count();
            if remaining == 0 {
                continue;
            }
            let metadata = match std::fs::symlink_metadata(&directory) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            if metadata.file_type().is_symlink() {
                return Err("Codex rollout directory is a symlink".to_string());
            }
            if !metadata.file_type().is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                entries_seen += 1;
                if entries_seen > Self::MAX_SESSION_ENTRIES {
                    break 'walk;
                }
                let kind = entry.file_type().map_err(|error| error.to_string())?;
                if !kind.is_file() || kind.is_symlink() {
                    continue;
                }
                files += 1;
                if files > Self::MAX_SESSION_FILES {
                    break 'walk;
                }
                let path = entry.path();
                let Some(activity) = RolloutActivity::read(&path) else {
                    continue;
                };
                if expected.contains(&activity.sid) {
                    let id = activity.sid;
                    if unresolved.remove(&id) {
                        found.insert(id, activity.updated_at);
                        remaining -= 1;
                        if unresolved.is_empty() {
                            break 'walk;
                        }
                        if remaining == 0 {
                            break;
                        }
                    }
                }
            }
        }
        Ok(found)
    }
}

impl ProviderReader for Reader {
    fn read(&self) -> Result<Vec<ProviderSession>, String> {
        let indexed = self.indexed()?;
        let metadata = self.metadata(&indexed)?;
        let archives = self.archives().ok().flatten();
        let mut sessions = Vec::new();
        for (id, indexed) in indexed {
            let Some(metadata) = metadata.get(&id) else {
                continue;
            };
            let archive = archives
                .as_ref()
                .and_then(|archives| archives.get(&id).copied())
                .or_else(|| self.previous.get(&id).and_then(|session| session.archive));
            sessions.push(ProviderSession {
                kind: "acp:codex".to_string(),
                sid: id,
                agent: "codex".to_string(),
                agent_name: "Codex".to_string(),
                name: indexed.title.clone(),
                cwd: metadata.cwd.clone(),
                created_at: metadata.created_at,
                updated_at: indexed.updated_at.max(metadata.updated_at),
                conversation_title: indexed.title.clone(),
                last_prompt: indexed.title,
                archive,
                ..Default::default()
            });
        }
        Ok(sessions)
    }
}

pub(super) struct RolloutActivity {
    sid: String,
    updated_at: i64,
}

impl RolloutActivity {
    pub(super) fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        let start = stem.len().checked_sub(36)?;
        let sid = stem.get(start..)?.to_string();
        let updated_at = metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis()
            .min(i64::MAX as u128) as i64;
        Some(Self { sid, updated_at })
    }

    pub(super) fn refresh(&self, sessions: &mut [ProviderSession]) -> bool {
        let Some(session) = sessions.iter_mut().find(|session| session.sid == self.sid) else {
            return false;
        };
        session.updated_at = session.updated_at.max(self.updated_at);
        true
    }
}

#[derive(Deserialize)]
struct CodexIndexLine {
    id: String,
    thread_name: String,
    updated_at: String,
}

struct CodexIndexed {
    title: String,
    updated_at: i64,
}

#[derive(Deserialize)]
struct CodexMetaLine {
    payload: CodexMetaPayload,
}

#[derive(Deserialize)]
struct CodexMetaPayload {
    session_id: String,
    timestamp: String,
    cwd: String,
}

struct CodexMeta {
    cwd: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Deserialize)]
struct CodexConversationLine {
    #[serde(rename = "type")]
    kind: String,
    payload: CodexConversationPayload,
}

impl CodexConversationLine {
    fn message(self) -> Option<Message> {
        if self.kind != "response_item" || self.payload.kind != "message" {
            return None;
        }
        let role = self.payload.role.clone();
        match role.as_str() {
            "user" => self.payload.user_message(),
            "assistant" => self.payload.assistant_message(),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct CodexConversationPayload {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: Vec<CodexConversationContent>,
    #[serde(default)]
    internal_chat_message_metadata_passthrough: Option<CodexMessageMetadata>,
}

impl CodexConversationPayload {
    fn user_message(self) -> Option<Message> {
        if self
            .internal_chat_message_metadata_passthrough
            .as_ref()
            .is_some_and(CodexMessageMetadata::internal)
        {
            return None;
        }
        let mut parts = Vec::new();
        let mut attachments = Vec::new();
        let mut image = None;
        for content in self.content {
            match content.kind.as_str() {
                "input_text" => {
                    if let Some(tag) = CodexImageTag::parse(&content.text) {
                        image = Some(tag);
                    } else if content.text.trim() != "</image>" && !content.text.trim().is_empty() {
                        parts.push(content.text);
                    }
                }
                "input_image" => {
                    if let Some(tag) = image.take() {
                        attachments.push(tag.attachment());
                    }
                }
                _ => {}
            }
        }
        let text = parts.join("\n");
        if Self::internal_text(&text) || text.trim().is_empty() && attachments.is_empty() {
            return None;
        }
        Some(Message::user_with_attachments(text, attachments))
    }

    fn assistant_message(self) -> Option<Message> {
        let parts = self
            .content
            .into_iter()
            .filter(|content| content.kind == "output_text" && !content.text.trim().is_empty())
            .map(|content| content.text)
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return None;
        }
        Some(Message::Assistant {
            blocks: vec![AssistantBlock::Text(parts.join("\n"))],
        })
    }

    fn internal_text(text: &str) -> bool {
        let text = text.trim_start();
        text.starts_with("<codex_internal_context") || text.starts_with("<subagent_notification>")
    }
}

#[derive(Deserialize)]
struct CodexConversationContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Default, Deserialize)]
struct CodexMessageMetadata {
    #[serde(default)]
    content_item_kinds: Vec<String>,
}

impl CodexMessageMetadata {
    fn internal(&self) -> bool {
        !self.content_item_kinds.is_empty()
            && !self
                .content_item_kinds
                .iter()
                .any(|kind| kind.starts_with("user."))
    }
}

struct CodexImageTag(PathBuf);

impl CodexImageTag {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if !text.starts_with("<image ") || !text.ends_with('>') {
            return None;
        }
        let (_, path) = text.split_once("path=\"")?;
        let (path, _) = path.split_once('"')?;
        if path.is_empty() {
            return None;
        }
        Some(Self(PathBuf::from(path)))
    }

    fn attachment(self) -> AgentAttachment {
        let name = self
            .0
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_string();
        let mime_type = match self
            .0
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("tif" | "tiff") => "image/tiff",
            Some("heic") => "image/heic",
            _ => "application/octet-stream",
        }
        .to_string();
        let size = std::fs::metadata(&self.0)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        AgentAttachment {
            path: self.0.to_string_lossy().into_owned(),
            name,
            mime_type,
            size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    struct Fixture {
        root: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().unwrap(),
            }
        }

        fn path(&self) -> &Path {
            self.root.path()
        }

        fn write(&self, path: &str, content: &str) {
            let path = self.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        fn archive(&self, id: &str, archived: bool, archived_at: Option<i64>, updated_at: i64) {
            let connection = Connection::open(self.path().join("state_5.sqlite")).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, archived INTEGER NOT NULL, archived_at INTEGER, updated_at INTEGER NOT NULL)",
                )
                .unwrap();
            connection
                .execute(
                    "INSERT OR REPLACE INTO threads (id, archived, archived_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, archived, archived_at, updated_at],
                )
                .unwrap();
        }
    }

    #[test]
    fn codex_reader_joins_the_index_to_rollout_metadata() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Casual greeting","updated_at":"2026-10-18T16:16:54.778882Z"}
"#,
        );
        fixture.write(
            "sessions/2026/10/18/rollout-2026-10-18T18-16-34-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl",
            r#"{"type":"session_meta","payload":{"session_id":"01a14fcc-f46a-7c12-b000-779be98d72ae","timestamp":"2026-10-18T16:16:34.454Z","cwd":"/work/vmux"}}
"#,
        );

        let sessions = Reader::at(fixture.path()).read().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].sid, "01a14fcc-f46a-7c12-b000-779be98d72ae");
        assert_eq!(sessions[0].cwd, "/work/vmux");
        assert_eq!(sessions[0].name, "Casual greeting");
    }

    #[test]
    fn codex_reader_projects_the_native_archive_state() {
        let fixture = Fixture::new();
        let sid = "01a14fcc-f46a-7c12-b000-779be98d72ae";
        fixture.write(
            "session_index.jsonl",
            &format!(
                "{{\"id\":\"{sid}\",\"thread_name\":\"Archived task\",\"updated_at\":\"2026-10-18T16:16:54Z\"}}\n"
            ),
        );
        fixture.write(
            &format!("sessions/2026/10/18/rollout-2026-10-18T18-16-34-{sid}.jsonl"),
            &format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"session_id\":\"{sid}\",\"timestamp\":\"2026-10-18T16:16:34Z\",\"cwd\":\"/work/vmux\"}}}}\n"
            ),
        );
        fixture.archive(sid, true, Some(1_760_806_000), 1_760_806_000);

        let archived = Reader::at(fixture.path()).read().unwrap();

        assert_eq!(
            archived[0].archive,
            Some(ArchiveState::Archived(1_760_806_000_000))
        );

        fixture.archive(sid, false, None, 1_760_806_100);
        let active = Reader::at(fixture.path())
            .with_previous(&archived)
            .read()
            .unwrap();

        assert_eq!(
            active[0].archive,
            Some(ArchiveState::Active(1_760_806_100_000))
        );
    }

    #[test]
    fn codex_writer_updates_the_native_archive_state() {
        let fixture = Fixture::new();
        let sid = "01a14fcc-f46a-7c12-b000-779be98d72ae";
        fixture.archive(sid, false, None, 1_760_806_000);
        let reader = Reader::at(fixture.path());

        let archived = reader.set_archive(sid, true).unwrap();
        let states = reader.archives().unwrap().unwrap();

        assert!(matches!(archived, ArchiveState::Archived(_)));
        assert_eq!(states.get(sid), Some(&archived));

        let active = reader.set_archive(sid, false).unwrap();
        let states = reader.archives().unwrap().unwrap();

        assert!(matches!(active, ArchiveState::Active(_)));
        assert_eq!(states.get(sid), Some(&active));
    }

    #[test]
    fn codex_reader_loads_visible_conversation_without_starting_the_agent() {
        let fixture = Fixture::new();
        let sid = "01a14fcc-f46a-7c12-b000-779be98d72ae";
        fixture.write(
            "sessions/2026/10/18/rollout-2026-10-18T18-16-34-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl",
            r#"{"type":"session_meta","payload":{"session_id":"01a14fcc-f46a-7c12-b000-779be98d72ae","timestamp":"2026-10-18T16:16:34.454Z","cwd":"/work/vmux"}}
{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"hidden"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix session loading"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done"}]}}
{"type":"custom_tool_call_output","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hidden"}]}}
"#,
        );

        let messages = Reader::at(fixture.path())
            .conversation(sid, Timestamp::parse("2026-10-18T16:16:34.454Z"), None)
            .unwrap()
            .messages;

        assert_eq!(
            messages,
            vec![
                Message::user("Fix session loading"),
                Message::Assistant {
                    blocks: vec![AssistantBlock::Text("Done".into())],
                },
            ]
        );
    }

    #[test]
    fn codex_reader_ignores_internal_user_envelopes() {
        let lines = [
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>hidden</subagent_notification>"}],"internal_chat_message_metadata_passthrough":{"content_item_kinds":["multi_agent.subagent_notification"]}}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<codex_internal_context source=\"goal\">hidden</codex_internal_context>"}]}}"#,
        ];

        for line in lines {
            let line = serde_json::from_str::<CodexConversationLine>(line).unwrap();
            assert!(line.message().is_none());
        }
    }

    #[test]
    fn codex_reader_restores_image_attachments() {
        let fixture = Fixture::new();
        let image = fixture.path().join("screenshot.png");
        std::fs::write(&image, [1, 2, 3]).unwrap();
        let line = serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": format!("<image name=[Image #1] path=\"{}\">", image.display())},
                    {"type": "input_image", "image_url": "data:image/png;base64,AQID"},
                    {"type": "input_text", "text": "</image>"},
                    {"type": "input_text", "text": "Review this [Image #1]"}
                ],
                "internal_chat_message_metadata_passthrough": {
                    "content_item_kinds": ["user.text", "user.image", "user.text", "user.text"]
                }
            }
        });
        let line = serde_json::from_value::<CodexConversationLine>(line).unwrap();

        let Some(Message::User { text, attachments }) = line.message() else {
            panic!()
        };

        assert_eq!(text, "Review this [Image #1]");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, image.to_string_lossy());
        assert_eq!(attachments[0].name, "screenshot.png");
        assert_eq!(attachments[0].mime_type, "image/png");
        assert_eq!(attachments[0].size, 3);
    }

    #[test]
    fn codex_conversation_reads_only_the_bounded_file_tail() {
        let fixture = Fixture::new();
        let path = "sessions/rollout.jsonl";
        fixture.write(
            path,
            &format!(
                "{}\n{}\n{}\n",
                "x".repeat(256),
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"old"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"latest"}]}}"#,
            ),
        );

        let read = Reader::conversation_file(&fixture.path().join(path), None, 180).unwrap();

        assert!(read.tail);
        assert_eq!(
            read.messages,
            vec![Message::Assistant {
                blocks: vec![AssistantBlock::Text("latest".into())],
            }]
        );
    }

    #[test]
    fn codex_conversation_resumes_after_the_last_complete_record() {
        let fixture = Fixture::new();
        let path = fixture.path().join("sessions/rollout.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let first = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]}}"#;
        let second = r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"second"}]}}"#;
        std::fs::write(&path, format!("{first}\n{}", &second[..second.len() / 2])).unwrap();

        let initial = Reader::conversation_file(&path, None, 1024).unwrap();

        assert_eq!(initial.messages, vec![Message::user("first")]);
        assert_eq!(initial.cursor, first.len() as u64 + 1);
        std::fs::write(&path, format!("{first}\n{second}\n")).unwrap();

        let appended = Reader::conversation_file(&path, Some(initial.cursor), 1024).unwrap();

        assert!(!appended.tail);
        assert_eq!(
            appended.messages,
            vec![Message::Assistant {
                blocks: vec![AssistantBlock::Text("second".into())],
            }]
        );
        assert_eq!(appended.cursor, std::fs::metadata(path).unwrap().len());
    }

    #[test]
    fn codex_reader_uses_rollout_activity_after_the_index_timestamp() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Resumed task","updated_at":"1970-01-01T00:00:01Z"}
"#,
        );
        fixture.write(
            "sessions/2026/10/02/rollout-2026-10-02T12-44-37-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl",
            r#"{"type":"session_meta","payload":{"session_id":"01a14fcc-f46a-7c12-b000-779be98d72ae","timestamp":"2026-10-02T10:44:37.930Z","cwd":"/work/vmux"}}
"#,
        );

        let sessions = Reader::at(fixture.path()).read().unwrap();

        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].updated_at > Timestamp::parse("1970-01-01T00:00:01Z"));
    }

    #[test]
    fn codex_reader_reuses_known_metadata_when_rollout_still_exists() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Updated title","updated_at":"2026-10-21T03:00:00Z"}
"#,
        );
        fixture.write(
            "sessions/2026/10/21/rollout-2026-10-21T02-59-50-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl",
            "not read while cached\n",
        );
        let previous = ProviderSession {
            sid: "01a14fcc-f46a-7c12-b000-779be98d72ae".into(),
            cwd: "/work/vmux".into(),
            created_at: Timestamp::parse("2026-10-21T02:59:50Z"),
            ..Default::default()
        };

        let sessions = Reader::at(fixture.path())
            .with_previous(&[previous])
            .read()
            .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].cwd, "/work/vmux");
        assert_eq!(sessions[0].name, "Updated title");
    }

    #[test]
    fn codex_reader_drops_cached_metadata_after_rollout_removal() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Removed","updated_at":"2026-10-21T03:00:00Z"}
"#,
        );
        let previous = ProviderSession {
            sid: "01a14fcc-f46a-7c12-b000-779be98d72ae".into(),
            cwd: "/work/vmux".into(),
            created_at: Timestamp::parse("2026-10-21T02:59:50Z"),
            ..Default::default()
        };

        let sessions = Reader::at(fixture.path())
            .with_previous(&[previous])
            .read()
            .unwrap();

        assert!(sessions.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn codex_reader_rejects_a_symlinked_cached_rollout_directory() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Task","updated_at":"2026-10-21T03:00:00Z"}
"#,
        );
        let target = fixture.path().join("rollouts");
        std::fs::create_dir_all(&target).unwrap();
        let date_parent = fixture.path().join("sessions/2026/10");
        std::fs::create_dir_all(&date_parent).unwrap();
        symlink(target, date_parent.join("21")).unwrap();
        let previous = ProviderSession {
            sid: "01a14fcc-f46a-7c12-b000-779be98d72ae".into(),
            cwd: "/work/vmux".into(),
            created_at: Timestamp::parse("2026-10-21T02:59:50Z"),
            ..Default::default()
        };

        let result = Reader::at(fixture.path()).with_previous(&[previous]).read();

        assert!(result.is_err());
    }

    #[test]
    fn codex_reader_rejects_filename_identity_spoofing() {
        let fixture = Fixture::new();
        fixture.write(
            "session_index.jsonl",
            r#"{"id":"01a14fcc-f46a-7c12-b000-779be98d72ae","thread_name":"Task","updated_at":"2026-10-21T03:00:00Z"}
"#,
        );
        fixture.write(
            "sessions/rollout-01a14fcc-f46a-7c12-b000-779be98d72ae.jsonl",
            r#"{"type":"session_meta","payload":{"session_id":"01a14fcc-f46a-7c12-b000-000000000000","timestamp":"2026-10-21T02:00:00Z","cwd":"/spoofed"}}
"#,
        );

        let sessions = Reader::at(fixture.path()).read().unwrap();

        assert!(sessions.is_empty());
    }
}
