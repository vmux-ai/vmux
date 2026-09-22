use std::collections::HashMap;

use base64::Engine;
use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use vmux_core::event::{FileDirEntry, FileLine, FileOpenEvent, FilePreviewRequest};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::platform::now_millis;
use vmux_ui::util::cn;

use crate::page_model::{dir_select_index, image_mime, span_style};

#[derive(Clone, PartialEq)]
pub(super) enum Preview {
    None,
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image(String),
    Video {
        url: String,
        path: String,
        native: bool,
    },
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
}

pub(super) fn image_data_url(bytes: &[u8], path: &str) -> String {
    let mime = image_mime(path).unwrap_or("application/octet-stream");

    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub(super) fn clear_preview(
    mut preview: Signal<Preview>,
    mut thumbs: Signal<HashMap<String, String>>,
) {
    preview.set(Preview::None);
    thumbs.set(HashMap::new());
}

pub(super) fn request_preview(path: String) {
    let _ = send(&FilePreviewRequest { path, thumb: false });
}

fn request_thumb(path: String) {
    let _ = send(&FilePreviewRequest { path, thumb: true });
}

pub(super) fn open_path(path: String) {
    let _ = send(&FileOpenEvent { path });
}

pub(super) fn parent_of(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((prefix, _)) => prefix.to_string(),
        None => path.to_string(),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

const PANE_CLASS: &str = "min-h-0 overflow-y-auto rounded-2xl bg-foreground/[0.025] p-2 ring-1 ring-inset ring-primary/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";
pub(super) const VIDEO_HOST_ID: &str = "vmux-video-host";

fn row_class(selected: bool) -> String {
    let base =
        "flex items-center gap-2 rounded-md px-2 py-1 cursor-default transition-all duration-100";
    let state = if selected {
        "bg-primary/12 text-foreground shadow-[inset_2px_0_0_0_var(--primary),0_0_18px_-4px_color-mix(in_oklab,var(--primary)_45%,transparent)]"
    } else {
        "text-foreground/75 hover:bg-foreground/[0.05]"
    };
    cn([base, state])
}

pub(super) fn visible_entries(all: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
    if show_hidden {
        all.to_vec()
    } else {
        all.iter()
            .filter(|entry| !entry.name.starts_with('.'))
            .cloned()
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_dir(
    mut dir_entries: Signal<Vec<FileDirEntry>>,
    mut parent_entries: Signal<Vec<FileDirEntry>>,
    mut path: Signal<String>,
    mut selected: Signal<usize>,
    mut preview: Signal<Preview>,
    mut thumbs: Signal<HashMap<String, String>>,
    show_hidden: bool,
    entries: Vec<FileDirEntry>,
    parent: Vec<FileDirEntry>,
    new_path: String,
    select_path: Option<String>,
) {
    thumbs.set(HashMap::new());
    preview.set(Preview::None);
    parent_entries.set(parent);
    path.set(new_path);
    let visible = visible_entries(&entries, show_hidden);
    let selected_index = select_path
        .as_deref()
        .map(|path| dir_select_index(&visible, path))
        .unwrap_or(0);
    selected.set(selected_index);
    if let Some(entry) = visible.get(selected_index) {
        request_preview(entry.path.clone());
    }
    for entry in &visible {
        if !entry.is_dir && image_mime(&entry.path).is_some() {
            request_thumb(entry.path.clone());
        }
    }
    dir_entries.set(entries);
}

#[component]
fn EntryVisual(entry: FileDirEntry, thumb: Option<String>) -> Element {
    let entry = &entry;
    let thumb = thumb.as_ref();
    if let Some(url) = thumb {
        return rsx! {
            img { src: "{url}", class: "h-5 w-5 shrink-0 rounded object-cover ring-1 ring-border" }
        };
    }
    rsx! { TypeIcon { path: entry.path.to_string(), is_dir: entry.is_dir, class: "h-5 w-5 shrink-0 opacity-80" } }
}

#[component]
fn PreviewPane(preview: Preview) -> Element {
    let preview = &preview;
    match preview {
        Preview::None | Preview::Dir(_) => rsx! {
            div { class: "text-xs text-muted-foreground opacity-60", "" }
        },
        Preview::Image(url) => rsx! {
            img { src: "{url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20" }
        },
        Preview::Video { url, path, native } => {
            if *native {
                let path = path.clone();
                rsx! {
                    div {
                        key: "{path}",
                        id: VIDEO_HOST_ID,
                        class: "h-full w-full rounded-xl bg-black/40 ring-1 ring-primary/20",
                    }
                }
            } else {
                rsx! {
                    video {
                        id: "preview-video",
                        src: "{url}",
                        controls: true,
                        autoplay: false,
                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20",
                    }
                }
            }
        }
        Preview::Text(lines) => rsx! {
            div { class: "h-full w-full overflow-auto font-mono text-xs leading-snug",
                for line in lines.iter() {
                    div { key: "{line.line_no}", class: "whitespace-pre",
                        for (index, span) in line.spans.iter().enumerate() {
                            span { key: "{index}", style: "{span_style(span)}", "{span.text}" }
                        }
                    }
                }
            }
        },
        Preview::Info {
            size,
            modified,
            kind,
        } => rsx! {
            div { class: "space-y-1 text-center text-xs text-muted-foreground",
                div {
                    class: "uppercase tracking-wide text-foreground/80",
                    {match kind.as_str() {
                        "image (too large to preview)" => translate("editor-preview-large-image"),
                        "binary" => translate("editor-preview-binary"),
                        "file" => translate("editor-preview-file"),
                        _ => kind.clone(),
                    }}
                }
                div { "{format_size(*size)}" }
                if !modified.is_empty() {
                    div { class: "opacity-70", "{modified}" }
                }
            }
        },
        Preview::Error(message) => rsx! {
            div { class: "text-xs text-ansi-1", "{message}" }
        },
    }
}

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
            class: "grid min-h-0 flex-1 grid-cols-[minmax(8rem,14rem)_minmax(10rem,1fr)_minmax(12rem,1.3fr)] gap-3 p-3",
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
                    div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-foreground/[0.02] p-4 ring-1 ring-inset ring-primary/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                        PreviewPane { preview: (window.preview)() }
                    }
                },
            }
        }
    }
}

const PARENT_CLASS: &str = "flex items-center gap-2 rounded-md px-2 py-1 text-foreground/45 cursor-default transition-colors hover:bg-foreground/[0.04]";
const PARENT_CURRENT_CLASS: &str = "flex items-center gap-2 rounded-md bg-primary/10 px-2 py-1 text-foreground cursor-default shadow-[inset_2px_0_0_0_color-mix(in_oklab,var(--primary)_60%,transparent)]";

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
        match column {
            Column::Current(index) => {
                self.window.select(index, entry.path.clone());
            }
            Column::Parent => {
                self.window.ascend(entry.path.clone());
            }
            Column::Child => {
                self.window.descend(entry.path.clone());
            }
        }
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
        self.window.open(&pending.entry);
        true
    }
}
