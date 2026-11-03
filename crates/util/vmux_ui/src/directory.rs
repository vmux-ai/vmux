use std::collections::HashMap;

use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use vmux_ecs::event::FileDirEntry;

use crate::cn::cn;
use crate::file_icon::TypeIcon;
use crate::focus::FocusClaim;
use crate::platform::Platform;
use crate::scroll::ScrollIntoView;

#[component]
pub fn DirectoryNavigator(
    path: String,
    parent_entries: Vec<FileDirEntry>,
    entries: Vec<FileDirEntry>,
    children: Option<Vec<FileDirEntry>>,
    selected: usize,
    thumbs: HashMap<String, String>,
    preview: Element,
    on_select: EventHandler<(usize, FileDirEntry)>,
    on_ascend: EventHandler<String>,
    on_descend: EventHandler<String>,
    on_open: EventHandler<FileDirEntry>,
    on_next: EventHandler<()>,
    on_previous: EventHandler<()>,
    on_activate: EventHandler<()>,
    on_parent: EventHandler<()>,
    on_toggle_hidden: EventHandler<()>,
) -> Element {
    let keys = DirectoryKeys {
        on_next,
        on_previous,
        on_activate,
        on_parent,
        on_toggle_hidden,
    };
    let clicks = DirectoryClick {
        pending: use_signal(|| Option::<PendingOpen>::None),
        on_select,
        on_ascend,
        on_descend,
        on_open,
    };
    let current_name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let parent_identity = path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("/")
        .to_string();
    let child_identity = entries
        .get(selected)
        .map(|entry| entry.path.clone())
        .unwrap_or_default();
    use_effect(move || {
        ScrollIntoView::nearest(&format!("dir-row-{selected}"));
    });

    rsx! {
        div {
            id: DIRECTORY_NAVIGATOR_ID,
            tabindex: "-1",
            autofocus: true,
            class: "grid min-h-0 flex-1 grid-cols-[minmax(8rem,14rem)_minmax(10rem,18rem)_minmax(12rem,1fr)] gap-4 px-3 py-3 font-sans outline-none",
            onmousedown: move |_| DirectoryKeys::focus(),
            onclick: move |event: Event<MouseData>| clicks.pane(event.client_coordinates()),
            onkeydown: move |event: KeyboardEvent| {
                keys.offer(&event);
            },

            DirectoryColumn { identity: parent_identity,
                for entry in parent_entries {
                    {
                        let row = entry.clone();
                        let target = entry.path.clone();
                        rsx! {
                            div {
                                key: "{entry.path}",
                                class: DirectoryRowStyle::parent(entry.name == current_name),
                                title: "{entry.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.ascend(target.clone(), row.clone(), event.client_coordinates());
                                },
                                DirectoryEntryVisual { entry: entry.clone(), thumb: None }
                                span { class: "truncate text-[13px]", "{entry.name}" }
                            }
                        }
                    }
                }
            }

            DirectoryColumn { identity: path.clone(),
                for (index, entry) in entries.into_iter().enumerate() {
                    {
                        let row = entry.clone();
                        let opened = entry.clone();
                        rsx! {
                            div {
                                key: "{entry.path}",
                                id: "dir-row-{index}",
                                class: DirectoryRowStyle::class(index == selected),
                                title: "{entry.path}",
                                onclick: move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    clicks.select(index, row.clone(), event.client_coordinates());
                                },
                                ondoubleclick: move |_| on_open.call(opened.clone()),
                                DirectoryEntryVisual { entry: entry.clone(), thumb: thumbs.get(&entry.path).cloned() }
                                span { class: "truncate text-[13px]", "{entry.name}" }
                            }
                        }
                    }
                }
            }

            if let Some(children) = children {
                DirectoryColumn { identity: child_identity.clone(),
                    for entry in children {
                        {
                            let row = entry.clone();
                            let target = entry.path.clone();
                            rsx! {
                                div {
                                    key: "{entry.path}",
                                    class: DirectoryRowStyle::class(false),
                                    title: "{entry.path}",
                                    onclick: move |event: Event<MouseData>| {
                                        event.stop_propagation();
                                        clicks.descend(target.clone(), row.clone(), event.client_coordinates());
                                    },
                                    DirectoryEntryVisual { entry: entry.clone(), thumb: None }
                                    span { class: "truncate text-[13px]", "{entry.name}" }
                                }
                            }
                        }
                    }
                }
            } else {
                DirectoryColumn { identity: child_identity, preview: true,
                    {preview}
                }
            }
        }
    }
}

