use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vmux_session::ProviderSession;

use super::{BoundedLines, PromptText, Reader as ProviderReader, Timestamp};

pub(super) struct Reader {
    index: PathBuf,
    sessions: PathBuf,
    previous: HashMap<String, ProviderSession>,
}

impl Reader {
    const MAX_SESSION_DEPTH: usize = 8;
    const MAX_SESSION_ENTRIES: usize = 100_000;
    const MAX_SESSION_FILES: usize = 50_000;

    pub(super) fn at(root: &Path) -> Self {
        Self {
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

    fn metadata(&self, indexed: &HashMap<String, CodexIndexed>) -> HashMap<String, CodexMeta> {
        let mut metadata = HashMap::new();
        let Ok(root) = self.sessions.canonicalize() else {
            return metadata;
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
                let updated_at = entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|elapsed| elapsed.as_millis() as i64)
                    .unwrap_or_default();
                let known_id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .and_then(|stem| stem.get(stem.len().saturating_sub(36)..))
                    .filter(|id| indexed.contains_key(*id));
                let candidate = match known_id.and_then(|id| self.previous.get(id)) {
                    Some(previous) => CodexMeta {
                        id: previous.sid.clone(),
                        cwd: previous.cwd.clone(),
                        created_at: previous.created_at,
                        updated_at,
                    },
                    None => {
                        let Ok(file) = File::open(&path) else {
                            continue;
                        };
                        let Some(line) = BoundedLines::new(BufReader::new(file)).next() else {
                            continue;
                        };
                        let Ok(envelope) = serde_json::from_str::<CodexMetaLine>(&line) else {
                            continue;
                        };
                        if !indexed.contains_key(&envelope.payload.session_id)
                            || envelope.payload.cwd.trim().is_empty()
                        {
                            continue;
                        }
                        CodexMeta {
                            id: envelope.payload.session_id,
                            cwd: envelope.payload.cwd,
                            created_at: Timestamp::parse(&envelope.payload.timestamp),
                            updated_at,
                        }
                    }
                };
                if let Some(current) = metadata.get_mut(&candidate.id) {
                    current.updated_at = current.updated_at.max(candidate.updated_at);
                    if candidate.created_at < current.created_at {
                        current.cwd = candidate.cwd;
                        current.created_at = candidate.created_at;
                    }
                } else {
                    metadata.insert(candidate.id.clone(), candidate);
                }
            }
        }
        metadata
    }
}

impl ProviderReader for Reader {
    fn read(&self) -> Result<Vec<ProviderSession>, String> {
        let indexed = self.indexed()?;
        let metadata = self.metadata(&indexed);
        let mut sessions = Vec::new();
        for (id, indexed) in indexed {
            let Some(metadata) = metadata.get(&id) else {
                continue;
            };
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
                ..Default::default()
            });
        }
        Ok(sessions)
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
    id: String,
    cwd: String,
    created_at: i64,
    updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
