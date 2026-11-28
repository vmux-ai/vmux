use crate::host::snapshot::{
    CommandBarProjectRoots, CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot,
    WriteCommandBarSnapshots,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::prelude::{Changed, Commands};
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vmux_api::command_bar::{
    CommandBarRecentFile, CommandBarUiState, PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use vmux_ecs::UiStateWrite;
use vmux_runner::WinitWake;

pub(super) struct CompletionPlugin;

impl Plugin for CompletionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_plugins(UiEventPlugin::<(PathCompleteRequest,)>::default())
            .add_observer(request)
            .add_systems(
                Update,
                (
                    sync_roots.after(WriteCommandBarSnapshots),
                    poll_walks.after(sync_roots),
                    answer_index.after(poll_walks),
                    start_paths.after(answer_index),
                    answer_paths.after(start_paths),
                ),
            );
    }
}

#[derive(Component, Default)]
struct CompletionIndex {
    generation: u64,
}

impl CompletionIndex {
    const MAX_RESULTS: usize = 200;
    const ACTIVE_PROJECT_LIFT: i32 = 40;
    const RECENT_FILE_LIFT: i32 = 16;

    fn rank(
        roots: &[(&Path, &RootWalkResult)],
        active: Option<&Path>,
        recent: &[String],
        query: &str,
        request_id: u64,
    ) -> Option<PathCompleteResponse> {
        if roots.is_empty() {
            return None;
        }
        let mut terms = Vec::new();
        let mut needed = 0u64;
        for term in query.split_whitespace() {
            let term = term.to_lowercase();
            needed |= RootWalkResult::mask_for(&term);
            terms.push(term);
        }
        let mut partial = false;
        let mut seen = 0usize;
        let mut best: Vec<(i32, &Path, &WalkedPath)> = Vec::with_capacity(Self::MAX_RESULTS + 1);
        for favoured in [true, false] {
            for (root, walked) in roots {
                let is_favoured = active == Some(*root);
                if is_favoured != favoured {
                    continue;
                }
                partial |= walked.partial;
                let lift = if is_favoured {
                    Self::ACTIVE_PROJECT_LIFT
                } else {
                    0
                };
                for path in &walked.paths {
                    let Some(score) = RootWalkResult::score_path(path, &terms, needed) else {
                        continue;
                    };
                    Self::offer(&mut best, &mut seen, (score + lift, *root, path));
                }
            }
        }
        Self::apply_recency(&mut best, recent);
        let mut completions = Vec::with_capacity(best.len());
        for (_, root, path) in best {
            let project = root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());
            completions.push(PathEntry {
                name: path.relative.clone(),
                is_dir: path.is_dir,
                full_path: root.join(&path.relative).to_string_lossy().into_owned(),
                project,
            });
        }
        Some(PathCompleteResponse {
            request_id,
            completions,
            truncated: partial,
            total: u32::try_from(seen).unwrap_or(u32::MAX),
        })
    }

    fn compare_hit(
        left: &(i32, &Path, &WalkedPath),
        right: &(i32, &Path, &WalkedPath),
    ) -> Ordering {
        right
            .0
            .cmp(&left.0)
            .then(left.2.relative.len().cmp(&right.2.relative.len()))
            .then(left.2.relative.cmp(&right.2.relative))
    }

    fn offer<'a>(
        best: &mut Vec<(i32, &'a Path, &'a WalkedPath)>,
        seen: &mut usize,
        candidate: (i32, &'a Path, &'a WalkedPath),
    ) {
        *seen += 1;
        if best.len() == Self::MAX_RESULTS
            && let Some(worst) = best.last()
            && Self::compare_hit(&candidate, worst).is_ge()
        {
            return;
        }
        let at = best.partition_point(|held| Self::compare_hit(&candidate, held).is_ge());
        best.insert(at, candidate);
        best.truncate(Self::MAX_RESULTS);
    }

    fn apply_recency(best: &mut [(i32, &Path, &WalkedPath)], recent: &[String]) {
        if recent.is_empty() {
            return;
        }
        let mut lifted = false;
        for (score, root, path) in best.iter_mut() {
            let Some(root_str) = root.to_str() else {
                continue;
            };
            let matched = recent.iter().any(|candidate| {
                candidate
                    .strip_prefix(root_str)
                    .is_some_and(|rest| rest.strip_prefix('/').unwrap_or(rest) == path.relative)
            });
            if matched {
                *score += Self::RECENT_FILE_LIFT;
                lifted = true;
            }
        }
        if lifted {
            best.sort_by(Self::compare_hit);
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct CompletionRoot(PathBuf);

#[derive(Component)]
struct RootWalkBuiltAt(Instant);

impl RootWalkBuiltAt {
    const TTL: Duration = Duration::from_secs(60);

    fn expired(&self) -> bool {
        self.0.elapsed() > Self::TTL
    }
}

#[derive(Component)]
struct RootWalkTask(Task<RootWalkResult>);

impl RootWalkTask {
    fn spawn(root: &Path, proxy: Option<&EventLoopProxyWrapper>) -> Self {
        let walked = root.to_path_buf();
        let wake = WinitWake::beside(proxy);
        Self(IoTaskPool::get().spawn(async move {
            let _wake = wake;
            RootWalkResult::scan(&walked)
        }))
    }
}

struct WalkedPath {
    relative: String,
    folded: Option<String>,
    mask: u64,
    is_dir: bool,
}

#[derive(Component, Default)]
struct RootWalkResult {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    paths: Vec<WalkedPath>,
    partial: bool,
}

impl RootWalkResult {
    const MAX_INDEXED_PATHS: usize = 400_000;

    fn entry(relative: String, is_dir: bool) -> WalkedPath {
        let lowered = relative.to_lowercase();
        let mask = Self::mask_for(&lowered);
        let folded = if lowered == relative {
            None
        } else {
            Some(lowered)
        };
        WalkedPath {
            relative,
            folded,
            mask,
            is_dir,
        }
    }

    fn scan(root: &Path) -> Self {
        let (entries, partial) =
            super::fs_io::CommandBarFsIo::walk_root(root, Self::MAX_INDEXED_PATHS);
        let mut walked = Self {
            paths: Vec::with_capacity(entries.len()),
            partial,
        };
        for (relative, is_dir) in entries {
            walked.push_capped(relative, is_dir, Self::MAX_INDEXED_PATHS);
        }
        walked
    }

    fn push_capped(&mut self, relative: String, is_dir: bool, ceiling: usize) {
        if self.paths.len() >= ceiling {
            self.partial = true;
            return;
        }
        self.paths.push(Self::entry(relative, is_dir));
    }

    fn mask_for(folded: &str) -> u64 {
        let mut held = 0u64;
        for ch in folded.chars() {
            if ch.is_ascii_lowercase() {
                held |= 1 << (ch as u8 - b'a');
            } else if ch.is_ascii_digit() {
                held |= 1 << (26 + ch as u8 - b'0');
            }
        }
        held
    }

    fn score_path(path: &WalkedPath, terms: &[String], needed: u64) -> Option<i32> {
        if path.mask & needed != needed {
            return None;
        }
        let folded = match &path.folded {
            Some(folded) => folded.as_str(),
            None => path.relative.as_str(),
        };
        let mut total = 0;
        for term in terms {
            total += Self::score_term(folded, term)?;
        }
        Some(total)
    }

    fn score_term(lowered: &str, needle: &str) -> Option<i32> {
        if needle.is_empty() {
            return Some(0);
        }
        let base = Self::walk_subsequence(lowered, needle)?;
        let basename = lowered.rsplit('/').next().unwrap_or(lowered);
        let mut score = base;
        if basename.contains(needle) {
            score += 60;
        }
        if basename.starts_with(needle) {
            score += 40;
        }
        score -= (lowered.len() / 16) as i32;
        Some(score)
    }

    fn walk_subsequence(haystack: &str, needle: &str) -> Option<i32> {
        let mut remaining = needle.chars();
        let Some(mut wanted) = remaining.next() else {
            return Some(0);
        };
        let mut score = 0i32;
        let mut previous_end: Option<usize> = None;
        let mut previous_char: Option<char> = None;
        for (at, ch) in haystack.chars().enumerate() {
            if ch == wanted {
                score += match previous_end {
                    Some(previous) if previous + 1 == at => 12,
                    Some(previous) => -((at - previous - 1).min(8usize) as i32),
                    None => 0,
                };
                if at == 0 {
                    score += 10;
                } else if previous_char
                    .is_some_and(|prev| matches!(prev, '/' | '_' | '-' | '.' | ' '))
                {
                    score += 8;
                }
                previous_end = Some(at);
                let Some(next) = remaining.next() else {
                    return Some(score);
                };
                wanted = next;
            }
            previous_char = Some(ch);
        }
        None
    }
}

#[derive(Component, Clone)]
struct PendingProjectCompletion {
    request_id: u64,
    query: String,
    roots: Vec<PathBuf>,
    answered_with: u64,
}

#[derive(Component)]
struct PathCompletionRequest {
    request_id: u64,
    query: String,
}

impl PathCompletionRequest {
    fn list_directory(query: &str, request_id: u64) -> PathCompleteResponse {
        super::fs_io::CommandBarFsIo::list_directory(
            query,
            request_id,
            CompletionIndex::MAX_RESULTS,
        )
    }
}

#[derive(Component)]
struct PathCompletionOperation {
    task: Task<PathCompleteResponse>,
}

#[derive(SystemParam)]
struct Sources<'w, 's> {
    workspace: Single<'w, 's, Ref<'static, CommandBarWorkspaceSnapshot>>,
    projects: Single<'w, 's, Ref<'static, CommandBarProjectRoots>>,
    work: Single<'w, 's, Ref<'static, CommandBarWorkSnapshot>>,
}

impl Sources<'_, '_> {
    fn roots(&self, query: &str) -> Vec<PathBuf> {
        let trimmed = query.trim();
        if trimmed.starts_with('/') || trimmed.starts_with('~') || trimmed.starts_with('.') {
            return Vec::new();
        }
        self.all()
    }

    fn all(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for candidate in self
            .workspace
            .project_root
            .as_deref()
            .into_iter()
            .chain(self.projects.roots.iter().map(String::as_str))
        {
            let root = PathBuf::from(candidate.trim());
            if roots.contains(&root) || !root.is_dir() {
                continue;
            }
            roots.push(root);
        }
        roots
    }

    fn include(roots: &mut Vec<PathBuf>, additional: &[PathBuf]) {
        for root in additional {
            if !roots.contains(root) {
                roots.push(root.clone());
            }
        }
    }

    fn active_root(&self) -> Option<PathBuf> {
        for candidate in [
            self.projects.active.as_deref(),
            self.workspace.project_root.as_deref(),
        ] {
            let Some(candidate) = candidate else {
                continue;
            };
            let trimmed = candidate.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
        None
    }

    fn recent_paths(&self) -> Vec<String> {
        Self::extract_recent_paths(&self.work.recent_files)
    }

    fn extract_recent_paths(files: &[CommandBarRecentFile]) -> Vec<String> {
        let mut opened = Vec::with_capacity(files.len());
        for file in files {
            let Some(path) = file.url.strip_prefix("file://") else {
                continue;
            };
            opened.push(path.to_string());
        }
        opened
    }
}

type RootSnapshotQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CompletionRoot,
        Option<&'static RootWalkResult>,
        Option<&'static RootWalkBuiltAt>,
        Has<RootWalkTask>,
    ),
>;

#[derive(SystemParam)]
struct CompletionSurface<'w, 's> {
    browsers: Option<NonSend<'w, Browsers>>,
    index: Single<'w, 's, &'static CompletionIndex>,
}

impl CompletionSurface<'_, '_> {
    fn can_emit_to(&self, entity: &Entity) -> bool {
        self.browsers
            .as_deref()
            .is_some_and(|browsers| browsers.can_emit_to(entity))
    }

    fn generation(&self) -> u64 {
        self.index.generation
    }
}

#[derive(SystemParam)]
struct RequestIndex<'w, 's> {
    pending: Query<'w, 's, &'static PendingProjectCompletion>,
    roots: RootSnapshotQuery<'w, 's>,
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Project file index"), CompletionIndex::default()));
}

fn request(
    trigger: On<UiInput<PathCompleteRequest>>,
    sources: Sources,
    surface: CompletionSurface,
    state: RequestIndex,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let asking = trigger.event().webview;
    if !surface.can_emit_to(&asking) {
        return;
    }
    let query = &trigger.event().payload.query;
    let request_id = trigger.event().payload.request_id;
    let roots = sources.roots(query);
    if roots.is_empty() {
        commands
            .entity(asking)
            .remove::<PendingProjectCompletion>()
            .insert(PathCompletionRequest {
                request_id,
                query: query.to_string(),
            });
        return;
    }
    let mut wanted = sources.all();
    for waiting in &state.pending {
        Sources::include(&mut wanted, &waiting.roots);
    }
    Sources::include(&mut wanted, &roots);

    let mut walking_any = false;
    let mut ready: Vec<(&Path, &RootWalkResult)> = Vec::new();
    for root in &wanted {
        if let Some((_, existing, result, _, walking)) = state
            .roots
            .iter()
            .find(|(_, candidate, _, _, _)| &candidate.0 == root)
        {
            if roots.contains(root) {
                walking_any |= walking;
                if let Some(result) = result {
                    ready.push((existing.0.as_path(), result));
                }
            }
        } else {
            commands.spawn((
                CompletionRoot(root.clone()),
                RootWalkTask::spawn(root, proxy.as_deref()),
            ));
            if roots.contains(root) {
                walking_any = true;
            }
        }
    }

    if walking_any {
        commands.entity(asking).insert(PendingProjectCompletion {
            request_id,
            query: query.to_string(),
            roots: roots.clone(),
            answered_with: surface.generation(),
        });
    } else {
        commands.entity(asking).remove::<PendingProjectCompletion>();
    }
    let active = sources.active_root();
    let recent = sources.recent_paths();
    let Some(response) =
        CompletionIndex::rank(&ready, active.as_deref(), &recent, query, request_id)
    else {
        commands.entity(asking).insert(PathCompletionRequest {
            request_id,
            query: query.to_string(),
        });
        return;
    };
    commands
        .entity(asking)
        .remove::<PathCompletionRequest>()
        .remove::<PathCompletionOperation>();
    commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
        asking, &response,
    ));
}

