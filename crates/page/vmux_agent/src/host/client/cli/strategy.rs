use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vmux_core::agent::AgentKind;
use vmux_service::message::Message;

use crate::McpServerConfig;
use crate::strategy::AgentStrategy;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CliModelCatalog {
    pub selected: String,
    pub models: Vec<vmux_wire::room::ModelOptionEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub transcript: PathBuf,
    pub mtime: SystemTime,
    pub title: String,
    pub cross_runtime: bool,
}

pub(crate) struct PromptHistory;

impl PromptHistory {
    const KEEP: usize = 200;
    const BUDGET: u64 = 1024 * 1024;

    pub(crate) fn lines_of(path: &Path) -> Vec<String> {
        SessionTail::tail_of(path, Self::BUDGET)
    }

    pub(crate) fn recent(spoken: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut history = Vec::new();
        for text in spoken.into_iter().rev() {
            if text.trim().is_empty() || !seen.insert(text.clone()) {
                continue;
            }
            history.push(text);
            if history.len() == Self::KEEP {
                break;
            }
        }
        history.reverse();
        history
    }
}

pub(crate) struct SameProject;

impl SameProject {
    pub(crate) fn covers(entry: &str, cwd: &Path) -> bool {
        let entry = Path::new(entry);
        entry.starts_with(cwd) || cwd.starts_with(entry)
    }
}

pub(crate) struct SessionTail;

impl SessionTail {
    const BUDGET: u64 = 256 * 1024;

    pub(crate) fn lines_of(path: &Path) -> Vec<String> {
        Self::tail_of(path, Self::BUDGET)
    }

    fn tail_of(path: &Path, budget: u64) -> Vec<String> {
        use std::io::{Read, Seek, SeekFrom};

        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(end) = file.seek(SeekFrom::End(0)) else {
            return Vec::new();
        };
        let from = end.saturating_sub(budget);
        if file.seek(SeekFrom::Start(from)).is_err() {
            return Vec::new();
        }
        let mut read = Vec::new();
        if file.read_to_end(&mut read).is_err() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&read);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if from > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        lines
    }
}

pub(crate) fn lines_skipping_invalid_utf8<R: std::io::BufRead>(
    reader: R,
) -> impl Iterator<Item = String> {
    reader
        .lines()
        .map_while(|line| match line {
            Ok(line) => Some(Some(line)),
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => Some(None),
            Err(_) => None,
        })
        .flatten()
}

pub trait CliAgentStrategy: AgentStrategy {
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn model_catalog(&self) -> CliModelCatalog {
        CliModelCatalog::default()
    }

    fn model_args(&self, _model: &str) -> Vec<String> {
        Vec::new()
    }

    fn model_env(&self, _model: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    fn effort_args(&self, _level: &str) -> Vec<String> {
        Vec::new()
    }

    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn prepare_launch(&self, _mcp: &McpServerConfig) {}
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
    fn list_sessions(&self) -> Vec<ResumableSession> {
        Vec::new()
    }

    fn latest_message(&self, _transcript: &Path) -> String {
        String::new()
    }

    fn prompt_history(&self, _cwd: &Path) -> Vec<String> {
        Vec::new()
    }

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        Err(format!("transcript loading unsupported for {session_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::PromptHistory;

    #[test]
    fn a_worktree_and_the_repo_it_came_from_share_a_history() {
        use super::SameProject;
        use std::path::Path;

        let repo = "/w/vmux-cloud";
        let tree = Path::new("/w/vmux-cloud/.worktrees/vmx-198");

        assert!(
            SameProject::covers(repo, tree),
            "prompts typed in the repo are the same work as prompts typed in its worktree, and \
             a worktree that has only just been made would otherwise recall nothing"
        );
        assert!(SameProject::covers(
            "/w/vmux-cloud/.worktrees/vmx-198",
            Path::new(repo)
        ));
        assert!(
            !SameProject::covers("/w/vmux-cloud-2", tree),
            "a sibling whose name merely starts the same is a different project"
        );
    }

    #[test]
    fn history_ends_with_the_newest_prompt_and_keeps_one_of_each() {
        let spoken = vec![
            "oldest".to_string(),
            "repeated".to_string(),
            "  ".to_string(),
            "repeated".to_string(),
            "newest".to_string(),
        ];

        assert_eq!(
            PromptHistory::recent(spoken),
            vec![
                "oldest".to_string(),
                "repeated".to_string(),
                "newest".to_string()
            ],
            "the reader presses up expecting what they typed last, so the newest entry has to \
             be the one the walker reaches first; a duplicate keeps only its latest place, and \
             blank lines are not prompts"
        );
    }
}
