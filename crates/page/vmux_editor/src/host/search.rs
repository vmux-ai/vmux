use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use vmux_core::event::{ExplorerSearchFile, ExplorerSearchMatch, ExplorerSearchRequest};

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
            files: outcome.files,
            capped: outcome.capped,
        });
    }
}

#[derive(Component)]
struct RunningSearch(Task<SearchOutcome>);

struct SearchOutcome {
    root: PathBuf,
    query: String,
    files: Vec<ExplorerSearchFile>,
    capped: bool,
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
        let mut files = Vec::new();
        let mut found = 0usize;
        let mut scanned = 0usize;
        let mut capped = false;
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
            if found >= MAX_MATCHES || scanned >= MAX_FILES_SCANNED {
                capped = true;
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
            let Some(file) = self.scan(entry.path(), MAX_MATCHES - found) else {
                continue;
            };
            found += file.matches.len();
            files.push(file);
        }
        SearchOutcome {
            root: self.root,
            query: self.query,
            files,
            capped,
        }
    }

    fn scan(&self, path: &Path, budget: usize) -> Option<ExplorerSearchFile> {
        let text = FileText::read(path)?;
        let limit = budget.min(MAX_MATCHES_PER_FILE);
        let mut matches = Vec::new();
        let mut capped = false;
        for (index, line) in text.lines().enumerate() {
            let Some(found) = self.pattern.find(line) else {
                continue;
            };
            if matches.len() >= limit {
                capped = true;
                break;
            }
            matches.push(ExplorerSearchMatch {
                line: index as u32 + 1,
                col: Utf16Col::at(line, found.start()),
                end_col: Utf16Col::at(line, found.end()),
                preview: LinePreview::of(line),
            });
        }
        if matches.is_empty() {
            return None;
        }
        Some(ExplorerSearchFile {
            path: path.to_string_lossy().into_owned(),
            matches,
            capped,
        })
    }
}

struct SearchPattern;

impl SearchPattern {
    fn compile(request: &ExplorerSearchRequest) -> Option<Regex> {
        if request.query.trim().is_empty() {
            return None;
        }
        let mut source = match request.regex {
            true => crate::edit::search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        if request.whole_word {
            let literal = match request.regex {
                true => None,
                false => Some(request.query.as_str()),
            };
            source = WholeWord::around(&source, literal);
        }
        RegexBuilder::new(&source)
            .case_insensitive(!request.case_sensitive)
            .size_limit(1 << 20)
            .build()
            .ok()
    }
}

struct WholeWord;

impl WholeWord {
    fn around(source: &str, literal: Option<&str>) -> String {
        let (lead, trail) = match literal {
            None => (true, true),
            Some(query) => (
                Self::is_word(query.chars().next()),
                Self::is_word(query.chars().next_back()),
            ),
        };
        let mut pattern = String::with_capacity(source.len() + 8);
        if lead {
            pattern.push_str("\\b");
        }
        pattern.push_str("(?:");
        pattern.push_str(source);
        pattern.push(')');
        if trail {
            pattern.push_str("\\b");
        }
        pattern
    }

    fn is_word(edge: Option<char>) -> bool {
        matches!(edge, Some(c) if c.is_alphanumeric() || c == '_')
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

        fn search(&self, request: ExplorerSearchRequest) -> SearchOutcome {
            let anchor = self.dir.path().join("anchor.rs");
            std::fs::write(&anchor, "").expect("anchor");
            ProjectSearch::of(&anchor, &request).expect("pattern").run()
        }

        fn named(&self, request: ExplorerSearchRequest) -> Vec<String> {
            let mut named = Vec::new();
            for file in self.search(request).files {
                let relative = Path::new(&file.path)
                    .strip_prefix(self.dir.path())
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or(file.path);
                for hit in file.matches {
                    named.push(format!("{relative}:{}", hit.line));
                }
            }
            named.sort();
            named
        }

        fn hits(&self, query: &str) -> Vec<String> {
            self.named(ExplorerSearchRequest {
                query: query.to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
            })
        }

        fn words(&self, query: &str, regex: bool) -> Vec<String> {
            self.named(ExplorerSearchRequest {
                query: query.to_string(),
                regex,
                case_sensitive: false,
                whole_word: true,
            })
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
            whole_word: false,
        });

