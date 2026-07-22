#![allow(non_snake_case)]

//! Explorer panel rendering, motion, context menus, and user intents.

use std::collections::HashSet;
use std::path::Path;

use crate::page_model::merge_tree_motion_rows;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::file_icon::type_icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use wasm_bindgen::{JsCast, closure::Closure};

const TREE_MOTION_MS: i32 = 170;
const NOTICE_MS: i32 = 2400;

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
    let _ = try_cef_bin_emit_rkyv(&FileOpenEvent { path });
}

fn toggle_dir(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerTreeToggle { path });
}

fn prefetch_dir(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerTreePrefetch { path });
}

fn refresh_dir(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerTreeRefresh { path });
}

fn close_editor(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerCloseEditor { path });
}

fn goto_line(line: u32) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerGoto {
        path: String::new(),
        line,
    });
}

fn open_search_match(result: ExplorerSearchMatch) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerSearchOpen {
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
    let _ = try_cef_bin_emit_rkyv(&ExplorerCreate {
        parent,
        name,
        is_dir,
    });
}

fn rename_entry(path: String, name: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerRename { path, name });
}

fn delete_entry(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerDelete { path });
}

fn menu_position(x: f64, y: f64) -> (f64, f64) {
    let Some(window) = web_sys::window() else {
        return (x, y);
    };
    let width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(x + 200.0);
    let height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(y + 240.0);
    (
        x.min((width - 190.0).max(8.0)),
        y.min((height - 220.0).max(8.0)),
    )
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

fn schedule_tree_focus(path: String, mut generation: Signal<u32>) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    let Some(window) = web_sys::window() else {
        return;
    };
    let focus = Closure::once(move || {
        if generation() != id {
            return;
        }
        let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&tree_row_id(&path)))
        else {
            return;
        };
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
        element.scroll_into_view_with_scroll_into_view_options(&options);
        if let Ok(element) = element.dyn_into::<web_sys::HtmlElement>() {
            let _ = element.focus();
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        focus.as_ref().unchecked_ref(),
        TREE_MOTION_MS + 20,
    );
    focus.forget();
}

fn cancel_tree_focus(mut generation: Signal<u32>) {
    let id = generation.peek().wrapping_add(1);
    generation.set(id);
}

