use crate::components::badge::Badge;
use crate::components::skeleton::Skeleton;
use crate::util::cn;
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
            Self::Primary => "bg-primary/15 text-primary ring-primary/30 hover:bg-primary/25",
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
            class: "flex h-full w-full flex-col overflow-hidden bg-background bg-[radial-gradient(120%_80%_at_50%_-10%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_60%)] text-foreground font-sans text-sm",
            {children}
        }
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
    onselect: EventHandler<String>,
) -> Element {
    rsx! {
        nav { class: "flex shrink-0 overflow-x-auto border-b border-foreground/[0.07] px-5 py-2.5",
            div { class: "mx-auto flex min-w-max items-center gap-0.5 rounded-xl bg-foreground/[0.06] p-1 ring-1 ring-inset ring-foreground/[0.06]",
                for tab in tabs {
                    a {
                        href: "{tab.href}",
                        onclick: move |event| {
                            event.prevent_default();
                            onselect.call(tab.href.clone());
                        },
                        aria_current: if tab.id == active { "page" } else { "false" },
                        class: if tab.id == active {
                            "flex h-7 items-center rounded-lg bg-background px-3 text-xs font-semibold text-foreground shadow-sm ring-1 ring-inset ring-foreground/[0.08]"
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
                class: "mt-3 w-full rounded-xl bg-foreground/[0.04] px-4 py-2.5 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 transition-colors placeholder:text-muted-foreground/60 focus:bg-foreground/[0.06] focus:ring-primary/30",
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
pub fn ManagerList(children: Element, #[props(default)] class: String) -> Element {
    let content_class = if class.is_empty() {
        "mx-auto flex max-w-3xl flex-col gap-2.5".to_string()
    } else {
        class
    };
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto px-5 py-5",
            div { class: "{content_class}", {children} }
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
pub fn ManagerBadge(#[props(default)] tone: ManagerTone, children: Element) -> Element {
    rsx! {
        Badge { class: cn(["rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide ring-1 ring-inset", tone.classes()]),
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
                class: "w-full min-w-0 appearance-none rounded-xl bg-background/55 py-2 pl-3 pr-9 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 transition-colors hover:bg-background/70 focus-visible:ring-primary/40 disabled:pointer-events-none disabled:opacity-50",
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
                Skeleton { class: "h-10 w-10 shrink-0 rounded-xl bg-foreground/[0.06]" }
                div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                    Skeleton { class: "h-3 w-32 bg-foreground/[0.06]" }
                    Skeleton { class: "h-2.5 w-48 bg-foreground/[0.05]" }
                }
            }
        }
    }
}
