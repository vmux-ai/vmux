use dioxus::prelude::*;

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
