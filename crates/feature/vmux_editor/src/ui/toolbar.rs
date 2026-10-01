use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ecs::event::{ExplorerCloseEditor, FileFindRequest, FileOpenEvent, OpenEditorItem};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::scroll::ScrollIntoView;

use super::{FIND_INPUT_ID, focus_file_input};

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct EditorTabItem {
    name: String,
    context: String,
    path: String,
    active: bool,
    dirty: bool,
    is_dir: bool,
}

impl EditorTabItem {
    pub(super) fn all(items: &[OpenEditorItem]) -> Vec<Self> {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for item in items {
            *seen.entry(item.name.as_str()).or_insert(0) += 1;
        }
        let mut tabs = Vec::with_capacity(items.len());
        for item in items {
            let shared = seen.get(item.name.as_str()).copied().unwrap_or(0) > 1;
            let context = match shared {
                true => Self::parent_of(&item.path),
                false => String::new(),
            };
            tabs.push(Self {
                name: item.name.clone(),
                context,
                path: item.path.clone(),
                active: item.active,
                dirty: item.dirty,
                is_dir: item.is_dir,
            });
        }
        tabs
    }

    fn element_id(&self) -> String {
        format!("editor-tab-{}", self.path)
    }

    fn parent_of(path: &str) -> String {
        let Some((parent, _)) = path.trim_end_matches('/').rsplit_once('/') else {
            return String::new();
        };
        match parent.rsplit('/').next() {
            Some("") | None => "/".to_string(),
            Some(name) => name.to_string(),
        }
    }
}

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
    query: Signal<String>,
    forward: Signal<bool>,
    vim: bool,
    total: u32,
    index: u32,
) -> Element {
    let mut query = query;
    let mut regex = use_signal(|| vim);
    let ime = use_ime_guard();
    let mut close = move || {
        query.set(String::new());
        let _ = send(&FileFindRequest {
            done: true,
            ..Default::default()
        });
        focus_file_input();
    };
    let ask = move |text: String| {
        let _ = send(&FileFindRequest {
            query: text,
            step: false,
            reverse: false,
            done: false,
            regex: regex(),
            forward: forward(),
        });
    };
    let mut retype = move |text: String| {
        query.set(text.clone());
        ask(text);
    };
    let step = move |reverse: bool| {
        let _ = send(&FileFindRequest {
            query: query.peek().clone(),
            step: true,
            reverse,
            done: false,
            regex: regex(),
            forward: forward(),
        });
    };
    let confirm = move |reverse: bool| {
        step(reverse);
        if vim {
            focus_file_input();
        }
    };
    let count = match (total, index) {
        (0, _) => translate("editor-find-no-results"),
        (total, 0) => format!("{total}"),
        (total, index) => format!("{index}/{total}"),
    };

    rsx! {
        div {
            class: "flex h-6 shrink-0 items-center gap-1 rounded-md bg-foreground/[0.06] pl-1 pr-1 ring-1 ring-inset ring-foreground/10",
            input {
                id: FIND_INPUT_ID,
                r#type: "text",
                class: "w-40 bg-transparent font-sans text-[11px] text-foreground outline-none placeholder:text-muted-foreground",
                placeholder: translate("editor-find-placeholder"),
                value: "{query}",
                oninput: move |event| retype(event.value()),
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
                            confirm(event.modifiers().shift());
                        }
                        Key::Escape => {
                            event.prevent_default();
                            close();
                        }
                        _ => {}
                    }
                },
            }
            button {
                r#type: "button",
                class: if regex() {
                    "shrink-0 rounded bg-foreground/15 px-1 font-mono text-[10px] text-foreground"
                } else {
                    "shrink-0 rounded px-1 font-mono text-[10px] text-foreground/50 hover:bg-foreground/10 hover:text-foreground"
                },
                title: translate("editor-find-regex"),
                onclick: move |_| {
                    regex.toggle();
                    ask(query.peek().clone());
                },
                ".*"
            }
            span {
                class: if total == 0 && !query().is_empty() {
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
                onclick: move |_| step(true),
                "‹"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-next"),
                onclick: move |_| step(false),
                "›"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-close"),
                onclick: move |_| close(),
                "✕"
            }
        }
    }
}

#[component]
pub(super) fn EditorTabStrip(tabs: Vec<EditorTabItem>) -> Element {
    let active_id = match tabs.iter().find(|tab| tab.active) {
        Some(tab) => tab.element_id(),
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
fn EditorTab(tab: EditorTabItem) -> Element {
    let open_path = tab.path.clone();
    let close_path = tab.path.clone();
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
            id: tab.element_id(),
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

#[cfg(test)]
mod tests {
    use super::*;

    struct OpenEditorFixture;

    impl OpenEditorFixture {
        fn build(name: &str, path: &str) -> OpenEditorItem {
            OpenEditorItem {
                name: name.to_string(),
                path: path.to_string(),
                active: false,
                dirty: false,
                is_dir: false,
            }
        }
    }

    #[test]
    fn only_a_shared_basename_carries_its_directory() {
        let tabs = EditorTabItem::all(&[
            OpenEditorFixture::build("mod.rs", "/w/alpha/mod.rs"),
            OpenEditorFixture::build("page.rs", "/w/beta/page.rs"),
            OpenEditorFixture::build("mod.rs", "/w/beta/mod.rs"),
        ]);
        assert_eq!(tabs[0].context, "alpha");
        assert_eq!(tabs[1].context, "");
        assert_eq!(tabs[2].context, "beta");
    }

    #[test]
    fn a_shared_basename_at_the_root_names_the_root() {
        let tabs = EditorTabItem::all(&[
            OpenEditorFixture::build("a.rs", "/a.rs"),
            OpenEditorFixture::build("a.rs", "/w/a.rs"),
        ]);
        assert_eq!(tabs[0].context, "/");
        assert_eq!(tabs[1].context, "w");
    }
}
