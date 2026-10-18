#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};

use crate::event::*;
use crate::state::GitPanel;

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    focused_panel: GitPanel,
    fetching: bool,
    can_push: bool,
) -> Element {
    let changed = repository.files.len();
    let clean = changed == 0;
    let head = repository.commits.first().cloned();
    let push_path = repository.repo_root.clone();
    rsx! {
        header { class: "shrink-0 bg-foreground/[0.018]",
            div { class: "flex h-12 min-w-0 items-center gap-2 px-4",
                GitSymbolView {
                    symbol: GitSymbol::Repository,
                    class: "size-4 shrink-0 text-muted-foreground".to_string(),
                }
                span {
                    class: "min-w-0 truncate text-sm font-semibold tracking-[-0.015em]",
                    title: "{repository.repo_root}",
                    "{repository.repo_name}"
                }
                BreadcrumbSeparator {}
                span { class: "flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground",
                    LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0" }
                    span { class: "truncate text-foreground/90", "{repository.branch}" }
                }
                if let Some(head) = head {
                    BreadcrumbSeparator {}
                    span {
                        class: "flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground",
                        title: "{head.sha} — {head.summary}",
                        LineIconView { icon: LineIcon::GitCommit, class: "size-3.5 shrink-0" }
                        code { class: "shrink-0 font-mono text-[10px] text-foreground/85", "{head.short_sha}" }
                        span { class: "hidden min-w-0 truncate lg:inline", "{head.summary}" }
                    }
                }
                span { class: if clean {
                        "hidden shrink-0 items-center gap-1.5 text-[10px] font-medium text-ansi-2 sm:flex"
                    } else {
                        "hidden shrink-0 items-center gap-1.5 text-[10px] font-medium text-amber-400 sm:flex"
                    },
                    span { class: if clean { "size-1.5 rounded-full bg-ansi-2" } else { "size-1.5 rounded-full bg-amber-400" } }
                    if clean { {translate("git-status-clean")} } else { "{changed} " {translate("git-changes")} }
                }
                div { class: "ml-auto flex shrink-0 items-center gap-2 text-[10px] tabular-nums text-muted-foreground",
                    if fetching {
                        FetchIndicator {}
                    }
                    if repository.ahead > 0 {
                        span { class: "inline-flex items-center gap-0.5",
                            LineIconView { icon: LineIcon::ArrowUp, class: "size-3" }
                            "{repository.ahead}"
                        }
                    }
                    if repository.behind > 0 {
                        span { class: "inline-flex items-center gap-0.5",
                            LineIconView { icon: LineIcon::ArrowDown, class: "size-3" }
                            "{repository.behind}"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        class: "h-7 gap-1 px-2.5 text-[10px] font-medium",
                        disabled: !can_push,
                        onclick: move |_| {
                            let _ = send(&GitPushRequest { path: push_path.clone() });
                        },
                        LineIconView { icon: LineIcon::Upload, class: "size-3.5" }
                        span { class: "hidden sm:inline", {translate("git-push-label")} }
                    }
                }
            }
            nav { class: "flex h-9 items-end gap-1 px-3",
                ViewTab {
                    panel: GitPanel::Files,
                    active: focused_panel == GitPanel::Files,
                    icon: LineIcon::File,
                    label: translate("git-changes"),
                    count: repository.files.len(),
                }
                ViewTab {
                    panel: GitPanel::Branches,
                    active: focused_panel == GitPanel::Branches,
                    icon: LineIcon::GitBranch,
                    label: translate("git-branches"),
                    count: repository.branches.len()
                        + repository.remote_branches.len()
                        + repository.tags.len(),
                }
                ViewTab {
                    panel: GitPanel::Commits,
                    active: focused_panel == GitPanel::Commits,
                    icon: LineIcon::Clock,
                    label: translate("git-commits"),
                    count: repository.commits.len(),
                }
                ViewTab {
                    panel: GitPanel::Stash,
                    active: focused_panel == GitPanel::Stash,
                    icon: LineIcon::Package,
                    label: translate("git-stashes"),
                    count: repository.stashes.len(),
                }
            }
        }
    }
}

#[component]
fn BreadcrumbSeparator() -> Element {
    rsx! {
        span { class: "shrink-0 text-xs text-muted-foreground/45", "/" }
    }
}

#[component]
fn ViewTab(panel: GitPanel, active: bool, icon: LineIcon, label: String, count: usize) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active {
                "flex h-8 items-center gap-1.5 rounded-t-md bg-foreground/[0.07] px-2.5 text-[11px] font-medium text-foreground"
            } else {
                "flex h-8 items-center gap-1.5 rounded-t-md px-2.5 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.035] hover:text-foreground"
            },
            onclick: move |_| {
                let _ = send(&GitPanelSelectRequest { panel });
            },
            LineIconView { icon, class: "size-3.5" }
            span { "{label}" }
            if count > 0 {
                span { class: "text-[9px] tabular-nums text-muted-foreground", "{count}" }
            }
        }
    }
}

#[component]
fn FetchIndicator() -> Element {
    rsx! {
        span {
            class: "inline-flex items-center gap-1 text-sky-400",
            role: "status",
            aria_live: "polite",
            LineIconView { icon: LineIcon::RefreshCw, class: "size-3 animate-spin" }
            span { class: "hidden sm:inline", {translate("git-fetching")} }
        }
    }
}
