#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::space::ProjectRow;
use vmux_ecs::event::space::{ProjectActivateRequest, ProjectPickerRequest};
use vmux_ui::components::breadcrumb::BreadcrumbSeparator;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{BuiltinIconView, LineIcon, LineIconView};

use crate::event::*;
use crate::state::GitPanel;

use super::shared::ListPager;

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    projects: Vec<ProjectRow>,
    selected_commit: String,
    can_push: bool,
) -> Element {
    let push_path = repository.repo_root.clone();
    let mut repository_open = use_signal(|| false);
    let mut branch_open = use_signal(|| false);
    let mut commit_open = use_signal(|| false);
    let branch_pager = ListPager::use_state();
    let commit_pager = ListPager::use_state();
    let branch_total = repository.branches.len();
    let commit_total = repository.commits.len();
    let selected_commit_entry = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit)
        .cloned();
    let revision_label = selected_commit_entry
        .as_ref()
        .map(|commit| commit.short_sha.clone())
        .unwrap_or_default();
    let revision_title = selected_commit_entry
        .as_ref()
        .map(|commit| commit.summary.clone())
        .unwrap_or_default();
    let project_picker_dir = repository.repo_root.clone();
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
                        class: "flex h-8 min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs font-semibold tracking-[-0.015em] text-foreground hover:bg-foreground/[0.055]",
                        title: "{repository.repo_root}",
                        onclick: move |_| repository_open.set(!repository_open()),
                        BuiltinIconView {
                            icon: vmux_api::BuiltinIcon::Project,
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
                                            div { class: "min-w-0 flex-1",
                                                div { class: "min-w-0 flex-1 truncate font-medium", "{label}" }
                                                div { class: "truncate font-mono text-[10px] text-muted-foreground", "{display_path}" }
                                            }
                                            if active {
                                                LineIconView { icon: LineIcon::Check, class: "size-3.5 shrink-0" }
                                            }
                                        }
                                    }
                                }
                            }
                            if !root_projects.is_empty() {
                                div { class: "mx-1 my-1 h-px bg-foreground/[0.08]" }
                            }
                            button {
                                r#type: "button",
                                class: "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs font-medium text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                                onclick: move |_| {
                                    repository_open.set(false);
                                    let _ = send(&ProjectPickerRequest {
                                        current_dir: project_picker_dir.clone(),
                                        pane_id: None,
                                    });
                                },
                                LineIconView { icon: LineIcon::Plus, class: "size-4 shrink-0" }
                                span { {translate("common-add")} }
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
                if selected_commit_entry.is_some() {
                    BreadcrumbSeparator {}
                    div { class: "flex min-w-0 items-center rounded-md hover:bg-foreground/[0.055]",
                        Dropdown {
                            open: commit_open(),
                            class: "min-w-0".to_string(),
                            on_dismiss: move |_| commit_open.set(false),
                            button {
                                r#type: "button",
                                class: "flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:text-foreground",
                                title: "{revision_title}",
                                onclick: move |_| commit_open.set(!commit_open()),
                                LineIconView { icon: LineIcon::GitCommit, class: "size-3.5 shrink-0" }
                                span { class: "min-w-0 truncate text-xs font-medium text-foreground/85", "{revision_label}" }
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
                                                    class: if selected_commit == commit.sha {
                                                        "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
                                                    } else {
                                                        "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
                                                    },
                                                    title: "{commit.summary}",
                                                    onclick: move |_| {
                                                        commit_open.set(false);
                                                        let _ = send(&GitPanelSelectRequest { panel: GitPanel::Commits });
                                                        let _ = send(&GitCommitSelectRequest { commit: sha.clone() });
                                                    },
                                                    code { class: "shrink-0 font-mono text-[10px] text-git-reference", "{commit.short_sha}" }
                                                    span { class: "min-w-0 flex-1 truncate font-medium", "{commit.summary}" }
                                                    span { class: "shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "mr-1 flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                            title: translate("git-branch-changes"),
                            aria_label: translate("git-branch-changes"),
                            onclick: move |_| {
                                commit_open.set(false);
                                let _ = send(&GitBranchDiffSelectRequest);
                            },
                            LineIconView { icon: LineIcon::X, class: "size-3" }
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "ml-auto h-8 gap-1.5 px-3 text-[11px] font-medium",
                    disabled: !can_push,
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: push_path.clone() });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "size-4" }
                    span { {translate("git-push-label")} }
                }
            }
        }
    }
}

