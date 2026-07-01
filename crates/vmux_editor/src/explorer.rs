#![allow(non_snake_case)]

//! Dumb Explorer panel for the file editor page: renders the backend-pushed
//! tree, open-editors, and outline view-models and emits user intents. All
//! state lives in the native plugin; this module only renders and forwards.

use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::file_icon::type_icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};

fn open_file(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FileOpenEvent { path });
}

fn toggle_dir(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerTreeToggle { path });
}

fn close_editor(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerCloseEditor { path });
}

fn goto_line(path: String, line: u32) {
    let _ = try_cef_bin_emit_rkyv(&ExplorerGoto { path, line });
}

fn chevron(expanded: bool) -> Element {
    rsx! {
        span {
            class: "inline-block w-3 shrink-0 text-center text-[10px] text-foreground/50 transition-transform",
            style: if expanded { "transform:rotate(90deg)" } else { "" },
            "\u{203A}"
        }
    }
}

fn section_header(title: String, open: Signal<bool>, on_toggle: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1 px-2 py-1 cursor-default text-[11px] font-bold uppercase tracking-wide text-foreground/70 hover:text-foreground",
            onclick: move |_| on_toggle.call(()),
            {chevron(open())}
            span { class: "truncate", "{title}" }
        }
    }
}

#[component]
pub fn ExplorerPanel() -> Element {
    let mut root_name = use_signal(|| "Explorer".to_string());
    let mut rows = use_signal(Vec::<TreeRow>::new);
    let mut open_editors = use_signal(Vec::<OpenEditorItem>::new);
    let mut outline = use_signal(Vec::<OutlineRow>::new);
    let mut current_path = use_signal(String::new);
    let mut show_open = use_signal(|| true);
    let mut show_files = use_signal(|| true);
    let mut show_outline = use_signal(|| true);

    let _tree = use_bin_event_listener::<ExplorerTreeEvent, _>(EXPLORER_TREE_EVENT, move |e| {
        root_name.set(e.root_name);
        rows.set(e.rows);
    });
    let _open =
        use_bin_event_listener::<OpenEditorsEvent, _>(EXPLORER_OPEN_EDITORS_EVENT, move |e| {
            open_editors.set(e.items);
        });
    let _outline = use_bin_event_listener::<OutlineEvent, _>(EXPLORER_OUTLINE_EVENT, move |e| {
        outline.set(e.items);
    });
    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        current_path.set(m.abs_path);
    });

    rsx! {
        div { class: "flex h-full w-full flex-col overflow-hidden bg-foreground/[0.04] font-sans text-xs text-foreground select-none",
            div { class: "flex h-9 shrink-0 items-center px-4 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                "Explorer"
            }
            div { class: "min-h-0 flex-1 overflow-y-auto pb-4",

                {section_header("Open Editors".to_string(), show_open, EventHandler::new(move |_| show_open.set(!show_open())))}
                if show_open() {
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
                                        "group flex items-center gap-1 px-2 py-0.5 cursor-default bg-cyan-400/12 text-foreground"
                                    } else {
                                        "group flex items-center gap-1 px-2 py-0.5 cursor-default text-foreground/75 hover:bg-foreground/[0.08]"
                                    },
                                    style: "padding-left:20px;",
                                    onclick: move |_| open_file(p_open.clone()),
                                    span {
                                        class: "inline-block w-3 shrink-0 cursor-default text-center text-foreground/50 opacity-0 group-hover:opacity-100 hover:text-foreground",
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

                {section_header(root_name(), show_files, EventHandler::new(move |_| show_files.set(!show_files())))}
                if show_files() {
                    for r in rows() {
                        {
                            let path_t = r.path.clone();
                            let is_dir = r.is_dir;
                            let pad = (r.depth as u32) * 12 + 8;
                            rsx! {
                                div {
                                    key: "{r.path}",
                                    class: "flex items-center gap-1 px-1 py-0.5 cursor-default text-foreground/80 hover:bg-foreground/[0.08]",
                                    style: "padding-left:{pad}px;",
                                    title: "{r.path}",
                                    onclick: move |_| {
                                        if is_dir {
                                            toggle_dir(path_t.clone());
                                        } else {
                                            open_file(path_t.clone());
                                        }
                                    },
                                    if is_dir {
                                        {chevron(r.expanded)}
                                    } else {
                                        span { class: "inline-block w-3 shrink-0" }
                                    }
                                    {type_icon(&r.path, is_dir, "h-4 w-4 shrink-0 opacity-80")}
                                    span { class: "truncate", "{r.name}" }
                                }
                            }
                        }
                    }
                }

                {section_header("Outline".to_string(), show_outline, EventHandler::new(move |_| show_outline.set(!show_outline())))}
                if show_outline() {
                    for s in outline() {
                        {
                            let line = s.line;
                            let pad = (s.depth as u32) * 12 + 20;
                            rsx! {
                                div {
                                    key: "{s.line}-{s.name}",
                                    class: "flex items-center gap-1 px-1 py-0.5 cursor-default text-foreground/75 hover:bg-foreground/[0.08]",
                                    style: "padding-left:{pad}px;",
                                    onclick: move |_| goto_line(current_path(), line),
                                    {outline_glyph(s.kind)}
                                    span { class: "truncate", "{s.name}" }
                                }
                            }
                        }
                    }
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