        assert_eq!(found.files.len(), 1);
        let hits = &found.files[0].matches;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
        assert_eq!(hits[0].col, 10);
        assert_eq!(hits[0].end_col, 16);
        assert_eq!(hits[0].preview, "let \u{1F600} = \"needle\";");
    }

    #[test]
    fn a_case_sensitive_search_rejects_what_an_insensitive_one_accepts() {
        let project = Project::new();
        project.write("src/lib.rs", "Needle");

        let sensitive = project.search(ExplorerSearchRequest {
            query: "needle".to_string(),
            regex: false,
            case_sensitive: true,
            whole_word: false,
        });
        assert!(sensitive.files.is_empty());
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
            whole_word: false,
        });

        assert_eq!(found.files.len(), 2);
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
                    whole_word: false,
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
                    whole_word: false,
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
            whole_word: false,
        });

        let mut noisy = None;
        let mut quiet = None;
        for file in &found.files {
            if file.path.ends_with("noisy.txt") {
                noisy = Some(file);
            }
            if file.path.ends_with("quiet.txt") {
                quiet = Some(file);
            }
        }
        let noisy = noisy.expect("noisy file");
        assert_eq!(noisy.matches.len(), MAX_MATCHES_PER_FILE);
        assert!(noisy.capped);
        let quiet = quiet.expect("quiet file");
        assert_eq!(quiet.matches.len(), 1);
        assert!(!quiet.capped);
        assert!(!found.capped);
    }

    #[test]
    fn a_file_reports_every_hit_it_holds_under_one_entry() {
        let project = Project::new();
        project
            .write("src/lib.rs", "needle\nquiet\nneedle\n")
            .write("src/other.rs", "needle\n");

        let found = project.search(ExplorerSearchRequest {
            query: "needle".to_string(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
        });

        assert_eq!(found.files.len(), 2);
        for file in &found.files {
            let expected = if file.path.ends_with("lib.rs") { 2 } else { 1 };
            assert_eq!(file.matches.len(), expected);
            assert!(!file.capped);
        }
    }

    #[test]
    fn a_whole_word_literal_rejects_the_hits_buried_inside_longer_words() {
        let project = Project::new();
        project
            .write("bare.txt", "let value = 1")
            .write("prefixed.txt", "let revalue = 1")
            .write("suffixed.txt", "let valuegram = 1")
            .write("punctuated.txt", "let (value) = 1");

        assert_eq!(
            project.words("value", false),
            vec!["bare.txt:1".to_string(), "punctuated.txt:1".to_string()]
        );
    }

    #[test]
    fn a_whole_word_literal_with_no_word_edge_still_matches() {
        let project = Project::new();
        project
            .write("arrow.rs", "fn f() -> T")
            .write("plain.rs", "let a - b = 1");

        assert_eq!(project.words("->", false), vec!["arrow.rs:1".to_string()]);
    }

    #[test]
    fn a_whole_word_literal_bounds_only_the_end_that_is_a_word() {
        let project = Project::new();
        project
            .write("bare.txt", "call .value here")
            .write("glued.txt", "call .valuegram here");

        assert_eq!(
            project.words(".value", false),
            vec!["bare.txt:1".to_string()]
        );
    }

    #[test]
    fn a_whole_word_regex_bounds_the_whole_alternation() {
        let project = Project::new();
        project
            .write("bare.txt", "one two")
            .write("glued.txt", "oneself twofold");

        assert_eq!(
            project.words("one\\|two", true),
            vec!["bare.txt:1".to_string()]
        );
    }
}
