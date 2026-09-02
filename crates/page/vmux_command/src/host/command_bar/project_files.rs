use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::Entity;
use bevy::tasks::{IoTaskPool, Task, block_on, futures_lite::future};
use ignore::WalkBuilder;

use crate::event::{CommandBarRecentFile, PathCompleteResponse, PathEntry};

const MAX_INDEXED_PATHS: usize = 400_000;
pub const MAX_RESULTS: usize = 200;
const INDEX_TTL: Duration = Duration::from_secs(60);
const ACTIVE_PROJECT_LIFT: i32 = 40;
const RECENT_FILE_LIFT: i32 = 16;

const MAX_PENDING_ASKS: usize = 8;

#[derive(Clone)]
pub struct Asked {
    pub webview: Entity,
    pub query: String,
    roots: Vec<PathBuf>,
    answered_with: u64,
}

pub struct ProjectCompletions {
    pub entries: Vec<PathEntry>,
    pub partial: bool,
    pub total: usize,
}

impl ProjectCompletions {
    pub fn listed(entries: Vec<PathEntry>, total: usize) -> Self {
        Self {
            entries,
            partial: false,
            total,
        }
    }

    pub fn response(self) -> PathCompleteResponse {
        PathCompleteResponse {
            completions: self.entries,
            truncated: self.partial,
            total: u32::try_from(self.total).unwrap_or(u32::MAX),
        }
    }
}

#[derive(bevy::prelude::Resource, Default)]
pub struct ProjectIndex {
    roots: Vec<RootIndex>,
    asked: Vec<Asked>,
    generation: u64,
}

impl ProjectIndex {
    pub fn matches(
        &mut self,
        roots: &[PathBuf],
        bias: &RankBias,
        query: &str,
        webview: Entity,
    ) -> Option<ProjectCompletions> {
        self.remember(webview, query, roots);
        self.sync(roots);
        self.rank(roots, query, bias)
    }

    fn remember(&mut self, webview: Entity, query: &str, roots: &[PathBuf]) {
        let asked = Asked {
            webview,
            query: query.to_string(),
            roots: roots.to_vec(),
            answered_with: self.generation,
        };
        for held in &mut self.asked {
            if held.webview == webview {
                *held = asked;
                return;
            }
        }
        if self.asked.len() == MAX_PENDING_ASKS {
            self.asked.remove(0);
        }
        self.asked.push(asked);
    }

    pub fn warm(&mut self, roots: &[PathBuf]) {
        self.sync(roots);
    }

    pub fn pending(&self) -> Vec<Asked> {
        self.asked.clone()
    }

    pub fn settled_for(
        &mut self,
        webview: Entity,
        roots: &[PathBuf],
        bias: &RankBias,
    ) -> Option<ProjectCompletions> {
        let at = self.asked.iter().position(|ask| ask.webview == webview)?;
        let query = self.asked[at].query.clone();
        self.sync(roots);
        if self.asked[at].answered_with == self.generation {
            self.forget_once_complete(webview);
            return None;
        }
        self.asked[at].answered_with = self.generation;
        let ranked = self.rank(roots, &query, bias);
        self.forget_once_complete(webview);
        ranked
    }

    fn forget_once_complete(&mut self, webview: Entity) {
        for index in &self.roots {
            if index.walking() {
                return;
            }
        }
        self.forget(webview);
    }

    pub fn forget(&mut self, webview: Entity) {
        self.asked.retain(|ask| ask.webview != webview);
    }

    // The index now outlives the ask that built it, so the roots the caller
    // asked about are what bound the answer rather than everything held.
    fn rank(&self, roots: &[PathBuf], query: &str, bias: &RankBias) -> Option<ProjectCompletions> {
        let mut ready = Vec::new();
        let mut partial = false;
        for index in &self.roots {
            if !roots.contains(&index.root) {
                continue;
            }
            let Some(walk) = index.walked() else {
                continue;
            };
            partial |= walk.partial;
            ready.push((index.root.as_path(), walk.paths.as_slice()));
        }
        if ready.is_empty() {
            return None;
        }
        let ranked = FuzzyRank::across(&ready, bias, query);
        Some(ProjectCompletions {
            entries: ranked.entries,
            partial,
            total: ranked.total,
        })
    }

    // Every root some surface is still waiting on, not just this caller's. One
    // command bar asking about its project must not evict the half-built index
    // another is waiting for.
    fn wanted(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut wanted = roots.to_vec();
        for ask in &self.asked {
            for root in &ask.roots {
                if !wanted.contains(root) {
                    wanted.push(root.clone());
                }
            }
        }
        wanted
    }

