use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::UiEventPlugin;
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use vmux_ecs::event::{ExplorerSearchFile, ExplorerSearchMatch, ExplorerSearchRequest};
use vmux_ecs::{Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget};

use super::explorer::{
    ExplorerSearchCancelRequest, ExplorerSearchCompleted, ExplorerSearchQuery,
    ExplorerSearchRunRequest,
};
use super::file_lifecycle::FileDir;
use crate::FileView;

pub(crate) struct SearchPlugin;

impl Plugin for SearchPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_plugins(UiEventPlugin::<(ExplorerSearchRequest,)>::default())
            .add_observer(start_project)
            .add_observer(cancel_project)
            .add_systems(Update, finish_project);
    }
}

#[derive(Component)]
struct SearchRun {
    view: Entity,
    generation: u64,
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    cancel: Arc<AtomicBool>,
}

#[derive(Component)]
struct SearchTask(Task<SearchOutput>);

struct SearchOutput {
    root: PathBuf,
    files: Vec<ExplorerSearchFile>,
    capped: bool,
}

fn cancel_project(
    trigger: On<ExplorerSearchCancelRequest>,
    runs: Query<(Entity, &SearchRun, &OperationState)>,
    mut commands: Commands,
) {
    for (operation, run, state) in &runs {
        if run.view != trigger.entity {
            continue;
        }
        run.cancel.store(true, Ordering::Relaxed);
        if matches!(state, OperationState::Pending | OperationState::Running) {
            commands.trigger(OperationCompleted {
                entity: operation,
                result: Err("Search canceled".to_string()),
            });
        }
        commands
            .entity(operation)
            .remove::<(SearchRun, SearchTask)>();
    }
}

fn start_project(
    trigger: On<ExplorerSearchRunRequest>,
    views: Query<&FileView>,
    runs: Query<(Entity, &SearchRun, &OperationState)>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let view = trigger.entity;
    for (operation, run, state) in &runs {
        if run.view != view {
            continue;
        }
        run.cancel.store(true, Ordering::Relaxed);
        if matches!(state, OperationState::Pending | OperationState::Running) {
            commands.trigger(OperationCompleted {
                entity: operation,
                result: Err("Search superseded".to_string()),
            });
        }
        commands
            .entity(operation)
            .remove::<(SearchRun, SearchTask)>();
    }
    let Ok(file) = views.get(view) else {
        return;
    };
    let Some(scanner) = SearchScanner::compile(
        &file.path,
        &trigger.query,
        trigger.regex,
        trigger.case_sensitive,
        trigger.whole_word,
    ) else {
        return;
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let output = scanner.run(&worker_cancel);
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        output
    });
    commands.spawn((
        Operation::bundle("editor.search").unwrap().running(),
        OperationTarget::new(view),
        ChildOf(view),
        SearchRun {
            view,
            generation: trigger.generation,
            query: trigger.query.clone(),
            regex: trigger.regex,
            case_sensitive: trigger.case_sensitive,
            whole_word: trigger.whole_word,
            cancel,
        },
        SearchTask(task),
    ));
}

fn finish_project(
    mut tasks: Query<(Entity, &SearchRun, &mut SearchTask, &OperationState)>,
    queries: Query<&ExplorerSearchQuery>,
    mut commands: Commands,
) {
    for (operation, run, mut task, state) in &mut tasks {
        let Some(output) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(operation)
            .remove::<(SearchRun, SearchTask)>();
        let canceled = run.cancel.load(Ordering::Relaxed)
            || !matches!(state, OperationState::Pending | OperationState::Running);
        if canceled {
            if matches!(state, OperationState::Pending | OperationState::Running) {
                commands.trigger(OperationCompleted {
                    entity: operation,
                    result: Err("Search canceled".to_string()),
                });
            }
            continue;
        }
        commands.trigger(OperationCompleted {
            entity: operation,
            result: Ok(()),
        });
        let current = queries
            .get(run.view)
            .is_ok_and(|query| query.generation == run.generation);
        if !current {
            continue;
        }
        commands.trigger(ExplorerSearchCompleted {
            entity: run.view,
            generation: run.generation,
            root: output.root.to_string_lossy().into_owned(),
            query: run.query.clone(),
            regex: run.regex,
            case_sensitive: run.case_sensitive,
            whole_word: run.whole_word,
            files: output.files,
            capped: output.capped,
        });
    }
}

struct SearchScanner {
    root: PathBuf,
    pattern: Regex,
}

impl SearchScanner {
    const MAX_MATCHES: usize = 500;
    const MAX_MATCHES_PER_FILE: usize = 40;
    const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
    const MAX_FILES_SCANNED: usize = 40_000;
    const MAX_PREVIEW_CHARS: usize = 240;

