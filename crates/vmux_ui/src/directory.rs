use std::collections::HashMap;

use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use vmux_core::event::FileDirEntry;

use crate::file_icon::TypeIcon;
use crate::focus::FocusClaim;
use crate::platform::now_millis;
use crate::scroll::ScrollIntoView;
use crate::util::cn;

#[derive(Clone, PartialEq)]
pub enum DirectoryNavigatorEvent {
    Select { index: usize, entry: FileDirEntry },
    Ascend { target: String },
    Descend { target: String },
    Open { entry: FileDirEntry },
    ToggleHidden,
}

#[component]
pub fn DirectoryNavigator(
    path: String,
    parent_entries: Vec<FileDirEntry>,
    entries: Vec<FileDirEntry>,
    children: Option<Vec<FileDirEntry>>,
    selected: usize,
    thumbs: HashMap<String, String>,
    show_hidden: bool,
    preview: Element,
    on_event: EventHandler<DirectoryNavigatorEvent>,
) -> Element {
    let clicks = DirectoryClick {
        pending: use_signal(|| Option::<PendingOpen>::None),
        on_event,
    };
    let current_name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let parent_entries = visible_directory_entries(&parent_entries, show_hidden);
    let entries = visible_directory_entries(&entries, show_hidden);
    let keyboard_entries = entries.clone();
    let keyboard_path = path.clone();

    rsx! {
        div {
            id: DIRECTORY_NAVIGATOR_ID,
            tabindex: "-1",
            autofocus: true,
            class: "grid min-h-0 flex-1 grid-cols-[minmax(8rem,14rem)_minmax(10rem,1fr)_minmax(12rem,1.3fr)] gap-3 p-3 outline-none",
            onmousedown: move |_| FocusClaim::new(DIRECTORY_NAVIGATOR_ID).request(),
            onclick: move |event: Event<MouseData>| clicks.pane(event.client_coordinates()),
            onkeydown: move |event: KeyboardEvent| {
                let key = event.key().to_string();
                let next = match key.as_str() {
                    "j" | "ArrowDown" => Some(
                        (selected + 1).min(keyboard_entries.len().saturating_sub(1)),
                    ),
                    "k" | "ArrowUp" => Some(selected.saturating_sub(1)),
                    _ => None,
                };
                if let Some(index) = next {
                    event.prevent_default();
                    event.stop_propagation();
                    if let Some(entry) = keyboard_entries.get(index).cloned() {
                        on_event.call(DirectoryNavigatorEvent::Select { index, entry });
                        ScrollIntoView::nearest(&format!("dir-row-{index}"));
                    }
                    return;
                }
                match key.as_str() {
                    "l" | "ArrowRight" | "Enter" => {
                        event.prevent_default();
                        event.stop_propagation();
                        if let Some(entry) = keyboard_entries.get(selected).cloned() {
                            on_event.call(DirectoryNavigatorEvent::Open { entry });
                        }
                    }
                    "h" | "ArrowLeft" => {
                        event.prevent_default();
                        event.stop_propagation();
                        on_event.call(DirectoryNavigatorEvent::Ascend {
                            target: keyboard_path.clone(),
                        });
                    }
                    "." => {
                        event.prevent_default();
                        event.stop_propagation();
                        on_event.call(DirectoryNavigatorEvent::ToggleHidden);
                    }
                    _ => {}
                }
            },

            div { class: DIRECTORY_PANE_CLASS,
                for entry in parent_entries {
                    {
                        let row = entry.clone();
                        let target = entry.path.clone();
                        rsx! {
                            div {
                                key: "{entry.path}",
                                class: if entry.name == current_name { DIRECTORY_PARENT_CURRENT_CLASS } else { DIRECTORY_PARENT_CLASS },
                                title: "{entry.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.shift(
                                        DirectoryNavigatorEvent::Ascend { target: target.clone() },
                                        row.clone(),
                                        event.client_coordinates(),
                                    );
                                },
                                DirectoryEntryVisual { entry: entry.clone(), thumb: None }
                                span { class: "truncate text-xs", "{entry.name}" }
                            }
                        }
                    }
                }
            }

            div { class: DIRECTORY_PANE_CLASS,
                for (index, entry) in entries.into_iter().enumerate() {
                    {
                        let row = entry.clone();
                        let opened = entry.clone();
                        rsx! {
                            div {
                                key: "{entry.path}",
                                id: "dir-row-{index}",
                                class: directory_row_class(index == selected),
                                title: "{entry.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.select(index, row.clone(), event.client_coordinates());
                                },
                                ondoubleclick: move |_| on_event.call(DirectoryNavigatorEvent::Open { entry: opened.clone() }),
                                DirectoryEntryVisual { entry: entry.clone(), thumb: thumbs.get(&entry.path).cloned() }
                                span { class: "truncate text-xs", "{entry.name}" }
                            }
                        }
                    }
                }
            }

            if let Some(children) = children {
                div { class: DIRECTORY_PANE_CLASS,
                    for entry in visible_directory_entries(&children, show_hidden) {
                        {
                            let row = entry.clone();
                            let target = entry.path.clone();
                            rsx! {
                                div {
                                    key: "{entry.path}",
                                    class: directory_row_class(false),
                                    title: "{entry.path}",
                                    onclick: move |event: Event<MouseData>| {
                                        event.stop_propagation();
                                        clicks.shift(
                                            DirectoryNavigatorEvent::Descend { target: target.clone() },
                                            row.clone(),
                                            event.client_coordinates(),
                                        );
                                    },
                                    DirectoryEntryVisual { entry: entry.clone(), thumb: None }
                                    span { class: "truncate text-xs", "{entry.name}" }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-foreground/[0.02] p-4 ring-1 ring-inset ring-primary/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                    {preview}
                }
            }
        }
    }
}

pub const DIRECTORY_PANE_CLASS: &str = "min-h-0 overflow-y-auto rounded-2xl bg-foreground/[0.025] p-2 ring-1 ring-inset ring-primary/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";
const DIRECTORY_NAVIGATOR_ID: &str = "directory-navigator";
const DIRECTORY_PARENT_CLASS: &str = "flex items-center gap-2 rounded-md px-2 py-1 text-foreground/45 cursor-default transition-colors hover:bg-foreground/[0.04]";
const DIRECTORY_PARENT_CURRENT_CLASS: &str = "flex items-center gap-2 rounded-md bg-primary/10 px-2 py-1 text-foreground cursor-default shadow-[inset_2px_0_0_0_color-mix(in_oklab,var(--primary)_60%,transparent)]";

pub fn directory_row_class(selected: bool) -> String {
    let base =
        "flex items-center gap-2 rounded-md px-2 py-1 cursor-default transition-all duration-100";
    let state = if selected {
        "bg-primary/12 text-foreground shadow-[inset_2px_0_0_0_var(--primary),0_0_18px_-4px_color-mix(in_oklab,var(--primary)_45%,transparent)]"
    } else {
        "text-foreground/75 hover:bg-foreground/[0.05]"
    };
    cn([base, state])
}

#[component]
pub fn DirectoryEntryVisual(entry: FileDirEntry, thumb: Option<String>) -> Element {
    if let Some(url) = thumb {
        return rsx! {
            img { src: "{url}", class: "h-5 w-5 shrink-0 rounded object-cover ring-1 ring-border" }
        };
    }
    rsx! {
        TypeIcon { path: entry.path, is_dir: entry.is_dir, class: "h-5 w-5 shrink-0 opacity-80" }
    }
}

pub fn visible_directory_entries(entries: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
    if show_hidden {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|entry| !entry.name.starts_with('.'))
        .cloned()
        .collect()
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
struct DirectoryClick {
    pending: Signal<Option<PendingOpen>>,
    on_event: EventHandler<DirectoryNavigatorEvent>,
}

impl DirectoryClick {
    fn select(mut self, index: usize, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        self.on_event
            .call(DirectoryNavigatorEvent::Select { index, entry });
    }

    fn shift(mut self, event: DirectoryNavigatorEvent, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        self.on_event.call(event);
        self.pending.set(Some(PendingOpen {
            entry,
            at: now_millis(),
            origin: (at.x, at.y),
        }));
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
        self.on_event.call(DirectoryNavigatorEvent::Open {
            entry: pending.entry,
        });
        true
    }
}