fn sync_roots(
    sources: Sources,
    pending: Query<&PendingProjectCompletion>,
    existing_roots: RootSnapshotQuery,
    mut index: Single<&mut CompletionIndex>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut wanted = sources.all();
    for waiting in &pending {
        Sources::include(&mut wanted, &waiting.roots);
    }
    for (entity, root, _, built_at, walking) in &existing_roots {
        if !wanted.contains(&root.0) {
            commands.entity(entity).despawn();
            index.generation += 1;
            continue;
        }
        if !walking && built_at.is_some_and(RootWalkBuiltAt::expired) {
            commands
                .entity(entity)
                .insert(RootWalkTask::spawn(&root.0, proxy.as_deref()));
        }
    }
    for root in &wanted {
        if existing_roots
            .iter()
            .any(|(_, existing, _, _, _)| &existing.0 == root)
        {
            continue;
        }
        commands.spawn((
            CompletionRoot(root.clone()),
            RootWalkTask::spawn(root, proxy.as_deref()),
        ));
    }
}

fn poll_walks(
    mut tasks: Query<(Entity, &mut RootWalkTask)>,
    mut index: Single<&mut CompletionIndex>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(walk) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<RootWalkTask>()
            .insert((walk, RootWalkBuiltAt(Instant::now())));
        index.generation += 1;
    }
}

