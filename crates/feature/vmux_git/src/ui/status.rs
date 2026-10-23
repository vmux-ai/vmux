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
use crate::state::{GitBranchCollection, GitCatalogProgress, GitPanel, HeaderDropdown};

use super::branches::{BranchCollectionTabs, BranchRow, TagRow};

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    open_dropdown: HeaderDropdown,
    projects: Vec<ProjectRow>,
    focused_panel: GitPanel,
    selected_branch: String,
    branch_collection: GitBranchCollection,
    selected_commit: String,
    can_push: bool,
    branches_progress: GitCatalogProgress,
    selected_branches_progress: GitCatalogProgress,
    commits_progress: GitCatalogProgress,
) -> Element {
    let push_path = repository.repo_root.clone();
    let repository_open = open_dropdown == HeaderDropdown::Repository;
    let branch_open = open_dropdown == HeaderDropdown::Branch;
    let commit_open = open_dropdown == HeaderDropdown::Commit;
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
    let branch_label = if focused_panel == GitPanel::Branches {
        selected_branch.clone()
    } else {
        repository.branch.clone()
    };
    let branch_base = if focused_panel == GitPanel::Branches {
        repository.base_ref.clone()
    } else {
        String::new()
    };
    let selected_reference_count = branch_collection.len(&repository);
    let selected_catalog = GitCatalog::from(branch_collection);
    let first_commit = repository
        .commits
        .first()
        .map(|commit| commit.sha.clone())
        .unwrap_or_default();
    let project_picker_dir = repository.repo_root.clone();
    let root_projects = projects
        .into_iter()
        .filter(|project| project.depth == 0 && !project.missing)
        .collect::<Vec<_>>();
    rsx! {
        header { class: "relative z-40 shrink-0 overflow-visible bg-card/55",
            div { class: "flex h-14 min-w-0 items-center gap-1 px-4",
                Dropdown {
                    open: repository_open,
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| {
                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                    },
                    button {
                        r#type: "button",
                        class: "flex h-8 min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs font-semibold tracking-[-0.015em] text-foreground hover:bg-foreground/[0.055]",
                        title: "{repository.repo_root}",
                        onclick: move |_| {
                            let open = if repository_open {
                                HeaderDropdown::Closed
                            } else {
                                HeaderDropdown::Repository
                            };
                            let _ = send(&HeaderDropdownRequest { open });
                        },
                        BuiltinIconView {
                            icon: vmux_api::BuiltinIcon::Project,
                            class: "size-4 shrink-0 text-muted-foreground".to_string(),
                        }
                        span { class: "min-w-0 truncate", "{repository.repo_name}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0 text-muted-foreground" }
                    }
                    if repository_open {
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
                                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
                                    let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
                    open: branch_open,
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| {
                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                    },
                    button {
                        r#type: "button",
                        class: "flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                        onclick: move |_| {
                            let open = if branch_open {
                                HeaderDropdown::Closed
                            } else {
                                HeaderDropdown::Branch
                            };
                            let _ = send(&HeaderDropdownRequest { open });
                        },
                        LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0" }
                        if !branch_base.is_empty() && branch_base != branch_label {
                            span { class: "truncate text-muted-foreground", "{branch_base}" }
                            span { class: "shrink-0 text-muted-foreground/65", "←" }
                        }
                        span { class: "truncate text-foreground/90", "{branch_label}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
                    }
                    if branch_open {
                        div {
                            class: "absolute left-0 top-9 z-40 flex max-h-96 min-w-80 flex-col overflow-hidden rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                            if focused_panel == GitPanel::Branches {
                                div { class: "flex shrink-0 items-center gap-2 px-1 pb-1.5",
                                    BranchCollectionTabs { branch_collection }
                                    span { class: "text-[9px] tabular-nums text-muted-foreground", "{selected_reference_count}" }
                                }
                                div {
                                    class: "min-h-0 overflow-y-auto",
                                    onscroll: move |event: Event<ScrollData>| {
                                        let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                                        if remaining <= 192 && selected_branches_progress.has_more && !selected_branches_progress.loading {
                                            let _ = send(&GitCatalogNextRequest { catalog: selected_catalog });
                                        }
                                    },
                                    match branch_collection {
                                        GitBranchCollection::Local => rsx! {
                                            for (index, branch) in repository.branches.iter().cloned().enumerate() {
                                                BranchRow {
                                                    key: "header-local-{branch.name}",
                                                    branch,
                                                    index,
                                                    remote: false,
                                                    selected_branch: selected_branch.clone(),
                                                    onselected: move |_| {
                                                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                    },
                                                }
                                            }
                                        },
                                        GitBranchCollection::Remote => rsx! {
                                            for (index, branch) in repository.remote_branches.iter().cloned().enumerate() {
                                                BranchRow {
                                                    key: "header-remote-{branch.name}",
                                                    branch,
                                                    index,
                                                    remote: true,
                                                    selected_branch: selected_branch.clone(),
                                                    onselected: move |_| {
                                                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                    },
                                                }
                                            }
                                        },
                                        GitBranchCollection::Tags => rsx! {
                                            for (index, tag) in repository.tags.iter().cloned().enumerate() {
                                                TagRow {
                                                    key: "header-tag-{tag.name}",
                                                    tag,
                                                    index,
                                                    selected_branch: selected_branch.clone(),
                                                    onselected: move |_| {
                                                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                    },
                                                }
                                            }
                                        },
                                    }
                                    if selected_branches_progress.loading {
                                        div { class: "flex flex-col gap-2 px-2.5 py-2",
                                            for width in ["w-9/12", "w-11/12", "w-8/12"] {
                                                vmux_ui::components::skeleton::Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div {
                                    class: "min-h-0 overflow-y-auto",
                                    onscroll: move |event: Event<ScrollData>| {
                                        let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                                        if remaining <= 192 && branches_progress.has_more && !branches_progress.loading {
                                            let _ = send(&GitCatalogNextRequest { catalog: GitCatalog::LocalBranches });
                                        }
                                    },
                                    for branch in repository.branches.iter() {
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
                                                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
                            if repository.branches.is_empty() && branch_collection == GitBranchCollection::Local {
                                if branches_progress.loading || selected_branches_progress.loading {
                                    div { class: "flex flex-col gap-2 px-2.5 py-2",
                                        for width in ["w-9/12", "w-11/12", "w-8/12"] {
                                            vmux_ui::components::skeleton::Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                        }
                                    }
                                } else {
                                    div { class: "px-2 py-3 text-xs text-muted-foreground", {translate("git-no-branches")} }
                                }
                            }
                        }
                    }
                }
                BreadcrumbSeparator {}
                ViewPicker {
                    open_dropdown,
                    focused_panel,
                    changes: repository.files.len(),
                    commits: repository.commits.len(),
                    stashes: repository.stashes.len(),
                    first_commit,
                }
                if focused_panel == GitPanel::Commits && selected_commit_entry.is_some() {
                    div { class: "flex min-w-0 items-center rounded-md hover:bg-foreground/[0.055]",
                        BreadcrumbSeparator {}
                        Dropdown {
                            open: commit_open,
                            class: "min-w-0".to_string(),
                            on_dismiss: move |_| {
                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                            },
                            button {
                                r#type: "button",
                                class: "flex h-8 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:text-foreground",
                                title: "{revision_title}",
                                onclick: move |_| {
                                    let open = if commit_open {
                                        HeaderDropdown::Closed
                                    } else {
                                        HeaderDropdown::Commit
                                    };
                                    let _ = send(&HeaderDropdownRequest { open });
                                },
                                LineIconView { icon: LineIcon::GitCommit, class: "size-3.5 shrink-0" }
                                span { class: "min-w-0 truncate text-xs font-medium text-foreground/85", "{revision_label}" }
                                LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
                            }
                            if commit_open {
                                div {
                                    class: "absolute left-0 top-9 z-40 max-h-80 min-w-80 overflow-y-auto rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                                    onscroll: move |event: Event<ScrollData>| {
                                        let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                                        if remaining <= 192 && commits_progress.has_more && !commits_progress.loading {
                                            let _ = send(&GitCatalogNextRequest {
                                                catalog: GitCatalog::Commits,
                                            });
                                        }
                                    },
                                    for commit in repository.commits.iter() {
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
                                                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
fn ViewPicker(
    open_dropdown: HeaderDropdown,
    focused_panel: GitPanel,
    changes: usize,
    commits: usize,
    stashes: usize,
    first_commit: String,
) -> Element {
    let open = open_dropdown == HeaderDropdown::View;
    let (icon, label, count) = match focused_panel {
        GitPanel::Files => (LineIcon::File, translate("git-changes"), changes),
        GitPanel::Branches => (LineIcon::GitBranch, translate("git-branch-changes"), 0),
        GitPanel::Commits => (LineIcon::GitCommit, translate("git-commits"), commits),
        GitPanel::Stash => (LineIcon::Package, translate("git-stashes"), stashes),
    };
    rsx! {
        Dropdown {
            open,
            class: "shrink-0".to_string(),
            on_dismiss: move |_| {
                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
            },
            button {
                r#type: "button",
                class: "flex h-8 items-center gap-2 rounded-md px-2.5 text-xs font-medium text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                onclick: move |_| {
                    let open = if open {
                        HeaderDropdown::Closed
                    } else {
                        HeaderDropdown::View
                    };
                    let _ = send(&HeaderDropdownRequest { open });
                },
                LineIconView { icon, class: "size-3.5 shrink-0" }
                span { "{label}" }
                if count > 0 {
                    span { class: "text-[10px] tabular-nums", "{count}" }
                }
                LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0" }
            }
            if open {
                div { class: "absolute left-0 top-9 z-40 min-w-48 rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Files,
                        icon: LineIcon::File,
                        label: translate("git-changes"),
                        count: changes,
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
                        onselect: move |_| {
                            let _ = send(&GitBranchDiffSelectRequest);
                        },
                    }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Commits,
                        icon: LineIcon::GitCommit,
                        label: translate("git-commits"),
                        count: commits,
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
                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
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
