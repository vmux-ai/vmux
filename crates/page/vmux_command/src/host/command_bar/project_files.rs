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

#[derive(bevy::prelude::Resource, Default)]
pub struct ProjectIndex {
    roots: Vec<RootIndex>,
    asked: Option<String>,
    answered_with: usize,
}

impl ProjectIndex {
    pub fn matches(&mut self, roots: &[PathBuf], query: &str) -> Option<Vec<PathEntry>> {
        self.sync(roots);
        self.asked = Some(query.to_string());
        self.answered_with = self.ready_count();
        self.rank(query)
    }

    pub fn settled(&mut self, roots: &[PathBuf]) -> Option<(String, Vec<PathEntry>)> {
        let query = self.asked.clone()?;
        self.sync(roots);
        let ready = self.ready_count();
        if ready == self.answered_with {
            return None;
        }
        self.answered_with = ready;
        let ranked = self.rank(&query)?;
        Some((query, ranked))
    }

    pub fn forget(&mut self) {
        self.asked = None;
    }

    pub fn asked_query(&self) -> Option<String> {
        self.asked.clone()
    }

    fn ready_count(&self) -> usize {
        let mut ready = 0;
        for index in &self.roots {
            if matches!(index, RootIndex::Ready { .. }) {
                ready += 1;
            }
        }
        ready
    }

    fn rank(&self, query: &str) -> Option<Vec<PathEntry>> {
        let mut ready = Vec::new();
        for index in &self.roots {
            let RootIndex::Ready { root, files, .. } = index else {
                continue;
            };
            ready.push((root.as_path(), files.as_slice()));
        }
        if ready.is_empty() {
            return None;
        }
        Some(FuzzyRank::across(&ready, query))
    }

    fn sync(&mut self, roots: &[PathBuf]) {
        self.roots.retain(|index| roots.contains(index.root()));
        for root in roots {
            match self.roots.iter().position(|index| index.root() == root) {
                Some(at) => self.roots[at].advance(),
                None => self.roots.push(RootIndex::start(root)),
            }
        }
    }
}

enum RootIndex {
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

impl RootIndex {
    fn start(root: &Path) -> Self {
        let walked = root.to_path_buf();
        Self::Building {
            root: root.to_path_buf(),
            task: IoTaskPool::get().spawn(async move { ProjectWalk::of(&walked) }),
        }
    }

    fn root(&self) -> &PathBuf {
        match self {
            Self::Building { root, .. } | Self::Ready { root, .. } => root,
        }
    }

    fn advance(&mut self) {
        match self {
            Self::Building { root, task } => {
                let Some(files) = block_on(future::poll_once(task)) else {
                    return;
                };
                *self = Self::Ready {
                    root: std::mem::take(root),
                    files,
                    built_at: Instant::now(),
                };
            }
            Self::Ready { root, built_at, .. } => {
                if built_at.elapsed() > INDEX_TTL {
                    *self = Self::start(&root.clone());
                }
            }
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
    fn across(roots: &[(&Path, &[String])], query: &str) -> Vec<PathEntry> {
        let needle = query.to_lowercase();
        let mut scored: Vec<(i32, &Path, &String)> = Vec::new();
        for (root, files) in roots {
            for file in files.iter() {
                let Some(score) = FuzzyScore::of(file, &needle) else {
                    continue;
                };
                scored.push((score, root, file));
            }
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.2.len().cmp(&b.2.len()))
                .then(a.2.cmp(b.2))
        });
        scored.truncate(MAX_RESULTS);
        let qualify = roots.len() > 1;
        let mut entries = Vec::with_capacity(scored.len());
        for (_, root, file) in scored {
            let name = match qualify {
                true => format!("{}/{file}", ProjectLabel::of(root)),
                false => file.clone(),
            };
            entries.push(PathEntry {
                name,
                is_dir: false,
                full_path: root.join(file).to_string_lossy().into_owned(),
            });
        }
        entries
    }
}

struct ProjectLabel;

impl ProjectLabel {
    fn of(root: &Path) -> String {
        let Some(name) = root.file_name() else {
            return root.to_string_lossy().into_owned();
        };
        name.to_string_lossy().into_owned()
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
        let ranked = FuzzyRank::across(&[(Path::new("/root"), &files)], "handler");
        assert_eq!(ranked[0].name, "crates/vmux_core/src/handler.rs");
    }

    #[test]
    fn a_path_fragment_query_matches_across_separators() {
        let files = vec![
            "crates/page/vmux_command/src/page.rs".to_string(),
            "crates/vmux_ui/src/page.rs".to_string(),
        ];
        let ranked = FuzzyRank::across(&[(Path::new("/root"), &files)], "command/page");
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "crates/page/vmux_command/src/page.rs");
    }

    #[test]
    fn a_query_whose_letters_are_out_of_order_does_not_match() {
        assert!(FuzzyScore::of("src/handler.rs", "rendlah").is_none());
    }

    #[test]
    fn a_hit_in_every_project_is_ranked_together_and_named_by_its_project() {
        let dashboard = vec!["src/main.rs".to_string()];
        let vmux = vec!["src/main.rs".to_string()];
        let ranked = FuzzyRank::across(
            &[
                (Path::new("/code/dashboard"), &dashboard),
                (Path::new("/code/vmux"), &vmux),
            ],
            "main",
        );

        let named: Vec<&str> = ranked.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(named, ["dashboard/src/main.rs", "vmux/src/main.rs"]);
        let opened: Vec<&str> = ranked
            .iter()
            .map(|entry| entry.full_path.as_str())
            .collect();
        assert_eq!(
            opened,
            ["/code/dashboard/src/main.rs", "/code/vmux/src/main.rs"]
        );
    }

    #[test]
    fn a_better_hit_in_a_later_project_outranks_a_worse_hit_in_the_first() {
        let first = vec!["src/unrelated_handler_helper.rs".to_string()];
        let second = vec!["handler.rs".to_string()];
        let ranked = FuzzyRank::across(
            &[(Path::new("/a"), &first), (Path::new("/b"), &second)],
            "handler",
        );

        assert_eq!(ranked[0].name, "b/handler.rs");
    }

    #[test]
    fn ranked_entries_carry_an_absolute_path_for_opening() {
        let files = vec!["src/main.rs".to_string()];
        let ranked = FuzzyRank::across(&[(Path::new("/root"), &files)], "main");
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
