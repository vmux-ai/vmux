#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;
use vmux_api::space::ProjectRow;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::breadcrumb::BreadcrumbSeparator;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::dropdown::{Dropdown, DropdownPopup};
use vmux_ui::components::project_picker::{ProjectSelectPopup, ProjectSelectTrigger};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

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
    selected_branch_base: String,
    branch_collection: GitBranchCollection,
    selected_branch_collection: GitBranchCollection,
    selected_commit: String,
    can_push: bool,
    branches_progress: GitCatalogProgress,
    selected_branches_progress: GitCatalogProgress,
    commits_progress: GitCatalogProgress,
    viewed_count: usize,
    viewed_total: u32,
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
        selected_branch_base
    } else {
        String::new()
    };
    let checkout = (focused_panel == GitPanel::Branches)
        .then(|| selected_branch_collection.checkout(&repository, &selected_branch))
        .flatten();
    let checkout_required = focused_panel == GitPanel::Branches
        && checkout.as_ref().is_some_and(|checkout| !checkout.current);
    let checkout_branch = checkout
        .as_ref()
        .map(|checkout| checkout.branch.clone())
        .unwrap_or_default();
    let checkout_path = checkout
        .as_ref()
        .map(|checkout| checkout.checkout.clone())
        .unwrap_or_default();
    let checkout_start = checkout
        .as_ref()
        .map(|checkout| checkout.start_point.clone())
        .unwrap_or_default();
    let selected_reference_count = branch_collection.len(&repository);
    let selected_catalog = GitCatalog::from(branch_collection);
    let first_commit = repository
        .commits
        .first()
        .map(|commit| commit.sha.clone())
        .unwrap_or_default();
    let project_picker_dir = repository.repo_root.clone();
    let mut root_projects = projects
        .into_iter()
        .filter(|project| project.depth == 0 && !project.missing)
        .collect::<Vec<_>>();
    if !root_projects
        .iter()
        .any(|project| project.path == repository.repo_root)
    {
        root_projects.insert(
            0,
            ProjectRow {
                path: repository.repo_root.clone(),
                label: repository.repo_name.clone(),
                display_path: repository.repo_root.clone(),
                is_active: true,
                is_worktree: repository.is_worktree,
                branch: repository.branch.clone(),
                ..Default::default()
            },
        );
    }
    rsx! {
        header { class: "relative z-40 shrink-0 overflow-visible bg-transparent",
            div { class: "flex h-14 min-w-0 items-center gap-1 px-4",
                Dropdown {
                    open: repository_open,
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| {
                        let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                    },
                    ProjectSelectTrigger {
                        label: repository.repo_name.clone(),
                        title: repository.repo_root.clone(),
                        open: repository_open,
                        onclick: move |_| {
                            let open = if repository_open {
                                HeaderDropdown::Closed
                            } else {
                                HeaderDropdown::Repository
                            };
                            let _ = send(&HeaderDropdownRequest { open });
                        },
                    }
                    if repository_open {
                        ProjectSelectPopup {
                            projects: root_projects.clone(),
                            selected_path: repository.repo_root.clone(),
                            class: "absolute left-0 top-9 z-40 max-h-72 min-w-64 overflow-y-auto".to_string(),
                            on_pick: move |project: ProjectRow| {
                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                let _ = send(&GitRepositoryRequest { path: project.path });
                            },
                            on_add: move |_| {
                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                let _ = send(&GitRepositoryPickerRequest {
                                    path: project_picker_dir.clone(),
                                });
                            },
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
                        DropdownPopup {
                            class: "absolute left-0 top-9 z-40 flex max-h-96 min-w-80 flex-col overflow-hidden p-1.5",
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
                                                {
                                                    let checked_out = GitBranchCollection::Local
                                                        .is_checked_out(&repository, &branch.name);
                                                    rsx! {
                                                        BranchRow {
                                                            key: "header-local-{branch.name}",
                                                            branch,
                                                            index,
                                                            remote: false,
                                                            selected_branch: selected_branch.clone(),
                                                            checked_out,
                                                            onselected: move |_| {
                                                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        GitBranchCollection::Remote => rsx! {
                                            for (index, branch) in repository.remote_branches.iter().cloned().enumerate() {
                                                {
                                                    let checked_out = GitBranchCollection::Remote
                                                        .is_checked_out(&repository, &branch.name);
                                                    rsx! {
                                                        BranchRow {
                                                            key: "header-remote-{branch.name}",
                                                            branch,
                                                            index,
                                                            remote: true,
                                                            selected_branch: selected_branch.clone(),
                                                            checked_out,
                                                            onselected: move |_| {
                                                                let _ = send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                            },
                                                        }
                                                    }
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
                                    if selected_reference_count > 0 && selected_branches_progress.loaded && selected_branches_progress.loading {
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
                                            let name = branch.name.clone();
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
                                                        let _ = send(&GitPanelSelectRequest { panel: GitPanel::Branches });
                                                        let _ = send(&GitBranchCollectionSelectRequest {
                                                            collection: GitBranchCollection::Local,
                                                        });
                                                        let _ = send(&GitBranchSelectRequest {
                                                            reference: name.clone(),
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
                            if selected_reference_count == 0 {
                                if selected_branches_progress.loading && !selected_branches_progress.initialized {
                                    div { class: "flex min-h-28 flex-col justify-center gap-2 px-2.5 py-2",
                                        for width in ["w-9/12", "w-11/12", "w-8/12"] {
                                            vmux_ui::components::skeleton::Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                        }
                                    }
                                } else {
                                    div { class: "flex min-h-28 items-center justify-center px-3 text-xs text-muted-foreground", {translate("common-no-results")} }
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
                    commit_selected: selected_commit_entry.is_some(),
                }
                if focused_panel == GitPanel::Commits && selected_commit_entry.is_some() {
                    div { class: "flex min-w-0 items-center",
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
                                DropdownPopup {
                                    class: "absolute left-0 top-9 z-40 min-w-80 overflow-hidden p-1.5",
                                    div {
                                        class: "max-h-80 overflow-y-auto",
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
                        }
                    }
                }
                if focused_panel == GitPanel::Branches && viewed_total > 0 {
                    span { class: "ml-auto shrink-0 text-[11px] tabular-nums text-muted-foreground",
                        "{viewed_count} / {viewed_total} "
                        {translate("git-viewed")}
                    }
                }
                if checkout_required {
                    Button {
                        variant: ButtonVariant::Primary,
                        class: if viewed_total > 0 {
                            "h-8 gap-2.5 px-3 text-[11px] font-medium"
                        } else {
                            "ml-auto h-8 gap-2.5 px-3 text-[11px] font-medium"
                        },
                        onclick: move |_| {
                            if checkout_start.is_empty() {
                                let _ = send(&ProjectActivateRequest {
                                    path: push_path.clone(),
                                    branch: checkout_branch.clone(),
                                    checkout: checkout_path.clone(),
                                    pane_id: None,
                                });
                            } else {
                                let _ = send(&GitCreateBranchRequest {
                                    repo_root: push_path.clone(),
                                    branch: checkout_branch.clone(),
                                    start_point: checkout_start.clone(),
                                });
                            }
                        },
                        LineIconView { icon: LineIcon::GitBranch, class: "size-4" }
                        span { {translate("git-checkout")} }
                    }
                } else {
                    Button {
                        variant: ButtonVariant::Primary,
                        class: if focused_panel == GitPanel::Branches && viewed_total > 0 {
                            "h-8 gap-2.5 px-3 text-[11px] font-medium"
                        } else {
                            "ml-auto h-8 gap-2.5 px-3 text-[11px] font-medium"
                        },
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
}

#[component]
fn ViewPicker(
    open_dropdown: HeaderDropdown,
    focused_panel: GitPanel,
    changes: usize,
    commits: usize,
    stashes: usize,
    first_commit: String,
    commit_selected: bool,
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
            div { class: "flex items-center rounded-md hover:bg-foreground/[0.055]",
                button {
                    r#type: "button",
                    class: "flex h-8 items-center gap-2 rounded-md px-2.5 text-xs font-medium text-muted-foreground hover:text-foreground",
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
                if focused_panel == GitPanel::Commits && commit_selected {
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
            if open {
                DropdownPopup { class: "absolute left-0 top-9 z-40 min-w-48 p-1.5",
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
                    div { class: "mx-1 my-1 h-px bg-foreground/[0.07]" }
                    ViewPickerItem {
                        active: false,
                        icon: LineIcon::MessagesRound,
                        label: translate("sessions-title"),
                        count: 0,
                        onselect: move |_| {
                            let _ = send(&OpenRequest {
                                value: "vmux://sessions/".to_string(),
                                open: Some(OpenTarget::InPlace),
                            });
                        },
                    }
                    ViewPickerItem {
                        active: false,
                        icon: LineIcon::Notebook,
                        label: translate("layout-projects"),
                        count: 0,
                        onselect: move |_| {
                            let _ = send(&OpenRequest {
                                value: "vmux://projects/".to_string(),
                                open: Some(OpenTarget::InPlace),
                            });
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
