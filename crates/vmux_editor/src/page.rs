#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::page_model::{clamp_selection, gutter_width, image_mime, span_style};
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_git::ui::{DiffView, GitBar};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const CONTAINER_ID: &str = "file-container";
const MEASURE_ID: &str = "file-measure";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Dir,
    Text,
    Image,
}

#[derive(Clone, PartialEq)]
enum Preview {
    None,
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image(String),
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
}

fn blob_url(bytes: &[u8]) -> Option<String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

fn revoke(url: &str) {
    let _ = web_sys::Url::revoke_object_url(url);
}

fn clear_blob_state(
    mut image_url: Signal<Option<String>>,
    mut preview: Signal<Preview>,
    mut thumbs: Signal<HashMap<String, String>>,
) {
    if let Some(old) = image_url() {
        revoke(&old);
        image_url.set(None);
    }
    if let Preview::Image(old) = &*preview.read() {
        revoke(old);
    }
    preview.set(Preview::None);
    for url in thumbs.read().values() {
        revoke(url);
    }
    thumbs.set(HashMap::new());
}

fn request_preview(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FilePreviewRequest { path, thumb: false });
}

fn request_thumb(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FilePreviewRequest { path, thumb: true });
}

fn open_path(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FileOpenEvent { path });
}

fn parent_of(path: &str) -> String {
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

const PANE_CLASS: &str = "min-h-0 overflow-y-auto rounded-2xl bg-white/[0.025] p-2 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";

fn row_class(selected: bool) -> String {
    let base =
        "flex items-center gap-2.5 rounded-lg px-3 py-2 cursor-default transition-all duration-100";
    if selected {
        format!(
            "{base} bg-cyan-400/12 text-foreground shadow-[inset_2px_0_0_0_rgb(34,211,238),0_0_18px_-4px_rgba(34,211,238,0.45)]"
        )
    } else {
        format!("{base} text-foreground/75 hover:bg-white/[0.05]")
    }
}

fn visible_entries(all: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
    if show_hidden {
        all.to_vec()
    } else {
        all.iter()
            .filter(|e| !e.name.starts_with('.'))
            .cloned()
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_dir(
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
) {
    for url in thumbs.read().values() {
        revoke(url);
    }
    thumbs.set(HashMap::new());
    selected.set(0);
    if let Preview::Image(old) = &*preview.read() {
        revoke(old);
    }
    preview.set(Preview::None);
    parent_entries.set(parent);
    path.set(new_path);
    let vis = visible_entries(&entries, show_hidden);
    if let Some(first) = vis.first() {
        request_preview(first.path.clone());
    }
    for e in &vis {
        if !e.is_dir && image_mime(&e.path).is_some() {
            request_thumb(e.path.clone());
        }
    }
    dir_entries.set(entries);
}

fn folder_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
        }
    }
}

fn file_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
        }
    }
}

fn text_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
            path { d: "M16 13H8" }
            path { d: "M16 17H8" }
            path { d: "M10 9H8" }
        }
    }
}

fn code_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "m16 18 6-6-6-6" }
            path { d: "m8 6-6 6 6 6" }
        }
    }
}

fn config_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1" }
            path { d: "M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1" }
        }
    }
}

fn image_glyph(class: &str) -> Element {
    rsx! {
        Icon { class: "{class}",
            path { d: "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z" }
            path { d: "m21 15-5-5L5 21" }
        }
    }
}

fn logo_icon(d: &str, class: &str) -> Element {
    rsx! {
        Icon { class: "{class}", fill: "currentColor", stroke: "none",
            path { d: "{d}" }
        }
    }
}

fn ext_of(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn type_icon(path: &str, is_dir: bool, class: &str) -> Element {
    if is_dir {
        return folder_glyph(class);
    }
    let name = path.rsplit('/').next().unwrap_or("");
    let ext = ext_of(path);
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico"
    ) {
        return image_glyph(class);
    }
    let key = match name {
        "Dockerfile" => "dockerfile",
        "CMakeLists.txt" => "cmake",
        _ => ext.as_str(),
    };
    if let Some(d) = crate::lang_icon::lang_logo(key) {
        return logo_icon(d, class);
    }
    match ext.as_str() {
        "ini" | "cfg" | "conf" | "lock" | "env" | "properties" | "editorconfig" => {
            config_glyph(class)
        }
        "txt" | "log" | "csv" | "text" => text_glyph(class),
        "java" | "vala" | "d" | "ron" => code_glyph(class),
        _ if matches!(name, "Makefile" | "makefile" | "GNUmakefile") => config_glyph(class),
        _ => file_glyph(class),
    }
}

