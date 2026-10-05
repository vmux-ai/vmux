use dioxus::prelude::*;
use vmux_api::command_bar::{CommandBarResultItem, CommandBarSection};
use vmux_ui::cn::cn;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::favicon::Favicon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::PageIconView;

#[component]
pub fn ResultRow(
    index: usize,
    item: CommandBarResultItem,
    selected: bool,
    on_activate: EventHandler<()>,
    on_hover: EventHandler<()>,
) -> Element {
    let section = item.section.clone();
    let disabled = item.disabled;
    let row_class = if selected {
        "flex min-h-15 min-w-0 w-full items-center justify-between overflow-hidden bg-primary/12 px-3.5 py-2.5 text-foreground shadow-[inset_2px_0_0_0_var(--primary),0_0_18px_-4px_color-mix(in_oklab,var(--primary)_45%,transparent)]"
    } else {
        "flex min-h-15 min-w-0 w-full items-center justify-between overflow-hidden px-3.5 py-2.5 hover:bg-foreground/5"
    };
    rsx! {
        if let Some(section) = section {
            SectionRow { section }
        }
        div {
            id: "command-bar-item-{index}",
            class: cn([row_class, if disabled { "cursor-default" } else { "cursor-pointer" }]),
            onclick: move |_| {
                if !disabled {
                    on_activate.call(());
                }
            },
            onmouseenter: move |_| on_hover.call(()),
            if item.pending {
                PendingRow { index }
            } else {
                div { class: "flex min-w-0 flex-1 items-start gap-2 overflow-hidden",
                    RowIcon { item: item.clone() }
                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                        div { class: "flex min-w-0 items-center gap-2",
                            span { class: "min-w-0 truncate text-base leading-snug text-foreground", "{item.title}" }
                            if item.active {
                                span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", {translate("common-active")} }
                            }
                        }
                        if !item.subtitle.is_empty() || !item.badge.is_empty() {
                            div { class: "flex min-w-0 items-center gap-1.5",
                                if !item.badge.is_empty() {
                                    span { class: "max-w-full shrink-0 truncate rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground", "{item.badge}" }
                                }
                                if !item.subtitle.is_empty() {
                                    span { class: "min-w-0 truncate text-sm leading-snug text-muted-foreground", "{item.subtitle}" }
                                }
                            }
                        }
                        if !item.detail.is_empty() {
                            span { class: "min-w-0 truncate text-xs leading-snug text-muted-foreground/70", "{item.detail}" }
                        }
                    }
                }
                span {
                    class: "ml-3 min-w-0 max-w-[46%] shrink-0 truncate rounded-md px-2 py-1 text-right font-mono text-[11px] text-muted-foreground",
                    title: "{item.trailing}",
                    "{item.trailing}"
                }
            }
        }
    }
}

#[component]
fn RowIcon(item: CommandBarResultItem) -> Element {
    if !item.file_path.is_empty() {
        return if item.directory {
            rsx! {
                Icon { class: "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground",
                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                }
            }
        } else {
            rsx! {
                Icon { class: "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground",
                    path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                    path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                }
            }
        };
    }
    if !item.favicon_url.is_empty() {
        return rsx! {
            Favicon {
                favicon_url: item.favicon_url,
                url: item.url,
                class: "mt-0.5 h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                globe_class: "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground".to_string(),
            }
        };
    }
    if !item.url.is_empty() {
        return rsx! {
            PageIconView {
                icon: item.icon,
                url: item.url,
                img_class: "mt-0.5 h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                icon_class: "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground".to_string(),
            }
        };
    }
    if item.leading.is_empty() {
        return rsx! {};
    }
    rsx! {
        span { class: "mt-0.5 shrink-0 font-mono text-sm text-muted-foreground", "{item.leading}" }
    }
}

#[component]
fn PendingRow(index: usize) -> Element {
    let title = ["w-2/5", "w-3/5", "w-1/2", "w-7/12"][index % 4];
    let detail = ["w-3/4", "w-1/2", "w-5/6", "w-2/3"][index % 4];
    rsx! {
        div { class: "flex min-w-0 flex-1 items-start gap-2 overflow-hidden",
            span { class: "shrink-0 text-sm text-muted-foreground/40", "\u{21ba}" }
            div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                Skeleton { class: cn(["h-3 bg-muted-foreground/20", title]) }
                Skeleton { class: cn(["h-2.5 bg-muted-foreground/10", detail]) }
            }
        }
    }
}

#[component]
fn SectionRow(section: CommandBarSection) -> Element {
    if section.labels.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "sticky top-0 z-10 flex min-w-0 items-center gap-1.5 border-y border-foreground/[0.06] bg-background/95 px-4 py-1.5 backdrop-blur",
            for label in section.labels {
                span {
                    class: "flex h-6 min-w-0 items-center rounded-md bg-foreground/[0.045] px-2 text-[11px] text-muted-foreground/75",
                    title: "{label}",
                    span { class: "min-w-0 truncate", "{label}" }
                }
            }
            span { class: "ml-auto flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full bg-foreground/[0.055] px-1.5 font-mono text-[10px] tabular-nums text-muted-foreground/55", "{section.count}" }
        }
    }
}
