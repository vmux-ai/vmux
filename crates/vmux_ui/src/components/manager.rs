use crate::hooks::{MenuDirection, move_selection, use_selector};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_MANAGER_SELECT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ManagerTone {
    #[default]
    Neutral,
    Cyan,
    Green,
    Amber,
}

impl ManagerTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "bg-foreground/[0.06] text-muted-foreground ring-foreground/10",
            Self::Cyan => "bg-cyan-400/10 text-cyan-700 dark:text-cyan-300 ring-cyan-400/20",
            Self::Green => {
                "bg-emerald-400/10 text-emerald-700 dark:text-emerald-300 ring-emerald-400/20"
            }
            Self::Amber => "bg-amber-400/10 text-amber-700 dark:text-amber-300 ring-amber-400/20",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ManagerButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Ghost,
}

impl ManagerButtonVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Primary => {
                "bg-cyan-400/15 text-cyan-700 dark:text-cyan-200 ring-cyan-400/30 hover:bg-cyan-400/25"
            }
            Self::Secondary => {
                "bg-foreground/[0.05] text-foreground/80 ring-foreground/10 hover:bg-foreground/[0.09]"
            }
            Self::Danger => {
                "bg-foreground/[0.05] text-foreground/70 ring-foreground/10 hover:bg-ansi-1/15 hover:text-ansi-1"
            }
            Self::Ghost => {
                "text-muted-foreground ring-transparent hover:bg-foreground/[0.08] hover:text-foreground"
            }
        }
    }
}

#[component]
pub fn ManagerPage(children: Element) -> Element {
    rsx! {
        main {
            class: "flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-sans text-sm",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);",
            {children}
        }
    }
}

#[component]
pub fn ManagerHeader(
    title: String,
    count: usize,
    search_value: String,
    search_placeholder: String,
    onsearch: EventHandler<FormEvent>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    actions: Element,
) -> Element {
    rsx! {
        header { class: "shrink-0 border-b border-foreground/[0.07] px-5 py-3",
            div { class: "flex items-center gap-3",
                h1 { class: "text-base font-semibold tracking-tight", "{title}" }
                span { class: "text-xs tabular-nums text-muted-foreground/70", "{count}" }
                div { class: "flex-1" }
                {actions}
            }
            input {
                r#type: "search",
                class: "mt-3 w-full rounded-xl bg-foreground/[0.04] px-4 py-2.5 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 transition-colors placeholder:text-muted-foreground/60 focus:bg-foreground/[0.06] focus:ring-cyan-400/30",
                placeholder: "{search_placeholder}",
                value: "{search_value}",
                oninput: move |event| onsearch.call(event),
                onkeydown: move |event| {
                    if let Some(handler) = &onkeydown {
                        handler.call(event);
                    }
                },
            }
        }
    }
}

#[component]
pub fn ManagerList(children: Element) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto px-5 py-5",
            div { class: "mx-auto flex max-w-3xl flex-col gap-2.5", {children} }
        }
    }
}