fn entry_visual(entry: &FileDirEntry, thumb: Option<&String>) -> Element {
    if let Some(url) = thumb {
        return rsx! {
            img { src: "{url}", class: "h-6 w-6 shrink-0 rounded object-cover ring-1 ring-border" }
        };
    }
    type_icon(&entry.path, entry.is_dir, "h-5 w-5 shrink-0 opacity-80")
}

fn render_preview(preview: &Preview) -> Element {
    match preview {
        Preview::None => rsx! {
            div { class: "text-xs text-muted-foreground opacity-60", "" }
        },
        Preview::Image(url) => rsx! {
            img { src: "{url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
        },
        Preview::Text(lines) => rsx! {
            div { class: "h-full w-full overflow-auto font-mono text-xs leading-snug",
                for line in lines.iter() {
                    div { key: "{line.line_no}", class: "whitespace-pre",
                        for (i, s) in line.spans.iter().enumerate() {
                            span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                        }
                    }
                }
            }
        },
        Preview::Dir(entries) => rsx! {
            div { class: "h-full w-full overflow-auto",
                for e in entries.iter() {
                    div { key: "{e.path}", class: "flex items-center gap-2 rounded px-2 py-1 text-foreground/90",
                        {entry_visual(e, None)}
                        span { class: "truncate text-xs", "{e.name}" }
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
                div { class: "uppercase tracking-wide text-foreground/80", "{kind}" }
                div { "{format_size(*size)}" }
                if !modified.is_empty() {
                    div { class: "opacity-70", "{modified}" }
                }
            }
        },
        Preview::Error(m) => rsx! {
            div { class: "text-xs text-ansi-1", "{m}" }
        },
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut path = use_signal(String::new);
    let mut total_lines = use_signal(|| 0u32);
    let mut first_line = use_signal(|| 0u32);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut error = use_signal(String::new);
    let dir_entries = use_signal(Vec::<FileDirEntry>::new);
    let parent_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut parent_path = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut back_dir = use_signal(|| Option::<String>::None);
    let mut show_hidden = use_signal(|| true);
    let mut mode = use_signal(|| Mode::Text);
    let mut image_url = use_signal(|| Option::<String>::None);
    let mut preview = use_signal(|| Preview::None);
    let mut thumbs = use_signal(HashMap::<String, String>::new);
    let mut theme_style = use_signal(String::new);
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));
    let mut git_path = use_signal(String::new);
    let show_diff = use_signal(|| false);
    let mut git_nonce = use_signal(|| 0u32);
    let git_display = use_signal(String::new);
    let git_branch = use_signal(String::new);
    let git_ahead = use_signal(|| 0u32);
    let git_behind = use_signal(|| 0u32);
    let git_staged = use_signal(|| 0u32);
    let git_message = use_signal(String::new);

    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        clear_blob_state(image_url, preview, thumbs);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = m.path.rsplit('/').next().unwrap_or(&m.path).to_string();
            doc.set_title(&name);
        }
        path.set(m.path);
        git_path.set(m.abs_path);
        total_lines.set(m.total_lines);
        mode.set(Mode::Text);
        git_nonce.set(git_nonce() + 1);
    });

    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_line.set(p.first_line);
        total_lines.set(p.total_lines);
        lines.set(p.lines);
    });

    let _err = use_bin_event_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    let _dir = use_bin_event_listener::<FileDirEvent, _>(FILE_DIR_EVENT, move |d| {
        clear_blob_state(image_url, preview, thumbs);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = d
                .path
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or(&d.path)
                .to_string();
            doc.set_title(&name);
        }
        parent_path.set(d.parent_path);
        git_path.set(d.abs_path);
        git_nonce.set(git_nonce() + 1);
        mode.set(Mode::Dir);
        apply_dir(
            dir_entries,
            parent_entries,
            path,
            selected,
            preview,
            thumbs,
            show_hidden(),
            d.entries,
            d.parent_entries,
            d.path,
        );
    });

    let _img = use_bin_event_listener::<FileImageEvent, _>(FILE_IMAGE_EVENT, move |e| {
        clear_blob_state(image_url, preview, thumbs);
        image_url.set(blob_url(&e.bytes));
        mode.set(Mode::Image);
    });

    let _prev = use_bin_event_listener::<FilePreviewEvent, _>(FILE_PREVIEW_EVENT, move |ev| {
        if ev.thumb {
            if let PreviewKind::Image { bytes, .. } = ev.kind
                && let Some(url) = blob_url(&bytes)
            {
                let old = thumbs.write().insert(ev.path.clone(), url);
                if let Some(old) = old {
                    revoke(&old);
                }
            }
            return;
        }
        let vis = visible_entries(&dir_entries.read(), show_hidden());
        let sel_path = vis.get(selected()).map(|e| e.path.clone());
        if sel_path.as_deref() != Some(ev.path.as_str()) {
            return;
        }
        let next = match ev.kind {
            PreviewKind::Image { bytes, .. } => match blob_url(&bytes) {
                Some(u) => Preview::Image(u),
                None => Preview::Error("failed to decode image".into()),
            },
            PreviewKind::Text(l) => Preview::Text(l),
            PreviewKind::Dir(e) => Preview::Dir(e),
            PreviewKind::Info {
                size,
                modified,
                kind,
            } => Preview::Info {
                size,
                modified,
                kind,
            },
            PreviewKind::Error(m) => Preview::Error(m),
        };
        if let Preview::Image(old) = &*preview.read() {
            revoke(old);
        }
        preview.set(next);
    });

    let _theme = use_bin_event_listener::<FileThemeEvent, _>(FILE_THEME_EVENT, move |t| {
        let mut s = String::new();
        if !t.font_family.is_empty() {
            s.push_str(&format!(
                "font-family:\"{}\",\"JetBrainsMono NF\",monospace;",
                t.font_family
            ));
        }
        if t.font_size > 0.0 {
            s.push_str(&format!("font-size:{}px;", t.font_size));
        }
        if t.line_height > 0.0 {
            s.push_str(&format!("line-height:{};", t.line_height));
        }
        theme_style.set(s);
    });

    use_effect(move || {
        setup_measurement(cell_dims);
        focus_container();
    });

    let gw = gutter_width(total_lines());
    let cur_basename = path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let header_path = {
        let g = git_display();
        if g.is_empty() { path() } else { g }
    };

    rsx! {
        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-mono text-sm leading-normal",
            style: "outline:none;background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);{theme_style}",

            onmousedown: move |_| focus_container(),

            onwheel: move |e: Event<WheelData>| {
                if mode() != Mode::Text || show_diff() {
                    return;
                }
                e.prevent_default();
                let (_, ch) = cell_dims();
                let line_px = if ch > 0.0 { ch } else { 16.0 };
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::WheelEvent>() else {
                    return;
                };
                let delta_lines = match raw.delta_mode() {
                    web_sys::WheelEvent::DOM_DELTA_LINE => raw.delta_y(),
                    web_sys::WheelEvent::DOM_DELTA_PAGE => raw.delta_y() * 20.0,
                    _ => raw.delta_y() / line_px,
                };
                let notches = delta_lines.round() as i64;
                if notches == 0 {
                    return;
                }
                let next = (first_line() as i64 + notches).max(0) as u32;
                let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_line: next });
            },

            onkeydown: move |e: Event<KeyboardData>| {
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let key = raw.key();
                match mode() {
                    Mode::Dir => {
                        let vis = visible_entries(&dir_entries.read(), show_hidden());
                        let len = vis.len();
                        let cur = selected();
                        match key.as_str() {
                            "j" | "ArrowDown" => {
                                e.prevent_default();
                                let next = if len == 0 { 0 } else { (cur + 1).min(len - 1) };
                                selected.set(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "k" | "ArrowUp" => {
                                e.prevent_default();
                                let next = cur.saturating_sub(1);
                                selected.set(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "l" | "ArrowRight" | "Enter" => {
                                e.prevent_default();
                                let Some(ent) = vis.get(cur).cloned() else {
                                    return;
                                };
                                if ent.is_dir {
                                    let children = match &*preview.read() {
                                        Preview::Dir(c) => Some(c.clone()),
                                        _ => None,
                                    };
                                    if let Some(children) = children {
                                        let cur_entries = dir_entries.read().clone();
                                        parent_path.set(parent_of(&ent.path));
                                        apply_dir(
                                            dir_entries,
                                            parent_entries,
                                            path,
                                            selected,
                                            preview,
                                            thumbs,
                                            show_hidden(),
                                            children,
                                            cur_entries,
                                            ent.path.clone(),
                                        );
                                    }
                                    open_path(ent.path);
                                } else {
                                    back_dir.set(Some(parent_of(&ent.path)));
                                    open_path(ent.path);
                                }
                            }
                            "h" | "ArrowLeft" | "Escape" => {
                                let pp = parent_path();
                                if !pp.is_empty() {
                                    e.prevent_default();
                                    let pe = parent_entries.read().clone();
                                    if !pe.is_empty() {
                                        parent_path.set(parent_of(&pp));
                                        apply_dir(
                                            dir_entries,
                                            parent_entries,
                                            path,
                                            selected,
                                            preview,
                                            thumbs,
                                            show_hidden(),
                                            pe,
                                            Vec::new(),
                                            pp.clone(),
                                        );
                                    }
                                    open_path(pp);
                                }
                            }
                            "." => {
                                e.prevent_default();
                                let next = !show_hidden();
                                show_hidden.set(next);
                                let vis2 = visible_entries(&dir_entries.read(), next);
                                let idx = clamp_selection(cur, vis2.len());
                                selected.set(idx);
                                if let Some(p) = vis2.get(idx).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        if matches!(key.as_str(), "Escape" | "h")
                            && let Some(d) = back_dir()
                        {
                            e.prevent_default();
                            open_path(d);
                            return;
                        }
                        if mode() != Mode::Text || show_diff() {
                            return;
                        }
                        let cur = first_line() as i64;
                        let next = match key.as_str() {
                            "ArrowDown" => cur + 1,
                            "ArrowUp" => cur - 1,
                            "PageDown" => cur + 20,
                            "PageUp" => cur - 20,
                            "Home" => 0,
                            _ => return,
                        };
                        e.prevent_default();
                        let _ = try_cef_bin_emit_rkyv(&FileScrollEvent {
                            top_line: next.max(0) as u32,
                        });
                    }
                }
            },

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-white/[0.07] bg-black/20 px-4 font-sans text-xs text-muted-foreground",
                {type_icon(&header_path, mode() == Mode::Dir, "h-4 w-4 shrink-0 text-foreground/80")}
                span { class: "truncate text-foreground/90", "{header_path}" }
            }

            GitBar {
                path: git_path,
                show_diff,
                nonce: git_nonce,
                display_path: git_display,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
            }

            {
                let msg = error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-50 flex items-center justify-center",
                        style: "background:rgba(0,0,0,0.6);",
                        div {
                            class: "rounded-md border border-ansi-1 bg-background px-4 py-2 text-sm text-ansi-1",
                            "{msg}"
                        }
                    }
                })
            }

            match mode() {
                Mode::Image => rsx! {
                    div { class: "flex min-h-0 flex-1 items-center justify-center overflow-auto p-4",
                        if let Some(url) = image_url() {
                            img { src: "{url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
                        }
                    }
                },
                Mode::Dir => rsx! {
                    div {
                        class: "grid min-h-0 flex-1 gap-3 p-3",
                        style: "grid-template-columns: minmax(8rem,14rem) minmax(10rem,1fr) minmax(12rem,1.3fr);",

                        div { class: PANE_CLASS,
                            for e in visible_entries(&parent_entries(), show_hidden()) {
                                div {
                                    key: "{e.path}",
                                    class: if e.name == cur_basename { "flex items-center gap-2.5 rounded-lg bg-cyan-400/10 px-3 py-2 text-foreground shadow-[inset_2px_0_0_0_rgba(34,211,238,0.6)]" } else { "flex items-center gap-2.5 rounded-lg px-3 py-2 text-foreground/45 transition-colors hover:bg-white/[0.04]" },
                                    {entry_visual(&e, None)}
                                    span { class: "truncate text-xs", "{e.name}" }
                                }
                            }
                        }

                        div { class: PANE_CLASS,
                            for (i, e) in visible_entries(&dir_entries(), show_hidden()).into_iter().enumerate() {
                                {
                                    let p_sel = e.path.clone();
                                    let p_open = e.path.clone();
                                    let is_dir = e.is_dir;
                                    let thumb = thumbs().get(&e.path).cloned();
                                    rsx! {
                                        div {
                                            key: "{e.path}",
                                            class: row_class(i == selected()),
                                            title: "{e.path}",
                                            onclick: move |_| {
                                                selected.set(i);
                                                request_preview(p_sel.clone());
                                            },
                                            ondoubleclick: move |_| {
                                                if !is_dir {
                                                    back_dir.set(Some(parent_of(&p_open)));
                                                }
                                                open_path(p_open.clone());
                                            },
                                            {entry_visual(&e, thumb.as_ref())}
                                            span { class: "truncate text-xs", "{e.name}" }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-white/[0.02] p-4 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                            {render_preview(&preview())}
                        }
                    }
                },
                Mode::Text => rsx! {
                    if show_diff() {
                        DiffView { path: git_path, nonce: git_nonce }
                    } else {
                        div { class: "min-h-0 flex-1 overflow-auto",
                            div { class: "min-w-max py-2",
                                for line in lines().iter() {
                                    div { key: "{line.line_no}", class: "group flex hover:bg-white/[0.035]",
                                        span {
                                            class: "sticky left-0 z-[1] shrink-0 select-none bg-background pl-4 pr-5 text-right tabular-nums opacity-40 group-hover:opacity-90",
                                            style: "min-width:calc(var(--cw, 1ch) * {gw} + 2.25rem);",
                                            "{line.line_no + 1}"
                                        }
                                        span { class: "whitespace-pre pr-8",
                                            for (i, s) in line.spans.iter().enumerate() {
                                                span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

fn focus_container() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CONTAINER_ID))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

fn setup_measurement(cell_dims: Signal<(f64, f64)>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };

    if document.get_element_by_id(MEASURE_ID).is_some() {
        do_measure(cell_dims);
        return;
    }

    let measure: web_sys::Element = document.create_element("span").unwrap();
    measure
        .set_attribute(
            "style",
            "position:absolute;visibility:hidden;white-space:pre;font:inherit",
        )
        .unwrap();
    measure.set_attribute("id", MEASURE_ID).unwrap();
    let measure_node: &web_sys::Node = measure.as_ref();
    measure_node.set_text_content(Some(&"X".repeat(80)));
    container.append_child(&measure).unwrap();

    do_measure(cell_dims);

    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        do_measure(cell_dims);
    }) as Box<dyn FnMut(JsValue)>);

    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&container);
        observer.observe(&measure);
        std::mem::forget(observer);
    }
    callback.forget();
}

fn do_measure(mut cell_dims: Signal<(f64, f64)>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };
    let Some(measure) = document.get_element_by_id(MEASURE_ID) else {
        return;
    };

    let rect = measure.get_bounding_client_rect();
    let cw = rect.width() / 80.0;

    let ch = window
        .get_computed_style(&container)
        .ok()
        .flatten()
        .and_then(|cs| {
            cs.get_property_value("line-height")
                .ok()
                .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        })
        .unwrap_or(rect.height());

    if cw <= 0.0 || ch <= 0.0 {
        return;
    }

    cell_dims.set((cw, ch));

    let html: &web_sys::HtmlElement = container.unchecked_ref();
    let _ = html.style().set_property("--cw", &format!("{cw}px"));
    let _ = html.style().set_property("--ch", &format!("{ch}px"));

    let vh = container.client_height() as f64;

    let _ = try_cef_bin_emit_rkyv(&FileResizeEvent {
        char_height: ch as f32,
        viewport_height: vh as f32,
    });
}
