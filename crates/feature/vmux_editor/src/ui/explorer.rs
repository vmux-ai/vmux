#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use super::state::FileUi;
use dioxus::prelude::*;
use vmux_ecs::event::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownOption, DropdownPopup};
use vmux_ui::components::manager::ManagerSearch;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::tree_row::{
    FILE_TREE_LIST_GROUP, FILE_TREE_ROW_BASE, FILE_TREE_STICKY_SURFACE, TreeRowAccent,
};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::Platform;
use vmux_ui::scroll::ScrollIntoView;

pub const SEARCH_INPUT_ID: &str = "explorer-search-input";

#[component]
pub fn ExplorerPanel(
    visible: Memo<bool>,
    caret_line: u32,
    view: ReadSignal<SidebarView>,
) -> Element {
    let notice =
        FileUi::current().use_value(|state: &FileUiState| Some(&state.explorer.explorer_notice));
    let panel = FileUi::current().use_value(|state| state.explorer.explorer_panel.as_ref());
    let prompt = FileUi::current().use_value(|state| Some(&state.explorer.explorer_prompt));
    let results = FileUi::current().use_value(|state| Some(&state.explorer.explorer_search));
    let search = SearchState {
        results: results.into(),
    };
    let rows = use_signal(Vec::<MotionRow>::new);
    let tree = TreeRows {
        rows,
        generation: use_signal(|| 0u32),
    };
    let focus_generation = use_signal(|| 0u32);
    let mut focus_revision = use_signal(|| 0u64);
    let scroller = use_signal(|| None::<Rc<MountedData>>);
    let files_sticky = StickySection::new(scroller);
    let outline_sticky = StickySection::new(scroller);
    let tree_focus = TreeFocus {
        key: use_signal(String::new),
    };
    let outline_focus = TreeFocus {
        key: use_signal(String::new),
    };
    let show_files = panel.read().as_ref().is_none_or(|panel| panel.files);
    let show_outline = panel.read().as_ref().is_none_or(|panel| panel.outline);
    let menu = use_signal(|| None::<TreeMenu>);

    use_effect(move || {
        if !visible() {
            TreeFocus::cancel(focus_generation);
        }
    });

    let tree_event = FileUi::current().use_field(|state| state.explorer.explorer_tree.as_ref());
    use_effect(move || {
        tree_event.for_each(|e| tree.reconcile(e.rows));
    });
    let focus_event = FileUi::current().use_field(|state| state.explorer.explorer_focus.as_ref());
    use_effect(move || {
        focus_event.for_each(|e| {
            if e.revision <= *focus_revision.peek() {
                return;
            }
            focus_revision.set(e.revision);
            if visible() {
                tree_focus.at(e.path.clone());
                TreeFocus::schedule(e.path, focus_generation, e.reveal);
            }
        });
    });

    rsx! {
        div { class: "group/panel relative flex h-full w-full flex-col overflow-hidden bg-background font-sans text-xs text-foreground select-none",
            SearchInput { search, view }
            SearchView { view, search }
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
                ExplorerFilesSection {
                    visible,
                    view,
                    show_files,
                    tree,
                    tree_focus,
                    files_sticky,
                    menu,
                }
                ExplorerOutlineSection {
                    visible,
                    view,
                    caret_line,
                    show_outline,
                    outline_focus,
                    outline_sticky,
                }
            }
            if let Some(current) = menu() {
                ExplorerContextMenu { current, menu }
            }
            if let Some(current) = prompt().filter(|prompt| prompt.open) {
                ExplorerPromptDialog { current }
            }
            if let Some(current) = notice() {
                ExplorerNoticeBanner { notice: current }
            }
        }
    }
}

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

    fn title(self) -> String {
        match self {
            Self::Explorer => translate("editor-explorer"),
            Self::Search => translate("editor-search"),
        }
    }
}

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

#[derive(PartialEq, Eq, Debug)]
struct SearchSummary {
    total: usize,
    files: usize,
    floor: bool,
}

impl From<&ExplorerSearchEvent> for SearchSummary {
    fn from(results: &ExplorerSearchEvent) -> Self {
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
}

impl SearchSummary {
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

    fn file_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }

    fn relative_parent(root: &str, path: &str) -> String {
        let relative = Path::new(path)
            .strip_prefix(root)
            .unwrap_or(Path::new(path));
        let Some(parent) = relative.parent() else {
            return String::new();
        };
        parent.to_string_lossy().into_owned()
    }

    fn count_for_file(file: &ExplorerSearchFile) -> String {
        let shown = file.matches.len();
        match file.capped {
            true => format!("{shown}+"),
            false => format!("{shown}"),
        }
    }
}

struct PreviewSpan {
    before: String,
    hit: String,
    after: String,
}

impl PreviewSpan {
    fn split(preview: &str, col: u32, end_col: u32) -> Self {
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
    results: ReadSignal<Option<ExplorerSearchEvent>>,
}

impl SearchState {
    fn run(self) {
        let results = self.results.read();
        let Some(results) = results.as_ref() else {
            return;
        };
        if results.query.trim().is_empty() {
            self.clear();
            return;
        }
        let _ = send(&ExplorerSearchRequest {
            query: results.query.clone(),
            regex: results.regex,
            case_sensitive: results.case_sensitive,
            whole_word: results.whole_word,
        });
    }