fn answer_index(
    sources: Sources,
    surface: CompletionSurface,
    existing_roots: RootSnapshotQuery,
    mut pending: Query<(Entity, &mut PendingProjectCompletion)>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let active = sources.active_root();
    let recent = sources.recent_paths();
    let generation = surface.generation();
    for (webview, mut waiting) in &mut pending {
        if !surface.can_emit_to(&webview) {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>()
                .remove::<PathCompletionRequest>()
                .remove::<PathCompletionOperation>();
            continue;
        }
        let roots = sources.roots(&waiting.query);
        if roots.is_empty() {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>();
            continue;
        }
        waiting.roots.clone_from(&roots);
        let mut walking_any = false;
        let mut ready: Vec<(&Path, &RootWalkResult)> = Vec::new();
        for (_, root, result, _, walking) in &existing_roots {
            if !roots.contains(&root.0) {
                continue;
            }
            walking_any |= walking;
            if let Some(result) = result {
                ready.push((root.0.as_path(), result));
            }
        }
        let response = if waiting.answered_with != generation {
            waiting.answered_with = generation;
            CompletionIndex::rank(
                &ready,
                active.as_deref(),
                &recent,
                &waiting.query,
                waiting.request_id,
            )
        } else {
            None
        };
        if !walking_any {
            commands
                .entity(webview)
                .remove::<PendingProjectCompletion>();
        }
        let Some(response) = response else {
            continue;
        };
        commands
            .entity(webview)
            .remove::<PathCompletionRequest>()
            .remove::<PathCompletionOperation>();
        commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
            webview, &response,
        ));
    }
}

