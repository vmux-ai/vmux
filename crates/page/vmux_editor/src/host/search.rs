use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use vmux_core::event::{ExplorerSearchMatch, ExplorerSearchRequest};

use crate::dir::project_root;
use crate::{FileView, GlobalSearchRequest};

const MAX_MATCHES: usize = 500;
const MAX_MATCHES_PER_FILE: usize = 40;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES_SCANNED: usize = 40_000;
const MAX_PREVIEW_CHARS: usize = 240;

pub(crate) struct ProjectSearchPlugin;

impl Plugin for ProjectSearchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(ExplorerSearchRequest,)>::default())
            .add_observer(start_project_search)
            .add_systems(Update, finish_project_search);
    }
}

fn start_project_search(
    trigger: On<BinReceive<ExplorerSearchRequest>>,
    views: Query<&FileView>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(view) = views.get(entity) else {
        return;
    };
    let Some(search) = ProjectSearch::of(&view.path, &trigger.event().payload) else {
        commands.entity(entity).remove::<RunningSearch>();
        return;
    };
    let task = IoTaskPool::get().spawn(async move { search.run() });
    commands.entity(entity).insert(RunningSearch(task));
}

fn finish_project_search(
    mut running: Query<(Entity, &FileView, &mut RunningSearch)>,
    mut writer: MessageWriter<GlobalSearchRequest>,
    mut commands: Commands,
) {
    for (entity, view, mut search) in &mut running {
        let Some(outcome) = block_on(future::poll_once(&mut search.0)) else {
            continue;
        };
        commands.entity(entity).remove::<RunningSearch>();
        writer.write(GlobalSearchRequest {
            target_path: view.path.clone(),
            root: outcome.root.to_string_lossy().into_owned(),
            query: outcome.query,
            matches: outcome.matches,
        });
    }
}

#[derive(Component)]
struct RunningSearch(Task<SearchOutcome>);

struct SearchOutcome {
    root: PathBuf,
    query: String,
    matches: Vec<ExplorerSearchMatch>,
}

struct ProjectSearch {
    root: PathBuf,
    query: String,
    pattern: Regex,
}

impl ProjectSearch {
    fn of(start: &Path, request: &ExplorerSearchRequest) -> Option<Self> {
        let pattern = SearchPattern::compile(request)?;
        Some(Self {
            root: project_root(start),
            query: request.query.clone(),
            pattern,
        })
    }

