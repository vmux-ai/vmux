use std::collections::HashMap;

use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use vmux_core::event::FileDirEntry;
use vmux_ui::platform::now_millis;

use crate::page::{
    EntryVisual, PANE_CLASS, Preview, PreviewPane, apply_dir, open_path, parent_of,
    request_preview, row_class, visible_entries,
};

#[component]
pub(crate) fn DirColumns(window: DirWindow) -> Element {
    let clicks = DirClick {
        window,
        pending: use_signal(|| Option::<PendingOpen>::None),
    };
    let show_hidden = (window.show_hidden)();
    let cur_basename = window.basename();
    let selected = (window.selected)();
    let thumbs = (window.thumbs)();

    rsx! {
        div {
            class: "grid min-h-0 flex-1 gap-3 p-3",
            style: "grid-template-columns: minmax(8rem,14rem) minmax(10rem,1fr) minmax(12rem,1.3fr);",
            onclick: move |event: Event<MouseData>| clicks.pane(event.client_coordinates()),

            div { class: PANE_CLASS,
                for e in window.parents() {
                    {
                        let entry = e.clone();
                        rsx! {
                            div {
                                key: "{e.path}",
                                class: if e.name == cur_basename { PARENT_CURRENT_CLASS } else { PARENT_CLASS },
                                title: "{e.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.row(Column::Parent, entry.clone(), event.client_coordinates());
                                },
                                EntryVisual { entry: e.clone(), thumb: None }
                                span { class: "truncate text-xs", "{e.name}" }
                            }
                        }
                    }
                }
            }

            div { class: PANE_CLASS,
                for (i, e) in window.entries().into_iter().enumerate() {
                    {
                        let entry = e.clone();
                        let opened = e.clone();
                        rsx! {
                            div {
                                key: "{e.path}",
                                id: "dir-row-{i}",
                                class: row_class(i == selected),
                                title: "{e.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.row(Column::Current(i), entry.clone(), event.client_coordinates());
                                },
                                ondoubleclick: move |_| window.open(&opened),
                                EntryVisual { entry: e.clone(), thumb: thumbs.get(&e.path).cloned() }
                                span { class: "truncate text-xs", "{e.name}" }
                            }
                        }
                    }
                }
            }

            match window.children() {
                Some(children) => rsx! {
                    div { class: PANE_CLASS,
                        for e in visible_entries(&children, show_hidden) {
                            {
                                let entry = e.clone();
                                rsx! {
                                    div {
                                        key: "{e.path}",
                                        class: row_class(false),
                                        title: "{e.path}",
                                        onclick: move |event: Event<MouseData>| {
                                            event.stop_propagation();
                                            clicks.row(Column::Child, entry.clone(), event.client_coordinates());
                                        },
                                        EntryVisual { entry: e.clone(), thumb: None }
                                        span { class: "truncate text-xs", "{e.name}" }
                                    }
                                }
                            }
                        }
                    }
                },
                None => rsx! {
                    div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-foreground/[0.02] p-4 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                        PreviewPane { preview: (window.preview)() }
                    }
                },
            }
        }
    }
}

const PARENT_CLASS: &str = "flex items-center gap-2 rounded-md px-2 py-1 text-foreground/45 cursor-default transition-colors hover:bg-foreground/[0.04]";
const PARENT_CURRENT_CLASS: &str = "flex items-center gap-2 rounded-md bg-cyan-400/10 px-2 py-1 text-foreground cursor-default shadow-[inset_2px_0_0_0_rgba(34,211,238,0.6)]";

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct DirWindow {
    pub dir_entries: Signal<Vec<FileDirEntry>>,
    pub parent_entries: Signal<Vec<FileDirEntry>>,
    pub path: Signal<String>,
    pub parent_path: Signal<String>,
    pub selected: Signal<usize>,
    pub preview: Signal<Preview>,
    pub thumbs: Signal<HashMap<String, String>>,
    pub came_from: Signal<String>,
    pub back_dir: Signal<Option<String>>,
    pub show_hidden: Signal<bool>,
}

