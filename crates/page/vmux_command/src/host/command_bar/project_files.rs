use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};

use crate::event::PathEntry;

const MAX_INDEXED_FILES: usize = 40_000;
const MAX_DEPTH: usize = 12;
const MAX_RESULTS: usize = 40;
const INDEX_TTL: Duration = Duration::from_secs(60);

const UNINTERESTING_DIRS: &[&str] = &[
    ".git",
    ".gradle",
    ".idea",
    ".next",
    ".turbo",
    ".venv",
    "DerivedData",
    "Pods",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "target",
    "vendor",
    "venv",
];

#[derive(Default)]
pub enum ProjectIndex {
    #[default]
    Idle,
    Building {
        root: PathBuf,
        task: Task<Vec<String>>,
    },
    Ready {
        root: PathBuf,
        files: Vec<String>,
        built_at: Instant,
    },
}

impl ProjectIndex {
    pub fn matches(&mut self, root: &Path, query: &str) -> Option<Vec<PathEntry>> {
        self.advance(root);
        let Self::Ready { files, .. } = self else {
            return None;
        };
        Some(FuzzyRank::of(files, root, query))
    }

    fn advance(&mut self, root: &Path) {
        match self {
            Self::Building {
                root: building,
                task,
            } => {
                if building != root {
                    *self = Self::start(root);
                    return;
                }
                let Some(files) = block_on(future::poll_once(task)) else {
                    return;
                };
                *self = Self::Ready {
                    root: root.to_path_buf(),
                    files,
                    built_at: Instant::now(),
                };
            }
            Self::Ready {
                root: ready,
                built_at,
                ..
            } => {
                if ready != root || built_at.elapsed() > INDEX_TTL {
                    *self = Self::start(root);
                }
            }
            Self::Idle => *self = Self::start(root),
        }
    }

    fn start(root: &Path) -> Self {
        let walked = root.to_path_buf();
        Self::Building {
            root: root.to_path_buf(),
            task: IoTaskPool::get().spawn(async move { ProjectWalk::of(&walked) }),
        }
    }
}

struct ProjectWalk;

impl ProjectWalk {
    fn of(root: &Path) -> Vec<String> {
        let mut files = Vec::new();
        let mut frontier = vec![(root.to_path_buf(), 0usize)];
        while let Some((dir, depth)) = frontier.pop() {
            if files.len() >= MAX_INDEXED_FILES {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if depth + 1 > MAX_DEPTH || Self::is_uninteresting(name) {
                        continue;
                    }
                    frontier.push((entry.path(), depth + 1));
                    continue;
                }
                if name.starts_with('.') {
                    continue;
                }
                let Ok(relative) = entry.path().strip_prefix(root).map(Path::to_path_buf) else {
                    continue;
                };
                files.push(relative.to_string_lossy().into_owned());
                if files.len() >= MAX_INDEXED_FILES {
                    break;
                }
            }
        }
        files
    }

    fn is_uninteresting(name: &str) -> bool {
        name.starts_with('.') || UNINTERESTING_DIRS.contains(&name)
    }
}

struct FuzzyRank;

impl FuzzyRank {
    fn of(files: &[String], root: &Path, query: &str) -> Vec<PathEntry> {
        let needle = query.to_lowercase();
        let mut scored: Vec<(i32, &String)> = Vec::new();
        for file in files {
            let Some(score) = FuzzyScore::of(file, &needle) else {
                continue;
            };
            scored.push((score, file));
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1.len().cmp(&b.1.len()))
                .then(a.1.cmp(b.1))
        });
        scored.truncate(MAX_RESULTS);
        let mut entries = Vec::with_capacity(scored.len());
        for (_, file) in scored {
            entries.push(PathEntry {
                name: file.clone(),
                is_dir: false,
                full_path: root.join(file).to_string_lossy().into_owned(),
            });
        }
        entries
    }
}

struct FuzzyScore;

impl FuzzyScore {
    fn of(haystack: &str, needle: &str) -> Option<i32> {
        if needle.is_empty() {
            return Some(0);
        }
        let lowered = haystack.to_lowercase();
        let base = Self::walk(&lowered, needle)?;
        let basename = lowered.rsplit('/').next().unwrap_or(&lowered);
        let mut score = base;
        if basename.contains(needle) {
            score += 60;
        }
        if basename.starts_with(needle) {
            score += 40;
        }
        score -= (haystack.len() / 16) as i32;
        Some(score)
    }

    fn walk(haystack: &str, needle: &str) -> Option<i32> {
        let mut score = 0i32;
        let mut previous_end: Option<usize> = None;
        let mut cursor = 0usize;
        let bytes: Vec<char> = haystack.chars().collect();
        for wanted in needle.chars() {
            let mut found: Option<usize> = None;
            let mut index = cursor;
            while index < bytes.len() {
                if bytes[index] == wanted {
                    found = Some(index);
                    break;
                }
                index += 1;
            }
            let at = found?;
            score += match previous_end {
                Some(previous) if previous + 1 == at => 12,
                Some(previous) => -((at - previous - 1).min(8usize) as i32),
                None => 0,
            };
            if at == 0 {
                score += 10;
            } else if Self::is_boundary(bytes[at - 1]) {
                score += 8;
            }
            previous_end = Some(at);
            cursor = at + 1;
        }
        Some(score)
    }

    fn is_boundary(c: char) -> bool {
        matches!(c, '/' | '_' | '-' | '.' | ' ')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_basename_hit_outranks_a_scattered_path_hit() {
        let files = vec![
            "crates/vmux_core/src/handler.rs".to_string(),
            "docs/h/a/n/dler.md".to_string(),
        ];
        let ranked = FuzzyRank::of(&files, Path::new("/root"), "handler");
        assert_eq!(ranked[0].name, "crates/vmux_core/src/handler.rs");
    }

    #[test]
    fn a_path_fragment_query_matches_across_separators() {
        let files = vec![
            "crates/page/vmux_command/src/page.rs".to_string(),
            "crates/vmux_ui/src/page.rs".to_string(),
        ];
        let ranked = FuzzyRank::of(&files, Path::new("/root"), "command/page");
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "crates/page/vmux_command/src/page.rs");
    }

    #[test]
    fn a_query_whose_letters_are_out_of_order_does_not_match() {
        assert!(FuzzyScore::of("src/handler.rs", "rendlah").is_none());
    }

    #[test]
    fn ranked_entries_carry_an_absolute_path_for_opening() {
        let files = vec!["src/main.rs".to_string()];
        let ranked = FuzzyRank::of(&files, Path::new("/root"), "main");
        assert_eq!(ranked[0].full_path, "/root/src/main.rs");
    }

    #[test]
    fn the_walk_skips_build_output_and_dotted_directories() {
        let root = tempfile::tempdir().expect("tempdir");
        let keep = root.path().join("src");
        std::fs::create_dir_all(&keep).expect("src");
        std::fs::write(keep.join("main.rs"), "").expect("main");
        for skipped in ["target", "node_modules", ".git"] {
            let dir = root.path().join(skipped);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(dir.join("noise.rs"), "").expect("noise");
        }
        let files = ProjectWalk::of(root.path());
        assert_eq!(files, vec!["src/main.rs".to_string()]);
    }
}
