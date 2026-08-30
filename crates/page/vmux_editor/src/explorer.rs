#![allow(non_snake_case)]

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use crate::page::use_ime_guard;
use crate::page_model::merge_tree_motion_rows;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::tree_row::{
    SIDEBAR_STICKY_SURFACE, SIDEBAR_TREE_LIST_GROUP, SIDEBAR_TREE_ROW_BASE, TreeRowAccent,
};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{send, use_listener};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;

const TREE_MOTION_MS: u32 = 170;
const NOTICE_MS: u32 = 2400;
const TREE_ROW_HEIGHT: f64 = 22.0;
const STICKY_DEPTH_MAX: usize = 5;

#[derive(Clone, PartialEq)]
struct MotionRow {
    row: TreeRow,
    visible: bool,
}

#[derive(Clone, PartialEq)]
struct TreeMenu {
    path: String,
    name: String,
    is_dir: bool,
    is_root: bool,
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptKind {
    CreateFile,
    CreateDir,
    Rename,
    Delete,
}

#[derive(Clone, PartialEq)]
struct TreePrompt {
    kind: PromptKind,
    path: String,
    name: String,
}

#[derive(Clone, PartialEq)]
struct ExplorerNotice {
    ok: bool,
    message: String,
}

fn open_file(path: String) {
    let _ = send(&FileOpenEvent { path });
}

fn toggle_dir(path: String) {
    let _ = send(&ExplorerTreeToggle { path });
}

fn prefetch_dir(path: String) {
    let _ = send(&ExplorerTreePrefetch { path });
}

fn refresh_dir(path: String) {
    let _ = send(&ExplorerTreeRefresh { path });
}

fn collapse_all_dirs() {
    let _ = send(&ExplorerCollapseAll);
}

fn close_editor(path: String) {
    let _ = send(&ExplorerCloseEditor { path });
}

fn goto_line(line: u32) {
    let _ = send(&ExplorerGoto {
        path: String::new(),
        line,
    });
}

pub const SEARCH_INPUT_ID: &str = "explorer-search-input";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SidebarView {
    #[default]
    Explorer,
    Search,
}

impl SidebarView {
    fn is_search(self) -> bool {
        self == Self::Search
    }

    fn other(self) -> Self {
        match self {
            Self::Explorer => Self::Search,
            Self::Search => Self::Explorer,
        }
    }

    fn title(self) -> String {
        match self {
            Self::Explorer => translate("editor-explorer"),
            Self::Search => translate("editor-search"),
        }
    }

    fn switch_glyph(self) -> TitleGlyph {
        match self {
            Self::Explorer => TitleGlyph::Search,
            Self::Search => TitleGlyph::Files,
        }
    }

