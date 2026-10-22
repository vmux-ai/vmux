use crate::cn::cn;
use crate::components::badge::Badge;
use crate::components::page::{PageContent, PageHeader, PageShell};
use crate::components::skeleton::Skeleton;
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ManagerTone {
    #[default]
    Neutral,
    Primary,
    Green,
    Amber,
}

impl ManagerTone {
    pub fn for_runtime(runtime: &str) -> Self {
        match runtime {
            "native" => Self::Green,
            "node" => Self::Primary,
            "python" => Self::Amber,
            _ => Self::Neutral,
        }
    }
}

impl ManagerTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "bg-foreground/[0.06] text-muted-foreground ring-foreground/10",
            Self::Primary => "bg-primary/10 text-primary ring-primary/20",
            Self::Green => "bg-success/10 text-success ring-success/20",
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
                "vmux-primary-action text-primary ring-transparent hover:bg-foreground/[0.04]"
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
        PageShell { {children} }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerTab {
    pub id: String,
    pub label: String,
    pub href: String,
}

#[component]
pub fn ManagerTabs(
    active: String,
    tabs: Vec<ManagerTab>,
    #[props(default)] content_class: String,
    onselect: EventHandler<String>,
) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto flex w-full items-center justify-center".to_string()
    } else {
        content_class
    };
    rsx! {
        nav { class: "flex shrink-0 overflow-x-auto bg-foreground/[0.018] px-5 py-2.5",
            div { class: "{content_class}",
                div { class: "flex min-w-max items-center gap-0.5 rounded-lg bg-foreground/[0.055] p-1",
                    for tab in tabs {
                        a {
                            href: "{tab.href}",
                            onclick: move |event| {
                                event.prevent_default();
                                onselect.call(tab.href.clone());
                            },
                            aria_current: if tab.id == active { "page" } else { "false" },
                            class: if tab.id == active {
                                "flex h-7 items-center rounded-md bg-foreground/[0.08] px-3 text-xs font-semibold text-foreground"
                            } else {
                                "flex h-7 items-center rounded-lg px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
                            },
                            "{tab.label}"
                        }
                    }
                }
            }
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
    #[props(default)] content_class: String,
    actions: Element,
) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto w-full max-w-5xl".to_string()
    } else {
        content_class
    };
    rsx! {
        PageHeader { content_class,
            div {
                div { class: "flex items-center gap-3",
                    h1 { class: "text-base font-semibold tracking-tight", "{title}" }
                    span { class: "text-xs tabular-nums text-muted-foreground/70", "{count}" }
                    div { class: "flex-1" }
                    {actions}
                }
                input {
                    r#type: "search",
                    class: "mt-3 w-full rounded-lg bg-foreground/[0.05] px-4 py-2.5 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:bg-foreground/[0.075]",
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
}

#[component]
pub fn ManagerToolbar(children: Element, #[props(default)] content_class: String) -> Element {
    let content_class = if content_class.is_empty() {
        "mx-auto flex h-full w-full max-w-5xl items-center gap-2.5".to_string()
    } else {
        content_class
    };
    rsx! {
        header { class: "h-11 shrink-0 bg-foreground/[0.018] px-5",
            div { class: "{content_class}", {children} }
        }
    }
}

#[component]
pub fn ManagerControls(children: Element, #[props(default)] class: String) -> Element {
    let class = if class.is_empty() {
        "flex min-h-7 items-center justify-end gap-2".to_string()
    } else {
        class
    };
    rsx! {
        div { class: "{class}", {children} }
    }
}

#[component]
pub fn ManagerSearch(
    value: String,
    placeholder: String,
    onsearch: EventHandler<FormEvent>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default = rsx! {})] prefix: Element,
    #[props(default = rsx! {})] suffix: Element,
) -> Element {
    rsx! {
        div { class: "relative z-30 flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-foreground/[0.025] px-2.5 ring-1 ring-inset ring-foreground/[0.08] focus-within:bg-foreground/[0.05] focus-within:ring-foreground/[0.16]",
            LineIconView {
                icon: LineIcon::Search,
                class: "size-3.5 shrink-0 text-muted-foreground",
            }
            {prefix}
            input {
                r#type: "search",
                class: "h-full min-w-0 flex-1 bg-transparent text-[11px] text-foreground outline-none placeholder:text-muted-foreground/55",
                placeholder,
                value,
                oninput: move |event| onsearch.call(event),
                onkeydown: move |event| {
                    if let Some(handler) = &onkeydown {
                        handler.call(event);
                    }
                },
            }
            {suffix}
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ManagerView {
    #[default]
    List,
    Grid,
}

#[component]
pub fn ManagerViewToggle(
    view: ManagerView,
    list_label: String,
    grid_label: String,
    #[props(default = LineIcon::Grid2X2)] grid_icon: LineIcon,
    onchange: EventHandler<ManagerView>,
) -> Element {
    rsx! {
        div { class: "flex h-7 items-center gap-0.5 rounded-md bg-foreground/[0.025] p-0.5 ring-1 ring-inset ring-foreground/[0.07]",
            button {
                r#type: "button",
                aria_label: list_label,
                aria_pressed: view == ManagerView::List,
                class: if view == ManagerView::List {
                    "flex h-6 items-center gap-1.5 rounded-md bg-foreground/[0.08] px-2 text-[11px] font-medium text-foreground"
                } else {
                    "flex h-6 items-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::List),
                LineIconView { icon: LineIcon::List, class: "size-3.5" }
                span { class: "hidden sm:inline", "{list_label}" }
            }
            button {
                r#type: "button",
                aria_label: grid_label,
                aria_pressed: view == ManagerView::Grid,
                class: if view == ManagerView::Grid {
                    "flex h-6 items-center gap-1.5 rounded-md bg-foreground/[0.08] px-2 text-[11px] font-medium text-foreground"
                } else {
                    "flex h-6 items-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
                },
                onclick: move |_| onchange.call(ManagerView::Grid),
                LineIconView { icon: grid_icon, class: "size-3.5" }
                span { class: "hidden sm:inline", "{grid_label}" }
            }
        }
    }
}

#[component]
pub fn ManagerList(children: Element, #[props(default)] class: String) -> Element {
    let content_class = if class.is_empty() {
        "mx-auto flex w-full max-w-5xl flex-col gap-3".to_string()
    } else {
        class
    };
    rsx! {
        PageContent { content_class, {children} }
    }
}

#[component]
pub fn ManagerSection(label: String, count: usize, children: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-2",
            header { class: "flex items-center gap-2 px-1",
                h2 { class: "text-[11px] font-semibold text-foreground/80", "{label}" }
                span { class: "text-[10px] tabular-nums text-muted-foreground/55", "{count}" }
            }
            {children}
        }
    }
}

#[component]
pub fn ManagerCollection(view: ManagerView, children: Element) -> Element {
    rsx! {
        div { class: if view == ManagerView::Grid {
                "grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-2"
            } else {
                "flex flex-col gap-2"
            },
            {children}
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
    #[props(default = true)] icon_surface: bool,
    #[props(default)] selected: bool,
    #[props(default)] onselect: Option<EventHandler<()>>,
) -> Element {
    let interactive = onselect.is_some();
    let class = match (interactive, selected) {
        (_, true) => {
            "group flex items-center gap-4 rounded-lg bg-primary/[0.09] px-4 py-3 shadow-[inset_2px_0_0_var(--primary)] transition-colors"
        }
        (true, false) => {
            "group flex cursor-pointer items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3 transition-colors hover:bg-foreground/[0.065]"
        }
        (false, false) => {
            "group flex items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3 transition-colors hover:bg-foreground/[0.065]"
        }
    };
    rsx! {
        div {
            class,
            role: if interactive { "button" } else { "group" },
            tabindex: if interactive { "0" } else { "-1" },
            aria_selected: selected,
            onclick: move |_| {
                if let Some(handler) = &onselect {
                    handler.call(());
                }
            },
            onkeydown: move |event| {
                let activate = event.key() == Key::Enter
                    || matches!(event.key(), Key::Character(key) if key == " ");
                if !activate {
                    return;
                }
                event.prevent_default();
                if let Some(handler) = &onselect {
                    handler.call(());
                }
            },
            if show_icon {
                div { class: if icon_surface {
                        "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]"
                    } else {
                        "flex h-9 w-9 shrink-0 items-center justify-center"
                    },
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
pub fn ManagerCard(
    icon: Element,
    title: String,
    subtitle: String,
    meta: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
    #[props(default = true)] icon_surface: bool,
    #[props(default)] selected: bool,
    onselect: EventHandler<()>,
) -> Element {
    let keyboard_select = onselect;
    rsx! {
        div {
            class: if selected {
                "group flex min-h-28 min-w-0 cursor-pointer flex-col rounded-xl bg-primary/[0.09] p-3 shadow-[inset_2px_0_0_var(--primary)] transition-colors"
            } else {
                "group flex min-h-28 min-w-0 cursor-pointer flex-col rounded-xl bg-foreground/[0.03] p-3 transition-colors hover:bg-foreground/[0.06]"
            },
            role: "button",
            tabindex: "0",
            aria_selected: selected,
            onclick: move |_| onselect.call(()),
            onkeydown: move |event| {
                let activate = event.key() == Key::Enter
                    || matches!(event.key(), Key::Character(key) if key == " ");
                if !activate {
                    return;
                }
                event.prevent_default();
                keyboard_select.call(());
            },
            div { class: "flex min-w-0 items-start gap-3",
                if show_icon {
                    div { class: if icon_surface {
                            "flex size-10 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.065]"
                        } else {
                            "flex size-10 shrink-0 items-center justify-center"
                        },
                        {icon}
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-medium text-foreground/95", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{subtitle}" }
                    }
                }
                {actions}
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-1.5",
                {meta}
            }
        }
    }
}

#[component]
pub fn ManagerDetails(
    icon: Element,
    title: String,
    subtitle: String,
    children: Element,
    actions: Element,
    #[props(default = true)] show_icon: bool,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        aside { class: "flex min-h-0 w-[21rem] shrink-0 flex-col bg-foreground/[0.018]",
            header { class: "flex shrink-0 items-start gap-3 p-4",
                if show_icon {
                    div { class: "flex size-9 shrink-0 items-center justify-center", {icon} }
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "truncate text-sm font-semibold text-foreground", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "mt-0.5 truncate text-xs text-muted-foreground/70", "{subtitle}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                    aria_label: translate("common-close"),
                    onclick: move |event| onclose.call(event),
                    LineIconView { icon: LineIcon::X, class: "size-3.5" }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-4 pb-4", {children} }
            footer { class: "flex shrink-0 flex-wrap items-center justify-end gap-2 px-4 pb-4 pt-2",
                {actions}
            }
        }
    }
}

#[component]
pub fn ManagerThumbnail(src: Option<String>, fallback: String) -> Element {
    rsx! {
        if let Some(src) = src.filter(|src| !src.is_empty()) {
            img {
                class: "h-6 w-6 rounded object-contain",
                src,
                draggable: "false",
            }
        } else {
            span { class: "font-mono text-[10px] text-muted-foreground", "{fallback}" }
        }
    }
}

#[component]
pub fn ManagerBadge(
    #[props(default)] tone: ManagerTone,
    #[props(default = true)] uppercase: bool,
    children: Element,
) -> Element {
    let typography = if uppercase {
        "uppercase tracking-wide"
    } else {
        "tracking-normal"
    };
    rsx! {
        Badge { class: cn(["rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ring-inset", typography, tone.classes()]),
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
}

#[component]
pub fn ManagerSelect(
    items: Vec<ManagerSelectItem>,
    value: Option<String>,
    placeholder: String,
    #[props(default)] disabled: bool,
    onselect: EventHandler<String>,
) -> Element {
    let selected = value.unwrap_or_default();
    let empty = items.is_empty();

    rsx! {
        div { class: "relative min-w-0",
            select {
                class: "w-full min-w-0 appearance-none rounded-md bg-foreground/[0.055] py-2 pl-3 pr-9 text-xs text-foreground outline-none transition-colors hover:bg-foreground/[0.08] focus-visible:bg-foreground/[0.08] disabled:pointer-events-none disabled:opacity-50",
                disabled: disabled || empty,
                value: selected.clone(),
                onchange: move |event| onselect.call(event.value()),
                if selected.is_empty() {
                    option { value: "", disabled: true, "{placeholder}" }
                }
                for item in items {
                    option { value: item.value, "{item.label}" }
                }
            }
            svg { class: "pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m6 9 6 6 6-6" }
            }
        }
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
pub fn ManagerEmpty(
    title: String,
    detail: String,
    #[props(default = rsx! {})] actions: Element,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-3 py-16 text-center",
            div { class: "text-sm text-muted-foreground", "{title}" }
            if !detail.is_empty() {
                div { class: "text-xs text-muted-foreground/70", "{detail}" }
            }
            {actions}
        }
    }
}

#[component]
pub fn ManagerSkeleton() -> Element {
    rsx! {
        for i in 0..3 {
            div { key: "{i}", class: "flex items-center gap-4 rounded-lg bg-foreground/[0.03] px-4 py-3",
                Skeleton { class: "h-9 w-9 shrink-0 rounded-lg bg-foreground/[0.06]" }
                div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                    Skeleton { class: "h-3 w-32 bg-foreground/[0.06]" }
                    Skeleton { class: "h-2.5 w-48 bg-foreground/[0.05]" }
                }
            }
        }
    }
}