#[component]
pub fn ManagerRow(
    icon: Element,
    title: String,
    subtitle: String,
    meta: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
) -> Element {
    rsx! {
        div { class: "group flex items-center gap-4 rounded-2xl bg-foreground/[0.035] px-5 py-4 ring-1 ring-inset ring-foreground/10 backdrop-blur-xl transition-colors hover:bg-foreground/[0.07]",
            if show_icon {
                div { class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-foreground/[0.06] ring-1 ring-inset ring-foreground/10",
                    {icon}
                }
            }
            div { class: "flex min-w-0 flex-1 flex-col gap-1",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate font-medium text-foreground/95", "{title}" }
                    {meta}
                }
                if !subtitle.is_empty() {
                    span { class: "truncate text-xs text-muted-foreground/70", "{subtitle}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-2", {actions} }
        }
    }
}

#[component]
pub fn ManagerBadge(#[props(default)] tone: ManagerTone, children: Element) -> Element {
    rsx! {
        span { class: "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide ring-1 ring-inset {tone.classes()}",
            {children}
        }
    }
}

#[component]
pub fn ManagerButton(
    #[props(default)] variant: ManagerButtonVariant,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            class: "shrink-0 rounded-lg px-3 py-1.5 text-xs font-medium ring-1 ring-inset transition-colors disabled:pointer-events-none disabled:opacity-50 {variant.classes()}",
            disabled,
            onclick: move |event| onclick.call(event),
            {children}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerSelectItem {
    pub value: String,
    pub label: String,
    pub kind: ManagerSelectItemKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ManagerSelectItemKind {
    #[default]
    Default,
    User,
    Organization,
}

#[component]
pub fn ManagerSelect(
    items: Vec<ManagerSelectItem>,
    value: Option<String>,
    placeholder: String,
    #[props(default)] disabled: bool,
    onselect: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut highlighted = use_signal(|| 0usize);
    let id = use_hook(|| {
        format!(
            "manager-select-{}",
            NEXT_MANAGER_SELECT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let selected_index = value
        .as_ref()
        .and_then(|value| items.iter().position(|item| item.value == *value));
    let selected_item = selected_index.and_then(|index| items.get(index));
    let selected_label = selected_item
        .map(|item| item.label.as_str())
        .unwrap_or(&placeholder);

    let scroll_id = id.clone();
    use_selector(highlighted, move |index| {
        if open() {
            format!("{scroll_id}-option-{index}")
        } else {
            String::new()
        }
    });

    let key_items = items.clone();
    let key_select = onselect;
    let onkeydown = move |event: KeyboardEvent| {
        let control = event.modifiers().contains(Modifiers::CONTROL);
        let down = event.key() == Key::ArrowDown
            || (control && matches!(event.code(), Code::KeyN | Code::KeyJ));
        let up = event.key() == Key::ArrowUp
            || (control && matches!(event.code(), Code::KeyP | Code::KeyK));
        if (down || up) && !key_items.is_empty() {
            event.prevent_default();
            event.stop_propagation();
            if open() {
                let current = highlighted().min(key_items.len() - 1);
                let direction = if down {
                    MenuDirection::Next
                } else {
                    MenuDirection::Previous
                };
                highlighted.set(move_selection(current, key_items.len(), direction));
            } else {
                open.set(true);
                highlighted.set(if down {
                    selected_index.unwrap_or(0)
                } else {
                    selected_index.unwrap_or(key_items.len() - 1)
                });
            }
            return;
        }
        let activate = event.key() == Key::Enter
            || matches!(event.key(), Key::Character(ref character) if character == " ");
        if activate {
            event.prevent_default();
            event.stop_propagation();
            if open() {
                if let Some(item) = key_items.get(highlighted()) {
                    key_select.call(item.value.clone());
                }
                open.set(false);
            } else if !key_items.is_empty() {
                highlighted.set(selected_index.unwrap_or(0));
                open.set(true);
            }
            return;
        }
        if event.key() == Key::Escape && open() {
            event.prevent_default();
            event.stop_propagation();
            open.set(false);
        }
    };

    rsx! {
        div { class: "relative min-w-0",
            button {
                r#type: "button",
                class: "flex w-full min-w-0 items-center gap-2 rounded-xl bg-background/55 py-2 pl-3 pr-4 text-left text-xs text-foreground ring-1 ring-inset ring-foreground/10 transition-colors hover:bg-background/70 focus-visible:outline-none focus-visible:ring-primary/40 disabled:pointer-events-none disabled:opacity-50",
                disabled: disabled || items.is_empty(),
                aria_haspopup: "listbox",
                aria_expanded: open(),
                onkeydown,
                onblur: move |_| open.set(false),
                onclick: move |_| {
                    if !items.is_empty() {
                        highlighted.set(selected_index.unwrap_or(0));
                        open.toggle();
                    }
                },
                if let Some(item) = selected_item {
                    ManagerSelectItemIcon { kind: item.kind }
                }
                span { class: if selected_index.is_some() { "min-w-0 flex-1 truncate" } else { "min-w-0 flex-1 truncate text-muted-foreground" },
                    "{selected_label}"
                }
                svg { class: "h-4 w-4 shrink-0 transition-transform data-[open=true]:rotate-180", "data-open": open(), view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "m6 9 6 6 6-6" }
                }
            }
            if open() {
                div {
                    class: "absolute left-0 top-full z-[1000] mt-1 max-h-64 w-full min-w-48 overflow-y-auto rounded-xl bg-background/95 p-1 text-xs shadow-xl ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                    role: "listbox",
                    for (index, item) in items.iter().enumerate() {
                        div {
                            id: "{id}-option-{index}",
                            role: "option",
                            aria_selected: value.as_ref() == Some(&item.value),
                            class: if highlighted() == index {
                                "flex cursor-pointer items-center gap-2 rounded-lg bg-foreground/[0.09] px-3 py-2 text-foreground"
                            } else {
                                "flex cursor-pointer items-center gap-2 rounded-lg px-3 py-2 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground"
                            },
                            onpointerenter: move |_| highlighted.set(index),
                            onpointerdown: {
                                let item_value = item.value.clone();
                                move |event| {
                                    event.prevent_default();
                                    onselect.call(item_value.clone());
                                    open.set(false);
                                }
                            },
                            ManagerSelectItemIcon { kind: item.kind }
                            span { class: "min-w-0 flex-1 truncate", "{item.label}" }
                            if value.as_ref() == Some(&item.value) {
                                svg { class: "h-3.5 w-3.5 shrink-0", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "m5 12 4 4L19 6" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ManagerSelectItemIcon(kind: ManagerSelectItemKind) -> Element {
    match kind {
        ManagerSelectItemKind::Default => rsx! {},
        ManagerSelectItemKind::User => rsx! {
            svg { class: "h-3.5 w-3.5 shrink-0 text-muted-foreground/70", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                circle { cx: "12", cy: "8", r: "4" }
                path { d: "M4 21a8 8 0 0 1 16 0" }
            }
        },
        ManagerSelectItemKind::Organization => rsx! {
            svg { class: "h-3.5 w-3.5 shrink-0 text-muted-foreground/70", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M3 21h18" }
                path { d: "M6 21V5l6-3 6 3v16" }
                path { d: "M9 9h1" }
                path { d: "M14 9h1" }
                path { d: "M9 13h1" }
                path { d: "M14 13h1" }
                path { d: "M9 17h6" }
            }
        },
    }
}

#[component]
pub fn ManagerSpinner(detail: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 text-xs text-muted-foreground",
            span { class: "h-3.5 w-3.5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            if !detail.is_empty() {
                span { class: "max-w-44 truncate", "{detail}" }
            }
        }
    }
}

#[component]
pub fn ManagerEmpty(title: String, detail: String) -> Element {
    rsx! {
        div { class: "flex flex-col items-center gap-2 px-3 py-16 text-center",
            div { class: "text-sm text-muted-foreground", "{title}" }
            if !detail.is_empty() {
                div { class: "text-xs text-muted-foreground/70", "{detail}" }
            }
        }
    }
}

#[component]
pub fn ManagerSkeleton() -> Element {
    rsx! {
        for i in 0..3 {
            div { key: "{i}", class: "flex items-center gap-4 rounded-2xl bg-foreground/[0.035] px-5 py-4 ring-1 ring-inset ring-foreground/10",
                div { class: "h-10 w-10 shrink-0 animate-pulse rounded-xl bg-foreground/[0.06]" }
                div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                    div { class: "h-3 w-32 animate-pulse rounded bg-foreground/[0.06]" }
                    div { class: "h-2.5 w-48 animate-pulse rounded bg-foreground/[0.05]" }
                }
            }
        }
    }
}
