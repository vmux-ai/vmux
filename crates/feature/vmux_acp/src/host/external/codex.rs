use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, Utc};
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