    fn sync(&mut self, roots: &[PathBuf]) {
        let roots = &self.wanted(roots);
        let held = self.roots.len();
        self.roots.retain(|index| roots.contains(&index.root));
        if self.roots.len() != held {
            self.generation += 1;
        }
        for root in roots {
            let Some(at) = self.roots.iter().position(|index| &index.root == root) else {
                self.roots.push(RootIndex::start(root));
                continue;
            };
            if self.roots[at].advance() {
                self.generation += 1;
            }
        }
    }
}

struct RootIndex {
    root: PathBuf,
    walk: Option<ProjectWalk>,
    built_at: Option<Instant>,
    walking: Option<Task<ProjectWalk>>,
}

impl RootIndex {
    fn start(root: &Path) -> Self {
        let mut started = Self {
            root: root.to_path_buf(),
            walk: None,
            built_at: None,
            walking: None,
        };
        started.rewalk();
        started
    }

    fn rewalk(&mut self) {
        let walked = self.root.clone();
        self.walking = Some(IoTaskPool::get().spawn(async move { ProjectWalk::of(&walked) }));
    }

    fn walking(&self) -> bool {
        self.walking.is_some()
    }

    fn walked(&self) -> Option<&ProjectWalk> {
        self.walk.as_ref()
    }

    fn advance(&mut self) -> bool {
        if let Some(task) = &mut self.walking {
            let Some(walk) = block_on(future::poll_once(task)) else {
                return false;
            };
            self.walk = Some(walk);
            self.built_at = Some(Instant::now());
            self.walking = None;
            return true;
        }
        if self.built_at.is_some_and(|at| at.elapsed() > INDEX_TTL) {
            self.rewalk();
        }
        false
    }
}

struct WalkedPath {
    relative: String,
    folded: Option<String>,
    held: PathMask,
    is_dir: bool,
}

impl WalkedPath {
    fn file(relative: String) -> Self {
        Self::of(relative, false)
    }

    fn directory(relative: String) -> Self {
        Self::of(relative, true)
    }

    fn of(relative: String, is_dir: bool) -> Self {
        let lowered = relative.to_lowercase();
        let held = PathMask::of(&lowered);
        let folded = if lowered == relative {
            None
        } else {
            Some(lowered)
        };
        Self {
            relative,
            folded,
            held,
            is_dir,
        }
    }

    fn folded(&self) -> &str {
        match &self.folded {
            Some(folded) => folded,
            None => &self.relative,
        }
    }
}

struct ProjectWalk {
    paths: Vec<WalkedPath>,
    ceiling: usize,
    partial: bool,
}

impl ProjectWalk {
    fn holding(ceiling: usize) -> Self {
        Self {
            paths: Vec::new(),
            ceiling,
            partial: false,
        }
    }

