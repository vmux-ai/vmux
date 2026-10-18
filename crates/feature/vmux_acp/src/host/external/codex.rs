use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vmux_session::ProviderSession;

use super::{PromptText, Reader as ProviderReader, Timestamp};

pub(super) struct Reader {
    index: PathBuf,
    sessions: PathBuf,
}

impl Reader {
    pub(super) fn at(root: &Path) -> Self {
        Self {
            index: root.join("session_index.jsonl"),
            sessions: root.join("sessions"),
        }
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
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else {
                continue;
            };
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
        let mut directories = vec![self.sessions.clone()];
        while let Some(directory) = directories.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(id) = indexed.keys().find(|id| name.contains(id.as_str())) else {
                    continue;
                };
                let Ok(file) = File::open(&path) else {
                    continue;
                };
                let mut line = String::new();
                if BufReader::new(file).read_line(&mut line).is_err() {
                    continue;
                }
                let Ok(envelope) = serde_json::from_str::<CodexMetaLine>(&line) else {
                    continue;
                };
                if envelope.payload.session_id != **id || envelope.payload.cwd.trim().is_empty() {
                    continue;
                }
                metadata.insert(
                    (*id).clone(),
                    CodexMeta {
                        cwd: envelope.payload.cwd,
                        created_at: Timestamp::parse(&envelope.payload.timestamp),
                    },
                );
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
                updated_at: indexed.updated_at,
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
    cwd: String,
    created_at: i64,
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