    fn switch_label(self) -> String {
        match self {
            Self::Explorer => translate("editor-show-search"),
            Self::Search => translate("editor-show-explorer"),
        }
    }
}

struct SearchRowKey;

impl SearchRowKey {
    fn of(path: &str, hit: &ExplorerSearchMatch) -> String {
        format!("{path}:{}:{}", hit.line, hit.col)
    }
}

struct SearchFileName;

impl SearchFileName {
    fn of(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }
}

struct SearchFileDir;

impl SearchFileDir {
    fn of(root: &str, path: &str) -> String {
        let relative = Path::new(path)
            .strip_prefix(root)
            .unwrap_or(Path::new(path));
        let Some(parent) = relative.parent() else {
            return String::new();
        };
        parent.to_string_lossy().into_owned()
    }
}

struct SearchCount;

impl SearchCount {
    fn of(file: &ExplorerSearchFile) -> String {
        let shown = file.matches.len();
        match file.capped {
            true => format!("{shown}+"),
            false => format!("{shown}"),
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
struct SearchSummary {
    total: usize,
    files: usize,
    floor: bool,
}

impl SearchSummary {
    fn of(results: &ExplorerSearchEvent) -> Self {
        let mut total = 0usize;
        let mut floor = results.capped;
        for file in &results.files {
            total += file.matches.len();
            if file.capped {
                floor = true;
            }
        }
        Self {
            total,
            files: results.files.len(),
            floor,
        }
    }

    fn text(&self) -> String {
        let id = match self.floor {
            true => "editor-search-summary-capped",
            false => "editor-search-summary",
        };
        translate_with(
            id,
            &[
                ("results", TranslationValue::Number(self.total as i64)),
                ("files", TranslationValue::Number(self.files as i64)),
            ],
        )
    }
}

struct PreviewSpan {
    before: String,
    hit: String,
    after: String,
}

impl PreviewSpan {
    fn of(preview: &str, col: u32, end_col: u32) -> Self {
        let start = Self::char_at(preview, col);
        let end = Self::char_at(preview, end_col).max(start);
        let mut before = String::new();
        let mut hit = String::new();
        let mut after = String::new();
        for (index, character) in preview.chars().enumerate() {
            if index < start {
                before.push(character);
            } else if index < end {
                hit.push(character);
            } else {
                after.push(character);
            }
        }
        Self { before, hit, after }
    }

    fn char_at(preview: &str, utf16: u32) -> usize {
        let mut seen = 0u32;
        for (index, character) in preview.chars().enumerate() {
            if seen >= utf16 {
                return index;
            }
            seen += character.len_utf16() as u32;
        }
        preview.chars().count()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct SearchState {
    query: Signal<String>,
    regex: Signal<bool>,
    case_sensitive: Signal<bool>,
    whole_word: Signal<bool>,
    results: Signal<Option<ExplorerSearchEvent>>,
    collapsed: Signal<HashSet<String>>,
    opened: Signal<String>,
}

impl SearchState {
    fn run(self) {
        let text = self.query.peek().clone();
        if text.trim().is_empty() {
            self.clear();
            return;
        }
        let _ = send(&ExplorerSearchRequest {
            query: text,
            regex: (self.regex)(),
            case_sensitive: (self.case_sensitive)(),
            whole_word: (self.whole_word)(),
        });
    }

    fn clear(self) {
        let mut results = self.results;
        let mut collapsed = self.collapsed;
        let mut opened = self.opened;
        results.set(None);
        collapsed.set(HashSet::new());
        opened.set(String::new());
    }

    fn arrived(self, event: ExplorerSearchEvent) {
        let mut results = self.results;
        let mut collapsed = self.collapsed;
        collapsed.set(HashSet::new());
        results.set(Some(event));
    }

    fn is_collapsed(self, path: &str) -> bool {
        self.collapsed.read().contains(path)
    }

    fn toggle_group(self, path: &str) {
        let mut collapsed = self.collapsed;
        let mut next = collapsed.peek().clone();
        if !next.remove(path) {
            next.insert(path.to_string());
        }
        collapsed.set(next);
    }

    fn collapse_all(self) {
        let mut collapsed = self.collapsed;
        let mut next = HashSet::new();
        if let Some(results) = self.results.peek().as_ref() {
            for file in &results.files {
                next.insert(file.path.clone());
            }
        }
        collapsed.set(next);
    }

    fn open(self, path: &str, hit: &ExplorerSearchMatch) {
        let mut opened = self.opened;
        opened.set(SearchRowKey::of(path, hit));
        let _ = send(&ExplorerSearchOpen {
            path: path.to_string(),
            line: hit.line,
            col: hit.col,
            end_col: hit.end_col,
        });
    }

    fn showing(self, path: &str) {
        let mut opened = self.opened;
        if opened.peek().starts_with(&format!("{path}:")) {
            return;
        }
        opened.set(String::new());
    }

    fn keys(self) -> Vec<String> {
        let mut keys = Vec::new();
        let results = self.results.read();
        let Some(results) = results.as_ref() else {
            return keys;
        };
        let collapsed = self.collapsed.read();
        for file in &results.files {
            if collapsed.contains(&file.path) {
                continue;
            }
            for hit in &file.matches {
                keys.push(SearchRowKey::of(&file.path, hit));
            }
        }
        keys
    }
}

fn create_entry(parent: String, name: String, is_dir: bool) {
    let _ = send(&ExplorerCreate {
        parent,
        name,
        is_dir,
    });
}

fn rename_entry(path: String, name: String) {
    let _ = send(&ExplorerRename { path, name });
}

fn delete_entry(path: String) {
    let _ = send(&ExplorerDelete { path });
}

fn tree_row_id(path: &str) -> String {
    let hash = path
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("explorer-row-{hash:016x}")
}

fn schedule_tree_focus(path: String, mut generation: Signal<u32>, reveal: ExplorerReveal) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    spawn(async move {
        sleep_ms(TREE_MOTION_MS + 20).await;
        if generation() != id {
            return;
        }
        let row = tree_row_id(&path);
        ScrollIntoView::nearest(&row);
        if reveal == ExplorerReveal::Requested {
            FocusClaim::new(row).request();
        }
    });
}

fn cancel_tree_focus(mut generation: Signal<u32>) {
    let id = generation.peek().wrapping_add(1);
    generation.set(id);
}

#[derive(Clone, Copy)]
struct TreeRows {
    rows: Signal<Vec<MotionRow>>,
    generation: Signal<u32>,
}

impl TreeRows {
    fn reconcile(self, next: Vec<TreeRow>) {
        let mut rows = self.rows;
        let generation = self.generation;
        let id = self.claim();
        let next_paths: HashSet<String> = next.iter().map(|row| row.path.clone()).collect();
        let current = rows
            .read()
            .iter()
            .filter(|motion| motion.visible)
            .map(|motion| motion.row.clone())
            .collect::<Vec<_>>();
        if current.is_empty() {
            rows.set(
                next.into_iter()
                    .map(|row| MotionRow { row, visible: true })
                    .collect(),
            );
            return;
        }
        let merged = merge_tree_motion_rows(&current, &next)
            .into_iter()
            .map(|(row, visible)| MotionRow { row, visible })
            .collect();
        rows.set(merged);
        spawn(async move {
            sleep_ms(0).await;
            if generation() != id {
                return;
            }
            let mut opening = rows.read().clone();
            for item in &mut opening {
                if next_paths.contains(&item.row.path) {
                    item.visible = true;
                }
            }
            rows.set(opening);

            sleep_ms(TREE_MOTION_MS).await;
            if generation() != id {
                return;
            }
            rows.set(
                next.into_iter()
                    .map(|row| MotionRow { row, visible: true })
                    .collect(),
            );
        });
    }

    fn collapse(self, path: &str) {
        self.claim();
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let mut kept = Vec::new();
        for motion in self.rows.read().iter() {
            if motion.row.path.starts_with(&prefix) {
                continue;
            }
            let mut motion = motion.clone();
            if motion.row.path == path {
                motion.row.expanded = false;
            }
            kept.push(motion);
        }
        let mut rows = self.rows;
        rows.set(kept);
    }

    fn collapse_all(self) {
        self.claim();
        let mut kept = Vec::new();
        for motion in self.rows.read().iter() {
            if motion.row.depth > 0 {
                continue;
            }
            let mut motion = motion.clone();
            motion.row.expanded = false;
            kept.push(motion);
        }
        let mut rows = self.rows;
        rows.set(kept);
        collapse_all_dirs();
    }

    fn refresh(self, root: String) {
        refresh_dir(root);
        for motion in self.rows.peek().iter() {
            if motion.row.is_dir && motion.row.expanded {
                refresh_dir(motion.row.path.clone());
            }
        }
    }

    fn create_parent(self, focus: &str, root: &str) -> String {
        CreateTarget::of(self.rows.peek().as_slice(), focus, root)
    }

    fn expand(self, path: &str) {
        self.claim();
        let mut opened = self.rows.read().clone();
        for motion in &mut opened {
            if motion.row.path == path {
                motion.row.expanded = true;
                motion.row.loading = true;
            }
        }
        let mut rows = self.rows;
        rows.set(opened);
    }

    fn claim(self) -> u32 {
        let mut generation = self.generation;
        let id = generation().wrapping_add(1);
        generation.set(id);
        id
    }

    fn paths(self) -> Vec<String> {
        let mut paths = Vec::new();
        for motion in self.rows.peek().iter() {
            paths.push(motion.row.path.clone());
        }
        paths
    }
}

struct AncestorChain;

impl AncestorChain {
    fn of(depths: &[u16], top: usize) -> Vec<usize> {
        let Some(&start) = depths.get(top) else {
            return Vec::new();
        };
        let mut lowest = start;
        let mut chain = Vec::new();
        for index in (0..top).rev() {
            let depth = depths[index];
            if depth >= lowest {
                continue;
            }
            lowest = depth;
            chain.push(index);
            if depth == 0 {
                break;
            }
        }
        chain.reverse();
        chain.truncate(STICKY_DEPTH_MAX);
        chain
    }
}

struct CreateTarget;

impl CreateTarget {
    fn of(rows: &[MotionRow], focus: &str, root: &str) -> String {
        if focus.is_empty() {
            return root.to_string();
        }
        for motion in rows {
            if motion.row.path != focus {
                continue;
            }
            if motion.row.is_dir {
                return focus.to_string();
            }
            let Some(parent) = Path::new(focus).parent() else {
                return root.to_string();
            };
            return parent.to_string_lossy().into_owned();
        }
        root.to_string()
    }
}

struct OutlineKey;

impl OutlineKey {
    fn of(row: &OutlineRow) -> String {
        format!("{}-{}", row.line, row.name)
    }
}

struct CaretSymbol;

impl CaretSymbol {
    fn of(rows: &[OutlineRow], line: u32) -> String {
        let mut innermost = String::new();
        for row in rows {
            if row.contains(line) {
                innermost = OutlineKey::of(row);
            }
        }
        innermost
    }
}

#[derive(Clone, Copy)]
struct StickySection {
    scroller: Signal<Option<Rc<MountedData>>>,
    list: Signal<Option<Rc<MountedData>>>,
    top: Signal<usize>,
    measuring: Signal<bool>,
    pending: Signal<bool>,
}

impl StickySection {
    fn mounted(self, element: Rc<MountedData>) {
        let mut list = self.list;
        list.set(Some(element));
        self.measure();
    }

    fn measure(self) {
        let mut pending = self.pending;
        if *self.measuring.peek() {
            pending.set(true);
            return;
        }
        let scroller = self.scroller.peek().clone();
        let list = self.list.peek().clone();
        let (Some(scroller), Some(list)) = (scroller, list) else {
            return;
        };
        let mut measuring = self.measuring;
        let mut top = self.top;
        measuring.set(true);
        spawn(async move {
            let outer = scroller.get_client_rect().await;
            let inner = list.get_client_rect().await;
            measuring.set(false);
            if let (Ok(outer), Ok(inner)) = (outer, inner) {
                let hidden = outer.origin.y - inner.origin.y;
                let index = (hidden / TREE_ROW_HEIGHT).floor().max(0.0) as usize;
                if *top.peek() != index {
                    top.set(index);
                }
            }
            if *pending.peek() {
                pending.set(false);
                self.measure();
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TreeFocus {
    key: Signal<String>,
}

impl TreeFocus {
    fn at(self, key: String) {
        let mut current = self.key;
        current.set(key);
    }

    fn reveal(self, key: String) {
        let id = tree_row_id(&key);
        self.at(key);
        ScrollIntoView::nearest(&id);
        FocusClaim::new(id).request();
    }

    fn step(self, keys: &[String], forward: bool) {
        if keys.is_empty() {
            return;
        }
        let current = self.key.peek().clone();
        let at = keys.iter().position(|key| *key == current);
        let next = match (at, forward) {
            (Some(index), true) => (index + 1).min(keys.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => keys.len() - 1,
        };
        self.reveal(keys[next].clone());
    }
}

fn show_notice(
    mut notice: Signal<Option<ExplorerNotice>>,
    mut generation: Signal<u32>,
    value: ExplorerNotice,
) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    notice.set(Some(value));
    spawn(async move {
        sleep_ms(NOTICE_MS).await;
        if generation() == id {
            notice.set(None);
        }
    });
}

fn submit_prompt(mut prompt: Signal<Option<TreePrompt>>, draft: Signal<String>) {
    let Some(current) = prompt() else {
        return;
    };
    let name = draft().trim().to_string();
    match current.kind {
        PromptKind::CreateFile if !name.is_empty() => create_entry(current.path, name, false),
        PromptKind::CreateDir if !name.is_empty() => create_entry(current.path, name, true),
        PromptKind::Rename if !name.is_empty() => rename_entry(current.path, name),
        PromptKind::Delete => delete_entry(current.path),
        _ => return,
    }
    prompt.set(None);
}

#[component]
fn TreeIndentGuides(depth: u16, base: u32) -> Element {
    rsx! {
        for level in 0..depth {
            div {
                key: "{level}",
                class: "pointer-events-none absolute inset-y-0 w-px bg-foreground/20 opacity-0 transition-opacity duration-100 group-hover/tree:opacity-100",
                style: "left:{u32::from(level) * 12 + base}px;",
            }
        }
    }
}

#[component]
fn Chevron(expanded: bool, loading: bool) -> Element {
    if loading {
        return rsx! {
            span { class: "inline-block h-3 w-3 shrink-0 animate-spin rounded-full border border-foreground/25 border-t-foreground/70" }
        };
    }
    let class = if expanded {
        "inline-block w-4 shrink-0 rotate-90 text-center text-base leading-none text-foreground/60 transition-[rotate] duration-150 ease-out"
    } else {
        "inline-block w-4 shrink-0 rotate-0 text-center text-base leading-none text-foreground/60 transition-[rotate] duration-150 ease-out"
    };
    rsx! {
        span { class: "{class}", "\u{203A}" }
    }
}

#[component]
fn SectionHeader(title: String, open: Signal<bool>, on_toggle: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1 px-2 py-1 cursor-default text-[11px] font-bold uppercase tracking-wide text-foreground/70 transition-colors hover:text-foreground",
            onclick: move |_| on_toggle.call(()),
            Chevron { expanded: open(), loading: false }
            span { class: "truncate", "{title}" }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TitleGlyph {
    NewFile,
    NewFolder,
    Refresh,
    CollapseAll,
    Search,
    Files,
    Clear,
}

impl TitleGlyph {
    fn paths(self) -> &'static [&'static str] {
        match self {
            Self::NewFile => &[
                "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
                "M14 2v4a2 2 0 0 0 2 2h4",
                "M12 12v6",
                "M9 15h6",
            ],
            Self::NewFolder => &[
                "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
                "M12 10v6",
                "M9 13h6",
            ],
            Self::Refresh => &[
                "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8",
                "M21 3v5h-5",
                "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16",
                "M8 16H3v5",
            ],
            Self::CollapseAll => &["m7 20 5-5 5 5", "m7 4 5 5 5-5"],
            Self::Search => &["M19 11a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z", "m21 21-4.35-4.35"],
            Self::Files => &[
                "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z",
                "M10 3v18",
            ],
            Self::Clear => &[
                "M20 12a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z",
                "m6.34 6.34 11.32 11.32",
            ],
        }
    }
}

#[component]
fn TitleAction(glyph: TitleGlyph, label: String, on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "flex h-5 w-5 shrink-0 items-center justify-center rounded text-foreground/55 outline-none transition-colors hover:bg-foreground/[0.12] hover:text-foreground focus-visible:bg-foreground/[0.12] focus-visible:text-foreground",
            title: "{label}",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                on_press.call(());
            },
            TitleActionIcon { glyph }
        }
    }
}

#[component]
fn TitleActionIcon(glyph: TitleGlyph) -> Element {
    rsx! {
        svg {
            class: "h-3.5 w-3.5",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.8",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for d in glyph.paths() {
                path { key: "{d}", d: "{d}" }
            }
        }
    }
}

#[component]
fn StickyOverlay(children: Element) -> Element {
    rsx! {
        div { class: "sticky top-0 z-[12] h-0",
            div { class: "absolute inset-x-0 top-0 {SIDEBAR_STICKY_SURFACE}", {children} }
        }
    }
}

#[component]
fn StickyFolders(rows: Vec<TreeRow>, on_pick: EventHandler<String>) -> Element {
    if rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        StickyOverlay {
            for row in rows {
                StickyFolderRow { key: "{row.path}", row, on_pick }
            }
        }
    }
}

#[component]
fn StickyFolderRow(row: TreeRow, on_pick: EventHandler<String>) -> Element {
    let pad = u32::from(row.depth) * 12 + 8;
    let path = row.path.clone();
    rsx! {
        div {
            class: "flex h-[22px] items-center gap-1 px-1 cursor-default text-foreground/80 transition-colors duration-100 hover:bg-foreground/[0.08]",
            style: "padding-left:{pad}px;",
            title: "{row.path}",
            onclick: move |_| on_pick.call(path.clone()),
            Chevron { expanded: true, loading: false }
            span { class: "truncate", "{row.name}" }
        }
    }
}

#[component]
fn StickyOutline(rows: Vec<OutlineRow>, on_pick: EventHandler<u32>) -> Element {
    if rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        StickyOverlay {
            for row in rows {
                StickyOutlineRow { key: "{OutlineKey::of(&row)}", row, on_pick }
            }
        }
    }
}

#[component]
fn StickyOutlineRow(row: OutlineRow, on_pick: EventHandler<u32>) -> Element {
    let pad = u32::from(row.depth) * 12 + 20;
    let line = row.line;
    rsx! {
        div {
            class: "flex h-[22px] items-center gap-1 px-1 cursor-default text-foreground/80 transition-colors duration-100 hover:bg-foreground/[0.08]",
            style: "padding-left:{pad}px;",
            onclick: move |_| on_pick.call(line),
            OutlineGlyph { kind: row.kind }
            span { class: "truncate", "{row.name}" }
        }
    }
}

#[component]
fn SearchView(view: Signal<SidebarView>) -> Element {
    let search = SearchState {
        query: use_signal(String::new),
        regex: use_signal(|| false),
        case_sensitive: use_signal(|| false),
        whole_word: use_signal(|| false),
        results: use_signal(|| None::<ExplorerSearchEvent>),
        collapsed: use_signal(HashSet::new),
        opened: use_signal(String::new),
    };
    let hit_focus = TreeFocus {
        key: use_signal(String::new),
    };
    let mut query = search.query;
    let ime = use_ime_guard();

    let _results = use_listener::<ExplorerSearchEvent, _>(EXPLORER_SEARCH_EVENT, move |event| {
        search.arrived(event);
    });
    let _showing = use_listener::<ExplorerFocusEvent, _>(EXPLORER_FOCUS_EVENT, move |event| {
        search.showing(&event.path);
    });

    let active = view().is_search();
    let opened = search.opened.cloned();
    let focused = hit_focus.key.cloned();
    let mut summary = None;
    let mut root = String::new();
    let mut files = Vec::new();
    if let Some(found) = search.results.read().as_ref() {
        summary = Some(SearchSummary::of(found).text());
        root = found.root.clone();
        files = found.files.clone();
    }

    rsx! {
        div { class: if active { "contents" } else { "hidden" },
            div { class: "flex h-9 shrink-0 items-center gap-1.5 pl-2 pr-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                SidebarViewSwitch { view }
                span { class: "truncate", {SidebarView::Search.title()} }
                div {
                    class: "pointer-events-none ml-auto flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 focus-within:pointer-events-auto focus-within:opacity-100 group-hover/panel:pointer-events-auto group-hover/panel:opacity-100",
                    TitleAction {
                        glyph: TitleGlyph::Refresh,
                        label: translate("common-refresh"),
                        on_press: move |_| search.run(),
                    }
                    TitleAction {
                        glyph: TitleGlyph::Clear,
                        label: translate("editor-search-clear"),
                        on_press: move |_| {
                            query.set(String::new());
                            search.clear();
                        },
                    }
                    TitleAction {
                        glyph: TitleGlyph::CollapseAll,
                        label: translate("common-collapse-all"),
                        on_press: move |_| search.collapse_all(),
                    }
                }
            }
            div { class: "shrink-0 px-2 pb-1",
                div { class: "flex h-7 items-center gap-1 rounded-md bg-foreground/[0.06] px-2 text-foreground/85 ring-1 ring-inset ring-foreground/10 focus-within:ring-cyan-400/50",
                    input {
                        id: SEARCH_INPUT_ID,
                        r#type: "text",
                        class: "min-w-0 flex-1 bg-transparent font-sans text-[11px] text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: translate("editor-find-in-files-placeholder"),
                        value: "{query}",
                        oninput: move |event| query.set(event.value()),
                        oncompositionstart: move |_| ime.start(),
                        oncompositionend: move |_| ime.commit(),
                        onkeydown: move |event: Event<KeyboardData>| {
                            event.stop_propagation();
                            if ime.swallows(&event) {
                                return;
                            }
                            match event.key() {
                                Key::Enter => {
                                    event.prevent_default();
                                    search.run();
                                }
                                Key::Escape => {
                                    event.prevent_default();
                                    query.set(String::new());
                                    search.clear();
                                }
                                _ => {}
                            }
                        },
                    }
                    SearchToggle {
                        label: "Aa".to_string(),
                        hint: translate("editor-find-case"),
                        on: (search.case_sensitive)(),
                        on_press: move |_| {
                            let mut flag = search.case_sensitive;
                            flag.toggle();
                        },
                    }
                    SearchToggle {
                        label: "ab".to_string(),
                        hint: translate("editor-find-whole-word"),
                        on: (search.whole_word)(),
                        on_press: move |_| {
                            let mut flag = search.whole_word;
                            flag.toggle();
                        },
                    }
                    SearchToggle {
                        label: ".*".to_string(),
                        hint: translate("editor-find-regex"),
                        on: (search.regex)(),
                        on_press: move |_| {
                            let mut flag = search.regex;
                            flag.toggle();
                        },
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto pb-4",
                if let Some(summary) = summary {
                    div { class: "px-3 py-1 text-[10px] text-muted-foreground", "{summary}" }
                    div { class: "{SIDEBAR_TREE_LIST_GROUP}",
                        for file in files {
                            SearchFileGroup {
                                key: "{file.path}",
                                file,
                                root: root.clone(),
                                search,
                                hit_focus,
                                opened: opened.clone(),
                                focused: focused.clone(),
                            }
                        }
                    }
                } else if !query().is_empty() {
                    div { class: "flex h-6 items-center px-3 text-foreground/45",
                        {translate("editor-search-idle")}
                    }
                }
            }
        }
    }
}

#[component]
fn SearchFileGroup(
    file: ExplorerSearchFile,
    root: String,
    search: SearchState,
    hit_focus: TreeFocus,
    opened: String,
    focused: String,
) -> Element {
    let collapsed = search.is_collapsed(&file.path);
    let path_toggle = file.path.clone();
    let directory = SearchFileDir::of(&root, &file.path);
    let body = if collapsed {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-150 ease-out"
    } else {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-150 ease-out"
    };
    rsx! {
        div {
            div {
                class: "{SIDEBAR_STICKY_SURFACE} sticky top-0 z-[11] flex h-[22px] cursor-default items-center gap-1 px-1 text-foreground/85 transition-colors duration-100 hover:bg-foreground/[0.08]",
                title: "{file.path}",
                onclick: move |_| search.toggle_group(&path_toggle),
                Chevron { expanded: !collapsed, loading: false }
                {rsx! { TypeIcon { path: file.path.clone(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" } }}
                span { class: "shrink-0 truncate", {SearchFileName::of(&file.path)} }
                if !directory.is_empty() {
                    span { class: "min-w-0 truncate text-[10px] text-muted-foreground", "{directory}" }
                }
                span {
                    class: "ml-auto shrink-0 rounded-full bg-foreground/[0.12] px-1.5 text-[10px] tabular-nums text-muted-foreground",
                    {SearchCount::of(&file)}
                }
            }
            div { class: "{body}",
                div { class: "min-h-0 overflow-hidden",
                    for hit in file.matches.clone() {
                        SearchHitRow {
                            key: "{SearchRowKey::of(&file.path, &hit)}",
                            path: file.path.clone(),
                            hit,
                            search,
                            hit_focus,
                            opened: opened.clone(),
                            focused: focused.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchHitRow(
    path: String,
    hit: ExplorerSearchMatch,
    search: SearchState,
    hit_focus: TreeFocus,
    opened: String,
    focused: String,
) -> Element {
    let key = SearchRowKey::of(&path, &hit);
    let accent = TreeRowAccent::of(key == opened, key == focused);
    let span = PreviewSpan::of(&hit.preview, hit.col, hit.end_col);
    let key_click = key.clone();
    let path_click = path.clone();
    let hit_click = hit.clone();
    let path_key = path.clone();
    let hit_key = hit.clone();
    rsx! {
        div {
            id: "{tree_row_id(&key)}",
            tabindex: "-1",
            class: "{SIDEBAR_TREE_ROW_BASE} {accent.classes()} pl-6",
            title: "{path}:{hit.line}",
            onkeydown: move |event: Event<KeyboardData>| {
                let forward = match event.key() {
                    Key::ArrowDown => true,
                    Key::ArrowUp => false,
                    Key::Enter => {
                        event.prevent_default();
                        event.stop_propagation();
                        search.open(&path_key, &hit_key);
                        return;
                    }
                    _ => return,
                };
                event.prevent_default();
                event.stop_propagation();
                hit_focus.step(&search.keys(), forward);
            },
            onclick: move |_| {
                hit_focus.at(key_click.clone());
                search.open(&path_click, &hit_click);
            },
            span { class: "w-full truncate font-mono text-[10px]",
                span { class: "text-muted-foreground", "{span.before}" }
                span { class: "rounded-sm bg-cyan-400/30 text-foreground", "{span.hit}" }
                span { class: "text-muted-foreground", "{span.after}" }
            }
        }
    }
}

#[component]
fn SearchToggle(label: String, hint: String, on: bool, on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if on {
                "shrink-0 rounded bg-foreground/15 px-1 font-mono text-[10px] text-foreground"
            } else {
                "shrink-0 rounded px-1 font-mono text-[10px] text-foreground/50 hover:bg-foreground/10 hover:text-foreground"
            },
            title: "{hint}",
            onclick: move |event: Event<MouseData>| {
                event.stop_propagation();
                on_press.call(());
            },
            "{label}"
        }
    }
}

#[component]
fn SidebarViewSwitch(view: Signal<SidebarView>) -> Element {
    let current = view();
    rsx! {
        button {
            class: "flex h-5 w-5 shrink-0 items-center justify-center rounded text-foreground/55 outline-none transition-colors hover:bg-foreground/[0.12] hover:text-foreground focus-visible:bg-foreground/[0.12] focus-visible:text-foreground",
            title: current.switch_label(),
            onclick: move |event: Event<MouseData>| {
                event.stop_propagation();
                let mut view = view;
                let next = view.peek().other();
                view.set(next);
                if next.is_search() {
                    FocusClaim::new(SEARCH_INPUT_ID).request();
                }
            },
            TitleActionIcon { glyph: current.switch_glyph() }
        }
    }
}

fn prompt_title(kind: PromptKind) -> String {
    match kind {
        PromptKind::CreateFile => translate("editor-new-file"),
        PromptKind::CreateDir => translate("editor-new-folder"),
        PromptKind::Rename => translate("common-rename"),
        PromptKind::Delete => translate("common-delete"),
    }
}

fn localize_notice(message: &str) -> String {
    for (prefix, id) in [
        ("Created folder ", "editor-created-folder"),
        ("Created file ", "editor-created-file"),
        ("Renamed to ", "editor-renamed-to"),
        ("Deleted ", "editor-deleted"),
    ] {
        if let Some(name) = message.strip_prefix(prefix) {
            return translate_with(id, &[("name", TranslationValue::String(name))]);
        }
    }
    message.to_string()
}

#[component]
pub fn ExplorerPanel(visible: Signal<bool>, caret_line: u32, view: Signal<SidebarView>) -> Element {
    let mut root_name = use_signal(|| translate("editor-explorer"));
    let mut root_path = use_signal(String::new);
    let mut current_path = use_signal(String::new);
    let mut root_loading = use_signal(|| false);
    let rows = use_signal(Vec::<MotionRow>::new);
    let row_generation = use_signal(|| 0u32);
    let tree = TreeRows {
        rows,
        generation: row_generation,
    };
    let focus_generation = use_signal(|| 0u32);
    let scroller = use_signal(|| None::<Rc<MountedData>>);
    let files_list = use_signal(|| None::<Rc<MountedData>>);
    let files_top = use_signal(|| 0usize);
    let files_measuring = use_signal(|| false);
    let files_pending = use_signal(|| false);
    let files_sticky = StickySection {
        scroller,
        list: files_list,
        top: files_top,
        measuring: files_measuring,
        pending: files_pending,
    };
    let outline_list = use_signal(|| None::<Rc<MountedData>>);
    let outline_top = use_signal(|| 0usize);
    let outline_measuring = use_signal(|| false);
    let outline_pending = use_signal(|| false);
    let outline_sticky = StickySection {
        scroller,
        list: outline_list,
        top: outline_top,
        measuring: outline_measuring,
        pending: outline_pending,
    };
    let tree_focus = TreeFocus {
        key: use_signal(String::new),
    };
    let outline_focus = TreeFocus {
        key: use_signal(String::new),
    };
    let mut open_editors = use_signal(Vec::<OpenEditorItem>::new);
    let mut outline = use_signal(Vec::<OutlineRow>::new);
    let mut show_open = use_signal(|| true);
    let mut show_files = use_signal(|| true);
    let mut show_outline = use_signal(|| true);
    let mut menu = use_signal(|| None::<TreeMenu>);
    let mut prompt = use_signal(|| None::<TreePrompt>);
    let mut draft = use_signal(String::new);
    let mut notice = use_signal(|| None::<ExplorerNotice>);
    let notice_generation = use_signal(|| 0u32);
    let ime = use_ime_guard();

    use_effect(move || {
        if !visible() {
            cancel_tree_focus(focus_generation);
        }
    });

    let _tree = use_listener::<ExplorerTreeEvent, _>(EXPLORER_TREE_EVENT, move |e| {
        root_name.set(e.root_name);
        root_path.set(e.root_path);
        current_path.set(e.current_path);
        root_loading.set(e.loading);
        tree.reconcile(e.rows);
        if visible() && !e.focus_path.is_empty() {
            tree_focus.at(e.focus_path.clone());
            schedule_tree_focus(e.focus_path, focus_generation, ExplorerReveal::Followed);
        }
    });
    let _focus = use_listener::<ExplorerFocusEvent, _>(EXPLORER_FOCUS_EVENT, move |e| {
        if current_path() != e.path {
            current_path.set(e.path.clone());
        }
        if visible() {
            tree_focus.at(e.path.clone());
            schedule_tree_focus(e.path, focus_generation, e.reveal);
        }
    });
    let _open = use_listener::<OpenEditorsEvent, _>(EXPLORER_OPEN_EDITORS_EVENT, move |e| {
        open_editors.set(e.items);
    });
    let _outline = use_listener::<OutlineEvent, _>(EXPLORER_OUTLINE_EVENT, move |e| {
        outline.set(e.items);
    });
    let _fs_result = use_listener::<ExplorerFsResult, _>(EXPLORER_FS_RESULT_EVENT, move |e| {
        if e.ok && !e.open_path.is_empty() {
            open_file(e.open_path);
        }
        show_notice(
            notice,
            notice_generation,
            ExplorerNotice {
                ok: e.ok,
                message: if e.ok {
                    localize_notice(&e.message)
                } else {
                    e.message
                },
            },
        );
    });

    let open_body = if show_open() {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };
    let files_body = if show_files() {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };
    let outline_body = if show_outline() {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };

    use_effect(move || {
        let _layout = (
            visible(),
            view().is_search(),
            show_open(),
            show_files(),
            show_outline(),
            open_editors.read().len(),
            rows.read().len(),
            outline.read().len(),
        );
        spawn(async move {
            sleep_ms(TREE_MOTION_MS + 60).await;
            files_sticky.measure();
            outline_sticky.measure();
        });
    });

    let sticky_files = {
        let current = rows.read();
        let mut depths = Vec::with_capacity(current.len());
        for motion in current.iter() {
            depths.push(motion.row.depth);
        }
        let mut picked = Vec::new();
        for index in AncestorChain::of(&depths, files_top()) {
            picked.push(current[index].row.clone());
        }
        picked
    };
    let sticky_outline = {
        let current = outline.read();
        let mut depths = Vec::with_capacity(current.len());
        for row in current.iter() {
            depths.push(row.depth);
        }
        let mut picked = Vec::new();
        for index in AncestorChain::of(&depths, outline_top()) {
            picked.push(current[index].clone());
        }
        picked
    };
    let focused_path = tree_focus.key.cloned();
    let focused_symbol = outline_focus.key.cloned();
    let caret_symbol = CaretSymbol::of(&outline.read(), caret_line);
    let tree_empty = rows.read().is_empty();

    rsx! {
        div { class: "group/panel relative flex h-full w-full flex-col overflow-hidden bg-foreground/[0.04] font-sans text-xs text-foreground select-none",
            SearchView { view }
            div {
                class: if view().is_search() {
                    "hidden"
                } else {
                    "flex h-9 shrink-0 items-center gap-1.5 pl-2 pr-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground"
                },
                SidebarViewSwitch { view }
                span { class: "truncate", {SidebarView::Explorer.title()} }
                div {
                    class: "pointer-events-none ml-auto flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 focus-within:pointer-events-auto focus-within:opacity-100 group-hover/panel:pointer-events-auto group-hover/panel:opacity-100",
                    TitleAction {
                        glyph: TitleGlyph::NewFile,
                        label: translate("editor-new-file"),
                        on_press: move |_| {
                            let focus = tree_focus.key.peek().clone();
                            draft.set(String::new());
                            prompt
                                .set(
                                    Some(TreePrompt {
                                        kind: PromptKind::CreateFile,
                                        path: tree.create_parent(&focus, &root_path()),
                                        name: String::new(),
                                    }),
                                );
                        },
                    }
                    TitleAction {
                        glyph: TitleGlyph::NewFolder,
                        label: translate("editor-new-folder"),
                        on_press: move |_| {
                            let focus = tree_focus.key.peek().clone();
                            draft.set(String::new());
                            prompt
                                .set(
                                    Some(TreePrompt {
                                        kind: PromptKind::CreateDir,
                                        path: tree.create_parent(&focus, &root_path()),
                                        name: String::new(),
                                    }),
                                );
                        },
                    }
                    TitleAction {
                        glyph: TitleGlyph::Refresh,
                        label: translate("common-refresh"),
                        on_press: move |_| tree.refresh(root_path()),
                    }
                    TitleAction {
                        glyph: TitleGlyph::CollapseAll,
                        label: translate("common-collapse-all"),
                        on_press: move |_| tree.collapse_all(),
                    }
                }
            }
            div {
                class: if view().is_search() {
                    "hidden"
                } else {
                    "group/tree min-h-0 flex-1 overflow-y-auto pb-4"
                },
                onmounted: move |event: Event<MountedData>| {
                    let mut handle = scroller;
                    handle.set(Some(event.data()));
                    files_sticky.measure();
                    outline_sticky.measure();
                },
                onscroll: move |_| {
                    files_sticky.measure();
                    outline_sticky.measure();
                },
                SectionHeader { title: translate("editor-open-editors"), open: show_open, on_toggle: EventHandler::new(move |_| show_open.set(!show_open())) }
                div { class: "{open_body}",
                    div { class: "min-h-0 overflow-hidden",
                        for it in open_editors() {
                            {
                                let p_open = it.path.clone();
                                let p_close = it.path.clone();
                                let active = it.active;
                                let dirty = it.dirty;
                                rsx! {
                                    div {
                                        key: "{it.path}",
                                        class: if active {
                                            "group flex items-center gap-1 px-2 py-0.5 cursor-default bg-cyan-400/12 text-foreground transition-[background-color,opacity,transform] duration-150"
                                        } else {
                                            "group flex items-center gap-1 px-2 py-0.5 cursor-default text-foreground/75 transition-[background-color,opacity,transform] duration-150 hover:bg-foreground/[0.08]"
                                        },
                                        style: "padding-left:20px;",
                                        onclick: move |_| open_file(p_open.clone()),
                                        span {
                                            class: "inline-block w-3 shrink-0 cursor-default text-center text-foreground/50 opacity-0 transition-opacity group-hover:opacity-100 hover:text-foreground",
                                            onclick: move |e: Event<MouseData>| {
                                                e.stop_propagation();
                                                close_editor(p_close.clone());
                                            },
                                            "\u{00D7}"
                                        }
                                        {rsx! { TypeIcon { path: it.path.to_string(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" } }}
                                        span { class: "truncate", "{it.name}" }
                                        if dirty {
                                            span { class: "ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-cyan-300" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    id: "{tree_row_id(&root_path())}",
                    tabindex: "-1",
                    class: if current_path() == root_path() { "bg-cyan-400/10 outline-none" } else { "outline-none" },
                    oncontextmenu: move |e: Event<MouseData>| {
                        e.prevent_default();
                        let coordinates = e.client_coordinates();
                        let (x, y) = (coordinates.x, coordinates.y);
                        menu.set(Some(TreeMenu {
                            path: root_path(),
                            name: root_name(),
                            is_dir: true,
                            is_root: true,
                            x,
                            y,
                        }));
                    },
                    SectionHeader {
                        title: root_name(),
                        open: show_files,
                        on_toggle: EventHandler::new(move |_| show_files.set(!show_files())),
                    }
                }
                StickyFolders {
                    rows: sticky_files,
                    on_pick: move |path: String| {
                        ScrollIntoView::nearest(&tree_row_id(&path));
                    },
                }
                div { class: "{files_body}",
                    div {
                        class: "{SIDEBAR_TREE_LIST_GROUP} min-h-0 overflow-hidden",
                        onmounted: move |event: Event<MountedData>| files_sticky.mounted(event.data()),
                        if tree_empty {
                            if root_loading() {
                                div { class: "flex h-6 items-center gap-2 px-3 text-foreground/45",
                                    span { class: "h-3 w-3 animate-spin rounded-full border border-foreground/20 border-t-foreground/60" }
                                    {translate("common-loading")}
                                }
                            } else {
                                div { class: "flex h-6 items-center px-3 text-foreground/45",
                                    {translate("editor-explorer-empty")}
                                }
                            }
                        }
                        for motion in rows() {
                            {
                                let row = motion.row.clone();
                                let path_click = row.path.clone();
                                let path_key = row.path.clone();
                                let path_prefetch = row.path.clone();
                                let path_menu = row.path.clone();
                                let name_menu = row.name.clone();
                                let is_dir = row.is_dir;
                                let was_expanded = row.expanded;
                                let accent = TreeRowAccent::of(
                                    row.path == current_path(),
                                    row.path == focused_path,
                                );
                                let pad = (row.depth as u32) * 12 + 8;
                                let motion_class = if motion.visible {
                                    "opacity-100 translate-y-0 transition-[opacity,translate] duration-150 ease-out"
                                } else {
                                    "pointer-events-none opacity-0 -translate-y-1 transition-[opacity,translate] duration-150 ease-out"
                                };
                                rsx! {
                                    div { key: "{row.path}", class: "{motion_class}",
                                        div { class: "min-h-0 overflow-hidden",
                                            div {
                                                id: "{tree_row_id(&row.path)}",
                                                tabindex: "-1",
                                                class: "{SIDEBAR_TREE_ROW_BASE} {accent.classes()}",
                                                style: "padding-left:{pad}px;",
                                                title: "{row.path}",
                                                onmouseenter: move |_| {
                                                    if is_dir {
                                                        prefetch_dir(path_prefetch.clone());
                                                    }
                                                },
                                                oncontextmenu: move |e: Event<MouseData>| {
                                                    e.prevent_default();
                                                    e.stop_propagation();
                                                    let coordinates = e.client_coordinates();
                                                    let (x, y) = (coordinates.x, coordinates.y);
                                                    tree_focus.at(path_menu.clone());
                                                    menu.set(Some(TreeMenu {
                                                        path: path_menu.clone(),
                                                        name: name_menu.clone(),
                                                        is_dir,
                                                        is_root: false,
                                                        x,
                                                        y,
                                                    }));
                                                },
                                                onkeydown: move |e: Event<KeyboardData>| {
                                                    match e.key() {
                                                        Key::ArrowDown => {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            tree_focus.step(&tree.paths(), true);
                                                        }
                                                        Key::ArrowUp => {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            tree_focus.step(&tree.paths(), false);
                                                        }
                                                        Key::Enter => {
                                                            e.prevent_default();
                                                            e.stop_propagation();
                                                            if is_dir {
                                                                if was_expanded {
                                                                    tree.collapse(&path_key);
                                                                } else {
                                                                    tree.expand(&path_key);
                                                                }
                                                                toggle_dir(path_key.clone());
                                                            } else {
                                                                open_file(path_key.clone());
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                },
                                                onclick: move |_| {
                                                    tree_focus.at(path_click.clone());
                                                    if is_dir {
                                                        if was_expanded {
                                                            tree.collapse(&path_click);
                                                        } else {
                                                            tree.expand(&path_click);
                                                        }
                                                        toggle_dir(path_click.clone());
                                                    } else {
                                                        open_file(path_click.clone());
                                                    }
                                                },
                                                TreeIndentGuides { depth: row.depth, base: 14 }
                                                if is_dir {
                                                    Chevron { expanded: row.expanded, loading: row.loading }
                                                } else {
                                                    span { class: "inline-block w-4 shrink-0" }
                                                }
                                                if !is_dir {
                                                    {rsx! { TypeIcon { path: row.path.to_string(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" } }}
                                                }
                                                span { class: "truncate", "{row.name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                SectionHeader {
                    title: translate("editor-outline"),
                    open: show_outline,
                    on_toggle: EventHandler::new(move |_| show_outline.set(!show_outline())),
                }
                StickyOutline { rows: sticky_outline, on_pick: move |line: u32| goto_line(line) }
                div { class: "{outline_body}",
                    div {
                        class: "{SIDEBAR_TREE_LIST_GROUP} min-h-0 overflow-hidden",
                        onmounted: move |event: Event<MountedData>| outline_sticky.mounted(event.data()),
                        for s in outline() {
                            {
                                let line = s.line;
                                let key = OutlineKey::of(&s);
                                let key_step = key.clone();
                                let accent = TreeRowAccent::of(
                                    key == caret_symbol,
                                    key == focused_symbol,
                                );
                                let pad = (s.depth as u32) * 12 + 20;
                                rsx! {
                                    div {
                                        key: "{key}",
                                        id: "{tree_row_id(&key)}",
                                        tabindex: "-1",
                                        class: "{SIDEBAR_TREE_ROW_BASE} {accent.classes()}",
                                        style: "padding-left:{pad}px;",
                                        onkeydown: move |e: Event<KeyboardData>| {
                                            let forward = match e.key() {
                                                Key::ArrowDown => true,
                                                Key::ArrowUp => false,
                                                Key::Enter => {
                                                    e.prevent_default();
                                                    e.stop_propagation();
                                                    goto_line(line);
                                                    return;
                                                }
                                                _ => return,
                                            };
                                            e.prevent_default();
                                            e.stop_propagation();
                                            let mut keys = Vec::new();
                                            for row in outline.peek().iter() {
                                                keys.push(OutlineKey::of(row));
                                            }
                                            outline_focus.step(&keys, forward);
                                        },
                                        onclick: move |_| {
                                            outline_focus.at(key_step.clone());
                                            goto_line(line);
                                        },
                                        TreeIndentGuides { depth: s.depth, base: 26 }
                                        OutlineGlyph { kind: s.kind }
                                        span { class: "truncate", "{s.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(current) = menu() {
                div {
                    class: "fixed inset-0 z-[998]",
                    onclick: move |_| menu.set(None),
                    oncontextmenu: move |e| {
                        e.prevent_default();
                        menu.set(None);
                    },
                }
                div {
                    class: "fixed z-[999] min-w-[180px] origin-top-left animate-[dx-fade-zoom-in_120ms_ease-out_forwards] rounded-lg bg-background p-1 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28),inset_0_0_0_1px_var(--border)]",
                    style: "left:clamp(8px, {current.x}px, 100dvw - 190px);top:clamp(8px, {current.y}px, 100dvh - 220px);",
                    onclick: move |e| e.stop_propagation(),
                    if current.is_dir {
                        button {
                            class: "flex w-full items-center rounded-md px-3 py-2 text-left transition-colors hover:bg-foreground/[0.08]",
                            onclick: {
                                let path = current.path.clone();
                                move |_| {
                                    draft.set(String::new());
                                    prompt.set(Some(TreePrompt { kind: PromptKind::CreateFile, path: path.clone(), name: String::new() }));
                                    menu.set(None);
                                }
                            },
                            {translate("editor-new-file")}
                        }
                        button {
                            class: "flex w-full items-center rounded-md px-3 py-2 text-left transition-colors hover:bg-foreground/[0.08]",
                            onclick: {
                                let path = current.path.clone();
                                move |_| {
                                    draft.set(String::new());
                                    prompt.set(Some(TreePrompt { kind: PromptKind::CreateDir, path: path.clone(), name: String::new() }));
                                    menu.set(None);
                                }
                            },
                            {translate("editor-new-folder")}
                        }
                        div { class: "mx-2 my-1 h-px bg-border" }
                        button {
                            class: "flex w-full items-center rounded-md px-3 py-2 text-left transition-colors hover:bg-foreground/[0.08]",
                            onclick: {
                                let path = current.path.clone();
                                move |_| {
                                    refresh_dir(path.clone());
                                    menu.set(None);
                                }
                            },
                            {translate("common-refresh")}
                        }
                    }
                    if !current.is_root {
                        if current.is_dir {
                            div { class: "mx-2 my-1 h-px bg-border" }
                        }
                        button {
                            class: "flex w-full items-center rounded-md px-3 py-2 text-left transition-colors hover:bg-foreground/[0.08]",
                            onclick: {
                                let path = current.path.clone();
                                let name = current.name.clone();
                                move |_| {
                                    draft.set(name.clone());
                                    prompt.set(Some(TreePrompt { kind: PromptKind::Rename, path: path.clone(), name: name.clone() }));
                                    menu.set(None);
                                }
                            },
                            {translate("common-rename")}
                        }
                        button {
                            class: "flex w-full items-center rounded-md px-3 py-2 text-left text-red-600 transition-colors hover:bg-red-500/10 dark:text-red-300",
                            onclick: {
                                let path = current.path.clone();
                                let name = current.name.clone();
                                move |_| {
                                    prompt.set(Some(TreePrompt { kind: PromptKind::Delete, path: path.clone(), name: name.clone() }));
                                    menu.set(None);
                                }
                            },
                            {translate("common-delete")}
                        }
                    }
                }
            }

            if let Some(current) = prompt() {
                div {
                    class: "fixed inset-0 z-[1000] flex items-center justify-center bg-black/25 animate-[dx-fade-in_120ms_ease-out_forwards]",
                    onclick: move |_| prompt.set(None),
                    div {
                        class: "w-[min(360px,calc(100vw-32px))] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-xl bg-background p-4 shadow-[0_18px_60px_rgba(0,0,0,0.35),inset_0_0_0_1px_var(--border)]",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "text-sm font-semibold text-foreground", "{prompt_title(current.kind)}" }
                        if current.kind == PromptKind::Delete {
                            div { class: "mt-2 text-xs leading-relaxed text-muted-foreground",
                                {translate_with(
                                    "editor-delete-confirm",
                                    &[("name", TranslationValue::String(&current.name))],
                                )}
                            }
                        } else {
                            input {
                                class: "mt-3 w-full rounded-md border border-border bg-foreground/[0.04] px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-cyan-400/50",
                                autofocus: true,
                                value: "{draft}",
                                oninput: move |e| draft.set(e.value()),
                                oncompositionstart: move |_| ime.start(),
                                oncompositionend: move |_| ime.commit(),
                                onkeydown: move |e: Event<KeyboardData>| {
                                    e.stop_propagation();
                                    if ime.swallows(&e) {
                                        return;
                                    }
                                    if e.key() == Key::Enter {
                                        e.prevent_default();
                                        submit_prompt(prompt, draft);
                                    } else if e.key() == Key::Escape {
                                        prompt.set(None);
                                    }
                                },
                            }
                        }
                        div { class: "mt-4 flex justify-end gap-2",
                            button {
                                class: "rounded-md px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
                                onclick: move |_| prompt.set(None),
                                {translate("common-cancel")}
                            }
                            button {
                                class: if current.kind == PromptKind::Delete {
                                    "rounded-md bg-red-500 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-red-600"
                                } else {
                                    "rounded-md bg-cyan-500 px-3 py-1.5 text-xs font-medium text-slate-950 transition-colors hover:bg-cyan-400"
                                },
                                onclick: move |_| submit_prompt(prompt, draft),
                                {if current.kind == PromptKind::Delete {
                                    translate("common-delete")
                                } else {
                                    translate("common-save")
                                }}
                            }
                        }
                    }
                }
            }

            if let Some(current) = notice() {
                button {
                    class: if current.ok {
                        "absolute bottom-3 left-3 right-3 z-[997] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-lg bg-success/90 px-3 py-2 text-left text-xs text-white shadow-lg"
                    } else {
                        "absolute bottom-3 left-3 right-3 z-[997] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-lg bg-red-500/90 px-3 py-2 text-left text-xs text-white shadow-lg"
                    },
                    onclick: move |_| notice.set(None),
                    "{current.message}"
                }
            }
        }
    }
}

#[component]
pub fn OutlineGlyph(kind: u8) -> Element {
    let label = match kind {
        15 => "abc",
        12 => "fn",
        5 | 23 => "{}",
        _ => "\u{25C6}",
    };
    rsx! {
        span { class: "inline-block w-6 shrink-0 text-center text-[9px] font-semibold text-cyan-600 dark:text-cyan-300/80", "{label}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl MotionRow {
        fn dir(path: &str) -> Self {
            Self {
                row: TreeRow {
                    name: String::new(),
                    path: path.to_string(),
                    depth: 0,
                    is_dir: true,
                    expanded: false,
                    loading: false,
                },
                visible: true,
            }
        }

        fn file(path: &str) -> Self {
            let mut motion = Self::dir(path);
            motion.row.is_dir = false;
            motion
        }
    }

    fn span(name: &str, line: u32, end_line: u32, depth: u16) -> OutlineRow {
        OutlineRow {
            name: name.to_string(),
            kind: 12,
            line,
            end_line,
            depth,
        }
    }

    #[test]
    fn a_new_entry_lands_in_the_selected_folder_or_the_selected_file_s_folder() {
        let rows = vec![MotionRow::dir("/r/src"), MotionRow::file("/r/src/lib.rs")];
        assert_eq!(CreateTarget::of(&rows, "/r/src", "/r"), "/r/src");
        assert_eq!(CreateTarget::of(&rows, "/r/src/lib.rs", "/r"), "/r/src");
    }

    #[test]
    fn a_new_entry_lands_in_the_root_without_a_live_selection() {
        let rows = vec![MotionRow::dir("/r/src")];
        assert_eq!(CreateTarget::of(&rows, "", "/r"), "/r");
        assert_eq!(CreateTarget::of(&rows, "/r/dropped", "/r"), "/r");
    }

    #[test]
    fn the_caret_marks_the_innermost_symbol_holding_it_and_nothing_above_the_first() {
        let rows = [
            span("first", 0, 8, 0),
            span("nested", 2, 6, 1),
            span("second", 20, 30, 0),
        ];
        assert_eq!(CaretSymbol::of(&rows, 4), "2-nested");
        assert_eq!(CaretSymbol::of(&rows, 7), "0-first");
        assert_eq!(CaretSymbol::of(&rows, 12), "");
        assert_eq!(CaretSymbol::of(&[], 3), "");
    }

    #[test]
    fn a_symbol_with_no_extent_holds_the_caret_until_the_next_one_starts() {
        let rows = [
            span("open", 0, OutlineRow::OPEN_END, 0),
            span("bounded", 4, 6, 1),
            span("later", 40, OutlineRow::OPEN_END, 0),
        ];
        assert_eq!(CaretSymbol::of(&rows, 5), "4-bounded");
        assert_eq!(CaretSymbol::of(&rows, 20), "0-open");
        assert_eq!(CaretSymbol::of(&rows, 900), "40-later");
    }

    struct HitFile;

    impl HitFile {
        fn of(path: &str, lines: &[u32], capped: bool) -> ExplorerSearchFile {
            let mut matches = Vec::new();
            for line in lines {
                matches.push(ExplorerSearchMatch {
                    line: *line,
                    col: 0,
                    end_col: 1,
                    preview: String::new(),
                });
            }
            ExplorerSearchFile {
                path: path.to_string(),
                matches,
                capped,
            }
        }
    }

    #[test]
    fn the_highlight_lands_on_the_match_when_the_line_holds_astral_characters() {
        let span = PreviewSpan::of("let \u{1F600} = \"needle\";", 10, 16);

        assert_eq!(span.before, "let \u{1F600} = \"");
        assert_eq!(span.hit, "needle");
        assert_eq!(span.after, "\";");
    }

    #[test]
    fn a_match_past_the_truncated_preview_highlights_nothing_and_loses_no_text() {
        let span = PreviewSpan::of("short", 40, 46);

        assert_eq!(span.before, "short");
        assert!(span.hit.is_empty());
        assert!(span.after.is_empty());
    }

    #[test]
    fn a_total_reads_as_a_floor_when_either_the_sweep_or_any_file_was_capped() {
        let whole = ExplorerSearchEvent {
            root: "/r".to_string(),
            query: "needle".to_string(),
            files: vec![
                HitFile::of("/r/a.rs", &[1, 2], false),
                HitFile::of("/r/b.rs", &[7], false),
            ],
            capped: false,
        };
        assert_eq!(
            SearchSummary::of(&whole),
            SearchSummary {
                total: 3,
                files: 2,
                floor: false,
            }
        );

        let mut per_file = whole.clone();
        per_file.files[1].capped = true;
        assert!(SearchSummary::of(&per_file).floor);

        let mut swept = whole.clone();
        swept.capped = true;
        assert!(SearchSummary::of(&swept).floor);
    }

    #[test]
    fn a_group_shows_the_folder_under_the_root_and_nothing_for_a_root_level_file() {
        assert_eq!(SearchFileDir::of("/r", "/r/src/host/lib.rs"), "src/host");
        assert_eq!(SearchFileDir::of("/r", "/r/lib.rs"), "");
        assert_eq!(SearchFileDir::of("/r", "/elsewhere/lib.rs"), "/elsewhere");
    }

    #[test]
    fn chain_is_the_enclosing_folders_outermost_first() {
        let depths = [0, 1, 2, 3, 3, 1];
        assert_eq!(AncestorChain::of(&depths, 4), vec![0, 1, 2]);
        assert_eq!(AncestorChain::of(&depths, 5), vec![0]);
    }

    #[test]
    fn chain_skips_siblings_and_deeper_rows_above() {
        let depths = [0, 1, 2, 2, 1, 2];
        assert_eq!(AncestorChain::of(&depths, 5), vec![0, 4]);
    }

    #[test]
    fn chain_is_empty_at_the_first_row_and_past_the_end() {
        let depths = [0, 1, 2];
        assert!(AncestorChain::of(&depths, 0).is_empty());
        assert!(AncestorChain::of(&depths, 3).is_empty());
        assert!(AncestorChain::of(&[], 0).is_empty());
    }

    #[test]
    fn chain_keeps_the_outermost_levels_within_the_cap() {
        let depths: Vec<u16> = (0..12u16).collect();
        let chain = AncestorChain::of(&depths, 11);
        assert_eq!(chain.len(), STICKY_DEPTH_MAX);
        assert_eq!(chain, vec![0, 1, 2, 3, 4]);
    }
}