    fn of(root: &Path) -> Self {
        let mut walked = Self::holding(MAX_INDEXED_PATHS);
        let walk = WalkBuilder::new(root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .build();
        for entry in walk {
            if !walked.wants_more() {
                walked.partial = true;
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if entry.depth() == 0 {
                continue;
            }
            let Some(kind) = entry.file_type() else {
                continue;
            };
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            walked.push(relative.to_string_lossy().into_owned(), kind.is_dir());
        }
        walked
    }

    fn wants_more(&self) -> bool {
        self.paths.len() < self.ceiling
    }

    fn push(&mut self, relative: String, is_dir: bool) {
        if !self.wants_more() {
            self.partial = true;
            return;
        }
        if is_dir {
            self.paths.push(WalkedPath::directory(relative));
            return;
        }
        self.paths.push(WalkedPath::file(relative));
    }
}

#[derive(Default)]
pub struct RankBias {
    active: Option<PathBuf>,
    recent: Vec<String>,
}

impl RankBias {
    pub fn of(active: Option<&str>, recent: &[CommandBarRecentFile]) -> Self {
        let mut opened = Vec::with_capacity(recent.len());
        for file in recent {
            let Some(path) = file.url.strip_prefix("file://") else {
                continue;
            };
            opened.push(path.to_string());
        }
        let active = match active {
            Some(active) if !active.trim().is_empty() => Some(PathBuf::from(active.trim())),
            _ => None,
        };
        Self {
            active,
            recent: opened,
        }
    }

    fn favours(&self, root: &Path) -> bool {
        self.active.as_deref() == Some(root)
    }

    fn lift(&self, root: &Path) -> i32 {
        match self.favours(root) {
            true => ACTIVE_PROJECT_LIFT,
            false => 0,
        }
    }

    fn opened(&self, root: &str, relative: &str) -> bool {
        for path in &self.recent {
            let Some(held) = path.strip_prefix(root) else {
                continue;
            };
            if held.strip_prefix('/').unwrap_or(held) == relative {
                return true;
            }
        }
        false
    }
}

pub struct Ranked {
    pub entries: Vec<PathEntry>,
    pub total: usize,
}

struct FuzzyRank;

impl FuzzyRank {
    fn across(roots: &[(&Path, &[WalkedPath])], bias: &RankBias, query: &str) -> Ranked {
        let wanted = FuzzyQuery::of(query);
        let mut best = TopMatches::holding(MAX_RESULTS);
        for favoured in [true, false] {
            for (root, paths) in roots {
                if bias.favours(root) != favoured {
                    continue;
                }
                let lift = bias.lift(root);
                for path in paths.iter() {
                    let Some(score) = wanted.score(path) else {
                        continue;
                    };
                    best.offer(Match {
                        score: score + lift,
                        root,
                        path,
                    });
                }
            }
        }
        best.ranked(bias)
    }
}

struct TopMatches<'a> {
    keeping: usize,
    seen: usize,
    best: Vec<Match<'a>>,
}

impl<'a> TopMatches<'a> {
    fn holding(keeping: usize) -> Self {
        Self {
            keeping,
            seen: 0,
            best: Vec::with_capacity(keeping + 1),
        }
    }

    fn offer(&mut self, found: Match<'a>) {
        self.seen += 1;
        if self.best.len() == self.keeping {
            let Some(worst) = self.best.last() else {
                return;
            };
            if found.ranks_after(worst) {
                return;
            }
        }
        let at = self.best.partition_point(|held| found.ranks_after(held));
        self.best.insert(at, found);
        self.best.truncate(self.keeping);
    }

    fn resettle(&mut self, bias: &RankBias) {
        if bias.recent.is_empty() {
            return;
        }
        let mut lifted = false;
        for found in &mut self.best {
            let Some(root) = found.root.to_str() else {
                continue;
            };
            if !bias.opened(root, &found.path.relative) {
                continue;
            }
            found.score += RECENT_FILE_LIFT;
            lifted = true;
        }
        if !lifted {
            return;
        }
        self.best.sort_by(Match::order);
    }

    fn ranked(mut self, bias: &RankBias) -> Ranked {
        self.resettle(bias);
        let total = self.seen;
        let mut entries = Vec::with_capacity(self.best.len());
        for found in self.best {
            entries.push(PathEntry {
                name: found.path.relative.clone(),
                is_dir: found.path.is_dir,
                full_path: found
                    .root
                    .join(&found.path.relative)
                    .to_string_lossy()
                    .into_owned(),
                project: ProjectLabel::of(found.root),
            });
        }
        Ranked { entries, total }
    }
}

struct Match<'a> {
    score: i32,
    root: &'a Path,
    path: &'a WalkedPath,
}