    fn compile(
        start: &Path,
        query: &str,
        regex: bool,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Option<Self> {
        if query.trim().is_empty() {
            return None;
        }
        let mut source = match regex {
            true => crate::edit::search::Search::translate(query),
            false => regex::escape(query),
        };
        if whole_word {
            let literal = match regex {
                true => None,
                false => Some(query),
            };
            source = Self::whole_word(&source, literal);
        }
        let pattern = RegexBuilder::new(&source)
            .case_insensitive(!case_sensitive)
            .size_limit(1 << 20)
            .build()
            .ok()?;
        Some(Self {
            root: FileDir::project_root(start),
            pattern,
        })
    }

    fn run(self, cancel: &AtomicBool) -> SearchOutput {
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
            .max_filesize(Some(Self::MAX_FILE_BYTES))
            .build();
        for entry in walk {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if found >= Self::MAX_MATCHES || scanned >= Self::MAX_FILES_SCANNED {
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
            let Some(file) = self.scan(entry.path(), Self::MAX_MATCHES - found, cancel) else {
                continue;
            };
            found += file.matches.len();
            files.push(file);
        }
        SearchOutput {
            root: self.root,
            files,
            capped,
        }
    }

    fn scan(&self, path: &Path, budget: usize, cancel: &AtomicBool) -> Option<ExplorerSearchFile> {
        let text = crate::encoding::DecodedText::read(path)?.text;
        let limit = budget.min(Self::MAX_MATCHES_PER_FILE);
        let mut matches = Vec::new();
        let mut capped = false;
        for (index, line) in text.lines().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let Some(found) = self.pattern.find(line) else {
                continue;
            };
            if matches.len() >= limit {
                capped = true;
                break;
            }
            matches.push(ExplorerSearchMatch {
                line: index as u32 + 1,
                col: Self::utf16_col(line, found.start()),
                end_col: Self::utf16_col(line, found.end()),
                preview: line
                    .trim_end()
                    .chars()
                    .take(Self::MAX_PREVIEW_CHARS)
                    .collect(),
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

    fn whole_word(source: &str, literal: Option<&str>) -> String {
        let (lead, trail) = match literal {
            None => (true, true),
            Some(query) => (
                Self::word_edge(query.chars().next()),
                Self::word_edge(query.chars().next_back()),
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

    fn word_edge(edge: Option<char>) -> bool {
        matches!(edge, Some(c) if c.is_alphanumeric() || c == '_')
    }

    fn utf16_col(line: &str, byte: usize) -> u32 {
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
    use bevy_cef::prelude::UiInput;

    #[derive(Resource, Default)]
    struct CapturedResults(Vec<ExplorerSearchCompleted>);

    struct SearchFixture {
        dir: tempfile::TempDir,
        app: App,
        view: Entity,
    }

    impl SearchFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir_all(dir.path().join(".git")).expect("git");
            let anchor = dir.path().join("anchor.rs");
            std::fs::write(&anchor, "").expect("anchor");
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                crate::contract::ContractPlugin,
                SearchPlugin,
                crate::host::explorer::ExplorerPlugin,
            ))
            .init_non_send::<bevy_cef::prelude::Browsers>()
            .init_resource::<CapturedResults>()
            .add_observer(Self::capture);
            let view = app.world_mut().spawn(FileView { path: anchor }).id();
            Self { dir, app, view }
        }

        fn capture(trigger: On<ExplorerSearchCompleted>, mut results: ResMut<CapturedResults>) {
            results.0.push(ExplorerSearchCompleted {
                entity: trigger.entity,
                generation: trigger.generation,
                root: trigger.root.clone(),
                query: trigger.query.clone(),
                regex: trigger.regex,
                case_sensitive: trigger.case_sensitive,
                whole_word: trigger.whole_word,
                files: trigger.files.clone(),
                capped: trigger.capped,
            });
        }

        fn write(&mut self, relative: &str, body: &str) -> &mut Self {
            let path = self.dir.path().join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, body).expect("write");
            self
        }

        fn search(&mut self, request: ExplorerSearchRequest) -> ExplorerSearchCompleted {
            self.app
                .world_mut()
                .resource_mut::<CapturedResults>()
                .0
                .clear();
            self.app.world_mut().trigger(UiInput {
                webview: self.view,
                payload: request,
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(found) = self
                    .app
                    .world_mut()
                    .resource_mut::<CapturedResults>()
                    .0
                    .pop()
                {
                    return found;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "search did not complete"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        fn named(&mut self, request: ExplorerSearchRequest) -> Vec<String> {
            let mut named = Vec::new();
            let root = self.dir.path().to_path_buf();
            for file in self.search(request).files {
                let relative = Path::new(&file.path)
                    .strip_prefix(&root)
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or(file.path);
                for hit in file.matches {
                    named.push(format!("{relative}:{}", hit.line));
                }
            }
            named.sort();
            named
        }

        fn hits(&mut self, query: &str) -> Vec<String> {
            self.named(ExplorerSearchRequest {
                query: query.to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
            })
        }

        fn words(&mut self, query: &str, regex: bool) -> Vec<String> {
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
        let mut project = SearchFixture::new();
        project
            .write(".gitignore", "secret.txt\nbuilt/\n")
            .write("secret.txt", "needle")
            .write("built/out.txt", "needle")
            .write("src/main.rs", "needle");

        assert_eq!(project.hits("needle"), vec!["src/main.rs:1".to_string()]);
    }

    #[test]
    fn a_binary_file_is_skipped_but_its_text_neighbour_is_not() {
        let mut project = SearchFixture::new();
        project.write("keep.txt", "needle");
        std::fs::write(project.dir.path().join("blob.bin"), b"need\0le needle").expect("blob");

        assert_eq!(project.hits("needle"), vec!["keep.txt:1".to_string()]);
    }

    #[test]
    fn a_match_reports_its_one_based_line_and_utf16_columns() {
        let mut project = SearchFixture::new();
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
        let mut project = SearchFixture::new();
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
        let mut project = SearchFixture::new();
        project.write("a.txt", "a.c").write("b.txt", "abc");

        assert_eq!(project.hits("a.c"), vec!["a.txt:1".to_string()]);
    }

    #[test]
    fn a_regex_query_matches_by_pattern() {
        let mut project = SearchFixture::new();
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
    fn blank_and_unparseable_queries_spawn_no_search_operation() {
        let mut project = SearchFixture::new();
        for (query, regex) in [("   ", false), ("[unclosed", true)] {
            project.app.world_mut().trigger(UiInput {
                webview: project.view,
                payload: ExplorerSearchRequest {
                    query: query.to_string(),
                    regex,
                    case_sensitive: false,
                    whole_word: false,
                },
            });
            project.app.update();
            assert_eq!(
                project
                    .app
                    .world_mut()
                    .query::<&SearchRun>()
                    .iter(project.app.world())
                    .count(),
                0
            );
        }
    }

    #[test]
    fn clearing_search_cancels_active_search_and_drops_late_results() {
        let mut project = SearchFixture::new();
        project.write("src/lib.rs", "needle\n");
        project.app.world_mut().trigger(UiInput {
            webview: project.view,
            payload: ExplorerSearchRequest {
                query: "needle".to_string(),
                regex: false,
                case_sensitive: false,
                whole_word: false,
            },
        });
        project.app.world_mut().trigger(UiInput {
            webview: project.view,
            payload: vmux_ecs::event::ExplorerSearchClear,
        });
        for _ in 0..50 {
            project.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            project
                .app
                .world()
                .resource::<CapturedResults>()
                .0
                .is_empty()
        );
    }

    #[test]
    fn one_file_cannot_crowd_out_the_rest_of_the_results() {
        let mut project = SearchFixture::new();
        let mut noisy = String::new();
        for _ in 0..(SearchScanner::MAX_MATCHES_PER_FILE * 2) {
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
        assert_eq!(noisy.matches.len(), SearchScanner::MAX_MATCHES_PER_FILE);
        assert!(noisy.capped);
        let quiet = quiet.expect("quiet file");
        assert_eq!(quiet.matches.len(), 1);
        assert!(!quiet.capped);
        assert!(!found.capped);
    }

    #[test]
    fn a_file_reports_every_hit_it_holds_under_one_entry() {
        let mut project = SearchFixture::new();
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
        let mut project = SearchFixture::new();
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
        let mut project = SearchFixture::new();
        project
            .write("arrow.rs", "fn f() -> T")
            .write("plain.rs", "let a - b = 1");

        assert_eq!(project.words("->", false), vec!["arrow.rs:1".to_string()]);
    }

    #[test]
    fn a_whole_word_literal_bounds_only_the_end_that_is_a_word() {
        let mut project = SearchFixture::new();
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
        let mut project = SearchFixture::new();
        project
            .write("bare.txt", "one two")
            .write("glued.txt", "oneself twofold");

        assert_eq!(
            project.words("one\\|two", true),
            vec!["bare.txt:1".to_string()]
        );
    }
}
