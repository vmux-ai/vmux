use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use vmux_session::ProviderSession;

use super::{BoundedLines, PromptText, Reader as ProviderReader, Timestamp};

pub(super) struct Reader {
    sessions: PathBuf,
}

impl Reader {
    const MAX_SESSIONS: usize = 50_000;

    pub(super) fn at(root: &Path) -> Self {
        Self {
            sessions: root.join("logs/session"),
        }
    }

    fn read_meta(path: &Path) -> Option<VibeMeta> {
        if path.symlink_metadata().ok()?.file_type().is_symlink() {
            return None;
        }
        let file = File::open(path).ok()?;
        let mut meta = VibeMeta::default();
        for line in BoundedLines::new(BufReader::new(file)).take(160) {
            let line = line.trim();
            if line.starts_with("\"tools_available\"") {
                break;
            }
            meta.read(line);
        }
        if meta.session_id.is_empty() || meta.cwd.is_empty() {
            return None;
        }
        if meta.updated_at == 0 {
            meta.updated_at = FileTime::modified(path);
        }
        Some(meta)
    }
}

impl ProviderReader for Reader {
    fn read(&self) -> Result<Vec<ProviderSession>, String> {
        let root = match self.sessions.canonicalize() {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut sessions = Vec::new();
        for (index, entry) in entries.flatten().enumerate() {
            if index >= Self::MAX_SESSIONS {
                break;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let path = entry.path().join("meta.json");
            let Some(meta) = Self::read_meta(&path) else {
                continue;
            };
            let title = PromptText::compact(&meta.title);
            sessions.push(ProviderSession {
                kind: "acp:mistral-vibe".to_string(),
                sid: meta.session_id,
                agent: "mistral-vibe".to_string(),
                agent_name: "Mistral Vibe".to_string(),
                name: title.clone(),
                cwd: meta.cwd,
                created_at: meta.created_at,
                updated_at: meta.updated_at,
                conversation_title: title.clone(),
                last_prompt: title,
                ..Default::default()
            });
        }
        Ok(sessions)
    }
}

#[derive(Default)]
struct VibeMeta {
    session_id: String,
    title: String,
    cwd: String,
    created_at: i64,
    updated_at: i64,
}

impl VibeMeta {
    fn read(&mut self, line: &str) {
        if let Some(value) = JsonLine::string(line, "session_id") {
            self.session_id = value;
        } else if let Some(value) = JsonLine::string(line, "title") {
            self.title = value;
        } else if let Some(value) = JsonLine::string(line, "working_directory") {
            self.cwd = value;
        } else if let Some(value) = JsonLine::string(line, "start_time") {
            self.created_at = Timestamp::parse(&value);
        } else if let Some(value) = JsonLine::string(line, "end_time") {
            self.updated_at = Timestamp::parse(&value);
        }
    }
}

struct JsonLine;

impl JsonLine {
    fn string(line: &str, name: &str) -> Option<String> {
        let prefix = format!("\"{name}\":");
        let value = line.strip_prefix(&prefix)?.trim().trim_end_matches(',');
        serde_json::from_str(value).ok()
    }
}

struct FileTime;

impl FileTime {
    fn modified(path: &Path) -> i64 {
        let Ok(modified) = path.metadata().and_then(|metadata| metadata.modified()) else {
            return 0;
        };
        let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
            return 0;
        };
        duration.as_millis().min(i64::MAX as u128) as i64
    }
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
    fn vibe_reader_reads_metadata_without_the_large_tool_manifest() {
        let fixture = Fixture::new();
        fixture.write(
            "logs/session/session_1/meta.json",
            r#"{
  "session_id": "vibe-1",
  "start_time": "2026-10-18T16:00:00Z",
  "end_time": "2026-10-18T16:05:00Z",
  "environment": {
    "working_directory": "/work/vmux"
  },
  "title": "Build the session catalog",
  "stats": {
    "steps": 1
  },
  "tools_available": []
}
"#,
        );

        let sessions = Reader::at(fixture.path()).read().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].sid, "vibe-1");
        assert_eq!(sessions[0].cwd, "/work/vmux");
        assert_eq!(sessions[0].name, "Build the session catalog");
    }
}
