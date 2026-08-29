#![allow(non_snake_case)]

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use crate::page::use_ime_guard;
use crate::page_model::merge_tree_motion_rows;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::tree_row::SIDEBAR_TREE_ROW_FOCUS;
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

fn open_search_match(result: ExplorerSearchMatch) {
    let _ = send(&ExplorerSearchOpen {
        path: result.path,
        line: result.line,
        col: result.col,
        end_col: result.end_col,
    });
}

fn search_result_path(root: &str, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
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

#[derive(Clone, Copy)]
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
fn StickyFolders(rows: Vec<TreeRow>, on_pick: EventHandler<String>) -> Element {
    if rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "sticky top-0 z-[12] h-0",
            div { class: "absolute inset-x-0 top-0 border-b border-foreground/10 bg-background/95 backdrop-blur",
                for row in rows {
                    StickyFolderRow { key: "{row.path}", row, on_pick }
                }
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
        div { class: "sticky top-0 z-[12] h-0",
            div { class: "absolute inset-x-0 top-0 border-b border-foreground/10 bg-background/95 backdrop-blur",
                for row in rows {
                    StickyOutlineRow { key: "{OutlineKey::of(&row)}", row, on_pick }
                }
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
pub fn ExplorerPanel(visible: Signal<bool>) -> Element {
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
    let mut search = use_signal(|| None::<ExplorerSearchEvent>);
    let mut show_open = use_signal(|| true);
    let mut show_search = use_signal(|| true);
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
    let _search = use_listener::<ExplorerSearchEvent, _>(EXPLORER_SEARCH_EVENT, move |e| {
        search.set(Some(e));
        show_search.set(true);
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
    let search_body = if show_search() {
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
            show_open(),
            show_search(),
            show_files(),
            show_outline(),
            open_editors.read().len(),
            rows.read().len(),
            outline.read().len(),
            search.read().as_ref().map_or(0, |it| it.matches.len()),
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
    let tree_empty = rows.read().is_empty();

    rsx! {
        div { class: "group/panel relative flex h-full w-full flex-col overflow-hidden bg-foreground/[0.04] font-sans text-xs text-foreground select-none",
            div { class: "flex h-9 shrink-0 items-center gap-2 pl-4 pr-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                span { class: "truncate", {translate("editor-explorer")} }
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
                class: "group/tree min-h-0 flex-1 overflow-y-auto pb-4",
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

                if let Some(results) = search() {
                    SectionHeader { title: "Search".to_string(), open: show_search, on_toggle: EventHandler::new(move |_| show_search.set(!show_search())) }
                    div { class: "{search_body}",
                        div { class: "min-h-0 overflow-hidden pb-1",
                            div { class: "mx-2 mb-1 flex h-7 items-center gap-2 rounded-md bg-foreground/[0.06] px-2 text-foreground/85 ring-1 ring-inset ring-foreground/10",
                                svg {
                                    class: "h-3.5 w-3.5 shrink-0 text-cyan-400",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "1.8",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    circle { cx: "11", cy: "11", r: "8" }
                                    path { d: "m21 21-4.35-4.35" }
                                }
                                span { class: "min-w-0 flex-1 truncate font-mono", "{results.query}" }
                                span { class: "shrink-0 text-[10px] tabular-nums text-muted-foreground", "{results.matches.len()}" }
                            }
                            for result in results.matches.clone() {
                                {
                                    let display_path = search_result_path(&results.root, &result.path);
                                    let open_result = result.clone();
                                    rsx! {
                                        button {
                                            key: "{result.path}:{result.line}:{result.col}",
                                            class: "flex w-full min-w-0 flex-col gap-0.5 px-3 py-1 text-left text-foreground/75 transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
                                            title: "{result.path}:{result.line}",
                                            onclick: move |_| open_search_match(open_result.clone()),
                                            span { class: "flex min-w-0 items-baseline gap-1.5",
                                                span { class: "truncate text-[11px]", "{display_path}" }
                                                span { class: "shrink-0 text-[10px] tabular-nums text-cyan-400/80", "{result.line}" }
                                            }
                                            span { class: "w-full truncate font-mono text-[10px] text-muted-foreground", "{result.preview}" }
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
                        class: "min-h-0 overflow-hidden",
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
                                let active = row.path == current_path();
                                let focused = row.path == focused_path;
                                let focus_class = if focused { SIDEBAR_TREE_ROW_FOCUS } else { "" };
                                let row_class = if active {
                                    "relative flex h-[22px] items-center gap-1 px-1 cursor-default bg-cyan-400/12 text-foreground outline-none transition-colors duration-100"
                                } else {
                                    "relative flex h-[22px] items-center gap-1 px-1 cursor-default text-foreground/80 outline-none transition-colors duration-100 hover:bg-foreground/[0.08]"
                                };
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
                                                class: "{row_class} {focus_class}",
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
                        class: "min-h-0 overflow-hidden",
                        onmounted: move |event: Event<MountedData>| outline_sticky.mounted(event.data()),
                        for s in outline() {
                            {
                                let line = s.line;
                                let key = OutlineKey::of(&s);
                                let key_step = key.clone();
                                let focus_class = if key == focused_symbol { SIDEBAR_TREE_ROW_FOCUS } else { "" };
                                let pad = (s.depth as u32) * 12 + 20;
                                rsx! {
                                    div {
                                        key: "{key}",
                                        id: "{tree_row_id(&key)}",
                                        tabindex: "-1",
                                        class: "relative flex h-[22px] items-center gap-1 px-1 cursor-default text-foreground/75 outline-none transition-colors duration-100 hover:bg-foreground/[0.08] {focus_class}",
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
