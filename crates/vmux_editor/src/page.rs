#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::page_model::{
    clamp_selection, dir_select_index, gutter_width, image_mime, line_severity,
    severity_color_class, span_style, squiggle_style,
};
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_core::media::MediaKind;
use vmux_git::ui::{DiffView, GitBar, GitFooter};
use vmux_ui::file_icon::type_icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const CONTAINER_ID: &str = "file-container";
const MEASURE_ID: &str = "file-measure";
const VIDEO_HOST_ID: &str = "vmux-video-host";
const INPUT_ID: &str = "file-input";
const SCROLL_ID: &str = "file-scroll";
const SCROLL_EDGE: u32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Dir,
    Text,
    Media(MediaKind),
}

#[derive(Clone, PartialEq)]
enum Preview {
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

fn clear_blob_state(mut preview: Signal<Preview>, mut thumbs: Signal<HashMap<String, String>>) {
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

const PANE_CLASS: &str = "min-h-0 overflow-y-auto rounded-2xl bg-foreground/[0.025] p-2 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";

fn row_class(selected: bool) -> String {
    let base =
        "flex items-center gap-2 rounded-md px-2 py-1 cursor-default transition-all duration-100";
    if selected {
        format!(
            "{base} bg-cyan-400/12 text-foreground shadow-[inset_2px_0_0_0_rgb(34,211,238),0_0_18px_-4px_rgba(34,211,238,0.45)]"
        )
    } else {
        format!("{base} text-foreground/75 hover:bg-foreground/[0.05]")
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
    select_path: Option<String>,
) {
    for url in thumbs.read().values() {
        revoke(url);
    }
    thumbs.set(HashMap::new());
    if let Preview::Image(old) = &*preview.read() {
        revoke(old);
    }
    preview.set(Preview::None);
    parent_entries.set(parent);
    path.set(new_path);
    let vis = visible_entries(&entries, show_hidden);
    let sel_idx = select_path
        .as_deref()
        .map(|p| dir_select_index(&vis, p))
        .unwrap_or(0);
    selected.set(sel_idx);
    if let Some(sel) = vis.get(sel_idx) {
        request_preview(sel.path.clone());
    }
    for e in &vis {
        if !e.is_dir && image_mime(&e.path).is_some() {
            request_thumb(e.path.clone());
        }
    }
    dir_entries.set(entries);
}

fn entry_visual(entry: &FileDirEntry, thumb: Option<&String>) -> Element {
    if let Some(url) = thumb {
        return rsx! {
            img { src: "{url}", class: "h-5 w-5 shrink-0 rounded object-cover ring-1 ring-border" }
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
        Preview::Video { url, path, native } => {
            if *native {
                let path = path.clone();
                rsx! {
                    div {
                        key: "{path}",
                        id: VIDEO_HOST_ID,
                        class: "h-full w-full rounded-xl bg-black/40 ring-1 ring-cyan-400/20",
                        onmounted: move |_| report_video_rect(path.clone()),
                    }
                }
            } else {
                rsx! {
                    video {
                        id: "preview-video",
                        src: "{url}",
                        controls: true,
                        autoplay: false,
                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20",
                    }
                }
            }
        }
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
    let mut total_rows = use_signal(|| 0u32);
    let mut first_row = use_signal(|| 0u32);
    let mut gutter_hover = use_signal(|| false);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut diagnostics = use_signal(Vec::<FileDiagnostic>::new);
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
    let mut lsp_status = use_signal(|| Option::<FileLspStatusEvent>::None);
    let mut error = use_signal(String::new);
    let dir_entries = use_signal(Vec::<FileDirEntry>::new);
    let parent_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut parent_path = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut came_from = use_signal(String::new);
    let mut back_dir = use_signal(|| Option::<String>::None);
    let mut show_hidden = use_signal(|| true);
    let mut mode = use_signal(|| Mode::Text);
    let mut media = use_signal(|| Option::<FileMediaEvent>::None);
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
    let mut ed_mode = use_signal(|| vmux_core::editor::EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut dirty = use_signal(|| false);
    let mut composing = use_signal(|| false);
    let mut lsp_hover = use_signal(|| Option::<FileHoverEvent>::None);
    let mut hover_pos = use_signal(|| Option::<(u32, u32)>::None);
    let mut ctx_menu = use_signal(|| Option::<(f64, f64, u32, u32)>::None);
    let mut refs = use_signal(Vec::<RefItem>::new);
    let mut refs_sel = use_signal(|| 0usize);
    let mut refs_open = use_signal(|| false);
    let mut comps = use_signal(Vec::<CompletionItem>::new);
    let mut comp_open = use_signal(|| false);
    let mut comp_sel = use_signal(|| 0usize);
    let mut comp_anchor = use_signal(|| (0u32, 0u32));
    let mut last_scroll_req = use_signal(|| 0u32);

    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        clear_blob_state(preview, thumbs);
        media.set(None);
        reset_file_scroll();
        last_scroll_req.set(0);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = m.path.rsplit('/').next().unwrap_or(&m.path).to_string();
            doc.set_title(&name);
        }
        path.set(m.path);
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
        git_path.set(m.abs_path);
        total_lines.set(m.total_lines);
        mode.set(Mode::Text);
        git_nonce.set(git_nonce() + 1);
    });

    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_row.set(p.first_row);
        total_rows.set(p.total_rows);
        total_lines.set(p.total_lines);
        lines.set(p.lines);
        lsp_hover.set(None);
    });

    let _cur = use_bin_event_listener::<FileCursorEvent, _>(FILE_CURSOR_EVENT, move |c| {
        let moved = cursor() != c.primary;
        ed_mode.set(c.mode);
        ed_label.set(c.mode_label);
        cursor.set(c.primary);
        sel.set(c.selections);
        if moved {
            ensure_line_visible(c.primary.row, cell_dims().1);
        }
    });

    let _dirty = use_bin_event_listener::<FileDirtyEvent, _>(FILE_DIRTY_EVENT, move |d| {
        dirty.set(d.dirty);
    });

    let _hov = use_bin_event_listener::<FileHoverEvent, _>(FILE_HOVER_EVENT, move |h| {
        lsp_hover.set(Some(h));
    });

    let _refs = use_bin_event_listener::<FileReferencesEvent, _>(FILE_REFERENCES_EVENT, move |e| {
        refs.set(e.items);
        refs_sel.set(0);
        refs_open.set(true);
        focus_by_id("refs-panel");
    });

    let _comp = use_bin_event_listener::<FileCompletionEvent, _>(FILE_COMPLETION_EVENT, move |e| {
        comp_open.set(!e.items.is_empty());
        comps.set(e.items);
        comp_sel.set(0);
        comp_anchor.set((e.line, e.replace_from_col));
    });

    let _diag =
        use_bin_event_listener::<FileDiagnosticsEvent, _>(FILE_DIAGNOSTICS_EVENT, move |d| {
            if d.path != git_path() {
                return;
            }
            diagnostics.set(d.diagnostics);
        });

    let _lsp_status =
        use_bin_event_listener::<FileLspStatusEvent, _>(FILE_LSP_STATUS_EVENT, move |s| {
            if s.path != git_path() {
                return;
            }
            lsp_status.set(Some(s));
        });

    let _err = use_bin_event_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    let _dir = use_bin_event_listener::<FileDirEvent, _>(FILE_DIR_EVENT, move |d| {
        clear_blob_state(preview, thumbs);
        media.set(None);
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
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
        let came = came_from();
        came_from.set(String::new());
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
            (!came.is_empty()).then_some(came),
        );
    });

