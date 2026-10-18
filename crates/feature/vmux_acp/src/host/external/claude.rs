use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vmux_session::ProviderSession;

use super::{PromptText, Reader as ProviderReader};

pub(super) struct Reader {
    history: PathBuf,
}

impl Reader {
    pub(super) fn at(root: &Path) -> Self {
        Self {
            history: root.join("history.jsonl"),
        }
    }
}

impl ProviderReader for Reader {
    fn read(&self) -> Result<Vec<ProviderSession>, String> {
        let file = match File::open(&self.history) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut indexed = BTreeMap::<String, ClaudeIndexed>::new();
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else {
                continue;
            };
            let Ok(entry) = serde_json::from_str::<ClaudeHistoryLine>(&line) else {
                continue;
            };
            let prompt = PromptText::compact(&entry.display);
            if entry.session_id.trim().is_empty()
                || entry.project.trim().is_empty()
                || prompt.is_empty()
            {
                continue;
            }
            let session = indexed
                .entry(entry.session_id)
                .or_insert_with(|| ClaudeIndexed {
                    cwd: entry.project.clone(),
                    title: prompt.clone(),
                    latest: prompt.clone(),
                    created_at: entry.timestamp,
                    updated_at: entry.timestamp,
                });
            if entry.timestamp < session.created_at {
                session.created_at = entry.timestamp;
                session.title = prompt.clone();
                session.cwd.clone_from(&entry.project);
            }
            if entry.timestamp >= session.updated_at {
                session.updated_at = entry.timestamp;
                session.latest = prompt;
                session.cwd = entry.project;
            }
        }
        let sessions = indexed
            .into_iter()
            .map(|(sid, session)| ProviderSession {
                kind: "acp:claude".to_string(),
                sid,
                agent: "claude".to_string(),
                agent_name: "Claude Code".to_string(),
                name: session.title.clone(),
                description: (session.latest != session.title)
                    .then_some(session.latest.clone())
                    .unwrap_or_default(),
                cwd: session.cwd,
                created_at: session.created_at,
                updated_at: session.updated_at,
                conversation_title: session.title,
                last_prompt: session.latest,
            })
            .collect();
        Ok(sessions)
    }
}

#[derive(Deserialize)]
struct ClaudeHistoryLine {
    display: String,
    timestamp: i64,
    project: String,
    #[serde(rename = "sessionId")]
    session_id: String,
}

struct ClaudeIndexed {
    cwd: String,
    title: String,
    latest: String,
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
    fn claude_reader_groups_prompt_history_by_session() {
        let fixture = Fixture::new();
        fixture.write(
            "history.jsonl",
            concat!(
                r#"{"display":"First prompt","timestamp":100,"project":"/work/vmux","sessionId":"session-1"}"#,
                "\n",
                r#"{"display":"Latest prompt","timestamp":200,"project":"/work/vmux","sessionId":"session-1"}"#,
                "\n",
            ),
        );

        let sessions = Reader::at(fixture.path()).read().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "First prompt");
        assert_eq!(sessions[0].last_prompt, "Latest prompt");
        assert_eq!(sessions[0].updated_at, 200);
    }
}
