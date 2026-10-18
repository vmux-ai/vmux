#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::space::ProjectRow;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};

use crate::event::*;
use crate::state::GitPanel;

use super::shared::ListPager;

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    projects: Vec<ProjectRow>,
    focused_panel: GitPanel,
    fetching: bool,
    can_push: bool,
) -> Element {
    let changed = repository.files.len();
    let clean = changed == 0;
    let has_head = !repository.commits.is_empty();
    let push_path = repository.repo_root.clone();
    let mut repository_open = use_signal(|| false);
    let mut branch_open = use_signal(|| false);
    let mut commit_open = use_signal(|| false);
    let branch_pager = ListPager::use_state();
    let commit_pager = ListPager::use_state();
    let branch_total = repository.branches.len();
    let commit_total = repository.commits.len();
    let root_projects = projects
        .into_iter()
        .filter(|project| project.depth == 0 && !project.missing)
        .collect::<Vec<_>>();
    rsx! {
        header { class: "shrink-0 bg-card/55",
            div { class: "flex h-14 min-w-0 items-center gap-1 px-4",
                Dropdown {
                    open: repository_open(),
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| repository_open.set(false),
                    button {
                        r#type: "button",
                        class: "flex h-8 min-w-0 items-center gap-2 rounded-md px-2 text-left text-sm font-semibold tracking-[-0.015em] text-foreground hover:bg-foreground/[0.055]",
                        title: "{repository.repo_root}",
                        onclick: move |_| repository_open.set(!repository_open()),
                        GitSymbolView {
                            symbol: GitSymbol::Repository,
                            class: "size-4 shrink-0 text-muted-foreground".to_string(),
                        }
                        span { class: "min-w-0 truncate", "{repository.repo_name}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0 text-muted-foreground" }
                    }
                    if repository_open() {
                        div { class: "absolute left-0 top-9 z-40 max-h-72 min-w-64 overflow-y-auto rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                            for project in root_projects.iter() {
                                {
                                    let path = project.path.clone();
                                    let label = project.label.clone();
                                    let display_path = project.display_path.clone();
                                    let active = project.path == repository.repo_root;
                                    rsx! {
                                        button {
                                            key: "header-project-{path}",
                                            r#type: "button",
                                            class: if active {
                                                "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
                                            } else {
                                                "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
                                            },
                                            title: "{display_path}",
                                            onclick: move |_| {
                                                repository_open.set(false);
                                                let _ = send(&ProjectActivateRequest {
                                                    path: path.clone(),
                                                    branch: String::new(),
                                                    checkout: String::new(),
                                                    pane_id: None,
                                                });
                                            },
                                            TypeIcon { path: project.path.clone(), is_dir: true, class: "size-4 shrink-0" }
                                            div { class: "min-w-0 flex-1",
                                                div { class: "truncate font-medium", "{label}" }
                                                div { class: "truncate font-mono text-[10px] text-muted-foreground", "{display_path}" }
                                            }
                                            if active {
                                                LineIconView { icon: LineIcon::Check, class: "size-3.5 shrink-0" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                BreadcrumbSeparator {}
                Dropdown {
                    open: branch_open(),
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| branch_open.set(false),
                    button {
                        r#type: "button",
                        class: "flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                        onclick: move |_| branch_open.set(!branch_open()),
                        LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0" }
                        span { class: "truncate text-foreground/90", "{repository.branch}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
                    }
                    if branch_open() {
                        div {
                            class: "absolute left-0 top-9 z-40 max-h-80 min-w-72 overflow-y-auto rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                            onscroll: move |event| branch_pager.load_more(event, branch_total),
                            for branch in repository.branches.iter().take(branch_pager.visible(branch_total)) {
                                {
                                    let repo_root = repository.repo_root.clone();
                                    let name = branch.name.clone();
                                    let checkout = branch.checkout.clone();
                                    rsx! {
                                        button {
                                            key: "header-branch-{name}",
                                            r#type: "button",
                                            class: if branch.current {
                                                "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
                                            } else {
                                                "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
                                            },
                                            onclick: move |_| {
                                                branch_open.set(false);
                                                let _ = send(&ProjectActivateRequest {
                                                    path: repo_root.clone(),
                                                    branch: name.clone(),
                                                    checkout: checkout.clone(),
                                                    pane_id: None,
                                                });
                                            },
                                            LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0" }
                                            span { class: "min-w-0 flex-1 truncate font-medium", "{branch.name}" }
                                            if branch.current {
                                                LineIconView { icon: LineIcon::Check, class: "size-3.5 shrink-0" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if has_head {
                    BreadcrumbSeparator {}
                    Dropdown {
                        open: commit_open(),
                        class: "min-w-0".to_string(),
                        on_dismiss: move |_| commit_open.set(false),
                        button {
                            r#type: "button",
                            class: "flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                            title: translate("git-latest"),
                            onclick: move |_| commit_open.set(!commit_open()),
                            LineIconView { icon: LineIcon::GitCommit, class: "size-3.5 shrink-0" }
                            span { class: "shrink-0 text-[10px] font-medium text-foreground/85", {translate("git-latest")} }
                            LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
                        }
                        if commit_open() {
                            div {
                                class: "absolute left-0 top-9 z-40 max-h-80 min-w-80 overflow-y-auto rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                                onscroll: move |event| commit_pager.load_more(event, commit_total),
                                for commit in repository.commits.iter().take(commit_pager.visible(commit_total)) {
                                    {
                                        let sha = commit.sha.clone();
                                        rsx! {
                                            button {
                                                key: "header-commit-{sha}",
                                                r#type: "button",
                                                class: "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                                                title: "{commit.summary}",
                                                onclick: move |_| {
                                                    commit_open.set(false);
                                                    let _ = send(&GitPanelSelectRequest { panel: GitPanel::Commits });
                                                    let _ = send(&GitCommitSelectRequest { commit: sha.clone() });
                                                },
                                                code { class: "shrink-0 font-mono text-[10px] text-sky-400", "{commit.short_sha}" }
                                                span { class: "min-w-0 flex-1 truncate font-medium", "{commit.summary}" }
                                                span { class: "shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
            nav { class: "flex h-10 items-center gap-1 px-4",
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
                "flex h-8 items-center gap-1.5 rounded-md bg-foreground/[0.08] px-3 text-[11px] font-medium text-foreground"
            } else {
                "flex h-8 items-center gap-1.5 rounded-md px-3 text-[11px] font-medium text-muted-foreground hover:bg-foreground/[0.045] hover:text-foreground"
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
