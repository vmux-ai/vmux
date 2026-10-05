use dioxus::prelude::*;
use vmux_ecs::event::{ExplorerCloseEditor, FileFindRequest, FileOpenEvent, OpenEditorItem};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::scroll::ScrollIntoView;

use super::EditorFocus;

#[component]
pub(super) fn VimStatus(label: String) -> Element {
    if label.is_empty() {
        return rsx! {};
    }
    rsx! {
        span {
            class: "-ml-4 flex h-7 shrink-0 items-center bg-primary/20 px-3 text-[10px] font-semibold tracking-wider text-primary",
            "{label}"
        }
    }
}

#[component]
pub(super) fn FindBar(
    query: String,
    forward: bool,
    regex: bool,
    vim: bool,
    total: u32,
    index: u32,
) -> Element {
    let ime = use_ime_guard();
    let key_query = query.clone();
    let regex_query = query.clone();
    let previous_query = query.clone();
    let next_query = query.clone();
    let count = match (total, index) {
        (0, _) => translate("editor-find-no-results"),
        (total, 0) => format!("{total}"),
        (total, index) => format!("{index}/{total}"),
    };

    rsx! {
        div {
            class: "flex h-6 shrink-0 items-center gap-1 rounded-md bg-foreground/[0.06] pl-1 pr-1 ring-1 ring-inset ring-foreground/10",
            input {
                id: EditorFocus::FIND_INPUT_ID,
                r#type: "text",
                class: "w-40 bg-transparent font-sans text-[11px] text-foreground outline-none placeholder:text-muted-foreground",
                placeholder: translate("editor-find-placeholder"),
                value: "{query}",
                oninput: move |event| {
                    let _ = send(&FileFindRequest {
                        query: event.value(),
                        step: false,
                        reverse: false,
                        done: false,
                        regex,
                        forward,
                    });
                },
                oncompositionstart: move |_| ime.start(),
                oncompositionend: move |_| ime.commit(),
                onkeydown: move |event: Event<KeyboardData>| {
                    event.stop_propagation();
                    if ime.swallows(&event) {
                        return;
                    }
                    match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            let _ = send(&FileFindRequest {
                                query: key_query.clone(),
                                step: true,
                                reverse: event.modifiers().shift(),
                                done: false,
                                regex,
                                forward,
                            });
                            if vim {
                                EditorFocus::file();
                            }
                        }
                        Key::Escape => {
                            event.prevent_default();
                            let _ = send(&FileFindRequest {
                                done: true,
                                ..Default::default()
                            });
                            EditorFocus::file();
                        }
                        _ => {}
                    }
                },
            }
            button {
                r#type: "button",
                class: if regex {
                    "shrink-0 rounded bg-foreground/15 px-1 font-mono text-[10px] text-foreground"
                } else {
                    "shrink-0 rounded px-1 font-mono text-[10px] text-foreground/50 hover:bg-foreground/10 hover:text-foreground"
                },
                title: translate("editor-find-regex"),
                onclick: move |_| {
                    let _ = send(&FileFindRequest {
                        query: regex_query.clone(),
                        step: false,
                        reverse: false,
                        done: false,
                        regex: !regex,
                        forward,
                    });
                },
                ".*"
            }
            span {
                class: if total == 0 && !query.is_empty() {
                    "shrink-0 tabular-nums text-[10px] text-destructive"
                } else {
                    "shrink-0 tabular-nums text-[10px] text-muted-foreground"
                },
                "{count}"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-previous"),
                onclick: move |_| {
                    let _ = send(&FileFindRequest {
                        query: previous_query.clone(),
                        step: true,
                        reverse: true,
                        done: false,
                        regex,
                        forward,
                    });
                },
                "‹"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-next"),
                onclick: move |_| {
                    let _ = send(&FileFindRequest {
                        query: next_query.clone(),
                        step: true,
                        reverse: false,
                        done: false,
                        regex,
                        forward,
                    });
                },
                "›"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-close"),
                onclick: move |_| {
                    let _ = send(&FileFindRequest {
                        done: true,
                        ..Default::default()
                    });
                    EditorFocus::file();
                },
                "✕"
            }
        }
    }
}

#[component]
pub(super) fn EditorTabStrip(tabs: Vec<OpenEditorItem>) -> Element {
    let active_id = match tabs.iter().find(|tab| tab.active) {
        Some(tab) => format!("editor-tab-{}", tab.path),
        None => String::new(),
    };

    use_effect(use_reactive!(|active_id| {
        if active_id.is_empty() {
            return;
        }
        ScrollIntoView::nearest(&active_id);
    }));

    rsx! {
        div {
            class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto overflow-y-hidden py-1",
            for tab in tabs {
                EditorTab { key: "{tab.path}", tab }
            }
        }
    }
}

#[component]
fn EditorTab(tab: OpenEditorItem) -> Element {
    let open_path = tab.path.clone();
    let close_path = tab.path.clone();
    let element_id = format!("editor-tab-{}", tab.path);
    let class = match tab.active {
        true => {
            "group flex h-7 min-w-0 max-w-[14rem] shrink-0 cursor-default items-center gap-1.5 rounded-md bg-foreground/[0.10] px-2.5 text-ui text-foreground"
        }
        false => {
            "group flex h-7 min-w-0 max-w-[14rem] shrink-0 cursor-default items-center gap-1.5 rounded-md px-2.5 text-ui text-foreground/60 hover:bg-foreground/[0.05] hover:text-foreground/90"
        }
    };
    let close_title = match tab.dirty {
        true => translate("editor-unsaved"),
        false => translate("editor-close-editor"),
    };

    rsx! {
        div {
            id: element_id,
            class,
            title: "{tab.path}",
            onclick: move |_| {
                let _ = send(&FileOpenEvent {
                    path: open_path.clone(),
                });
            },
            TypeIcon { path: tab.path.clone(), is_dir: tab.is_dir, class: "h-4 w-4 shrink-0 opacity-80" }
            span { class: "truncate", "{tab.name}" }
            if !tab.context.is_empty() {
                span { class: "shrink-0 truncate text-[10px] text-muted-foreground/70", "{tab.context}" }
            }
            button {
                r#type: "button",
                class: "flex h-4 w-4 shrink-0 cursor-default items-center justify-center rounded-sm text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                aria_label: translate("editor-close-editor"),
                title: close_title,
                onclick: move |event: Event<MouseData>| {
                    event.stop_propagation();
                    let _ = send(&ExplorerCloseEditor {
                        path: close_path.clone(),
                    });
                },
                if tab.dirty {
                    span { class: "h-1.5 w-1.5 rounded-full bg-primary group-hover:hidden" }
                    span { class: "hidden leading-none group-hover:block", "\u{00D7}" }
                } else {
                    span { class: "leading-none", "\u{00D7}" }
                }
            }
        }
    }
}