impl Match<'_> {
    fn order(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then(self.path.relative.len().cmp(&other.path.relative.len()))
            .then(self.path.relative.cmp(&other.path.relative))
    }

    fn ranks_after(&self, other: &Self) -> bool {
        self.order(other).is_ge()
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

#[derive(Clone, Copy, Default)]
struct PathMask(u64);

impl PathMask {
    fn of(folded: &str) -> Self {
        let mut held = 0u64;
        for c in folded.chars() {
            held |= Self::bit(c);
        }
        Self(held)
    }

    fn bit(c: char) -> u64 {
        if c.is_ascii_lowercase() {
            return 1 << (c as u8 - b'a');
        }
        if c.is_ascii_digit() {
            return 1 << (26 + c as u8 - b'0');
        }
        0
    }

    fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn covers(self, needed: Self) -> bool {
        self.0 & needed.0 == needed.0
    }
}

struct FuzzyQuery {
    terms: Vec<String>,
    needed: PathMask,
}

impl FuzzyQuery {
    fn of(query: &str) -> Self {
        let mut terms = Vec::new();
        let mut needed = PathMask::default();
        for term in query.split_whitespace() {
            let term = term.to_lowercase();
            needed = needed.with(PathMask::of(&term));
            terms.push(term);
        }
        Self { terms, needed }
    }

    fn score(&self, path: &WalkedPath) -> Option<i32> {
        if !path.held.covers(self.needed) {
            return None;
        }
        let folded = path.folded();
        let mut total = 0;
        for term in &self.terms {
            total += FuzzyScore::of(folded, term)?;
        }
        Some(total)
    }
}

struct FuzzyScore;

impl FuzzyScore {
    fn of(lowered: &str, needle: &str) -> Option<i32> {
        if needle.is_empty() {
            return Some(0);
        }
        let base = Self::walk(lowered, needle)?;
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

    fn walk(haystack: &str, needle: &str) -> Option<i32> {
        let mut remaining = needle.chars();
        let Some(mut wanted) = remaining.next() else {
            return Some(0);
        };
        let mut score = 0i32;
        let mut previous_end: Option<usize> = None;
        let mut previous_char: Option<char> = None;
        for (at, c) in haystack.chars().enumerate() {
            if c == wanted {
                score += match previous_end {
                    Some(previous) if previous + 1 == at => 12,
                    Some(previous) => -((at - previous - 1).min(8usize) as i32),
                    None => 0,
                };
                if at == 0 {
                    score += 10;
                } else if previous_char.is_some_and(Self::is_boundary) {
                    score += 8;
                }
                previous_end = Some(at);
                let Some(next) = remaining.next() else {
                    return Some(score);
                };
                wanted = next;
            }
            previous_char = Some(c);
        }
        None
    }

    fn is_boundary(c: char) -> bool {
        matches!(c, '/' | '_' | '-' | '.' | ' ')
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

        fn indexed(&self) -> Vec<String> {
            let mut named = Vec::new();
            for path in &ProjectWalk::of(self.dir.path()).paths {
                let suffix = if path.is_dir { "/" } else { "" };
                named.push(format!("{}{suffix}", path.relative));
            }
            named.sort();
            named
        }
    }

    impl FuzzyRank {
        fn listed(
            roots: &[(&Path, &[WalkedPath])],
            bias: &RankBias,
            query: &str,
        ) -> Vec<PathEntry> {
            Self::across(roots, bias, query).entries
        }
    }

    fn files(paths: &[&str]) -> Vec<WalkedPath> {
        let mut walked = Vec::new();
        for path in paths {
            walked.push(WalkedPath::file(path.to_string()));
        }
        walked
    }

    struct Synthetic;

    impl Synthetic {
        fn paths(count: usize) -> Vec<WalkedPath> {
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
            let mut walked = Vec::with_capacity(count);
            let mut at = 0usize;
            while walked.len() < count {
                let area = areas[at % areas.len()];
                let package = at % 900;
                let module = (at / 7) % 60;
                let kind = kinds[(at / 3) % kinds.len()];
                let extension = extensions[(at / 11) % extensions.len()];
                walked.push(WalkedPath::file(format!(
                    "{area}/pkg_{package:03}/src/module_{module:02}/{kind}Impl{at}.{extension}"
                )));
                at += 1;
            }
            walked
        }
    }

    #[test]
    fn the_prefilter_never_rejects_a_path_the_scorer_would_have_matched() {
        let paths = Synthetic::paths(20_000);
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
            let needed = PathMask::of(&term);
            for path in &paths {
                if FuzzyScore::of(path.folded(), &term).is_none() {
                    continue;
                }
                assert!(
                    path.held.covers(needed),
                    "{term:?} matched {:?} but the prefilter rejected it",
                    path.relative
                );
            }
        }
    }

    #[test]
    fn a_monorepo_sized_index_ranks_a_late_best_hit_over_the_matches_before_it() {
        let mut paths = Synthetic::paths(300_000);
        paths.push(WalkedPath::file("handler.rs".to_string()));
        let ranked = FuzzyRank::listed(
            &[(Path::new("/code/monorepo"), &paths)],
            &RankBias::default(),
            "handler",
        );

        assert!(
            ranked.len() > 1,
            "nothing competed with the late hit, so its placement proves nothing"
        );
        assert_eq!(ranked[0].name, "handler.rs");
    }

    #[test]
    fn the_ranker_reports_how_many_matched_even_though_it_ships_only_the_best() {
        let mut paths = Vec::new();
        for at in 0..(MAX_RESULTS + 25) {
            paths.push(WalkedPath::file(format!("src/dir_{at:03}/main.rs")));
        }
        paths.push(WalkedPath::file("docs/readme.md".to_string()));

        let ranked = FuzzyRank::across(
            &[(Path::new("/code/monorepo"), &paths)],
            &RankBias::default(),
            "main.rs",
        );

        assert_eq!(ranked.entries.len(), MAX_RESULTS);
        assert_eq!(ranked.total, MAX_RESULTS + 25);
    }

    #[test]
    fn a_basename_hit_outranks_a_scattered_path_hit() {
        let paths = files(&["crates/vmux_core/src/handler.rs", "docs/h/a/n/dler.md"]);
        let ranked = FuzzyRank::listed(
            &[(Path::new("/root"), &paths)],
            &RankBias::default(),
            "handler",
        );
        assert_eq!(ranked[0].name, "crates/vmux_core/src/handler.rs");
    }

    #[test]
    fn a_path_fragment_query_matches_across_separators() {
        let paths = files(&[
            "crates/page/vmux_command/src/page.rs",
            "crates/vmux_ui/src/page.rs",
        ]);
        let ranked = FuzzyRank::listed(
            &[(Path::new("/root"), &paths)],
            &RankBias::default(),
            "command/page",
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "crates/page/vmux_command/src/page.rs");
    }

    #[test]
    fn a_query_whose_letters_are_out_of_order_does_not_match() {
        assert!(FuzzyScore::of("src/handler.rs", "rendlah").is_none());
    }

    #[test]
    fn every_space_separated_term_must_match_the_same_path() {
        let paths = files(&[
            "crates/app/vmux_mobile/src/main.rs",
            "crates/app/vmux_desktop/src/main.rs",
        ]);
        let ranked = FuzzyRank::listed(
            &[(Path::new("/root"), &paths)],
            &RankBias::default(),
            "mobile main.rs",
        );

        let named: Vec<&str> = ranked.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(named, ["crates/app/vmux_mobile/src/main.rs"]);
    }

    #[test]
    fn a_hit_in_every_project_is_ranked_together_and_named_by_its_project() {
        let dashboard = files(&["src/main.rs"]);
        let vmux = files(&["src/main.rs"]);
        let ranked = FuzzyRank::listed(
            &[
                (Path::new("/code/dashboard"), &dashboard),
                (Path::new("/code/vmux"), &vmux),
            ],
            &RankBias::default(),
            "main",
        );

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
        let first = files(&["src/unrelated_handler_helper.rs"]);
        let second = files(&["handler.rs"]);
        let ranked = FuzzyRank::listed(
            &[(Path::new("/a"), &first), (Path::new("/b"), &second)],
            &RankBias::default(),
            "handler",
        );

        assert_eq!(ranked[0].project, "b");
        assert_eq!(ranked[0].name, "handler.rs");
    }

    impl RankBias {
        fn after_visiting(urls: &[&str]) -> Self {
            let mut recent = Vec::new();
            for url in urls {
                recent.push(CommandBarRecentFile {
                    url: url.to_string(),
                    title: String::new(),
                });
            }
            Self::of(None, &recent)
        }
    }

    #[test]
    fn the_active_project_lifts_a_deep_hit_over_a_shallow_one_elsewhere() {
        let dashboard = files(&["src/main.rs"]);
        let vmux = files(&["client/crates/app/vmux_mobile/src/main.rs"]);
        let roots = [
            (Path::new("/code/dashboard"), dashboard.as_slice()),
            (Path::new("/code/vmux"), vmux.as_slice()),
        ];

        let cold = FuzzyRank::listed(&roots, &RankBias::default(), "main.rs");
        assert_eq!(
            cold[0].project, "dashboard",
            "the shallow path wins on its own, so the lift is what this test measures"
        );

        let lifted = FuzzyRank::listed(&roots, &RankBias::of(Some("/code/vmux"), &[]), "main.rs");
        assert_eq!(lifted[0].project, "vmux");
        assert_eq!(lifted[0].name, "client/crates/app/vmux_mobile/src/main.rs");
    }

    #[test]
    fn an_exact_name_match_elsewhere_outranks_a_scattered_hit_in_the_active_project() {
        let active = files(&["docs/h/a/n/dler.md"]);
        let other = files(&["handler.rs"]);
        let ranked = FuzzyRank::listed(
            &[
                (Path::new("/code/active"), active.as_slice()),
                (Path::new("/code/other"), other.as_slice()),
            ],
            &RankBias::of(Some("/code/active"), &[]),
            "handler",
        );

        assert_eq!(ranked[0].project, "other");
        assert_eq!(ranked[0].name, "handler.rs");
    }

    #[test]
    fn a_recently_opened_file_outranks_an_equally_named_cold_one() {
        let first = files(&["src/main.rs"]);
        let second = files(&["src/main.rs"]);
        let roots = [
            (Path::new("/code/first"), first.as_slice()),
            (Path::new("/code/second"), second.as_slice()),
        ];

        let cold = FuzzyRank::listed(&roots, &RankBias::default(), "main");
        assert_eq!(
            cold[0].project, "first",
            "the two score alike, so order alone decides until recency speaks"
        );

        let opened = RankBias::after_visiting(&["file:///code/second/src/main.rs"]);
        let lifted = FuzzyRank::listed(&roots, &opened, "main");
        assert_eq!(lifted[0].project, "second");
    }

    #[test]
    fn ranked_entries_carry_an_absolute_path_for_opening() {
        let paths = files(&["src/main.rs"]);
        let ranked = FuzzyRank::listed(
            &[(Path::new("/root"), &paths)],
            &RankBias::default(),
            "main",
        );
        assert_eq!(ranked[0].full_path, "/root/src/main.rs");
    }

    #[test]
    fn a_folder_named_by_the_query_outranks_every_file_inside_it() {
        let paths = vec![
            WalkedPath::directory("apps/mobile".to_string()),
            WalkedPath::file("apps/mobile/index.ts".to_string()),
            WalkedPath::file("apps/mobile/src/mobile_shell.ts".to_string()),
            WalkedPath::file("apps/web/mobile_breakpoints.css".to_string()),
        ];
        let ranked = FuzzyRank::listed(
            &[(Path::new("/root"), &paths)],
            &RankBias::default(),
            "mobile",
        );

        assert_eq!(ranked[0].name, "apps/mobile");
        assert!(ranked[0].is_dir);
        assert_eq!(ranked[0].full_path, "/root/apps/mobile");
    }

    #[test]
    fn the_walk_indexes_folders_beside_the_files_they_hold() {
        let project = Project::new();
        project.write("apps/mobile/index.ts", "");

        assert_eq!(
            project.indexed(),
            vec![
                "apps/".to_string(),
                "apps/mobile/".to_string(),
                "apps/mobile/index.ts".to_string(),
            ]
        );
    }

    #[test]
    fn the_walk_obeys_gitignore_instead_of_a_hardcoded_name_list() {
        let project = Project::new();
        project
            .write(".gitignore", "generated/\n")
            .write("generated/noise.rs", "")
            .write("vendor/real_source.rs", "")
            .write("src/main.rs", "");

        assert_eq!(
            project.indexed(),
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
        let mut walk = ProjectWalk::holding(2);

        walk.push("apps/mobile".to_string(), true);
        walk.push("apps/mobile/index.ts".to_string(), false);
        assert!(!walk.partial);

        walk.push("apps/mobile/late.ts".to_string(), false);
        assert!(walk.partial);
        assert_eq!(walk.paths.len(), 2);
    }

    impl ProjectIndex {
        fn answer(&mut self, webview: Entity, roots: &[PathBuf]) -> ProjectCompletions {
            let bias = RankBias::after_visiting(&[]);
            for _ in 0..500 {
                if let Some(answered) = self.settled_for(webview, roots, &bias) {
                    return answered;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the index never settled for {webview}");
        }
    }

    #[test]
    fn two_surfaces_waiting_on_a_cold_index_are_each_answered_from_their_own_project() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let one = Project::new();
        one.write("alpha/marker_one.rs", "");
        let two = Project::new();
        two.write("beta/marker_two.rs", "");
        let roots_one = vec![one.dir.path().to_path_buf()];
        let roots_two = vec![two.dir.path().to_path_buf()];

        let mut world = bevy::prelude::World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();

        let mut index = ProjectIndex::default();
        let bias = RankBias::after_visiting(&[]);
        index.matches(&roots_one, &bias, "marker", first);
        index.matches(&roots_two, &bias, "marker", second);

        let answered_one = index.answer(first, &roots_one);
        let answered_two = index.answer(second, &roots_two);

        let named = |completions: &ProjectCompletions| {
            let mut names = Vec::new();
            for entry in &completions.entries {
                names.push(entry.name.clone());
            }
            names.sort();
            names
        };
        assert_eq!(
            named(&answered_one),
            vec!["alpha/marker_one.rs".to_string()],
            "the first surface must still be answered after a second one asked"
        );
        assert_eq!(
            named(&answered_two),
            vec!["beta/marker_two.rs".to_string()],
            "neither surface may see the other project"
        );
    }
}