fn reconcile_rows(
    mut rows: Signal<Vec<MotionRow>>,
    mut generation: Signal<u32>,
    next: Vec<TreeRow>,
) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    let next_paths: HashSet<String> = next.iter().map(|row| row.path.clone()).collect();
    let current = rows
        .read()
        .iter()
        .map(|motion| motion.row.clone())
        .collect::<Vec<_>>();
    let merged = merge_tree_motion_rows(&current, &next)
        .into_iter()
        .map(|(row, visible)| MotionRow { row, visible })
        .collect();
    rows.set(merged);
    if let Some(window) = web_sys::window() {
        let enter_paths = next_paths;
        let enter = Closure::once(move || {
            if generation() != id {
                return;
            }
            let mut current = rows.read().clone();
            for item in &mut current {
                if enter_paths.contains(&item.row.path) {
                    item.visible = true;
                }
            }
            rows.set(current);
        });
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            enter.as_ref().unchecked_ref(),
            0,
        );
        enter.forget();

        let settle = Closure::once(move || {
            if generation() == id {
                rows.set(
                    next.into_iter()
                        .map(|row| MotionRow { row, visible: true })
                        .collect(),
                );
            }
        });
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            settle.as_ref().unchecked_ref(),
            TREE_MOTION_MS,
        );
        settle.forget();
    } else {
        rows.set(
            next.into_iter()
                .map(|row| MotionRow { row, visible: true })
                .collect(),
        );
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
    let Some(window) = web_sys::window() else {
        return;
    };
    let clear = Closure::once(move || {
        if generation() == id {
            notice.set(None);
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        clear.as_ref().unchecked_ref(),
        NOTICE_MS,
    );
    clear.forget();
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

fn chevron(expanded: bool, loading: bool) -> Element {
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

fn section_header(title: String, open: Signal<bool>, on_toggle: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1 px-2 py-1 cursor-default text-[11px] font-bold uppercase tracking-wide text-foreground/70 transition-colors hover:text-foreground",
            onclick: move |_| on_toggle.call(()),
            {chevron(open(), false)}
            span { class: "truncate", "{title}" }
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
    let focus_generation = use_signal(|| 0u32);
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

    use_effect(move || {
        if !visible() {
            cancel_tree_focus(focus_generation);
        }
    });

    let _tree = use_bin_event_listener::<ExplorerTreeEvent, _>(EXPLORER_TREE_EVENT, move |e| {
        root_name.set(e.root_name);
        root_path.set(e.root_path);
        current_path.set(e.current_path);
        root_loading.set(e.loading);
        reconcile_rows(rows, row_generation, e.rows);
        if visible() && !e.focus_path.is_empty() {
            schedule_tree_focus(e.focus_path, focus_generation);
        }
    });
    let _focus = use_bin_event_listener::<ExplorerFocusEvent, _>(EXPLORER_FOCUS_EVENT, move |e| {
        if current_path() != e.path {
            current_path.set(e.path.clone());
        }
        if visible() {
            schedule_tree_focus(e.path, focus_generation);
        }
    });
    let _open =
        use_bin_event_listener::<OpenEditorsEvent, _>(EXPLORER_OPEN_EDITORS_EVENT, move |e| {
            open_editors.set(e.items);
        });
    let _outline = use_bin_event_listener::<OutlineEvent, _>(EXPLORER_OUTLINE_EVENT, move |e| {
        outline.set(e.items);
    });
    let _search =
        use_bin_event_listener::<ExplorerSearchEvent, _>(EXPLORER_SEARCH_EVENT, move |e| {
            search.set(Some(e));
            show_search.set(true);
        });
    let _fs_result =
        use_bin_event_listener::<ExplorerFsResult, _>(EXPLORER_FS_RESULT_EVENT, move |e| {
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

    rsx! {
        div { class: "relative flex h-full w-full flex-col overflow-hidden bg-foreground/[0.04] font-sans text-xs text-foreground select-none",
            div { class: "flex h-9 shrink-0 items-center px-4 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                {translate("editor-explorer")}
            }
            div { class: "min-h-0 flex-1 overflow-y-auto pb-4",
                {section_header(translate("editor-open-editors"), show_open, EventHandler::new(move |_| show_open.set(!show_open())))}
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
                                        {type_icon(&it.path, false, "h-4 w-4 shrink-0 opacity-80")}
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
                    {section_header("Search".to_string(), show_search, EventHandler::new(move |_| show_search.set(!show_search())))}
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
                        let (x, y) = menu_position(coordinates.x, coordinates.y);
                        menu.set(Some(TreeMenu {
                            path: root_path(),
                            name: root_name(),
                            is_dir: true,
                            is_root: true,
                            x,
                            y,
                        }));
                    },
                    {section_header(root_name(), show_files, EventHandler::new(move |_| show_files.set(!show_files())))}
                }
                div { class: "{files_body}",
                    div { class: "min-h-0 overflow-hidden",
                        if root_loading() && rows.read().is_empty() {
                            div { class: "flex h-6 items-center gap-2 px-3 text-foreground/45",
                                span { class: "h-3 w-3 animate-spin rounded-full border border-foreground/20 border-t-foreground/60" }
                                {translate("common-loading")}
                            }
                        }
                        for motion in rows() {
                            {
                                let row = motion.row.clone();
                                let path_click = row.path.clone();
                                let path_prefetch = row.path.clone();
                                let path_menu = row.path.clone();
                                let name_menu = row.name.clone();
                                let is_dir = row.is_dir;
                                let active = row.path == current_path();
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
                                                class: if active {
                                                    "flex h-[22px] items-center gap-1 px-1 cursor-default bg-cyan-400/12 text-foreground outline-none transition-colors duration-100"
                                                } else {
                                                    "flex h-[22px] items-center gap-1 px-1 cursor-default text-foreground/80 outline-none transition-colors duration-100 hover:bg-foreground/[0.08]"
                                                },
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
                                                    let (x, y) = menu_position(coordinates.x, coordinates.y);
                                                    menu.set(Some(TreeMenu {
                                                        path: path_menu.clone(),
                                                        name: name_menu.clone(),
                                                        is_dir,
                                                        is_root: false,
                                                        x,
                                                        y,
                                                    }));
                                                },
                                                onclick: move |_| {
                                                    if is_dir {
                                                        toggle_dir(path_click.clone());
                                                    } else {
                                                        open_file(path_click.clone());
                                                    }
                                                },
                                                if is_dir {
                                                    {chevron(row.expanded, row.loading)}
                                                } else {
                                                    span { class: "inline-block w-4 shrink-0" }
                                                }
                                                {type_icon(&row.path, is_dir, "h-4 w-4 shrink-0 opacity-80")}
                                                span { class: "truncate", "{row.name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                {section_header(translate("editor-outline"), show_outline, EventHandler::new(move |_| show_outline.set(!show_outline())))}
                div { class: "{outline_body}",
                    div { class: "min-h-0 overflow-hidden",
                        for s in outline() {
                            {
                                let line = s.line;
                                let pad = (s.depth as u32) * 12 + 20;
                                rsx! {
                                    div {
                                        key: "{s.line}-{s.name}",
                                        class: "flex items-center gap-1 px-1 py-0.5 cursor-default text-foreground/75 transition-colors duration-100 hover:bg-foreground/[0.08]",
                                        style: "padding-left:{pad}px;",
                                        onclick: move |_| goto_line(line),
                                        {outline_glyph(s.kind)}
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
                    style: "left:{current.x}px;top:{current.y}px;",
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
                                onkeydown: move |e| {
                                    e.stop_propagation();
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
                        "absolute bottom-3 left-3 right-3 z-[997] animate-[dx-fade-zoom-in_150ms_ease-out_forwards] rounded-lg bg-emerald-500/90 px-3 py-2 text-left text-xs text-white shadow-lg"
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

fn outline_glyph(kind: u8) -> Element {
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