    fn run(self) -> SearchOutcome {
        let mut matches = Vec::new();
        let mut scanned = 0usize;
        let walk = WalkBuilder::new(&self.root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .max_filesize(Some(MAX_FILE_BYTES))
            .build();
        for entry in walk {
            if matches.len() >= MAX_MATCHES || scanned >= MAX_FILES_SCANNED {
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            let Some(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_file() {
                continue;
            }
            scanned += 1;
            self.scan(entry.path(), &mut matches);
        }
        SearchOutcome {
            root: self.root,
            query: self.query,
            matches,
        }
    }

    fn scan(&self, path: &Path, out: &mut Vec<ExplorerSearchMatch>) {
        let Some(text) = FileText::read(path) else {
            return;
        };
        let display = path.to_string_lossy().into_owned();
        let mut in_file = 0usize;
        for (index, line) in text.lines().enumerate() {
            if in_file >= MAX_MATCHES_PER_FILE || out.len() >= MAX_MATCHES {
                return;
            }
            let Some(found) = self.pattern.find(line) else {
                continue;
            };
            out.push(ExplorerSearchMatch {
                path: display.clone(),
                line: index as u32 + 1,
                col: Utf16Col::at(line, found.start()),
                end_col: Utf16Col::at(line, found.end()),
                preview: LinePreview::of(line),
            });
            in_file += 1;
        }
    }
}

struct SearchPattern;

impl SearchPattern {
    fn compile(request: &ExplorerSearchRequest) -> Option<Regex> {
        if request.query.trim().is_empty() {
            return None;
        }
        let source = match request.regex {
            true => crate::edit::search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        RegexBuilder::new(&source)
            .case_insensitive(!request.case_sensitive)
            .size_limit(1 << 20)
            .build()
            .ok()
    }
}

struct FileText;

impl FileText {
    fn read(path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        Some(crate::encoding::DecodedText::of(&bytes)?.text)
    }
}

struct LinePreview;

impl LinePreview {
    fn of(line: &str) -> String {
        line.trim_end().chars().take(MAX_PREVIEW_CHARS).collect()
    }
}

struct Utf16Col;

impl Utf16Col {
    fn at(line: &str, byte: usize) -> u32 {
        let mut index = byte.min(line.len());
        while index > 0 && !line.is_char_boundary(index) {
            index -= 1;
        }
        line[..index].encode_utf16().count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Project {
        dir: tempfile::TempDir,
    }

    impl Project {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir_all(dir.path().join(".git")).expect("git");
            Self { dir }
        }

        fn write(&self, relative: &str, body: &str) -> &Self {
            let path = self.dir.path().join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, body).expect("write");
            self
        }

        fn search(&self, request: ExplorerSearchRequest) -> Vec<ExplorerSearchMatch> {
            let anchor = self.dir.path().join("anchor.rs");
            std::fs::write(&anchor, "").expect("anchor");
            ProjectSearch::of(&anchor, &request)
                .expect("pattern")
                .run()
                .matches
        }

        fn hits(&self, query: &str) -> Vec<String> {
            let found = self.search(ExplorerSearchRequest {
                query: query.to_string(),
                regex: false,
                case_sensitive: false,
            });
            let mut named = Vec::new();
            for hit in found {
                let relative = Path::new(&hit.path)
                    .strip_prefix(self.dir.path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(hit.path);
                named.push(format!("{relative}:{}", hit.line));
            }
            named.sort();
            named
        }
    }

    #[test]
    fn a_gitignored_file_is_not_searched() {
        let project = Project::new();
        project
            .write(".gitignore", "secret.txt\nbuilt/\n")
            .write("secret.txt", "needle")
            .write("built/out.txt", "needle")
            .write("src/main.rs", "needle");

        assert_eq!(project.hits("needle"), vec!["src/main.rs:1".to_string()]);
    }

    #[test]
    fn a_binary_file_is_skipped_but_its_text_neighbour_is_not() {
        let project = Project::new();
        project.write("keep.txt", "needle");
        std::fs::write(project.dir.path().join("blob.bin"), b"need\0le needle").expect("blob");

        assert_eq!(project.hits("needle"), vec!["keep.txt:1".to_string()]);
    }

    #[test]
    fn a_match_reports_its_one_based_line_and_utf16_columns() {
        let project = Project::new();
        project.write("src/lib.rs", "first\nlet \u{1F600} = \"needle\";  \n");

        let found = project.search(ExplorerSearchRequest {
            query: "needle".to_string(),
            regex: false,
            case_sensitive: true,
        });

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].col, 10);
        assert_eq!(found[0].end_col, 16);
        assert_eq!(found[0].preview, "let \u{1F600} = \"needle\";");
    }

    #[test]
    fn a_case_sensitive_search_rejects_what_an_insensitive_one_accepts() {
        let project = Project::new();
        project.write("src/lib.rs", "Needle");

        let sensitive = project.search(ExplorerSearchRequest {
            query: "needle".to_string(),
            regex: false,
            case_sensitive: true,
        });
        assert!(sensitive.is_empty());
        assert_eq!(project.hits("needle"), vec!["src/lib.rs:1".to_string()]);
    }

    #[test]
    fn a_plain_query_matches_regex_metacharacters_literally() {
        let project = Project::new();
        project.write("a.txt", "a.c").write("b.txt", "abc");

        assert_eq!(project.hits("a.c"), vec!["a.txt:1".to_string()]);
    }

    #[test]
    fn a_regex_query_matches_by_pattern() {
        let project = Project::new();
        project.write("a.txt", "a.c").write("b.txt", "abc");

        let found = project.search(ExplorerSearchRequest {
            query: "a.c".to_string(),
            regex: true,
            case_sensitive: false,
        });

        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_blank_query_builds_no_search() {
        let project = Project::new();
        let anchor = project.dir.path().join("anchor.rs");
        std::fs::write(&anchor, "").expect("anchor");

        assert!(
            ProjectSearch::of(
                &anchor,
                &ExplorerSearchRequest {
                    query: "   ".to_string(),
                    regex: false,
                    case_sensitive: false,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn an_unparseable_regex_builds_no_search() {
        let project = Project::new();
        let anchor = project.dir.path().join("anchor.rs");
        std::fs::write(&anchor, "").expect("anchor");

        assert!(
            ProjectSearch::of(
                &anchor,
                &ExplorerSearchRequest {
                    query: "[unclosed".to_string(),
                    regex: true,
                    case_sensitive: false,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn one_file_cannot_crowd_out_the_rest_of_the_results() {
        let project = Project::new();
        let mut noisy = String::new();
        for _ in 0..(MAX_MATCHES_PER_FILE * 2) {
            noisy.push_str("needle\n");
        }
        project
            .write("noisy.txt", &noisy)
            .write("quiet.txt", "needle");

        let found = project.search(ExplorerSearchRequest {
            query: "needle".to_string(),
            regex: false,
            case_sensitive: false,
        });

        let mut from_noisy = 0;
        for hit in &found {
            if hit.path.ends_with("noisy.txt") {
                from_noisy += 1;
            }
        }
        assert_eq!(from_noisy, MAX_MATCHES_PER_FILE);
        assert!(found.iter().any(|hit| hit.path.ends_with("quiet.txt")));
    }
}