#[component]
pub(super) fn RepositoryPanelHeader(
    repository: GitRepositorySnapshot,
    focused_panel: GitPanel,
    fetching: bool,
    can_stage_all: bool,
) -> Element {
    rsx! {
        div { class: "flex h-11 shrink-0 items-center gap-2 px-3 text-[10px] tabular-nums text-muted-foreground",
            ViewPicker {
                focused_panel,
                changes: repository.files.len(),
                commits: repository.commits.len(),
                stashes: repository.stashes.len(),
                first_commit: repository
                    .commits
                    .first()
                    .map(|commit| commit.sha.clone())
                    .unwrap_or_default(),
            }
            if focused_panel == GitPanel::Files && repository.files.iter().any(|file| file.unstaged) {
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-7 w-7 shrink-0 p-0 text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                    title: translate("git-stage-all"),
                    aria_label: translate("git-stage-all"),
                    disabled: !can_stage_all,
                    onclick: {
                        let path = repository.repo_root.clone();
                        move |_| {
                            let _ = send(&GitStageAllRequest { path: path.clone() });
                        }
                    },
                    LineIconView { icon: LineIcon::Plus, class: "size-3.5" }
                }
            }
            div { class: "ml-auto flex shrink-0 items-center gap-2",
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
            }
        }
    }
}

#[component]
fn ViewPicker(
    focused_panel: GitPanel,
    changes: usize,
    commits: usize,
    stashes: usize,
    first_commit: String,
) -> Element {
    let mut open = use_signal(|| false);
    let (icon, label, count) = match focused_panel {
        GitPanel::Files => (LineIcon::File, translate("git-changes"), changes),
        GitPanel::Branches => (LineIcon::GitBranch, translate("git-branch-changes"), 0),
        GitPanel::Commits => (LineIcon::GitCommit, translate("git-commits"), commits),
        GitPanel::Stash => (LineIcon::Package, translate("git-stashes"), stashes),
    };
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "flex h-8 items-center gap-2 rounded-md px-2.5 text-xs font-medium text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                onclick: move |_| open.set(!open()),
                LineIconView { icon, class: "size-3.5 shrink-0" }
                span { "{label}" }
                if count > 0 {
                    span { class: "text-[10px] tabular-nums", "{count}" }
                }
                LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
            }
            if open() {
                div { class: "absolute left-0 top-9 z-40 min-w-48 rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Files,
                        icon: LineIcon::File,
                        label: translate("git-changes"),
                        count: changes,
                        open,
                        onselect: move |_| {
                            let _ = send(&GitPanelSelectRequest { panel: GitPanel::Files });
                        },
                    }
                    div { class: "mx-1 my-1 h-px bg-foreground/[0.07]" }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Branches,
                        icon: LineIcon::GitBranch,
                        label: translate("git-branch-changes"),
                        count: 0,
                        open,
                        onselect: move |_| {
                            let _ = send(&GitBranchDiffSelectRequest);
                        },
                    }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Commits,
                        icon: LineIcon::GitCommit,
                        label: translate("git-commits"),
                        count: commits,
                        open,
                        onselect: move |_| {
                            if first_commit.is_empty() {
                                let _ = send(&GitPanelSelectRequest { panel: GitPanel::Commits });
                            } else {
                                let _ = send(&GitCommitSelectRequest {
                                    commit: first_commit.clone(),
                                });
                            }
                        },
                    }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Stash,
                        icon: LineIcon::Package,
                        label: translate("git-stashes"),
                        count: stashes,
                        open,
                        onselect: move |_| {
                            let _ = send(&GitPanelSelectRequest { panel: GitPanel::Stash });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn ViewPickerItem(
    active: bool,
    icon: LineIcon,
    label: String,
    count: usize,
    mut open: Signal<bool>,
    onselect: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active {
                "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
            } else {
                "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
            },
            onclick: move |_| {
                open.set(false);
                onselect.call(());
            },
            LineIconView { icon, class: "size-3.5 shrink-0" }
            span { class: "min-w-0 flex-1 truncate font-medium", "{label}" }
            if count > 0 {
                span { class: "shrink-0 text-[9px] tabular-nums text-muted-foreground", "{count}" }
            }
        }
    }
}

#[component]
fn FetchIndicator() -> Element {
    rsx! {
        span {
            class: "inline-flex items-center gap-1 text-git-reference",
            role: "status",
            aria_live: "polite",
            LineIconView { icon: LineIcon::RefreshCw, class: "size-3 animate-spin" }
            span { class: "hidden sm:inline", {translate("git-fetching")} }
        }
    }
}