pub const DIRECTORY_PANE_CLASS: &str = "min-h-0 overflow-y-auto py-1";
const DIRECTORY_NAVIGATOR_ID: &str = "directory-navigator";

#[component]
fn DirectoryColumn(
    identity: String,
    #[props(default)] preview: bool,
    children: Element,
) -> Element {
    let class = if preview {
        "animate-[dx-fade-in_140ms_ease-out_both] motion-reduce:animate-none flex min-h-0 items-center justify-center overflow-auto px-3 py-2"
    } else {
        "animate-[dx-fade-in_140ms_ease-out_both] motion-reduce:animate-none min-h-0 overflow-y-auto py-1"
    };
    rsx! { for identity in [identity] { div { key: "{identity}", "data-vmux-directory-column": identity.clone(), class, {children.clone()} } } }
}

#[derive(Clone, Copy)]
pub struct DirectoryKeys {
    pub on_next: EventHandler<()>,
    pub on_previous: EventHandler<()>,
    pub on_activate: EventHandler<()>,
    pub on_parent: EventHandler<()>,
    pub on_toggle_hidden: EventHandler<()>,
}

impl DirectoryKeys {
    pub fn focus() {
        FocusClaim::new(DIRECTORY_NAVIGATOR_ID).request();
    }

    pub fn offer(self, event: &Event<KeyboardData>) -> bool {
        let modifiers = event.modifiers();
        if modifiers.ctrl() || modifiers.meta() || modifiers.alt() {
            return false;
        }
        let handler = match event.key().to_string().as_str() {
            "j" | "ArrowDown" => self.on_next,
            "k" | "ArrowUp" => self.on_previous,
            "l" | "ArrowRight" | "Enter" => self.on_activate,
            "h" | "ArrowLeft" => self.on_parent,
            "." => self.on_toggle_hidden,
            _ => return false,
        };
        event.prevent_default();
        event.stop_propagation();
        handler.call(());
        true
    }
}

struct DirectoryRowStyle;

impl DirectoryRowStyle {
    const ROW: &'static str = "relative flex h-8 min-w-0 cursor-default items-center gap-2 rounded-lg px-2.5 outline-none transition-colors duration-150";

    fn class(selected: bool) -> String {
        let tone = if selected {
            "[--directory-tint:18%] bg-[linear-gradient(110deg,color-mix(in_oklab,var(--primary-action-start)_var(--directory-tint),transparent),color-mix(in_oklab,var(--primary-action-middle)_var(--directory-tint),transparent),color-mix(in_oklab,var(--primary-action-end)_var(--directory-tint),transparent))] text-foreground"
        } else {
            "text-foreground/80 hover:bg-foreground/[0.035] hover:text-foreground"
        };
        cn([Self::ROW, tone])
    }

    fn parent(selected: bool) -> String {
        let tone = if selected {
            "[--directory-tint:9%] bg-[linear-gradient(110deg,color-mix(in_oklab,var(--primary-action-start)_var(--directory-tint),transparent),color-mix(in_oklab,var(--primary-action-middle)_var(--directory-tint),transparent),color-mix(in_oklab,var(--primary-action-end)_var(--directory-tint),transparent))] text-foreground/80"
        } else {
            "text-foreground/45 hover:bg-foreground/[0.035] hover:text-foreground/80"
        };
        cn([Self::ROW, tone])
    }
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
        Platform::now() - self.at < DOUBLE_CLICK_MS
            && (at.x - self.origin.0).abs() <= DOUBLE_CLICK_SLOP_PX
            && (at.y - self.origin.1).abs() <= DOUBLE_CLICK_SLOP_PX
    }
}

#[derive(Clone, Copy)]
struct DirectoryClick {
    pending: Signal<Option<PendingOpen>>,
    on_select: EventHandler<(usize, FileDirEntry)>,
    on_ascend: EventHandler<String>,
    on_descend: EventHandler<String>,
    on_open: EventHandler<FileDirEntry>,
}

impl DirectoryClick {
    fn select(mut self, index: usize, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        self.on_select.call((index, entry));
    }

    fn ascend(mut self, target: String, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        self.on_ascend.call(target);
        self.remember(entry, at);
    }

    fn descend(mut self, target: String, entry: FileDirEntry, at: ClientPoint) {
        if self.take_open(at) {
            return;
        }
        self.on_descend.call(target);
        self.remember(entry, at);
    }

    fn remember(&mut self, entry: FileDirEntry, at: ClientPoint) {
        self.pending.set(Some(PendingOpen {
            entry,
            at: Platform::now(),
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
        self.on_open.call(pending.entry);
        true
    }
}