fn start_paths(
    requests: Query<(Entity, &PathCompletionRequest), Changed<PathCompletionRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (webview, request) in &requests {
        let query = request.query.clone();
        let request_id = request.request_id;
        let wake = WinitWake::beside(proxy.as_deref());
        let task = IoTaskPool::get().spawn(async move {
            let _wake = wake;
            PathCompletionRequest::list_directory(&query, request_id)
        });
        commands
            .entity(webview)
            .remove::<PathCompletionRequest>()
            .insert(PathCompletionOperation { task });
    }
}

fn answer_paths(
    surface: CompletionSurface,
    mut paths: Query<(Entity, &mut PathCompletionOperation)>,
    mut commands: Commands,
) {
    for (webview, mut pending) in &mut paths {
        let Some(response) = block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if surface.can_emit_to(&webview) {
            commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
                webview, &response,
            ));
        }
        commands.entity(webview).remove::<PathCompletionOperation>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::UiStatePatch;

    #[derive(Resource, Default)]
    struct RecordedResponses(Vec<(Entity, PathCompleteResponse)>);

    impl RecordedResponses {
        fn capture(trigger: On<UiStateWrite<CommandBarUiState>>, mut recorded: ResMut<Self>) {
            if let Some(response) =
                <vmux_api::command_bar::CommandBarUiStatePatch as UiStatePatch<
                    PathCompleteResponse,
                >>::payload(trigger.event().update())
            {
                recorded
                    .0
                    .push((trigger.event().webview(), response.clone()));
            }
        }
    }

    impl RootWalkResult {
        fn files(paths: &[&str]) -> Self {
            let mut result = Self::default();
            for path in paths {
                result.paths.push(Self::entry((*path).to_string(), false));
            }
            result
        }

        fn synthetic(count: usize) -> Self {
            let areas = [
                "crates", "apps", "packages", "services", "libs", "tools", "infra", "docs",
            ];
            let kinds = [
                "Handler",
                "Service",
                "Model",
                "View",
                "Router",
                "Client",
                "Store",
                "Widget",
                "Adapter",
                "Registry",
                "設定",
                "Café",
                "İstanbul",
                "Größe",
            ];
            let extensions = ["rs", "ts", "tsx", "py", "go", "css", "md", "json"];
            let mut result = Self::default();
            let mut at = 0usize;
            while result.paths.len() < count {
                let area = areas[at % areas.len()];
                let package = at % 900;
                let module = (at / 7) % 60;
                let kind = kinds[(at / 3) % kinds.len()];
                let extension = extensions[(at / 11) % extensions.len()];
                result.paths.push(Self::entry(
                    format!(
                        "{area}/pkg_{package:03}/src/module_{module:02}/{kind}Impl{at}.{extension}"
                    ),
                    false,
                ));
                at += 1;
            }
            result
        }
    }

    #[test]
    fn path_query_lists_directories_then_visible_files_then_hidden_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("folder")).unwrap();
        std::fs::write(root.path().join("file"), "").unwrap();
        std::fs::write(root.path().join(".hidden"), "").unwrap();

        let response =
            PathCompletionRequest::list_directory(&format!("{}/", root.path().display()), 1);
        let names = response
            .completions
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["folder/", "file", ".hidden"]);
    }

    #[test]
    fn the_prefilter_never_rejects_a_path_the_scorer_would_have_matched() {
        let walked = RootWalkResult::synthetic(20_000);
        let terms = [
            "handler",
            "設定",
            "İstanbul",
            "Café",
            "Größe",
            "42",
            "impl7",
            "a",
            "z9",
            "router.ts",
        ];
        for term in terms {
            let term = term.to_lowercase();
            let needed = RootWalkResult::mask_for(&term);
            for path in &walked.paths {
                let folded = path.folded.as_deref().unwrap_or(path.relative.as_str());
                if RootWalkResult::score_term(folded, &term).is_none() {
                    continue;
                }
                assert_eq!(
                    path.mask & needed,
                    needed,
                    "{term:?} matched {:?} but the prefilter rejected it",
                    path.relative
                );
            }
        }
    }

    #[test]
    fn a_monorepo_sized_index_ranks_a_late_best_hit_over_the_matches_before_it() {
        let mut walked = RootWalkResult::synthetic(300_000);
        walked
            .paths
            .push(RootWalkResult::entry("handler.rs".to_string(), false));
        let ranked = CompletionIndex::rank(
            &[(Path::new("/code/monorepo"), &walked)],
            None,
            &[],
            "handler",
            1,
        )
        .unwrap()
        .completions;

        assert!(
            ranked.len() > 1,
            "nothing competed with the late hit, so its placement proves nothing"
        );
        assert_eq!(ranked[0].name, "handler.rs");
    }

    #[test]
    fn the_ranker_reports_how_many_matched_even_though_it_ships_only_the_best() {
        let mut walked = RootWalkResult::default();
        for at in 0..(CompletionIndex::MAX_RESULTS + 25) {
            walked.paths.push(RootWalkResult::entry(
                format!("src/dir_{at:03}/main.rs"),
                false,
            ));
        }
        walked
            .paths
            .push(RootWalkResult::entry("docs/readme.md".to_string(), false));

        let ranked = CompletionIndex::rank(
            &[(Path::new("/code/monorepo"), &walked)],
            None,
            &[],
            "main.rs",
            1,
        )
        .unwrap();

        assert_eq!(ranked.completions.len(), CompletionIndex::MAX_RESULTS);
        assert_eq!(ranked.total as usize, CompletionIndex::MAX_RESULTS + 25);
    }

    #[test]
    fn a_basename_hit_outranks_a_scattered_path_hit() {
        let walked =
            RootWalkResult::files(&["crates/util/vmux_ecs/src/handler.rs", "docs/h/a/n/dler.md"]);
        let ranked =
            CompletionIndex::rank(&[(Path::new("/root"), &walked)], None, &[], "handler", 1)
                .unwrap()
                .completions;
        assert_eq!(ranked[0].name, "crates/util/vmux_ecs/src/handler.rs");
    }

    #[test]
    fn a_path_fragment_query_matches_across_separators() {
        let walked = RootWalkResult::files(&[
            "crates/feature/vmux_command/src/page.rs",
            "crates/util/vmux_ui/src/page.rs",
        ]);
        let ranked = CompletionIndex::rank(
            &[(Path::new("/root"), &walked)],
            None,
            &[],
            "command/page",
            1,
        )
        .unwrap()
        .completions;
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "crates/feature/vmux_command/src/page.rs");
    }

    #[test]
    fn a_query_whose_letters_are_out_of_order_does_not_match() {
        assert!(RootWalkResult::score_term("src/handler.rs", "rendlah").is_none());
    }

    #[test]
    fn every_space_separated_term_must_match_the_same_path() {
        let walked = RootWalkResult::files(&[
            "crates/app/vmux_mobile/src/main.rs",
            "crates/app/vmux_desktop/src/main.rs",
        ]);
        let ranked = CompletionIndex::rank(
            &[(Path::new("/root"), &walked)],
            None,
            &[],
            "mobile main.rs",
            1,
        )
        .unwrap()
        .completions;

        let named: Vec<&str> = ranked.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(named, ["crates/app/vmux_mobile/src/main.rs"]);
    }

    #[test]
    fn a_hit_in_every_project_is_ranked_together_and_named_by_its_project() {
        let dashboard = RootWalkResult::files(&["src/main.rs"]);
        let vmux = RootWalkResult::files(&["src/main.rs"]);
        let ranked = CompletionIndex::rank(
            &[
                (Path::new("/code/dashboard"), &dashboard),
                (Path::new("/code/vmux"), &vmux),
            ],
            None,
            &[],
            "main",
            1,
        )
        .unwrap()
        .completions;

        let named: Vec<(&str, &str)> = ranked
            .iter()
            .map(|entry| (entry.project.as_str(), entry.name.as_str()))
            .collect();
        assert_eq!(
            named,
            [("dashboard", "src/main.rs"), ("vmux", "src/main.rs")]
        );
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
        let first = RootWalkResult::files(&["src/unrelated_handler_helper.rs"]);
        let second = RootWalkResult::files(&["handler.rs"]);
        let ranked = CompletionIndex::rank(
            &[(Path::new("/a"), &first), (Path::new("/b"), &second)],
            None,
            &[],
            "handler",
            1,
        )
        .unwrap()
        .completions;

        assert_eq!(ranked[0].project, "b");
        assert_eq!(ranked[0].name, "handler.rs");
    }

    #[test]
    fn the_active_project_lifts_a_deep_hit_over_a_shallow_one_elsewhere() {
        let dashboard = RootWalkResult::files(&["src/main.rs"]);
        let vmux = RootWalkResult::files(&["client/crates/app/vmux_mobile/src/main.rs"]);
        let roots = [
            (Path::new("/code/dashboard"), &dashboard),
            (Path::new("/code/vmux"), &vmux),
        ];

        let cold = CompletionIndex::rank(&roots, None, &[], "main.rs", 1)
            .unwrap()
            .completions;
        assert_eq!(
            cold[0].project, "dashboard",
            "the shallow path wins on its own, so the lift is what this test measures"
        );

        let lifted =
            CompletionIndex::rank(&roots, Some(Path::new("/code/vmux")), &[], "main.rs", 1)
                .unwrap()
                .completions;
        assert_eq!(lifted[0].project, "vmux");
        assert_eq!(lifted[0].name, "client/crates/app/vmux_mobile/src/main.rs");
    }

    #[test]
    fn an_exact_name_match_elsewhere_outranks_a_scattered_hit_in_the_active_project() {
        let active = RootWalkResult::files(&["docs/h/a/n/dler.md"]);
        let other = RootWalkResult::files(&["handler.rs"]);
        let ranked = CompletionIndex::rank(
            &[
                (Path::new("/code/active"), &active),
                (Path::new("/code/other"), &other),
            ],
            Some(Path::new("/code/active")),
            &[],
            "handler",
            1,
        )
        .unwrap()
        .completions;

        assert_eq!(ranked[0].project, "other");
        assert_eq!(ranked[0].name, "handler.rs");
    }

    #[test]
    fn a_recently_opened_file_outranks_an_equally_named_cold_one() {
        let first = RootWalkResult::files(&["src/main.rs"]);
        let second = RootWalkResult::files(&["src/main.rs"]);
        let roots = [
            (Path::new("/code/first"), &first),
            (Path::new("/code/second"), &second),
        ];

        let cold = CompletionIndex::rank(&roots, None, &[], "main", 1)
            .unwrap()
            .completions;
        assert_eq!(
            cold[0].project, "first",
            "the two score alike, so order alone decides until recency speaks"
        );

        let recent = Sources::extract_recent_paths(&[CommandBarRecentFile {
            url: "file:///code/second/src/main.rs".to_string(),
            title: String::new(),
        }]);
        let lifted = CompletionIndex::rank(&roots, None, &recent, "main", 1)
            .unwrap()
            .completions;
        assert_eq!(lifted[0].project, "second");
    }

    #[test]
    fn ranked_entries_carry_an_absolute_path_for_opening() {
        let walked = RootWalkResult::files(&["src/main.rs"]);
        let ranked = CompletionIndex::rank(&[(Path::new("/root"), &walked)], None, &[], "main", 1)
            .unwrap()
            .completions;
        assert_eq!(ranked[0].full_path, "/root/src/main.rs");
    }

    #[test]
    fn a_folder_named_by_the_query_outranks_every_file_inside_it() {
        let walked = RootWalkResult {
            paths: vec![
                RootWalkResult::entry("apps/mobile".to_string(), true),
                RootWalkResult::entry("apps/mobile/index.ts".to_string(), false),
                RootWalkResult::entry("apps/mobile/src/mobile_shell.ts".to_string(), false),
                RootWalkResult::entry("apps/web/mobile_breakpoints.css".to_string(), false),
            ],
            partial: false,
        };
        let ranked =
            CompletionIndex::rank(&[(Path::new("/root"), &walked)], None, &[], "mobile", 1)
                .unwrap()
                .completions;

        assert_eq!(ranked[0].name, "apps/mobile");
        assert!(ranked[0].is_dir);
        assert_eq!(ranked[0].full_path, "/root/apps/mobile");
    }

    #[test]
    fn the_walk_indexes_folders_beside_the_files_they_hold() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::create_dir_all(dir.path().join("apps/mobile")).unwrap();
        std::fs::write(dir.path().join("apps/mobile/index.ts"), "").unwrap();

        let mut named = Vec::new();
        for path in &RootWalkResult::scan(dir.path()).paths {
            let suffix = if path.is_dir { "/" } else { "" };
            named.push(format!("{}{suffix}", path.relative));
        }
        named.sort();
        assert_eq!(
            named,
            vec![
                "apps/".to_string(),
                "apps/mobile/".to_string(),
                "apps/mobile/index.ts".to_string(),
            ]
        );
    }

    #[test]
    fn the_walk_obeys_gitignore_instead_of_a_hardcoded_name_list() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "generated/\n").unwrap();
        std::fs::create_dir_all(dir.path().join("generated")).unwrap();
        std::fs::write(dir.path().join("generated/noise.rs"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("vendor")).unwrap();
        std::fs::write(dir.path().join("vendor/real_source.rs"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();

        let mut named = Vec::new();
        for path in &RootWalkResult::scan(dir.path()).paths {
            let suffix = if path.is_dir { "/" } else { "" };
            named.push(format!("{}{suffix}", path.relative));
        }
        named.sort();
        assert_eq!(
            named,
            vec![
                "src/".to_string(),
                "src/main.rs".to_string(),
                "vendor/".to_string(),
                "vendor/real_source.rs".to_string(),
            ]
        );
    }

    #[test]
    fn a_walk_that_hits_its_ceiling_drops_the_rest_and_owns_up_to_it() {
        let mut walk = RootWalkResult::default();
        walk.push_capped("apps/mobile".to_string(), true, 2);
        walk.push_capped("apps/mobile/index.ts".to_string(), false, 2);
        assert!(!walk.partial);

        walk.push_capped("apps/mobile/late.ts".to_string(), false, 2);
        assert!(walk.partial);
        assert_eq!(walk.paths.len(), 2);
    }

    #[test]
    fn canonical_root_entities_walk_and_answer_waiting_surfaces() {
        let one = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(one.path().join(".git")).unwrap();
        std::fs::create_dir_all(one.path().join("alpha")).unwrap();
        std::fs::write(one.path().join("alpha/marker_one.rs"), "").unwrap();

        let two = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(two.path().join(".git")).unwrap();
        std::fs::create_dir_all(two.path().join("beta")).unwrap();
        std::fs::write(two.path().join("beta/marker_two.rs"), "").unwrap();

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CompletionPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .init_resource::<RecordedResponses>()
            .add_observer(RecordedResponses::capture);
        app.insert_non_send(Browsers::default());
        app.world_mut()
            .spawn(CommandBarWorkspaceSnapshot::default());
        app.world_mut().spawn(CommandBarWorkSnapshot::default());
        app.world_mut().spawn(CommandBarProjectRoots {
            roots: vec![
                one.path().to_string_lossy().into_owned(),
                two.path().to_string_lossy().into_owned(),
            ],
            active: None,
        });
        let surface = app.world_mut().spawn_empty().id();
        app.world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(surface);
        app.update();

        app.world_mut().trigger(UiInput {
            webview: surface,
            payload: PathCompleteRequest {
                request_id: 7,
                query: "marker".to_string(),
            },
        });
        let mut names = Vec::new();
        for _ in 0..200 {
            app.update();
            if let Some((_, response)) = app
                .world()
                .resource::<RecordedResponses>()
                .0
                .iter()
                .rev()
                .find(|(entity, response)| {
                    *entity == surface
                        && response.request_id == 7
                        && response.completions.len() == 2
                })
            {
                names = response
                    .completions
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect();
                names.sort();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(names, ["alpha/marker_one.rs", "beta/marker_two.rs"]);
        assert!(
            app.world()
                .get::<PendingProjectCompletion>(surface)
                .is_none()
        );

        let mut roots_query = app.world_mut().query::<&mut CommandBarProjectRoots>();
        roots_query.single_mut(app.world_mut()).unwrap().roots =
            vec![one.path().to_string_lossy().into_owned()];
        app.update();
        let remaining = app
            .world_mut()
            .query::<&CompletionRoot>()
            .iter(app.world())
            .count();
        assert_eq!(remaining, 1);
    }
}