impl DirWindow {
    fn basename(self) -> String {
        (self.path)()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string()
    }

    fn entries(self) -> Vec<FileDirEntry> {
        visible_entries(&self.dir_entries.read(), (self.show_hidden)())
    }

    fn parents(self) -> Vec<FileDirEntry> {
        visible_entries(&self.parent_entries.read(), (self.show_hidden)())
    }

    fn children(self) -> Option<Vec<FileDirEntry>> {
        match &*self.preview.read() {
            Preview::Dir(entries) => Some(entries.clone()),
            _ => None,
        }
    }

    fn selection(self) -> Option<FileDirEntry> {
        self.entries().get((self.selected)()).cloned()
    }

    fn select(mut self, index: usize, path: String) {
        self.selected.set(index);
        request_preview(path);
    }

    fn ascend(mut self, target: String) -> bool {
        let up = (self.parent_path)();
        if up.is_empty() {
            return false;
        }
        let entries = self.parent_entries.read().clone();
        if entries.is_empty() {
            return false;
        }
        self.came_from.set(target.clone());
        self.parent_path.set(parent_of(&up));
        apply_dir(
            self.dir_entries,
            self.parent_entries,
            self.path,
            self.selected,
            self.preview,
            self.thumbs,
            (self.show_hidden)(),
            entries,
            Vec::new(),
            up.clone(),
            Some(target),
        );
        open_path(up);
        true
    }

    fn descend(mut self, target: String) -> bool {
        let Some(into) = self.selection() else {
            return false;
        };
        if !into.is_dir {
            return false;
        }
        let Some(children) = self.children() else {
            return false;
        };
        let siblings = self.dir_entries.read().clone();
        self.came_from.set(target.clone());
        self.parent_path.set(parent_of(&into.path));
        apply_dir(
            self.dir_entries,
            self.parent_entries,
            self.path,
            self.selected,
            self.preview,
            self.thumbs,
            (self.show_hidden)(),
            children,
            siblings,
            into.path.clone(),
            Some(target),
        );
        open_path(into.path);
        true
    }

    fn open(mut self, entry: &FileDirEntry) {
        if !entry.is_dir {
            self.back_dir.set(Some(parent_of(&entry.path)));
        }
        open_path(entry.path.clone());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Parent,
    Current(usize),
    Child,
}

const DOUBLE_CLICK_MS: i64 = 500;
const DOUBLE_CLICK_SLOP_PX: f64 = 6.0;

#[derive(Clone, PartialEq)]
struct PendingOpen {
    entry: FileDirEntry,
    at: i64,
    origin: (f64, f64),
}

impl PendingOpen {
    fn claims(&self, at: ClientPoint) -> bool {
        now_millis() - self.at < DOUBLE_CLICK_MS
            && (at.x - self.origin.0).abs() <= DOUBLE_CLICK_SLOP_PX
            && (at.y - self.origin.1).abs() <= DOUBLE_CLICK_SLOP_PX
    }
}

#[derive(Clone, Copy)]
struct DirClick {
    window: DirWindow,
    pending: Signal<Option<PendingOpen>>,
}

impl DirClick {
    fn row(mut self, column: Column, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        let shifted = match column {
            Column::Current(index) => {
                self.window.select(index, entry.path.clone());
                false
            }
            Column::Parent => self.window.ascend(entry.path.clone()),
            Column::Child => self.window.descend(entry.path.clone()),
        };
        if shifted {
            self.pending.set(Some(PendingOpen {
                entry,
                at: now_millis(),
                origin: (at.x, at.y),
            }));
        }
    }

    fn pane(mut self, at: ClientPoint) {
        self.take_open(at);
    }

    fn take_open(&mut self, at: ClientPoint) -> bool {
        let pending = (*self.pending.peek()).clone();
        let Some(pending) = pending else {
            return false;
        };
        if !pending.claims(at) {
            return false;
        }
        self.pending.set(None);
        self.window.open(&pending.entry);
        true
    }
}