    let _media = use_bin_event_listener::<FileMediaEvent, _>(FILE_MEDIA_EVENT, move |e| {
        clear_blob_state(preview, thumbs);
        let kind = e.kind;
        media.set(Some(e));
        mode.set(Mode::Media(kind));
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
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
            PreviewKind::Video { url, path, native } => Preview::Video { url, path, native },
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
        if mode() == Mode::Text {
            focus_file_input();
        } else {
            focus_container();
        }
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

    let comp_filtered: Vec<CompletionItem> = if comp_open() {
        let (cline, cfrom) = comp_anchor();
        let lt: String = lines()
            .iter()
            .find(|l| l.line_no == cline)
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .unwrap_or_default();
        let chars: Vec<char> = lt.chars().collect();
        let caret = cursor().col as usize;
        let from = cfrom as usize;
        let prefix: String = if from <= caret && from <= chars.len() {
            chars[from..caret.min(chars.len())].iter().collect()
        } else {
            String::new()
        };
        let pl = prefix.to_lowercase();
        comps()
            .into_iter()
            .filter(|c| c.label.to_lowercase().starts_with(&pl))
            .collect()
    } else {
        Vec::new()
    };
    let comp_sel_clamped = comp_sel().min(comp_filtered.len().saturating_sub(1));
    let comp_keys = comp_filtered.clone();

    rsx! {
        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-mono text-sm leading-normal",
            style: "outline:none;background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);{theme_style}",

            onmousedown: move |e: Event<MouseData>| {
                match mode() {
                    Mode::Text => {
                        e.prevent_default();
                        focus_file_input();
                    }
                    Mode::Dir => {
                        e.prevent_default();
                        focus_container();
                    }
                    Mode::Media(_) => focus_container(),
                }
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
                                scroll_dir_row_into_view(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "k" | "ArrowUp" => {
                                e.prevent_default();
                                let next = cur.saturating_sub(1);
                                selected.set(next);
                                scroll_dir_row_into_view(next);
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
                                            None,
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
                                    let came = path();
                                    came_from.set(came.clone());
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
                                            Some(came),
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
                                scroll_dir_row_into_view(idx);
                                if let Some(p) = vis2.get(idx).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            " " => {
                                e.prevent_default();
                                toggle_preview_video();
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
                        }
                    }
                }
            },

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-foreground/[0.07] bg-foreground/[0.06] px-4 font-sans text-xs text-muted-foreground",
                {type_icon(&header_path, mode() == Mode::Dir, "h-4 w-4 shrink-0 text-foreground/80")}
                span { class: "truncate text-foreground/90", "{header_path}" }
                if dirty() {
                    span { class: "h-1.5 w-1.5 shrink-0 rounded-full bg-cyan-300", title: "unsaved" }
                }
                div { class: "flex-1" }
                {
                    let lbl = ed_label();
                    (!lbl.is_empty() && mode() == Mode::Text).then(|| rsx! {
                        span {
                            class: "shrink-0 rounded bg-cyan-400/15 px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-cyan-700 dark:text-cyan-200",
                            "{lbl}"
                        }
                    })
                }
                {
                    lsp_status().map(|s| {
                        let (dot, label) = match s.state {
                            LspServerState::Ready => ("text-ansi-2", s.server.clone()),
                            LspServerState::Starting => ("text-ansi-3", format!("{} starting\u{2026}", s.server)),
                            LspServerState::Missing => ("text-ansi-1", format!("{} \u{2014} not installed", s.server)),
                        };
                        rsx! {
                            div {
                                class: "flex shrink-0 items-center gap-1.5 text-[11px]",
                                title: "LSP",
                                span { class: "{dot}", "\u{25CF}" }
                                span { class: "text-foreground/70", "{label}" }
                            }
                        }
                    })
                }
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
                Mode::Media(kind) => rsx! {
                    div { class: "flex min-h-0 flex-1 items-center justify-center overflow-auto p-4",
                        if let Some(m) = media() {
                            match kind {
                                MediaKind::Image => rsx! {
                                    img { src: "{m.url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
                                },
                                MediaKind::Video => rsx! {
                                    video {
                                        src: "{m.url}",
                                        controls: true,
                                        autoplay: false,
                                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20",
                                    }
                                },
                                MediaKind::Audio => rsx! {
                                    audio { src: "{m.url}", controls: true, class: "w-2/3" }
                                },
                                MediaKind::Pdf => {
                                    let display = path();
                                    let abs = m.abs_path.clone();
                                    rsx! {
                                        div { class: "flex flex-col items-center gap-3 rounded-2xl bg-white/[0.03] px-8 py-6 ring-1 ring-inset ring-cyan-400/15 backdrop-blur-2xl",
                                            span { class: "text-xs uppercase tracking-wide text-foreground/70", "PDF" }
                                            span { class: "max-w-md truncate text-sm text-foreground/90", "{display}" }
                                            button {
                                                class: "rounded-lg bg-cyan-400/15 px-3 py-1.5 text-xs font-semibold text-cyan-200 hover:bg-cyan-400/25",
                                                onclick: move |_| {
                                                    let _ = try_cef_bin_emit_rkyv(&FileOpenExternalRequest { path: abs.clone() });
                                                },
                                                "Open externally"
                                            }
                                        }
                                    }
                                }
                            }
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
                                    class: if e.name == cur_basename { "flex items-center gap-2 rounded-md bg-cyan-400/10 px-2 py-1 text-foreground shadow-[inset_2px_0_0_0_rgba(34,211,238,0.6)]" } else { "flex items-center gap-2 rounded-md px-2 py-1 text-foreground/45 transition-colors hover:bg-foreground/[0.04]" },
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
                                            id: "dir-row-{i}",
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

                        div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-foreground/[0.02] p-4 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                            {render_preview(&preview())}
                        }
                    }
                },
                Mode::Text => rsx! {
                    if show_diff() {
                        DiffView { path: git_path, nonce: git_nonce }
                    } else {
                        {
                            let (cw, ch) = cell_dims();
                            let gutter = gw as f64 * cw + 36.0;
                            let cx = gutter + cursor().col as f64 * cw;
                            let cy = cursor().row as f64 * ch;
                            let spacer = total_rows() as f64 * ch;
                            let txtcol = if composing() { "inherit" } else { "transparent" };
                            rsx! {
                                div {
                                    id: "file-scroll",
                                    class: "relative min-h-0 flex-1 overflow-auto",
                                    onmouseleave: move |_| {
                                        lsp_hover.set(None);
                                        hover_pos.set(None);
                                        gutter_hover.set(false);
                                    },
                                    onscroll: move |_| {
                                        let (_, ch) = cell_dims();
                                        if ch <= 0.0 {
                                            return;
                                        }
                                        let Some(el) = scroll_el() else {
                                            return;
                                        };
                                        let vis_first = (el.scroll_top() as f64 / ch).floor().max(0.0) as u32;
                                        let vis_rows = (el.client_height() as f64 / ch).ceil() as u32 + 1;
                                        let rfirst = first_row();
                                        let rend = rfirst + lines.read().len() as u32;
                                        let near_top = vis_first < rfirst.saturating_add(SCROLL_EDGE);
                                        let near_bot = vis_first + vis_rows + SCROLL_EDGE > rend;
                                        if (near_top || near_bot) && last_scroll_req() != vis_first {
                                            last_scroll_req.set(vis_first);
                                            let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_row: vis_first });
                                        }
                                    },
                                    div { class: "relative", style: "height:{spacer}px;",
                                        for (i, line) in lines().iter().enumerate() {
                                            {
                                                let ln = line.line_no;
                                                let lt = (first_row() + i as u32) as f64 * ch;
                                                let fold = line.fold;
                                                let diags = diagnostics();
                                                let sev = line_severity(&diags, ln);
                                                let line_diags: Vec<FileDiagnostic> = diags
                                                    .iter()
                                                    .filter(|d| d.line == ln)
                                                    .cloned()
                                                    .collect();
                                                rsx! {
                                                    div {
                                                        key: "{ln}",
                                                        class: "group flex hover:bg-foreground/[0.035]",
                                                        style: "position:absolute;left:0;right:0;top:{lt}px;",
                                                        onmousedown: move |e: Event<MouseData>| {
                                                            e.prevent_default();
                                                            ctx_menu.set(None);
                                                            let (cw, _) = cell_dims();
                                                            let g = gw as f64 * cw + 36.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::MouseEvent>()
                                                                && let Some(t) = raw
                                                                    .current_target()
                                                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                            {
                                                                let rect = t.get_bounding_client_rect();
                                                                let x = raw.client_x() as f64 - rect.left() - g;
                                                                let col = if cw > 0.0 {
                                                                    (x / cw).round().max(0.0) as u32
                                                                } else {
                                                                    0
                                                                };
                                                                if raw.meta_key() {
                                                                    let _ = try_cef_bin_emit_rkyv(&FileDefinitionRequest {
                                                                        line: ln,
                                                                        col,
                                                                    });
                                                                } else {
                                                                    let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                                        line: ln,
                                                                        col,
                                                                        extend: raw.shift_key(),
                                                                    });
                                                                }
                                                            }
                                                            focus_file_input();
                                                        },
                                                        oncontextmenu: move |e: Event<MouseData>| {
                                                            e.prevent_default();
                                                            let (cw, _) = cell_dims();
                                                            let g = gw as f64 * cw + 36.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::MouseEvent>()
                                                                && let Some(t) = raw
                                                                    .current_target()
                                                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                            {
                                                                let rect = t.get_bounding_client_rect();
                                                                let x = raw.client_x() as f64 - rect.left() - g;
                                                                let col = if cw > 0.0 {
                                                                    (x / cw).round().max(0.0) as u32
                                                                } else {
                                                                    0
                                                                };
                                                                ctx_menu.set(Some((
                                                                    raw.client_x() as f64,
                                                                    raw.client_y() as f64,
                                                                    ln,
                                                                    col,
                                                                )));
                                                            }
                                                        },
                                                        onmousemove: move |e: Event<MouseData>| {
                                                            let (cw, _) = cell_dims();
                                                            let g = gw as f64 * cw + 36.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::MouseEvent>()
                                                                && let Some(t) = raw
                                                                    .current_target()
                                                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                            {
                                                                let rect = t.get_bounding_client_rect();
                                                                let x = raw.client_x() as f64 - rect.left() - g;
                                                                let in_gutter = x < 0.0;
                                                                if gutter_hover() != in_gutter {
                                                                    gutter_hover.set(in_gutter);
                                                                }
                                                                if x < 0.0 {
                                                                    return;
                                                                }
                                                                let col = if cw > 0.0 {
                                                                    (x / cw).floor().max(0.0) as u32
                                                                } else {
                                                                    0
                                                                };
                                                                if hover_pos() != Some((ln, col)) {
                                                                    hover_pos.set(Some((ln, col)));
                                                                    lsp_hover.set(None);
                                                                    let _ = try_cef_bin_emit_rkyv(&FileHoverRequest {
                                                                        line: ln,
                                                                        col,
                                                                    });
                                                                }
                                                            }
                                                        },
                                                        span {
                                                            class: "sticky left-0 z-[1] relative flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums",
                                                            style: "min-width:calc(var(--cw, 1ch) * {gw} + 2.25rem);",
                                                            if let Some(s) = sev {
                                                                span { class: "pointer-events-none absolute left-1 {severity_color_class(s)}", "●" }
                                                            }
                                                            span { class: "text-right opacity-40 group-hover:opacity-90", "{ln + 1}" }
                                                            match fold {
                                                                FoldGutter::Open => {
                                                                    let vis = if gutter_hover() { "opacity-100" } else { "opacity-0" };
                                                                    rsx! {
                                                                        span {
                                                                            class: "absolute right-1 flex h-full cursor-pointer items-center text-base leading-none text-foreground/50 transition-opacity hover:!text-foreground {vis}",
                                                                            onmousedown: move |e: Event<MouseData>| {
                                                                                e.stop_propagation();
                                                                                e.prevent_default();
                                                                                let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                                            },
                                                                            "⌄"
                                                                        }
                                                                    }
                                                                }
                                                                FoldGutter::Collapsed => rsx! {
                                                                    span {
                                                                        class: "absolute right-1 flex h-full cursor-pointer items-center text-base leading-none text-foreground/70 hover:!text-foreground",
                                                                        onmousedown: move |e: Event<MouseData>| {
                                                                            e.stop_propagation();
                                                                            e.prevent_default();
                                                                            let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                                        },
                                                                        "›"
                                                                    }
                                                                },
                                                                FoldGutter::None => rsx! {},
                                                            }
                                                        }
                                                        span { class: "relative whitespace-pre pr-8",
                                                            for (i, s) in line.spans.iter().enumerate() {
                                                                span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                                                            }
                                                            for (di, d) in line_diags.iter().enumerate() {
                                                                {
                                                                    let color = match d.severity {
                                                                        DiagSeverity::Error => "rgb(239,68,68)",
                                                                        DiagSeverity::Warning => "rgb(245,158,11)",
                                                                        DiagSeverity::Info => "rgb(56,189,248)",
                                                                        DiagSeverity::Hint => "rgb(34,211,238)",
                                                                    };
                                                                    let dc = d.clone();
                                                                    rsx! {
                                                                        span {
                                                                            key: "d{di}",
                                                                            style: squiggle_style(d.start_col, d.end_col, color),
                                                                            onmouseenter: move |_| hover_diag.set(Some(dc.clone())),
                                                                            onmouseleave: move |_| hover_diag.set(None),
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if fold == FoldGutter::Collapsed {
                                                                span {
                                                                    class: "ml-1 rounded bg-white/10 px-1 text-foreground/40",
                                                                    "⋯"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        for s in sel().iter() {
                                            {
                                                let top = s.row as f64 * ch;
                                                let left = gutter + s.start as f64 * cw;
                                                let style = if s.end == u32::MAX {
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;right:0;")
                                                } else {
                                                    let w = (s.end.saturating_sub(s.start)) as f64 * cw;
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;width:{w}px;")
                                                };
                                                rsx! {
                                                    div {
                                                        key: "sel{s.line}",
                                                        class: "pointer-events-none absolute z-0 bg-cyan-400/20",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        div {
                                            class: "pointer-events-none absolute z-20 w-[2px] bg-cyan-300",
                                            style: "left:{cx}px;top:{cy}px;height:{ch}px;",
                                        }

                                        textarea {
                                            id: "file-input",
                                            class: "absolute z-10 resize-none overflow-hidden whitespace-pre border-0 bg-transparent p-0 caret-transparent outline-none",
                                            style: "left:{cx}px;top:{cy}px;min-width:2ch;height:{ch}px;color:{txtcol};",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| composing.set(true),
                                            oncompositionend: move |_| {
                                                composing.set(false);
                                                send_committed_text();
                                            },
                                            oninput: move |_| {
                                                if composing() {
                                                    return;
                                                }
                                                send_committed_text();
                                            },
                                            onkeydown: move |e: Event<KeyboardData>| {
                                                let dd = e.data();
                                                let Some(raw) = dd.downcast::<web_sys::KeyboardEvent>() else {
                                                    return;
                                                };
                                                if raw.is_composing() {
                                                    return;
                                                }
                                                let key = raw.key();
                                                if comp_open() && !comp_keys.is_empty() {
                                                    match key.as_str() {
                                                        "ArrowDown" => {
                                                            e.prevent_default();
                                                            comp_sel.set((comp_sel_clamped + 1).min(comp_keys.len() - 1));
                                                            return;
                                                        }
                                                        "ArrowUp" => {
                                                            e.prevent_default();
                                                            comp_sel.set(comp_sel_clamped.saturating_sub(1));
                                                            return;
                                                        }
                                                        "Enter" | "Tab" => {
                                                            e.prevent_default();
                                                            if let Some(it) = comp_keys.get(comp_sel_clamped) {
                                                                let (cline, cfrom) = comp_anchor();
                                                                let _ = try_cef_bin_emit_rkyv(&FileCompletionCommit {
                                                                    line: cline,
                                                                    replace_from_col: cfrom,
                                                                    text: it.insert_text.clone(),
                                                                });
                                                            }
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        "Escape" => {
                                                            e.prevent_default();
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                let mods = key_mods(raw);
                                                let chord = mods.ctrl || mods.alt || mods.meta;
                                                if ed_mode().accepts_text() && !chord && is_text_key(&key) {
                                                    return;
                                                }
                                                e.prevent_default();
                                                let _ = try_cef_bin_emit_rkyv(&FileKeyEvent {
                                                    key,
                                                    code: raw.code(),
                                                    mods,
                                                    repeat: raw.repeat(),
                                                });
                                            },
                                        }

                                        {
                                            lsp_hover().map(|h| {
                                                let (cw, ch) = cell_dims();
                                                let Some(i) = lines().iter().position(|l| l.line_no == h.line) else {
                                                    return rsx! {};
                                                };
                                                let hrow = first_row() + i as u32;
                                                let top = hrow as f64 * ch + ch;
                                                let left = gw as f64 * cw + 36.0 + h.col as f64 * cw;
                                                rsx! {
                                                    div {
                                                        class: "pointer-events-none absolute z-30 max-w-2xl overflow-hidden rounded-xl bg-foreground/[0.05] px-3 py-2 text-xs leading-snug text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                                                        style: "left:{left}px;top:{top}px;",
                                                        for (bi, b) in h.blocks.iter().enumerate() {
                                                            if b.code {
                                                                div {
                                                                    key: "b{bi}",
                                                                    class: "my-1 max-w-full overflow-x-auto whitespace-pre font-mono",
                                                                    for line in b.lines.iter() {
                                                                        div { key: "{line.line_no}",
                                                                            for (si, s) in line.spans.iter().enumerate() {
                                                                                span { key: "{si}", style: "{span_style(s)}", "{s.text}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                div {
                                                                    key: "b{bi}",
                                                                    class: "whitespace-pre-wrap opacity-80",
                                                                    "{b.text}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            })
                                        }

                                        {
                                            (comp_open() && !comp_filtered.is_empty()).then(|| {
                                                let (cline, cfrom) = comp_anchor();
                                                let top = cline as f64 * ch + ch;
                                                let left = gutter + cfrom as f64 * cw;
                                                rsx! {
                                                    div {
                                                        class: "absolute z-40 max-h-56 min-w-48 overflow-auto rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                                                        style: "left:{left}px;top:{top}px;",
                                                        for (i, it) in comp_filtered.iter().enumerate() {
                                                            div {
                                                                key: "{i}",
                                                                class: if i == comp_sel_clamped { "flex items-center gap-2 px-3 py-1 bg-cyan-400/15" } else { "flex items-center gap-2 px-3 py-1" },
                                                                span { class: "truncate", "{it.label}" }
                                                                if !it.detail.is_empty() {
                                                                    span { class: "ml-auto truncate text-[10px] text-foreground/40", "{it.detail}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            })
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }

            {
                hover_diag().map(|d| rsx! {
                    div {
                        class: "pointer-events-none absolute right-4 bottom-12 z-50 max-w-md rounded-xl bg-foreground/[0.04] px-3 py-2 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        div { class: "flex items-center gap-2",
                            span { class: "{severity_color_class(d.severity)}", "●" }
                            span { class: "whitespace-pre-wrap", "{d.message}" }
                        }
                        if let Some(src) = d.source.as_ref() {
                            div { class: "mt-1 opacity-50", "{src}" }
                        }
                    }
                })
            }

            {
                ctx_menu().map(|(x, y, line, col)| rsx! {
                    div {
                        class: "fixed inset-0 z-40",
                        onmousedown: move |_| ctx_menu.set(None),
                        oncontextmenu: move |e| {
                            e.prevent_default();
                            ctx_menu.set(None);
                        },
                    }
                    div {
                        class: "fixed z-50 min-w-44 overflow-hidden rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        style: "left:{x}px;top:{y}px;",
                        div {
                            class: "cursor-default px-3 py-1.5 hover:bg-cyan-400/15",
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&FileDefinitionRequest { line, col });
                                ctx_menu.set(None);
                            },
                            "Go to Definition"
                        }
                        div {
                            class: "cursor-default px-3 py-1.5 hover:bg-cyan-400/15",
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&FileReferencesRequest { line, col });
                                ctx_menu.set(None);
                            },
                            "Find References"
                        }
                    }
                })
            }

            {
                refs_open().then(|| {
                    let items = refs();
                    rsx! {
                        div {
                            id: "refs-panel",
                            tabindex: "0",
                            class: "absolute bottom-8 left-4 right-4 z-40 max-h-64 overflow-auto rounded-xl bg-foreground/[0.05] p-1 text-xs text-foreground/90 outline-none ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                            onkeydown: move |e: Event<KeyboardData>| {
                                let key = e
                                    .data()
                                    .downcast::<web_sys::KeyboardEvent>()
                                    .map(|k| k.key())
                                    .unwrap_or_default();
                                let len = refs.read().len();
                                match key.as_str() {
                                    "ArrowDown" | "j" => {
                                        e.prevent_default();
                                        if len > 0 {
                                            refs_sel.set((refs_sel() + 1).min(len - 1));
                                        }
                                    }
                                    "ArrowUp" | "k" => {
                                        e.prevent_default();
                                        refs_sel.set(refs_sel().saturating_sub(1));
                                    }
                                    "Enter" => {
                                        e.prevent_default();
                                        if let Some(it) = refs.read().get(refs_sel()) {
                                            let _ = try_cef_bin_emit_rkyv(&FileGotoRequest {
                                                path: it.path.clone(),
                                                line: it.line,
                                                col: it.col,
                                            });
                                        }
                                        refs_open.set(false);
                                        focus_file_input();
                                    }
                                    "Escape" => {
                                        e.prevent_default();
                                        refs_open.set(false);
                                        focus_file_input();
                                    }
                                    _ => {}
                                }
                            },
                            div { class: "px-2 py-1 text-[10px] uppercase tracking-wide text-foreground/50",
                                "{items.len()} references"
                            }
                            for (i, it) in items.iter().enumerate() {
                                {
                                    let nav = (it.path.clone(), it.line, it.col);
                                    rsx! {
                                        div {
                                            key: "{i}",
                                            class: if i == refs_sel() { "flex gap-2 rounded px-2 py-1 bg-cyan-400/15" } else { "flex gap-2 rounded px-2 py-1 hover:bg-foreground/[0.05]" },
                                            onmousedown: move |e: Event<MouseData>| {
                                                e.prevent_default();
                                                let _ = try_cef_bin_emit_rkyv(&FileGotoRequest {
                                                    path: nav.0.clone(),
                                                    line: nav.1,
                                                    col: nav.2,
                                                });
                                                refs_open.set(false);
                                                focus_file_input();
                                            },
                                            span { class: "shrink-0 text-cyan-700/80 dark:text-cyan-300/80", "{it.display}" }
                                            span { class: "truncate text-foreground/60", "{it.preview}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            }

            GitFooter {
                path: git_path,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
            }
        }
    }
}

fn scroll_dir_row_into_view(idx: usize) {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&format!("dir-row-{idx}")))
    else {
        return;
    };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

fn toggle_preview_video() {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("preview-video"))
    else {
        return;
    };
    let target: &JsValue = el.as_ref();
    let paused = js_sys::Reflect::get(target, &JsValue::from_str("paused"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let method = if paused { "play" } else { "pause" };
    if let Ok(f) = js_sys::Reflect::get(target, &JsValue::from_str(method))
        && let Ok(f) = f.dyn_into::<js_sys::Function>()
    {
        let _ = f.call0(target);
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

fn focus_file_input() {
    focus_by_id(INPUT_ID);
}

fn focus_by_id(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

fn scroll_el() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SCROLL_ID))
}

fn ensure_line_visible(line: u32, ch: f64) {
    if ch <= 0.0 {
        return;
    }
    let Some(el) = scroll_el() else {
        return;
    };
    let view_h = el.client_height() as f64;
    if view_h <= 0.0 {
        return;
    }
    let top = line as f64 * ch;
    let view_top = el.scroll_top() as f64;
    if top < view_top {
        el.set_scroll_top(top as i32);
    } else if top + ch > view_top + view_h {
        el.set_scroll_top((top + ch - view_h) as i32);
    }
}

fn reset_file_scroll() {
    if let Some(el) = scroll_el() {
        el.set_scroll_top(0);
    }
}

fn send_committed_text() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(INPUT_ID))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        let v = el.value();
        if !v.is_empty() {
            let _ = try_cef_bin_emit_rkyv(&FileTextInput { text: v });
            el.set_value("");
        }
    }
}

fn key_mods(raw: &web_sys::KeyboardEvent) -> KeyMods {
    KeyMods {
        ctrl: raw.ctrl_key(),
        alt: raw.alt_key(),
        shift: raw.shift_key(),
        meta: raw.meta_key(),
    }
}

fn is_text_key(key: &str) -> bool {
    key.chars().count() == 1
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

/// Emit the current on-screen rect of the native video host element so the backend
/// can position the `AVPlayer` overlay over it.
fn emit_video_rect(path: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(VIDEO_HOST_ID) else {
        return;
    };
    let rect = el.get_bounding_client_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let _ = try_cef_bin_emit_rkyv(&FileVideoRect {
        path: path.to_string(),
        x: rect.left() as f32,
        y: rect.top() as f32,
        w: rect.width() as f32,
        h: rect.height() as f32,
    });
}

/// Report the video host rect now and on every subsequent resize (window/layout),
/// keeping the native overlay aligned with the page element.
fn report_video_rect(path: String) {
    emit_video_rect(&path);
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(VIDEO_HOST_ID) else {
        return;
    };
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        emit_video_rect(&path);
    }) as Box<dyn FnMut(JsValue)>);
    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&el);
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

    let vh = document
        .get_element_by_id("file-scroll")
        .map(|e| e.client_height() as f64)
        .filter(|h| *h > 0.0)
        .unwrap_or_else(|| container.client_height() as f64);

    let _ = try_cef_bin_emit_rkyv(&FileResizeEvent {
        char_height: ch as f32,
        viewport_height: vh as f32,
    });
}
