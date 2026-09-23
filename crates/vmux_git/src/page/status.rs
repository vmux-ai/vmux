#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::badge::Badge;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;

use super::model::GitPanel;
use super::panel::{PanelHeader, PanelIcon};

#[component]
pub(super) fn StatusCard(
    repository: GitRepositoryEvent,
    focused_panel: Signal<GitPanel>,
    fetching: Signal<bool>,
) -> Element {
    let focused = focused_panel() == GitPanel::Status;

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused {
                "order-1 min-h-16 cursor-default border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_22%,transparent),0_14px_36px_rgb(0_0_0_/_12%)] sm:col-start-1 sm:row-start-1 sm:min-h-0 sm:order-none"
            } else {
                "order-1 min-h-16 cursor-default sm:col-start-1 sm:row-start-1 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Status),
            PanelHeader {
                index: 1,
                title: translate("git-status"),
                count: None,
                icon: PanelIcon::Git,
                icon_class: "bg-ansi-2/10 text-ansi-2 ring-1 ring-inset ring-ansi-2/15",
                badge_class: "",
                focused,
            }
            div { class: "flex min-h-0 flex-1 items-center gap-2 px-2 text-[11px]",
                span { class: "min-w-0 truncate font-semibold", "{repository.repo_name}" }
                span { class: "shrink-0 text-muted-foreground", "→" }
                span { class: "min-w-0 truncate font-medium", "{repository.branch}" }
                div { class: "ml-auto flex shrink-0 items-center gap-1.5 text-[10px] tabular-nums text-muted-foreground",
                    if fetching() {
                        FetchIndicator {}
                    }
                    if repository.ahead > 0 {
                        span { class: "inline-flex items-center gap-0.5 rounded-full border border-foreground/[0.08] bg-foreground/[0.035] px-1.5 py-0.5",
                            LineIconView { icon: LineIcon::ArrowUp, class: "h-3 w-3" }
                            "{repository.ahead}"
                        }
                    }
                    if repository.behind > 0 {
                        span { class: "inline-flex items-center gap-0.5 rounded-full border border-amber-400/20 bg-amber-400/[0.07] px-1.5 py-0.5 text-amber-400",
                            LineIconView { icon: LineIcon::ArrowDown, class: "h-3 w-3" }
                            "{repository.behind}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn StatusDetailCard(repository: GitRepositoryEvent, fetching: Signal<bool>) -> Element {
    let changed = repository.files.len();
    let staged = repository.files.iter().filter(|entry| entry.staged).count();
    let clean = changed == 0;

    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[26rem] border-t-ansi-2/30 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-ansi-2/[0.065] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-ansi-2/10 text-ansi-2 ring-1 ring-inset ring-ansi-2/15",
                    LineIconView { icon: LineIcon::GitBranch, class: "h-3 w-3" }
                }
                span { class: "font-mono text-[9px] font-semibold text-muted-foreground", "[0]" }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", {translate("git-status")} }
                if fetching() {
                    FetchIndicator {}
                }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-6 gap-1 rounded-md px-2 py-0 text-[10px] font-medium shadow-sm",
                    disabled: repository.branch.is_empty(),
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: repository.repo_root.clone() });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "h-3.5 w-3.5" }
                    span { class: "hidden sm:inline", {translate("git-push-label")} }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-4 sm:p-6",
                div { class: "mx-auto flex h-full max-w-3xl flex-col justify-center",
                    div { class: "flex items-start gap-4",
                        div { class: if clean {
                                "flex size-12 shrink-0 items-center justify-center rounded-2xl border border-ansi-2/20 bg-ansi-2/[0.08] text-ansi-2 shadow-[0_8px_24px_rgb(0_0_0_/_12%)]"
                            } else {
                                "flex size-12 shrink-0 items-center justify-center rounded-2xl border border-amber-400/20 bg-amber-400/[0.08] text-amber-400 shadow-[0_8px_24px_rgb(0_0_0_/_12%)]"
                            },
                            LineIconView { icon: if clean { LineIcon::ShieldCheck } else { LineIcon::GitBranch }, class: "h-5 w-5" }
                        }
                        div { class: "min-w-0 flex-1",
                            div { class: "flex flex-wrap items-center gap-2",
                                h2 { class: "truncate text-xl font-semibold tracking-[-0.025em]", "{repository.repo_name}" }
                                Badge { class: if clean {
                                        "rounded-full border border-ansi-2/20 bg-ansi-2/[0.08] px-2 py-0.5 text-[10px] font-medium text-ansi-2"
                                    } else {
                                        "rounded-full border border-amber-400/20 bg-amber-400/[0.08] px-2 py-0.5 text-[10px] font-medium text-amber-400"
                                    },
                                    if clean { {translate("git-status-clean")} } else { {translate("git-status-modified")} }
                                }
                            }
                            div { class: "mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground",
                                span { class: "inline-flex min-w-0 items-center gap-1.5 rounded-full border border-foreground/[0.08] bg-foreground/[0.035] px-2.5 py-1",
                                    LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0" }
                                    span { class: "truncate text-foreground", "{repository.branch}" }
                                }
                                span { class: "min-w-0 truncate rounded-full border border-foreground/[0.08] bg-foreground/[0.025] px-2.5 py-1",
                                    if repository.upstream.is_empty() { {translate("git-no-upstream")} } else { "{repository.upstream}" }
                                }
                            }
                        }
                    }
                    div { class: "mt-6 grid grid-cols-2 gap-2 sm:grid-cols-4",
                        StatusMetric { label: translate("git-changes"), value: changed, icon: LineIcon::File, class: "text-amber-400" }
                        StatusMetric { label: translate("git-staged-changes"), value: staged, icon: LineIcon::GitCommit, class: "text-ansi-2" }
                        StatusMetric { label: "↑".to_string(), value: repository.ahead as usize, icon: LineIcon::ArrowUp, class: "text-sky-400" }
                        StatusMetric { label: "↓".to_string(), value: repository.behind as usize, icon: LineIcon::ArrowDown, class: "text-violet-400" }
                    }
                    div { class: "mt-4 rounded-xl border border-foreground/[0.07] bg-foreground/[0.025] px-4 py-3 text-sm",
                        if clean {
                            div { class: "flex items-center gap-2 text-ansi-2",
                                LineIconView { icon: LineIcon::ShieldCheck, class: "h-4 w-4" }
                                span { class: "font-medium", {translate("git-repository-clean")} }
                            }
                        } else {
                            div { class: "flex items-center gap-2",
                                LineIconView { icon: LineIcon::File, class: "h-4 w-4 text-amber-400" }
                                span { class: "font-medium", "{changed} " {translate("git-changes")} }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FetchIndicator() -> Element {
    rsx! {
        span {
            class: "inline-flex h-5 shrink-0 items-center gap-1 rounded-full border border-sky-400/25 bg-sky-400/[0.08] px-1.5 font-medium text-sky-400",
            role: "status",
            aria_live: "polite",
            LineIconView { icon: LineIcon::RefreshCw, class: "h-3 w-3 animate-spin" }
            span { class: "hidden sm:inline", {translate("git-fetching")} }
        }
    }
}

#[component]
fn StatusMetric(label: String, value: usize, icon: LineIcon, class: &'static str) -> Element {
    rsx! {
        div { class: "rounded-xl border border-foreground/[0.07] bg-foreground/[0.025] p-3",
            div { class: "flex items-center justify-between gap-2 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground",
                span { class: "truncate", "{label}" }
                LineIconView { icon, class: "h-3.5 w-3.5 {class}" }
            }
            div { class: "mt-2 text-xl font-semibold tabular-nums", "{value}" }
        }
    }
}