    fn draft(self, query: String, regex: bool, case_sensitive: bool, whole_word: bool) {
        let _ = send(&ExplorerSearchDraftRequest {
            query,
            regex,
            case_sensitive,
            whole_word,
        });
    }

    fn clear(self) {
        let _ = send(&ExplorerSearchClear);
    }

    fn close(self) {
        self.clear();
        let _ = send(&ExplorerPanelViewSet { search: false });
    }

    fn is_collapsed(self, path: &str) -> bool {
        self.results
            .read()
            .as_ref()
            .is_some_and(|results| results.collapsed.iter().any(|entry| entry == path))
    }

    fn toggle_group(self, path: &str) {
        let _ = send(&ExplorerSearchGroupToggle {
            path: path.to_string(),
        });
    }

    fn collapse_all(self) {
        let _ = send(&ExplorerSearchCollapseAll);
    }

    fn open(self, path: &str, hit: &ExplorerSearchMatch) {
        let _ = send(&ExplorerSearchOpen {
            path: path.to_string(),
            line: hit.line,
            col: hit.col,
            end_col: hit.end_col,
        });
    }

    fn keys(self) -> Vec<String> {
        let mut keys = Vec::new();
        let results = self.results.read();
        let Some(results) = results.as_ref() else {
            return keys;
        };
        for file in &results.files {
            if results.collapsed.iter().any(|path| path == &file.path) {
                continue;
            }
            for hit in &file.matches {
                keys.push(hit.key(&file.path));
            }
        }
        keys
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TreeRows {
    rows: Signal<Vec<MotionRow>>,
    generation: Signal<u32>,
}

impl TreeRows {
    const MOTION_MS: u32 = 170;

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
        let merged = Self::merge(&current, &next)
            .into_iter()
            .map(|(row, visible)| MotionRow { row, visible })
            .collect();
        rows.set(merged);
        spawn(async move {
            Platform::sleep(0).await;
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

            Platform::sleep(Self::MOTION_MS).await;
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

    fn merge(current: &[TreeRow], next: &[TreeRow]) -> Vec<(TreeRow, bool)> {
        let current_paths: HashSet<&str> = current.iter().map(|row| row.path.as_str()).collect();
        let next_indices: HashMap<&str, usize> = next
            .iter()
            .enumerate()
            .map(|(index, row)| (row.path.as_str(), index))
            .collect();
        let mut exiting_after = vec![Vec::new(); next.len() + 1];
        let mut anchor = 0usize;
        for row in current {
            if let Some(index) = next_indices.get(row.path.as_str()) {
                anchor = index + 1;
            } else {
                exiting_after[anchor].push(row.clone());
            }
        }
        let exiting_count = exiting_after.iter().map(Vec::len).sum::<usize>();
        let mut merged = Vec::with_capacity(next.len() + exiting_count);
        merged.extend(exiting_after[0].drain(..).map(|row| (row, false)));
        for (index, row) in next.iter().cloned().enumerate() {
            let visible = current_paths.contains(row.path.as_str());
            merged.push((row, visible));
            merged.extend(exiting_after[index + 1].drain(..).map(|row| (row, false)));
        }
        merged
    }
}

#[derive(Clone, Copy, PartialEq)]
struct StickySection {
    scroller: Signal<Option<Rc<MountedData>>>,
    list: Signal<Option<Rc<MountedData>>>,
    top: Signal<usize>,
    measuring: Signal<bool>,
    pending: Signal<bool>,
}

impl StickySection {
    const ROW_HEIGHT: f64 = 22.0;
    const DEPTH_MAX: usize = 5;

    fn new(scroller: Signal<Option<Rc<MountedData>>>) -> Self {
        Self {
            scroller,
            list: use_signal(|| None::<Rc<MountedData>>),
            top: use_signal(|| 0usize),
            measuring: use_signal(|| false),
            pending: use_signal(|| false),
        }
    }

    fn ancestor_chain(depths: &[u16], top: usize) -> Vec<usize> {
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
        chain.truncate(Self::DEPTH_MAX);
        chain
    }

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
                let index = (hidden / Self::ROW_HEIGHT).floor().max(0.0) as usize;
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
    fn row_id(path: &str) -> String {
        let hash = path
            .as_bytes()
            .iter()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        format!("explorer-row-{hash:016x}")
    }

    fn outline_key(row: &OutlineRow) -> String {
        format!("{}-{}", row.line, row.name)
    }

    fn caret_symbol(rows: &[OutlineRow], line: u32) -> String {
        let mut innermost = String::new();
        for row in rows {
            if row.contains(line) {
                innermost = Self::outline_key(row);
            }
        }
        innermost
    }

    fn goto_line(line: u32) {
        if let Some(navigation) =
            try_consume_context::<super::outline_navigation::OutlineNavigation>()
        {
            navigation.select(line);
            return;
        }
        let _ = send(&ExplorerGoto {
            path: String::new(),
            line,
        });
    }

    fn schedule(path: String, mut generation: Signal<u32>, reveal: ExplorerReveal) {
        let id = generation().wrapping_add(1);
        generation.set(id);
        spawn(async move {
            Platform::sleep(TreeRows::MOTION_MS + 20).await;
            if generation() != id {
                return;
            }
            let row = Self::row_id(&path);
            ScrollIntoView::nearest(&row);
            if reveal == ExplorerReveal::Requested {
                FocusClaim::new(row).request();
            }
        });
    }

    fn cancel(mut generation: Signal<u32>) {
        let id = generation.peek().wrapping_add(1);
        generation.set(id);
    }

    fn at(self, key: String) {
        let mut current = self.key;
        current.set(key);
    }

    fn reveal(self, key: String) {
        let id = Self::row_id(&key);
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

#[component]
fn ExplorerNoticeBanner(notice: ExplorerNotice) -> Element {
    let Some(message) = notice.message else {
        return rsx! {};
    };
    let mut text = message.clone();
    if notice.ok {
        for (prefix, id) in [
            ("Created folder ", "editor-created-folder"),
            ("Created file ", "editor-created-file"),
            ("Renamed to ", "editor-renamed-to"),
            ("Deleted ", "editor-deleted"),
        ] {
            if let Some(name) = message.strip_prefix(prefix) {
                text = translate_with(id, &[("name", TranslationValue::String(name))]);
                break;
            }
        }
    }
    rsx! {
        button {
            class: if notice.ok {
                "absolute bottom-3 left-3 right-3 z-[997] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-lg bg-success/90 px-3 py-2 text-left text-xs text-white shadow-lg"
            } else {
                "absolute bottom-3 left-3 right-3 z-[997] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-lg bg-red-500/90 px-3 py-2 text-left text-xs text-white shadow-lg"
            },
            onclick: move |_| {
                let _ = send(&ExplorerNoticeDismissRequest);
            },
            "{text}"
        }
    }
}

#[component]
fn ExplorerFilesHeader(
    root_name: String,
    root_path: String,
    current_path: String,
    show_files: bool,
    tree_focus: TreeFocus,
    mut menu: Signal<Option<TreeMenu>>,
) -> Element {
    let root_menu_path = root_path.clone();
    let root_menu_name = root_name.clone();
    let root_refresh_path = root_path.clone();
    rsx! {
        div {
            id: "{TreeFocus::row_id(&root_path)}",
            tabindex: "-1",
            class: if current_path == root_path { "vmux-directory-selection outline-none" } else { "outline-none" },
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let coordinates = e.client_coordinates();
                menu.set(Some(TreeMenu {
                    path: root_menu_path.clone(),
                    name: root_menu_name.clone(),
                    is_dir: true,
                    is_root: true,
                    x: coordinates.x,
                    y: coordinates.y,
                }));
            },
            SectionHeader {
                title: root_name,
                open: show_files,
                on_toggle: EventHandler::new(move |_| {
                    let _ = send(&ExplorerFilesToggle);
                }),
                trailing: rsx! {
                    div {
                        class: "pointer-events-none ml-auto flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 focus-within:pointer-events-auto focus-within:opacity-100 group-hover/section:pointer-events-auto group-hover/section:opacity-100",
                        TitleButton {
                            glyph: TitleGlyph::NewFile,
                            label: translate("editor-new-file"),
                            on_press: move |_| {
                                let focus = tree_focus.key.peek().clone();
                                let _ = send(&ExplorerCreateFilePromptRequest { path: focus });
                            },
                        }
                        TitleButton {
                            glyph: TitleGlyph::NewFolder,
                            label: translate("editor-new-folder"),
                            on_press: move |_| {
                                let focus = tree_focus.key.peek().clone();
                                let _ = send(&ExplorerCreateDirectoryPromptRequest { path: focus });
                            },
                        }
                        TitleButton {
                            glyph: TitleGlyph::Refresh,
                            label: translate("common-refresh"),
                            on_press: move |_| {
                                let _ = send(&ExplorerTreeRefresh {
                                    path: root_refresh_path.clone(),
                                });
                            },
                        }
                        TitleButton {
                            glyph: TitleGlyph::CollapseAll,
                            label: translate("common-collapse-all"),
                            on_press: move |_| {
                                let _ = send(&ExplorerCollapseAll);
                            },
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ExplorerFilesSection(
    visible: Memo<bool>,
    view: ReadSignal<SidebarView>,
    show_files: bool,
    tree: TreeRows,
    tree_focus: TreeFocus,
    files_sticky: StickySection,
    menu: Signal<Option<TreeMenu>>,
) -> Element {
    let tree_state = FileUi::current().use_value(|state| state.explorer.explorer_tree.as_ref());
    let focus_state = FileUi::current().use_value(|state| state.explorer.explorer_focus.as_ref());
    let root_name = tree_state
        .read()
        .as_ref()
        .map(|tree| tree.root_name.clone())
        .unwrap_or_else(|| translate("editor-explorer"));
    let root_path = tree_state
        .read()
        .as_ref()
        .map(|tree| tree.root_path.clone())
        .unwrap_or_default();
    let current_path = focus_state
        .read()
        .as_ref()
        .map(|focus| focus.path.clone())
        .filter(|path| !path.is_empty())
        .or_else(|| {
            tree_state
                .read()
                .as_ref()
                .map(|tree| tree.current_path.clone())
        })
        .unwrap_or_default();
    let root_loading = tree_state.read().as_ref().is_none_or(|tree| tree.loading);
    let rows = (tree.rows)();
    use_effect(move || {
        let _layout = (
            visible(),
            view().is_search(),
            show_files,
            tree.rows.read().len(),
        );
        spawn(async move {
            Platform::sleep(TreeRows::MOTION_MS + 60).await;
            files_sticky.measure();
        });
    });
    let files_body = if show_files {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };
    let mut depths = Vec::with_capacity(rows.len());
    for motion in &rows {
        depths.push(motion.row.depth);
    }
    let mut sticky_files = Vec::new();
    for index in StickySection::ancestor_chain(&depths, (files_sticky.top)()) {
        sticky_files.push(rows[index].row.clone());
    }
    let focused_path = tree_focus.key.cloned();
    let tree_empty = rows.is_empty();
    let tree_unpainted = !rows.iter().any(|row| row.visible);

    rsx! {
        ExplorerFilesHeader {
            root_name,
            root_path,
            current_path: current_path.clone(),
            show_files,
            tree_focus,
            menu,
        }
        StickyFolders {
            rows: sticky_files,
            on_pick: move |path: String| {
                ScrollIntoView::nearest(&TreeFocus::row_id(&path));
            },
        }
        div { class: "{files_body}",
            div {
                class: "{FILE_TREE_LIST_GROUP} min-h-0 overflow-hidden",
                onmounted: move |event: Event<MountedData>| files_sticky.mounted(event.data()),
                if tree_empty || tree_unpainted {
                    if root_loading || !tree_empty {
                        ExplorerTreeSkeleton {}
                    } else {
                        div { class: "flex h-6 items-center px-3 text-foreground/45",
                            {translate("editor-explorer-empty")}
                        }
                    }
                }
                for motion in rows {
                    ExplorerTreeItemRow {
                        key: "{motion.row.path}",
                        motion,
                        current_path: current_path.clone(),
                        focused_path: focused_path.clone(),
                        tree,
                        tree_focus,
                        menu,
                    }
                }
            }
        }
    }
}

#[component]
fn ExplorerTreeItemRow(
    motion: MotionRow,
    current_path: String,
    focused_path: String,
    tree: TreeRows,
    tree_focus: TreeFocus,
    mut menu: Signal<Option<TreeMenu>>,
) -> Element {
    let row = motion.row;
    let path_click = row.path.clone();
    let path_key = row.path.clone();
    let path_prefetch = row.path.clone();
    let path_menu = row.path.clone();
    let name_menu = row.name.clone();
    let is_dir = row.is_dir;
    let accent = TreeRowAccent::resolve(row.path == current_path, row.path == focused_path);
    let pad = (row.depth as u32) * 12 + 8;
    let motion_class = if motion.visible {
        "opacity-100 translate-y-0 transition-[opacity,translate] duration-150 ease-out"
    } else {
        "pointer-events-none opacity-0 -translate-y-1 transition-[opacity,translate] duration-150 ease-out"
    };
    rsx! {
        div { class: "{motion_class}",
            div { class: "min-h-0 overflow-hidden",
                div {
                    id: "{TreeFocus::row_id(&row.path)}",
                    tabindex: "-1",
                    class: "{FILE_TREE_ROW_BASE} {accent.classes()}",
                    style: "padding-left:{pad}px;",
                    title: "{row.path}",
                    onmouseenter: move |_| {
                        if is_dir {
                            let _ = send(&ExplorerTreePrefetch {
                                path: path_prefetch.clone(),
                            });
                        }
                    },
                    oncontextmenu: move |e: Event<MouseData>| {
                        e.prevent_default();
                        e.stop_propagation();
                        let coordinates = e.client_coordinates();
                        tree_focus.at(path_menu.clone());
                        menu.set(Some(TreeMenu {
                            path: path_menu.clone(),
                            name: name_menu.clone(),
                            is_dir,
                            is_root: false,
                            x: coordinates.x,
                            y: coordinates.y,
                        }));
                    },
                    onkeydown: move |e: Event<KeyboardData>| match e.key() {
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
                                let _ = send(&ExplorerTreeToggle {
                                    path: path_key.clone(),
                                });
                            } else {
                                let _ = send(&FileOpenEvent {
                                    path: path_key.clone(),
                                });
                            }
                        }
                        _ => {}
                    },
                    onclick: move |_| {
                        tree_focus.at(path_click.clone());
                        if is_dir {
                            let _ = send(&ExplorerTreeToggle {
                                path: path_click.clone(),
                            });
                        } else {
                            let _ = send(&FileOpenEvent {
                                path: path_click.clone(),
                            });
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

#[component]
fn ExplorerOutlineSection(
    visible: Memo<bool>,
    view: ReadSignal<SidebarView>,
    caret_line: u32,
    show_outline: bool,
    outline_focus: TreeFocus,
    outline_sticky: StickySection,
) -> Element {
    let outline_state = FileUi::current().use_value(|state| state.explorer.outline.as_ref());
    let outline = use_memo(move || {
        outline_state
            .read()
            .as_ref()
            .map(|outline| outline.items.clone())
            .unwrap_or_default()
    });
    use_effect(move || {
        let _layout = (
            visible(),
            view().is_search(),
            show_outline,
            outline.read().len(),
        );
        spawn(async move {
            Platform::sleep(TreeRows::MOTION_MS + 60).await;
            outline_sticky.measure();
        });
    });
    let caret_symbol = TreeFocus::caret_symbol(&outline.read(), caret_line);
    let followed_symbol = caret_symbol.clone();
    use_effect(use_reactive!(|(followed_symbol, show_outline)| {
        if visible() && show_outline && !view().is_search() && !followed_symbol.is_empty() {
            ScrollIntoView::nearest_vertical(&TreeFocus::row_id(&followed_symbol));
        }
    }));
    let outline_body = if show_outline {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };
    let items = outline();
    let mut depths = Vec::with_capacity(items.len());
    for row in &items {
        depths.push(row.depth);
    }
    let mut sticky_outline = Vec::new();
    for index in StickySection::ancestor_chain(&depths, (outline_sticky.top)()) {
        sticky_outline.push(items[index].clone());
    }
    let focused_symbol = outline_focus.key.cloned();
    let mut keys = Vec::with_capacity(items.len());
    for row in &items {
        keys.push(TreeFocus::outline_key(row));
    }

    rsx! {
        SectionHeader {
            title: translate("editor-outline"),
            open: show_outline,
            on_toggle: EventHandler::new(move |_| {
                let _ = send(&ExplorerOutlineToggle);
            }),
        }
        StickyOutline {
            rows: sticky_outline,
            on_pick: move |line: u32| TreeFocus::goto_line(line),
        }
        div { class: "{outline_body}",
            div {
                class: "{FILE_TREE_LIST_GROUP} min-h-0 overflow-hidden",
                onmounted: move |event: Event<MountedData>| outline_sticky.mounted(event.data()),
                for row in items {
                    ExplorerOutlineItemRow {
                        key: "{TreeFocus::outline_key(&row)}",
                        row,
                        caret_symbol: caret_symbol.clone(),
                        focused_symbol: focused_symbol.clone(),
                        keys: keys.clone(),
                        outline_focus,
                    }
                }
            }
        }
    }
}

#[component]
fn ExplorerOutlineItemRow(
    row: OutlineRow,
    caret_symbol: String,
    focused_symbol: String,
    keys: Vec<String>,
    outline_focus: TreeFocus,
) -> Element {
    let line = row.line;
    let key = TreeFocus::outline_key(&row);
    let key_step = key.clone();
    let accent = TreeRowAccent::resolve(key == caret_symbol, key == focused_symbol);
    let pad = (row.depth as u32) * 12 + 20;
    rsx! {
        div {
            id: "{TreeFocus::row_id(&key)}",
            tabindex: "-1",
            class: "{FILE_TREE_ROW_BASE} {accent.classes()}",
            style: "padding-left:{pad}px;",
            onkeydown: move |e: Event<KeyboardData>| {
                let forward = match e.key() {
                    Key::ArrowDown => true,
                    Key::ArrowUp => false,
                    Key::Enter => {
                        e.prevent_default();
                        e.stop_propagation();
                        TreeFocus::goto_line(line);
                        return;
                    }
                    _ => return,
                };
                e.prevent_default();
                e.stop_propagation();
                outline_focus.step(&keys, forward);
            },
            onclick: move |_| {
                outline_focus.at(key_step.clone());
                TreeFocus::goto_line(line);
            },
            TreeIndentGuides { depth: row.depth, base: 26 }
            OutlineGlyph { kind: row.kind }
            span { class: "truncate", "{row.name}" }
        }
    }
}

#[component]
fn ExplorerContextMenu(current: TreeMenu, mut menu: Signal<Option<TreeMenu>>) -> Element {
    rsx! {
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
                            let _ = send(&ExplorerCreateFilePromptRequest { path: path.clone() });
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
                            let _ = send(&ExplorerCreateDirectoryPromptRequest {
                                path: path.clone(),
                            });
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
                            let _ = send(&ExplorerTreeRefresh { path: path.clone() });
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
                            let _ = send(&ExplorerRenamePromptRequest {
                                path: path.clone(),
                                name: name.clone(),
                            });
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
                            let _ = send(&ExplorerDeletePromptRequest {
                                path: path.clone(),
                                name: name.clone(),
                            });
                            menu.set(None);
                        }
                    },
                    {translate("common-delete")}
                }
            }
        }
    }
}

#[component]
fn ExplorerPromptDialog(current: ExplorerPromptState) -> Element {
    let ime = use_ime_guard();
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: Callback::new(move |open: bool| {
                if !open {
                    let _ = send(&ExplorerPromptDismissRequest);
                }
            }),
            attributes: vec![],
            DialogContent { class: "max-w-[360px] p-4", attributes: vec![],
                DialogTitle { attributes: vec![], "{translate(&current.title_message_id)}" }
                if current.destructive {
                    DialogDescription { class: "text-xs leading-relaxed", attributes: vec![],
                        {translate_with("editor-delete-confirm", &[("name", TranslationValue::String(&current.name))])}
                    }
                } else {
                    input {
                        class: "w-full rounded-md border border-border bg-foreground/[0.04] px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary/50",
                        autofocus: true,
                        value: "{current.draft}",
                        oninput: move |e| {
                            let _ = send(&ExplorerPromptDraftRequest { draft: e.value() });
                        },
                        oncompositionstart: move |_| ime.start(),
                        oncompositionend: move |_| ime.commit(),
                        onkeydown: move |e: Event<KeyboardData>| {
                            e.stop_propagation();
                            if ime.swallows(&e) {
                                return;
                            }
                            if e.key() == Key::Enter {
                                e.prevent_default();
                                let _ = send(&ExplorerPromptSubmitRequest);
                            } else if e.key() == Key::Escape {
                                let _ = send(&ExplorerPromptDismissRequest);
                            }
                        },
                    }
                }
                div { class: "flex justify-end gap-2",
                    Button {
                        size: ButtonSize::Xs,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            let _ = send(&ExplorerPromptDismissRequest);
                        },
                        {translate("common-cancel")}
                    }
                    Button {
                        size: ButtonSize::Xs,
                        variant: if current.destructive {
                            ButtonVariant::Destructive
                        } else {
                            ButtonVariant::Primary
                        },
                        onclick: move |_| {
                            let _ = send(&ExplorerPromptSubmitRequest);
                        },
                        {if current.destructive { translate("common-delete") } else { translate("common-save") }}
                    }
                }
            }
        }
    }
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
fn SectionHeader(
    title: String,
    open: bool,
    on_toggle: EventHandler<()>,
    #[props(default = rsx! {})] trailing: Element,
) -> Element {
    rsx! {
        div {
            class: "group/section flex h-7 items-center gap-1 px-3 cursor-default text-[11px] font-bold uppercase tracking-wide text-foreground/70 transition-colors hover:text-foreground",
            onclick: move |_| on_toggle.call(()),
            Chevron { expanded: open, loading: false }
            span { class: "min-w-0 flex-1 truncate", "{title}" }
            {trailing}
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TitleGlyph {
    NewFile,
    NewFolder,
    Refresh,
    CollapseAll,
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
            Self::Clear => &[
                "M20 12a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z",
                "m6.34 6.34 11.32 11.32",
            ],
        }
    }
}

#[component]
fn TitleButton(glyph: TitleGlyph, label: String, on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "flex h-5 w-5 shrink-0 items-center justify-center rounded text-foreground/55 outline-none transition-colors hover:bg-foreground/[0.12] hover:text-foreground focus-visible:bg-foreground/[0.12] focus-visible:text-foreground",
            title: "{label}",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                on_press.call(());
            },
            TitleButtonIcon { glyph }
        }
    }
}

#[component]
fn TitleButtonIcon(glyph: TitleGlyph) -> Element {
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
            div { class: "absolute inset-x-0 top-0 {FILE_TREE_STICKY_SURFACE}", {children} }
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
                StickyOutlineRow { key: "{TreeFocus::outline_key(&row)}", row, on_pick }
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
fn SearchView(view: ReadSignal<SidebarView>, search: SearchState) -> Element {
    let hit_focus = TreeFocus {
        key: use_signal(String::new),
    };

    let active = view().is_search();
    let focused = hit_focus.key.cloned();
    let current = search.results.read().as_ref().cloned().unwrap_or_default();
    let query = current.query.clone();
    let mut summary = None;
    let mut root = String::new();
    let mut files = Vec::new();
    let mut opened = String::new();
    if !current.query.is_empty() && !current.files.is_empty() {
        summary = Some(SearchSummary::from(&current).text());
        root = current.root.clone();
        files = current.files.clone();
        opened.clone_from(&current.opened);
    }

    rsx! {
        div { class: if active { "contents" } else { "hidden" },
            div { class: "flex h-9 shrink-0 items-center gap-1.5 pl-2 pr-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                span { class: "truncate", {SidebarView::Search.title()} }
                div {
                    class: "pointer-events-none ml-auto flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 focus-within:pointer-events-auto focus-within:opacity-100 group-hover/panel:pointer-events-auto group-hover/panel:opacity-100",
                    TitleButton {
                        glyph: TitleGlyph::Refresh,
                        label: translate("common-refresh"),
                        on_press: move |_| search.run(),
                    }
                    TitleButton {
                        glyph: TitleGlyph::Clear,
                        label: translate("editor-search-clear"),
                        on_press: move |_| search.close(),
                    }
                    TitleButton {
                        glyph: TitleGlyph::CollapseAll,
                        label: translate("common-collapse-all"),
                        on_press: move |_| search.collapse_all(),
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto pb-4",
                if let Some(summary) = summary {
                    div { class: "px-3 py-1 text-[10px] text-muted-foreground", "{summary}" }
                    div { class: "{FILE_TREE_LIST_GROUP}",
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
                } else if !query.is_empty() {
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
    let directory = SearchSummary::relative_parent(&root, &file.path);
    let body = if collapsed {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-150 ease-out"
    } else {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-150 ease-out"
    };
    rsx! {
        div {
            div {
                class: "{FILE_TREE_STICKY_SURFACE} sticky top-0 z-[11] flex h-[22px] cursor-default items-center gap-1 px-1 text-foreground/85 transition-colors duration-100 hover:bg-foreground/[0.08]",
                title: "{file.path}",
                onclick: move |_| search.toggle_group(&path_toggle),
                Chevron { expanded: !collapsed, loading: false }
                {rsx! { TypeIcon { path: file.path.clone(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" } }}
                span { class: "shrink-0 truncate", {SearchSummary::file_name(&file.path)} }
                if !directory.is_empty() {
                    span { class: "min-w-0 truncate text-[10px] text-muted-foreground", "{directory}" }
                }
                span {
                    class: "ml-auto shrink-0 rounded-full bg-foreground/[0.12] px-1.5 text-[10px] tabular-nums text-muted-foreground",
                    {SearchSummary::count_for_file(&file)}
                }
            }
            div { class: "{body}",
                div { class: "min-h-0 overflow-hidden",
                    for hit in file.matches.clone() {
                        SearchHitRow {
                            key: "{hit.key(&file.path)}",
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
    let key = hit.key(&path);
    let accent = TreeRowAccent::resolve(key == opened, key == focused);
    let span = PreviewSpan::split(&hit.preview, hit.col, hit.end_col);
    let key_click = key.clone();
    let path_click = path.clone();
    let hit_click = hit.clone();
    let path_key = path.clone();
    let hit_key = hit.clone();
    rsx! {
        div {
            id: "{TreeFocus::row_id(&key)}",
            tabindex: "-1",
            class: "{FILE_TREE_ROW_BASE} {accent.classes()} pl-6",
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
                span { class: "rounded-sm bg-primary/30 text-foreground", "{span.hit}" }
                span { class: "text-muted-foreground", "{span.after}" }
            }
        }
    }
}

#[component]
fn SearchInput(search: SearchState, view: ReadSignal<SidebarView>) -> Element {
    let ime = use_ime_guard();
    let (query, regex, case_sensitive, whole_word) = {
        let results = search.results.read();
        match results.as_ref() {
            Some(results) => (
                results.query.clone(),
                results.regex,
                results.case_sensitive,
                results.whole_word,
            ),
            None => (String::new(), false, false, false),
        }
    };
    rsx! {
        div {
            class: "contents",
            oncompositionstart: move |_| ime.start(),
            oncompositionend: move |_| ime.commit(),
            vmux_ui::components::manager::ManagerSearchRow { ManagerSearch {
                id: Some(SEARCH_INPUT_ID.to_string()),
                value: query,
                placeholder: translate("editor-find-in-files-placeholder"),
                onsearch: move |query: String| {
                    let searching = !query.trim().is_empty();
                    search.draft(query, regex, case_sensitive, whole_word);
                    if searching != view().is_search() {
                        let _ = send(&ExplorerPanelViewSet { search: searching });
                    }
                },
                onkeydown: move |event: KeyboardEvent| {
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
                            search.close();
                        }
                        _ => {}
                    }
                },
                suffix: rsx! { SearchOptions { search } },
            } }
        }
    }
}

#[component]
fn SearchOptions(search: SearchState) -> Element {
    let mut open = use_signal(|| false);
    let (query, regex, case_sensitive, whole_word) = {
        let results = search.results.read();
        match results.as_ref() {
            Some(results) => (
                results.query.clone(),
                results.regex,
                results.case_sensitive,
                results.whole_word,
            ),
            None => (String::new(), false, false, false),
        }
    };
    let case_query = query.clone();
    let whole_query = query.clone();
    let active = regex || case_sensitive || whole_word;
    rsx! {
        Dropdown {
            open: open(),
            keyboard_navigation: true,
            on_dismiss: move |_| open.set(false),
            Button {
                aria_label: translate("common-filter"),
                aria_haspopup: "menu",
                aria_expanded: open(),
                "data-vmux-dropdown-trigger": "true",
                size: ButtonSize::Icon,
                variant: ButtonVariant::Ghost,
                class: if active { "size-5 text-primary" } else { "size-5" },
                onmousedown: move |event: MouseEvent| event.prevent_default(),
                onclick: move |_| open.set(!open()),
                LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5" }
            }
            if open() {
                DropdownPopup { align: DropdownAlign::End, class: "min-w-44 p-1",
                    DropdownOption {
                        label: translate("editor-find-case"), selected: case_sensitive,
                        onclick: move |_| search.draft(case_query.clone(), regex, !case_sensitive, whole_word),
                        trailing: rsx! { if case_sensitive { LineIconView { icon: LineIcon::Check, class: "size-3" } } },
                    }
                    DropdownOption {
                        label: translate("editor-find-whole-word"), selected: whole_word,
                        onclick: move |_| search.draft(whole_query.clone(), regex, case_sensitive, !whole_word),
                        trailing: rsx! { if whole_word { LineIconView { icon: LineIcon::Check, class: "size-3" } } },
                    }
                    DropdownOption {
                        label: translate("editor-find-regex"), selected: regex,
                        onclick: move |_| search.draft(query.clone(), !regex, case_sensitive, whole_word),
                        trailing: rsx! { if regex { LineIconView { icon: LineIcon::Check, class: "size-3" } } },
                    }
                }
            }
        }
    }
}

#[component]
fn ExplorerTreeSkeleton() -> Element {
    rsx! {
        div { aria_busy: true, class: "flex flex-col gap-1 px-3 py-1",
            for width in ["w-7/12", "w-10/12", "w-8/12", "w-9/12", "w-6/12"] {
                div { class: "flex h-5 items-center gap-2",
                    Skeleton { class: "size-3 shrink-0 rounded-sm bg-foreground/[0.05]" }
                    Skeleton { class: "h-3 {width} bg-foreground/[0.055]" }
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
        span { class: "inline-block w-6 shrink-0 text-center text-[9px] font-semibold text-primary/80", "{label}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFixture;

    impl TestFixture {
        fn span(name: &str, line: u32, end_line: u32, depth: u16) -> OutlineRow {
            OutlineRow {
                name: name.to_string(),
                kind: 12,
                line,
                end_line,
                depth,
            }
        }

        fn hit_file(path: &str, lines: &[u32], capped: bool) -> ExplorerSearchFile {
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
    fn the_caret_marks_the_innermost_symbol_holding_it_and_nothing_above_the_first() {
        let rows = [
            TestFixture::span("first", 0, 8, 0),
            TestFixture::span("nested", 2, 6, 1),
            TestFixture::span("second", 20, 30, 0),
        ];
        assert_eq!(TreeFocus::caret_symbol(&rows, 4), "2-nested");
        assert_eq!(TreeFocus::caret_symbol(&rows, 7), "0-first");
        assert_eq!(TreeFocus::caret_symbol(&rows, 12), "");
        assert_eq!(TreeFocus::caret_symbol(&[], 3), "");
    }

    #[test]
    fn a_symbol_with_no_extent_holds_the_caret_until_the_next_one_starts() {
        let rows = [
            TestFixture::span("open", 0, OutlineRow::OPEN_END, 0),
            TestFixture::span("bounded", 4, 6, 1),
            TestFixture::span("later", 40, OutlineRow::OPEN_END, 0),
        ];
        assert_eq!(TreeFocus::caret_symbol(&rows, 5), "4-bounded");
        assert_eq!(TreeFocus::caret_symbol(&rows, 20), "0-open");
        assert_eq!(TreeFocus::caret_symbol(&rows, 900), "40-later");
    }

    #[test]
    fn the_highlight_lands_on_the_match_when_the_line_holds_astral_characters() {
        let span = PreviewSpan::split("let \u{1F600} = \"needle\";", 10, 16);

        assert_eq!(span.before, "let \u{1F600} = \"");
        assert_eq!(span.hit, "needle");
        assert_eq!(span.after, "\";");
    }

    #[test]
    fn a_match_past_the_truncated_preview_highlights_nothing_and_loses_no_text() {
        let span = PreviewSpan::split("short", 40, 46);

        assert_eq!(span.before, "short");
        assert!(span.hit.is_empty());
        assert!(span.after.is_empty());
    }

    #[test]
    fn a_total_reads_as_a_floor_when_either_the_sweep_or_any_file_was_capped() {
        let whole = ExplorerSearchEvent {
            root: "/r".to_string(),
            query: "needle".to_string(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
            files: vec![
                TestFixture::hit_file("/r/a.rs", &[1, 2], false),
                TestFixture::hit_file("/r/b.rs", &[7], false),
            ],
            capped: false,
            collapsed: Vec::new(),
            opened: String::new(),
        };
        assert_eq!(
            SearchSummary::from(&whole),
            SearchSummary {
                total: 3,
                files: 2,
                floor: false,
            }
        );

        let mut per_file = whole.clone();
        per_file.files[1].capped = true;
        assert!(SearchSummary::from(&per_file).floor);

        let mut swept = whole.clone();
        swept.capped = true;
        assert!(SearchSummary::from(&swept).floor);
    }

    #[test]
    fn a_group_shows_the_folder_under_the_root_and_nothing_for_a_root_level_file() {
        assert_eq!(
            SearchSummary::relative_parent("/r", "/r/src/host/lib.rs"),
            "src/host"
        );
        assert_eq!(SearchSummary::relative_parent("/r", "/r/lib.rs"), "");
        assert_eq!(
            SearchSummary::relative_parent("/r", "/elsewhere/lib.rs"),
            "/elsewhere"
        );
    }

    #[test]
    fn chain_is_the_enclosing_folders_outermost_first() {
        let depths = [0, 1, 2, 3, 3, 1];
        assert_eq!(StickySection::ancestor_chain(&depths, 4), vec![0, 1, 2]);
        assert_eq!(StickySection::ancestor_chain(&depths, 5), vec![0]);
    }

    #[test]
    fn chain_skips_siblings_and_deeper_rows_above() {
        let depths = [0, 1, 2, 2, 1, 2];
        assert_eq!(StickySection::ancestor_chain(&depths, 5), vec![0, 4]);
    }

    #[test]
    fn chain_is_empty_at_the_first_row_and_past_the_end() {
        let depths = [0, 1, 2];
        assert!(StickySection::ancestor_chain(&depths, 0).is_empty());
        assert!(StickySection::ancestor_chain(&depths, 3).is_empty());
        assert!(StickySection::ancestor_chain(&[], 0).is_empty());
    }

    #[test]
    fn chain_keeps_the_outermost_levels_within_the_cap() {
        let depths: Vec<u16> = (0..12u16).collect();
        let chain = StickySection::ancestor_chain(&depths, 11);
        assert_eq!(chain.len(), StickySection::DEPTH_MAX);
        assert_eq!(chain, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn tree_motion_merge_is_linear_ordered_and_marks_entries() {
        let row = |path: &str| TreeRow {
            name: path.to_string(),
            path: path.to_string(),
            depth: 0,
            is_dir: false,
            expanded: false,
            loading: false,
        };
        let current = vec![row("a"), row("b"), row("c"), row("d")];
        let next = vec![row("a"), row("x"), row("d")];
        let merged = TreeRows::merge(&current, &next);
        assert_eq!(
            merged
                .iter()
                .map(|(row, visible)| (row.path.as_str(), *visible))
                .collect::<Vec<_>>(),
            vec![
                ("a", true),
                ("b", false),
                ("c", false),
                ("x", false),
                ("d", true)
            ]
        );
    }

    #[test]
    fn a_tree_arriving_into_nothing_is_entirely_hidden() {
        let row = |path: &str| TreeRow {
            name: path.to_string(),
            path: path.to_string(),
            depth: 0,
            is_dir: false,
            expanded: false,
            loading: false,
        };

        let merged = TreeRows::merge(&[], &[row("a"), row("b")]);

        assert!(merged.iter().all(|(_, visible)| !visible));
    }
}
